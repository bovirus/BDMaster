/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

/*
 * Lightweight M2TS / MPEG-TS scanner. Parses the 192-byte BDAV packet format
 * (4-byte arrival timecode + 188-byte MPEG-TS packet) to discover PIDs from
 * PAT/PMT, total bytes per PID, and bitrate-over-time samples for charts.
 *
 * This is a pragmatic port of TSStreamFile.cs. It does not run the deep
 * codec parsers (TSCodec*.cs) — codec details still come from MPLS for now.
 */

use anyhow::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::{Duration, Instant};

/// What the scanner should do after a PES dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PesAction {
  /// Keep going.
  Continue,
  /// Abort the entire scan immediately.
  Stop,
  /// Continue scanning, but stop reassembling PES for this PID. The
  /// scanner still counts bytes per PID so measured-size accounting is
  /// preserved — only the per-packet `extend_from_slice` work is skipped.
  /// This is the big win for the full scan: once a non-PGS stream is
  /// initialized we don't need any more of its PES, but we do still need
  /// to know how many bytes it consumed.
  SkipPid,
}

/// Run the streaming scan against an opaque reader. The native and UDF code
/// paths both funnel into this entry point.
///
/// The callback signature is `fn(pid, stream_type, pes_payload, pmt) -> PesAction`
/// where `pmt` is the live PID → stream-type table populated from PAT/PMT.
pub fn scan_m2ts_streaming_from_reader<R, F>(reader: R, mut on_pes: F) -> Result<M2tsScanResult>
where
  R: Read,
  F: FnMut(u16, u8, &[u8], &HashMap<u16, u8>) -> PesAction,
{
  scan_inner(reader, |pid, st, payload, pmt| on_pes(pid, st, payload, pmt), |_| {})
}

pub fn scan_m2ts_streaming_from_reader_with_progress<R, F, P>(
  reader: R,
  mut on_pes: F,
  mut on_progress: P,
) -> Result<M2tsScanResult>
where
  R: Read,
  F: FnMut(u16, u8, &[u8], &HashMap<u16, u8>) -> PesAction,
  P: FnMut(M2tsScanProgress),
{
  scan_inner(
    reader,
    |pid, st, payload, pmt| on_pes(pid, st, payload, pmt),
    |progress| on_progress(progress),
  )
}

pub fn scan_m2ts_from_reader<R: Read>(reader: R) -> Result<M2tsScanResult> {
  scan_inner(reader, |_, _, _, _| PesAction::Continue, |_| {})
}

const TS_PACKET_SIZE: usize = 188;
const M2TS_PACKET_SIZE: usize = 192;
const MAX_PID: usize = 8192;
const SYNC_BYTE: u8 = 0x47;

#[derive(Debug, Clone)]
pub struct M2tsScanResult {
  pub bytes: u64,
  pub duration_seconds: f64,
  pub streams: HashMap<u16, StreamStats>,
  pub bitrate_samples: Vec<(f64, u64)>,
  pub program_pmt_pids: Vec<u16>,
  pub pcr_pid: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct M2tsScanProgress {
  pub bytes: u64,
  pub duration_seconds: f64,
  pub streams: HashMap<u16, StreamStats>,
}

#[derive(Debug, Clone)]
pub struct StreamStats {
  pub pid: u16,
  pub stream_type: u8,
  pub total_bytes: u64,
  pub packet_count: u64,
  /// First reassembled PES payload (without the PES header) up to ~64KB.
  /// Used by codec parsers to extract format details.
  pub pes_sample: Vec<u8>,
  /// PUSI-marked partial PES we are currently building, used to fill pes_sample.
  pub pes_in_progress: Vec<u8>,
  pub pes_started: bool,
}

const SAMPLE_INTERVAL_SECONDS: f64 = 1.0;

/// Streaming scan from a path. Equivalent to `scan_inner` over a buffered
/// `File`.
pub fn scan_m2ts_streaming<F>(path: &Path, on_pes: F) -> Result<M2tsScanResult>
where
  F: FnMut(u16, u8, &[u8], &HashMap<u16, u8>) -> PesAction,
{
  let file = File::open(path)?;
  let reader = BufReader::with_capacity(1 << 20, file);
  scan_inner(reader, on_pes, |_| {})
}

fn scan_inner<R, F, P>(reader: R, mut on_pes: F, mut on_progress: P) -> Result<M2tsScanResult>
where
  R: Read,
  F: FnMut(u16, u8, &[u8], &HashMap<u16, u8>) -> PesAction,
  P: FnMut(M2tsScanProgress),
{
  let mut reader = reader;
  let mut pmt_pid_set = std::collections::HashSet::<u16>::new();
  let mut pmt_pid_flags = [false; MAX_PID];
  let mut pmt_pids: Vec<u16> = Vec::new();
  let mut pid_to_stream_type: HashMap<u16, u8> = HashMap::new();
  let mut stream_type_by_pid = [0u8; MAX_PID];
  let mut pid_seen = [false; MAX_PID];
  let mut seen_pids: Vec<u16> = Vec::new();
  let mut total_bytes_by_pid = [0u64; MAX_PID];
  let mut packet_count_by_pid = [0u64; MAX_PID];
  let mut pending_pes: HashMap<u16, Vec<u8>> = HashMap::new();
  // PIDs whose PES we no longer need to reassemble. Once a stream's codec
  // has been initialized (and it's not PGS, which keeps accumulating
  // caption counts), the callback returns `SkipPid` and we skip the
  // expensive per-packet payload extend for that PID.
  let mut skip_pids = [false; MAX_PID];
  let mut total_bytes: u64 = 0;
  let mut pcr_pid: Option<u16> = None;
  let mut first_pcr_27mhz: Option<i128> = None;
  let mut last_pcr_27mhz: Option<i128> = None;

  let mut first_atc_27mhz: Option<i128> = None;
  let mut prev_atc_27mhz: Option<i128> = None;
  let mut atc_wraparound: i128 = 0;

  let mut bitrate_samples: Vec<(f64, u64)> = Vec::new();
  let mut window_start_seconds: f64 = 0.0;
  let mut window_bytes: u64 = 0;
  let mut last_progress_at = Instant::now();

  // BDInfo reads stream files in large chunks, then parses packets from
  // memory. Doing the same here avoids one buffered-read call per 192-byte
  // M2TS packet, which is a lot of avoidable overhead on 50 GB discs.
  const READ_CHUNK_SIZE: usize = 5 * 1024 * 1024;
  let mut buffer = vec![0u8; READ_CHUNK_SIZE + M2TS_PACKET_SIZE];
  let mut carry_len = 0usize;

  'outer: loop {
    let read_len = reader.read(&mut buffer[carry_len..])?;
    if read_len == 0 {
      break;
    }

    let available = carry_len + read_len;
    let mut offset = 0usize;
    while offset + M2TS_PACKET_SIZE <= available {
      let packet = &buffer[offset..offset + M2TS_PACKET_SIZE];
      offset += M2TS_PACKET_SIZE;

      total_bytes += M2TS_PACKET_SIZE as u64;

      if packet[4] != SYNC_BYTE {
        continue;
      }

      let atc = (((packet[0] as u32) & 0x3F) << 24)
        | ((packet[1] as u32) << 16)
        | ((packet[2] as u32) << 8)
        | (packet[3] as u32);
      let atc = atc as i128;
      if first_atc_27mhz.is_none() {
        first_atc_27mhz = Some(atc);
      }
      if let Some(prev) = prev_atc_27mhz {
        if atc + atc_wraparound < prev {
          atc_wraparound += 1 << 30;
        }
      }
      prev_atc_27mhz = Some(atc + atc_wraparound);

      let ts = &packet[4..4 + TS_PACKET_SIZE];
      let payload_unit_start = (ts[1] & 0x40) != 0;
      let pid: u16 = (((ts[1] as u16) & 0x1F) << 8) | (ts[2] as u16);
      let adaptation_field_control = (ts[3] >> 4) & 0x3;
      let has_adaptation = (adaptation_field_control & 0x2) != 0;
      let has_payload = (adaptation_field_control & 0x1) != 0;

      let mut payload_offset = 4usize;
      if has_adaptation {
        let af_len = ts[4] as usize;
        if af_len >= 1 {
          let flags = ts[5];
          let pcr_present = (flags & 0x10) != 0;
          if pcr_present && af_len >= 7 {
            let base = ((ts[6] as u64) << 25)
              | ((ts[7] as u64) << 17)
              | ((ts[8] as u64) << 9)
              | ((ts[9] as u64) << 1)
              | ((ts[10] as u64) >> 7);
            let ext = ((ts[10] as u64 & 0x01) << 8) | (ts[11] as u64);
            let pcr27 = base as i128 * 300 + ext as i128;
            if first_pcr_27mhz.is_none() {
              first_pcr_27mhz = Some(pcr27);
              pcr_pid = Some(pid);
            }
            last_pcr_27mhz = Some(pcr27);
          }
        }
        payload_offset += 1 + af_len;
      }
      if !has_payload || payload_offset >= TS_PACKET_SIZE {
        continue;
      }
      let payload = &ts[payload_offset..];

      let pid_index = pid as usize;
      if !pid_seen[pid_index] {
        pid_seen[pid_index] = true;
        seen_pids.push(pid);
      }
      total_bytes_by_pid[pid_index] += payload.len() as u64;
      packet_count_by_pid[pid_index] += 1;

      if pid == 0 && payload_unit_start {
        parse_pat(payload, &mut pmt_pids, &mut pmt_pid_set, &mut pmt_pid_flags);
      } else if pmt_pid_flags[pid_index] && payload_unit_start {
        parse_pmt(payload, &mut pid_to_stream_type, &mut stream_type_by_pid);
      } else if pid != 0 && !pmt_pid_flags[pid_index] && !skip_pids[pid_index] {
        // PES reassembly + dispatch
        if payload_unit_start && payload.len() >= 9 && payload[0] == 0x00 && payload[1] == 0x00 && payload[2] == 0x01 {
          // Flush previous PES for this PID, if any.
          let mut start_new_pes = true;
          if let Some(prev) = pending_pes.remove(&pid) {
            if !prev.is_empty() {
              let stream_type = *pid_to_stream_type.get(&pid).unwrap_or(&0);
              match on_pes(pid, stream_type, &prev, &pid_to_stream_type) {
                PesAction::Continue => {}
                PesAction::Stop => break 'outer,
                PesAction::SkipPid => {
                  skip_pids[pid_index] = true;
                  start_new_pes = false;
                }
              }
            }
          }
          if start_new_pes {
            let header_data_length = payload[8] as usize;
            let pes_header_size = 9usize + header_data_length;
            if payload.len() > pes_header_size {
              pending_pes
                .entry(pid)
                .or_insert_with(Vec::new)
                .extend_from_slice(&payload[pes_header_size..]);
            }
          }
        } else if let Some(buf) = pending_pes.get_mut(&pid) {
          buf.extend_from_slice(payload);
        }
      }

      if let Some(start) = first_atc_27mhz {
        let cur_seconds = ((atc + atc_wraparound) - start) as f64 / 27_000_000.0;
        window_bytes += M2TS_PACKET_SIZE as u64;
        while cur_seconds - window_start_seconds >= SAMPLE_INTERVAL_SECONDS {
          let bps = (window_bytes as f64 * 8.0 / SAMPLE_INTERVAL_SECONDS) as u64;
          bitrate_samples.push((window_start_seconds, bps));
          window_start_seconds += SAMPLE_INTERVAL_SECONDS;
          window_bytes = 0;
        }
      }
    }

    carry_len = available - offset;
    if carry_len > 0 {
      buffer.copy_within(offset..available, 0);
    }

    if last_progress_at.elapsed() >= Duration::from_secs(1) {
      on_progress(build_progress_snapshot(
        total_bytes,
        &seen_pids,
        &stream_type_by_pid,
        &total_bytes_by_pid,
        &packet_count_by_pid,
        first_pcr_27mhz,
        last_pcr_27mhz,
        first_atc_27mhz,
        prev_atc_27mhz,
      ));
      last_progress_at = Instant::now();
    }
  }

  // Flush any remaining accumulated PES so codec parsers get a final shot.
  for (pid, buf) in pending_pes.into_iter() {
    if !buf.is_empty() && !skip_pids[pid as usize] {
      let stream_type = *pid_to_stream_type.get(&pid).unwrap_or(&0);
      let _ = on_pes(pid, stream_type, &buf, &pid_to_stream_type);
    }
  }

  let mut stats: HashMap<u16, StreamStats> = HashMap::with_capacity(seen_pids.len());
  for pid in seen_pids {
    let pid_index = pid as usize;
    stats.insert(
      pid,
      StreamStats {
        pid,
        stream_type: stream_type_by_pid[pid_index],
        total_bytes: total_bytes_by_pid[pid_index],
        packet_count: packet_count_by_pid[pid_index],
        pes_sample: Vec::new(),
        pes_in_progress: Vec::new(),
        pes_started: false,
      },
    );
  }

  let duration_seconds = current_duration_seconds(first_pcr_27mhz, last_pcr_27mhz, first_atc_27mhz, prev_atc_27mhz);

  Ok(M2tsScanResult {
    bytes: total_bytes,
    duration_seconds,
    streams: stats,
    bitrate_samples,
    program_pmt_pids: pmt_pids,
    pcr_pid,
  })
}

pub fn scan_m2ts(path: &Path) -> Result<M2tsScanResult> {
  let file = File::open(path)?;
  let total_size = file.metadata()?.len();
  let mut reader = BufReader::with_capacity(1 << 20, file);

  let mut packet = [0u8; M2TS_PACKET_SIZE];
  let mut pmt_pids: Vec<u16> = Vec::new();
  let mut pmt_pid_set = std::collections::HashSet::<u16>::new();
  let mut pmt_pid_flags = [false; MAX_PID];
  let mut pid_to_stream_type: HashMap<u16, u8> = HashMap::new();
  let mut stream_type_by_pid = [0u8; MAX_PID];
  let mut stats: HashMap<u16, StreamStats> = HashMap::new();
  let mut total_bytes: u64 = 0;

  let mut pcr_pid: Option<u16> = None;
  let mut first_pcr_27mhz: Option<i128> = None;
  let mut last_pcr_27mhz: Option<i128> = None;

  // Use the 4-byte arrival timecode (BDAV/M2TS prefix) as a fallback time
  // source. It is masked to 30 bits at 27 MHz and wraps every ~40 s, so we
  // unwrap it monotonically.
  let mut first_atc_27mhz: Option<i128> = None;
  let mut prev_atc_27mhz: Option<i128> = None;
  let mut atc_wraparound: i128 = 0;

  let mut bitrate_samples: Vec<(f64, u64)> = Vec::new();
  let mut window_start_seconds: f64 = 0.0;
  let mut window_bytes: u64 = 0;

  loop {
    let mut filled = 0;
    while filled < M2TS_PACKET_SIZE {
      match reader.read(&mut packet[filled..]) {
        Ok(0) => break,
        Ok(n) => filled += n,
        Err(e) => return Err(e.into()),
      }
    }
    if filled < M2TS_PACKET_SIZE {
      break;
    }

    total_bytes += M2TS_PACKET_SIZE as u64;

    if packet[4] != SYNC_BYTE {
      // Not a synchronized packet; resync naively by scanning forward.
      // For now just skip — most well-formed M2TS files won't hit this.
      continue;
    }

    // Arrival timecode (30 bits @ 27MHz).
    let atc =
      (((packet[0] as u32) & 0x3F) << 24) | ((packet[1] as u32) << 16) | ((packet[2] as u32) << 8) | (packet[3] as u32);
    let atc = atc as i128;

    if first_atc_27mhz.is_none() {
      first_atc_27mhz = Some(atc);
    }
    if let Some(prev) = prev_atc_27mhz {
      if atc + atc_wraparound < prev {
        atc_wraparound += 1 << 30;
      }
    }
    prev_atc_27mhz = Some(atc + atc_wraparound);

    // TS header (4 bytes starting at offset 4).
    let ts = &packet[4..4 + TS_PACKET_SIZE];
    let payload_unit_start = (ts[1] & 0x40) != 0;
    let pid: u16 = (((ts[1] as u16) & 0x1F) << 8) | (ts[2] as u16);
    let adaptation_field_control = (ts[3] >> 4) & 0x3;
    let has_adaptation = (adaptation_field_control & 0x2) != 0;
    let has_payload = (adaptation_field_control & 0x1) != 0;

    let mut payload_offset = 4usize;
    if has_adaptation {
      let af_len = ts[4] as usize;
      // Adaptation field flags byte at offset 5 (if af_len >= 1).
      if af_len >= 1 {
        let flags = ts[5];
        let pcr_present = (flags & 0x10) != 0;
        if pcr_present && af_len >= 7 {
          // 33-bit base + 6 reserved + 9-bit ext at 27MHz = base * 300 + ext
          let base = ((ts[6] as u64) << 25)
            | ((ts[7] as u64) << 17)
            | ((ts[8] as u64) << 9)
            | ((ts[9] as u64) << 1)
            | ((ts[10] as u64) >> 7);
          let ext = ((ts[10] as u64 & 0x01) << 8) | (ts[11] as u64);
          let pcr27 = base as i128 * 300 + ext as i128;
          if first_pcr_27mhz.is_none() {
            first_pcr_27mhz = Some(pcr27);
            pcr_pid = Some(pid);
          }
          last_pcr_27mhz = Some(pcr27);
        }
      }
      payload_offset += 1 + af_len;
    }
    if !has_payload || payload_offset >= TS_PACKET_SIZE {
      // No payload to inspect.
      continue;
    }

    let payload = &ts[payload_offset..];

    // Per-PID counters.
    let entry = stats.entry(pid).or_insert(StreamStats {
      pid,
      stream_type: *pid_to_stream_type.get(&pid).unwrap_or(&0),
      total_bytes: 0,
      packet_count: 0,
      pes_sample: Vec::new(),
      pes_in_progress: Vec::new(),
      pes_started: false,
    });
    entry.total_bytes += payload.len() as u64;
    entry.packet_count += 1;

    // PES reassembly: only for non-PSI elementary streams (PID != 0 and
    // not in pmt_pid_set), and only when we still need a sample.
    if pid != 0 && !pmt_pid_set.contains(&pid) && entry.pes_sample.is_empty() {
      if payload_unit_start && payload.len() >= 9 && payload[0] == 0x00 && payload[1] == 0x00 && payload[2] == 0x01 {
        // Begin PES: skip past the PES header.
        let header_data_length = payload[8] as usize;
        let pes_header_size = 9usize + header_data_length;
        if payload.len() > pes_header_size {
          entry.pes_in_progress.clear();
          entry.pes_in_progress.extend_from_slice(&payload[pes_header_size..]);
          entry.pes_started = true;
        }
      } else if entry.pes_started {
        entry.pes_in_progress.extend_from_slice(payload);
      }
      if entry.pes_in_progress.len() >= 64 * 1024 {
        entry.pes_sample = std::mem::take(&mut entry.pes_in_progress);
        entry.pes_started = false;
      }
    }

    // PAT / PMT parsing.
    if pid == 0 && payload_unit_start {
      parse_pat(payload, &mut pmt_pids, &mut pmt_pid_set, &mut pmt_pid_flags);
    } else if pmt_pid_set.contains(&pid) && payload_unit_start {
      parse_pmt(payload, &mut pid_to_stream_type, &mut stream_type_by_pid);
    }

    // Bitrate samples — bucket bytes per second.
    if let Some(start) = first_atc_27mhz {
      let cur_seconds = ((atc + atc_wraparound) - start) as f64 / 27_000_000.0;
      window_bytes += M2TS_PACKET_SIZE as u64;
      while cur_seconds - window_start_seconds >= SAMPLE_INTERVAL_SECONDS {
        let bps = (window_bytes as f64 * 8.0 / SAMPLE_INTERVAL_SECONDS) as u64;
        bitrate_samples.push((window_start_seconds, bps));
        window_start_seconds += SAMPLE_INTERVAL_SECONDS;
        window_bytes = 0;
      }
    }
  }

  // Stamp the discovered stream types onto the per-PID stats. Also flush
  // any in-progress PES reassembly into the sample buffer.
  for stat in stats.values_mut() {
    if stat.stream_type == 0 {
      stat.stream_type = *pid_to_stream_type.get(&stat.pid).unwrap_or(&0);
    }
    if stat.pes_sample.is_empty() && !stat.pes_in_progress.is_empty() {
      stat.pes_sample = std::mem::take(&mut stat.pes_in_progress);
    }
  }

  let duration_seconds = match (first_pcr_27mhz, last_pcr_27mhz) {
    (Some(a), Some(b)) if b > a => (b - a) as f64 / 27_000_000.0,
    _ => match (first_atc_27mhz, prev_atc_27mhz) {
      (Some(a), Some(b)) if b > a => (b - a) as f64 / 27_000_000.0,
      _ => 0.0,
    },
  };

  if total_bytes == 0 {
    total_bytes = total_size;
  }

  Ok(M2tsScanResult {
    bytes: total_bytes,
    duration_seconds,
    streams: stats,
    bitrate_samples,
    program_pmt_pids: pmt_pids,
    pcr_pid,
  })
}

fn parse_pat(
  payload: &[u8],
  pmt_pids: &mut Vec<u16>,
  pmt_pid_set: &mut std::collections::HashSet<u16>,
  pmt_pid_flags: &mut [bool; MAX_PID],
) {
  if payload.is_empty() {
    return;
  }
  let pointer = payload[0] as usize;
  let start = 1 + pointer;
  if start + 8 > payload.len() {
    return;
  }
  let table_id = payload[start];
  if table_id != 0x00 {
    return;
  }
  let section_length = ((payload[start + 1] as usize & 0x0F) << 8) | payload[start + 2] as usize;
  let section_end = start + 3 + section_length;
  if section_end > payload.len() {
    return;
  }
  // Skip past 5-byte section header (transport_stream_id + version + section).
  let mut i = start + 8;
  let table_end = section_end.saturating_sub(4); // strip 4-byte CRC
  while i + 4 <= table_end {
    let program_number = ((payload[i] as u16) << 8) | payload[i + 1] as u16;
    let pid = (((payload[i + 2] as u16) & 0x1F) << 8) | payload[i + 3] as u16;
    i += 4;
    if program_number != 0 {
      if pmt_pid_set.insert(pid) {
        pmt_pid_flags[pid as usize] = true;
        pmt_pids.push(pid);
      }
    }
  }
}

fn build_progress_snapshot(
  bytes: u64,
  seen_pids: &[u16],
  stream_type_by_pid: &[u8; MAX_PID],
  total_bytes_by_pid: &[u64; MAX_PID],
  packet_count_by_pid: &[u64; MAX_PID],
  first_pcr_27mhz: Option<i128>,
  last_pcr_27mhz: Option<i128>,
  first_atc_27mhz: Option<i128>,
  prev_atc_27mhz: Option<i128>,
) -> M2tsScanProgress {
  let mut streams: HashMap<u16, StreamStats> = HashMap::with_capacity(seen_pids.len());
  for &pid in seen_pids {
    let pid_index = pid as usize;
    streams.insert(
      pid,
      StreamStats {
        pid,
        stream_type: stream_type_by_pid[pid_index],
        total_bytes: total_bytes_by_pid[pid_index],
        packet_count: packet_count_by_pid[pid_index],
        pes_sample: Vec::new(),
        pes_in_progress: Vec::new(),
        pes_started: false,
      },
    );
  }
  M2tsScanProgress {
    bytes,
    duration_seconds: current_duration_seconds(first_pcr_27mhz, last_pcr_27mhz, first_atc_27mhz, prev_atc_27mhz),
    streams,
  }
}

fn current_duration_seconds(
  first_pcr_27mhz: Option<i128>,
  last_pcr_27mhz: Option<i128>,
  first_atc_27mhz: Option<i128>,
  prev_atc_27mhz: Option<i128>,
) -> f64 {
  match (first_pcr_27mhz, last_pcr_27mhz) {
    (Some(a), Some(b)) if b > a => (b - a) as f64 / 27_000_000.0,
    _ => match (first_atc_27mhz, prev_atc_27mhz) {
      (Some(a), Some(b)) if b > a => (b - a) as f64 / 27_000_000.0,
      _ => 0.0,
    },
  }
}

fn parse_pmt(payload: &[u8], pid_to_stream_type: &mut HashMap<u16, u8>, stream_type_by_pid: &mut [u8; MAX_PID]) {
  if payload.is_empty() {
    return;
  }
  let pointer = payload[0] as usize;
  let start = 1 + pointer;
  if start + 12 > payload.len() {
    return;
  }
  let table_id = payload[start];
  if table_id != 0x02 {
    return;
  }
  let section_length = ((payload[start + 1] as usize & 0x0F) << 8) | payload[start + 2] as usize;
  let section_end = start + 3 + section_length;
  if section_end > payload.len() {
    return;
  }
  let program_info_length = ((payload[start + 10] as usize & 0x0F) << 8) | payload[start + 11] as usize;
  let mut i = start + 12 + program_info_length;
  let table_end = section_end.saturating_sub(4);
  while i + 5 <= table_end {
    let stream_type = payload[i];
    let elem_pid = (((payload[i + 1] as u16) & 0x1F) << 8) | payload[i + 2] as u16;
    let es_info_length = ((payload[i + 3] as usize & 0x0F) << 8) | payload[i + 4] as usize;
    pid_to_stream_type.insert(elem_pid, stream_type);
    stream_type_by_pid[elem_pid as usize] = stream_type;
    i += 5 + es_info_length;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Build a 188-byte TS packet (payload-only, cc=0), padded with 0xFF.
  fn ts_packet(pusi: bool, pid: u16, payload: &[u8]) -> Vec<u8> {
    let mut ts = vec![0xFFu8; TS_PACKET_SIZE];
    ts[0] = SYNC_BYTE;
    ts[1] = ((pid >> 8) as u8 & 0x1F) | if pusi { 0x40 } else { 0 };
    ts[2] = (pid & 0xFF) as u8;
    ts[3] = 0x10; // adaptation_field_control = payload only
    let n = payload.len().min(TS_PACKET_SIZE - 4);
    ts[4..4 + n].copy_from_slice(&payload[..n]);
    ts
  }

  /// Wrap a TS packet in the 192-byte M2TS frame (4-byte ATC prefix).
  fn m2ts(ts: &[u8]) -> Vec<u8> {
    let mut p = vec![0u8; 4];
    p.extend_from_slice(ts);
    p
  }

  /// Wrap a TS packet in the 192-byte M2TS frame with an explicit 30-bit
  /// arrival timecode (used to drive duration / bitrate-window logic).
  fn m2ts_atc(ts: &[u8], atc: u32) -> Vec<u8> {
    let mut p = vec![
      ((atc >> 24) as u8) & 0x3F,
      (atc >> 16) as u8,
      (atc >> 8) as u8,
      atc as u8,
    ];
    p.extend_from_slice(ts);
    p
  }

  /// Build a 188-byte TS packet that carries a PCR in its adaptation field.
  /// `with_payload` controls whether the adaptation_field_control also
  /// announces a payload (0x30) or is adaptation-only (0x20). The optional
  /// `payload` bytes are appended after the adaptation field.
  fn ts_packet_pcr(pid: u16, pcr_base: u64, pcr_ext: u16, with_payload: bool, payload: &[u8]) -> Vec<u8> {
    ts_packet_pcr_pusi(false, pid, pcr_base, pcr_ext, with_payload, payload)
  }

  fn ts_packet_pcr_pusi(
    pusi: bool,
    pid: u16,
    pcr_base: u64,
    pcr_ext: u16,
    with_payload: bool,
    payload: &[u8],
  ) -> Vec<u8> {
    let mut ts = vec![0xFFu8; TS_PACKET_SIZE];
    ts[0] = SYNC_BYTE;
    ts[1] = ((pid >> 8) as u8 & 0x1F) | if pusi { 0x40 } else { 0 };
    ts[2] = (pid & 0xFF) as u8;
    // adaptation_field_control: 0x20 = adaptation only, 0x30 = adaptation + payload.
    ts[3] = if with_payload { 0x30 } else { 0x20 };
    // Adaptation field length covers the flags byte + 6 PCR bytes = 7.
    let af_len = 7usize;
    ts[4] = af_len as u8;
    ts[5] = 0x10; // PCR_flag set
    ts[6] = (pcr_base >> 25) as u8;
    ts[7] = (pcr_base >> 17) as u8;
    ts[8] = (pcr_base >> 9) as u8;
    ts[9] = (pcr_base >> 1) as u8;
    ts[10] = (((pcr_base & 0x1) as u8) << 7) | ((pcr_ext >> 8) as u8 & 0x01);
    ts[11] = (pcr_ext & 0xFF) as u8;
    if with_payload {
      // Payload begins after the 4-byte TS header + 1 length byte + af_len.
      let payload_offset = 5 + af_len;
      let n = payload.len().min(TS_PACKET_SIZE - payload_offset);
      ts[payload_offset..payload_offset + n].copy_from_slice(&payload[..n]);
    }
    ts
  }

  /// Build a 188-byte TS packet with an empty adaptation field (af_len = 0)
  /// followed by a payload. Exercises the `af_len < 1` branch.
  fn ts_packet_empty_af(pusi: bool, pid: u16, payload: &[u8]) -> Vec<u8> {
    let mut ts = vec![0xFFu8; TS_PACKET_SIZE];
    ts[0] = SYNC_BYTE;
    ts[1] = ((pid >> 8) as u8 & 0x1F) | if pusi { 0x40 } else { 0 };
    ts[2] = (pid & 0xFF) as u8;
    ts[3] = 0x30; // adaptation + payload
    ts[4] = 0x00; // af_len = 0
    let payload_offset = 5usize;
    let n = payload.len().min(TS_PACKET_SIZE - payload_offset);
    ts[payload_offset..payload_offset + n].copy_from_slice(&payload[..n]);
    ts
  }

  /// A PES header carrying `header_data_length` filler bytes after the 9-byte
  /// fixed header, so the scanner has to skip them before the ES payload.
  fn pes_payload_with_header(header_data_length: u8, es: &[u8]) -> Vec<u8> {
    let mut v = vec![
      0x00,
      0x00,
      0x01, // PES start code
      0xE0, // stream id (video)
      0x00,
      0x00, // PES packet length
      0x80,
      0x00,               // flags
      header_data_length, // header data length
    ];
    v.extend(std::iter::repeat(0x55).take(header_data_length as usize));
    v.extend_from_slice(es);
    v
  }

  /// RAII guard that removes a temp file on drop.
  struct TempFile {
    path: std::path::PathBuf,
  }
  impl TempFile {
    fn new(name: &str, bytes: &[u8]) -> Self {
      let mut path = std::env::temp_dir();
      let unique = format!(
        "{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
          .duration_since(std::time::UNIX_EPOCH)
          .unwrap()
          .as_nanos()
      );
      path.push(unique);
      std::fs::write(&path, bytes).expect("write temp file");
      TempFile { path }
    }
  }
  impl Drop for TempFile {
    fn drop(&mut self) {
      let _ = std::fs::remove_file(&self.path);
    }
  }

  fn pat_payload(program: u16, pmt_pid: u16) -> Vec<u8> {
    vec![
      0x00, // pointer
      0x00, // table_id (PAT)
      0xB0,
      0x0D, // section_length = 13
      0x00,
      0x01, // transport_stream_id
      0x01, // version / current_next
      0x00, // section_number
      0x00, // last_section_number
      (program >> 8) as u8,
      (program & 0xFF) as u8,
      0xE0 | (pmt_pid >> 8) as u8,
      (pmt_pid & 0xFF) as u8,
      0x00,
      0x00,
      0x00,
      0x00, // CRC (not validated)
    ]
  }

  fn pmt_payload() -> Vec<u8> {
    vec![
      0x00, // pointer
      0x02, // table_id (PMT)
      0xB0, 0x17, // section_length = 23
      0x00, 0x01, // program_number
      0x01, 0x00, 0x00, // version / section / last
      0xE0, 0x00, // PCR PID
      0xF0, 0x00, // program_info_length = 0
      // ES1: AVC (0x1b) on PID 0x1011, es_info_length 0
      0x1b, 0xF0, 0x11, 0xF0, 0x00, // ES2: AC3 (0x81) on PID 0x1100, es_info_length 0
      0x81, 0xF1, 0x00, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, // CRC
    ]
  }

  fn pes_payload(es: &[u8]) -> Vec<u8> {
    let mut v = vec![
      0x00, 0x00, 0x01, // PES start code
      0xE0, // stream id (video)
      0x00, 0x00, // PES packet length
      0x80, 0x00, // flags
      0x00, // header data length = 0
    ];
    v.extend_from_slice(es);
    v
  }

  #[test]
  fn discovers_pmt_pids_and_stream_types_and_dispatches_pes() {
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x1011, &pes_payload(&[0xAA, 0xBB]))));

    let mut pes_calls: Vec<(u16, u8, Vec<u8>, bool)> = Vec::new();
    let result = scan_m2ts_streaming_from_reader(data.as_slice(), |pid, st, payload, pmt| {
      let has_ac3 = pmt.get(&0x1100) == Some(&0x81);
      pes_calls.push((pid, st, payload.to_vec(), has_ac3));
      PesAction::Continue
    })
    .expect("scan succeeds");

    // PAT discovered the PMT PID.
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
    // PMT mapped the AVC elementary stream PID to its type.
    let avc = result.streams.get(&0x1011).expect("AVC PID seen");
    assert_eq!(avc.stream_type, 0x1b);

    // The PES on PID 0x1011 was dispatched (flushed at end of stream), and
    // the PMT table passed to the callback knew about the AC3 PID.
    let call = pes_calls
      .iter()
      .find(|(pid, _, _, _)| *pid == 0x1011)
      .expect("PES dispatched for AVC PID");
    assert_eq!(call.1, 0x1b);
    assert_eq!(&call.2[..2], &[0xAA, 0xBB]);
    assert!(call.3, "PMT map should carry the AC3 PID");
  }

  #[test]
  fn empty_input_yields_no_streams() {
    let data: Vec<u8> = Vec::new();
    let result = scan_m2ts_from_reader(data.as_slice()).expect("scan succeeds");
    assert_eq!(result.bytes, 0);
    assert!(result.streams.is_empty());
    assert!(result.program_pmt_pids.is_empty());
  }

  #[test]
  fn packets_without_sync_byte_are_skipped() {
    // A 192-byte frame whose TS sync byte is wrong must be ignored, not panic.
    let mut bad = vec![0u8; M2TS_PACKET_SIZE];
    bad[4] = 0x00; // not 0x47
    let result = scan_m2ts_from_reader(bad.as_slice()).expect("scan succeeds");
    assert!(result.streams.is_empty());
  }

  #[test]
  fn truncated_final_frame_is_ignored() {
    // A complete packet followed by a partial (< 192 byte) frame: the
    // partial bytes are carried but never form a packet.
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&[0u8; 50]); // short trailing frame
    let result = scan_m2ts_from_reader(data.as_slice()).expect("scan succeeds");
    // Only the one complete 192-byte frame was counted.
    assert_eq!(result.bytes, M2TS_PACKET_SIZE as u64);
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
  }

  #[test]
  fn pcr_in_adaptation_field_sets_pcr_pid_and_duration() {
    // Two PCR samples one second apart (27 MHz) on the same PID. The PCR
    // PID should be recorded and the duration derived from the delta.
    let pid = 0x1011u16;
    let base0 = 1_000u64;
    // 1 second at 90 kHz = 90_000 ticks on the base clock.
    let base1 = base0 + 90_000;
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet_pcr(pid, base0, 0, false, &[])));
    data.extend_from_slice(&m2ts(&ts_packet_pcr(pid, base1, 0, false, &[])));
    let result = scan_m2ts_from_reader(data.as_slice()).expect("scan succeeds");
    assert_eq!(result.pcr_pid, Some(pid));
    assert!(
      (result.duration_seconds - 1.0).abs() < 1e-3,
      "expected ~1s, got {}",
      result.duration_seconds
    );
  }

  #[test]
  fn adaptation_plus_payload_carries_pes_after_adaptation_field() {
    // adaptation_field_control = 0x30: adaptation field (with PCR) AND a
    // PES payload in the same packet.
    let pid = 0x1011u16;
    let pes = pes_payload(&[0xDE, 0xAD, 0xBE, 0xEF]);
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts(&ts_packet_pcr_pusi(true, pid, 5_000, 100, true, &pes)));

    let mut dispatched: Vec<u8> = Vec::new();
    let result = scan_m2ts_streaming_from_reader(data.as_slice(), |p, _st, payload, _pmt| {
      if p == pid {
        dispatched = payload.to_vec();
      }
      PesAction::Continue
    })
    .expect("scan succeeds");
    assert_eq!(result.pcr_pid, Some(pid));
    // ts_packet pads with 0xFF, so the PES payload begins with the ES bytes.
    assert_eq!(&dispatched[..4], &[0xDE, 0xAD, 0xBE, 0xEF]);
  }

  #[test]
  fn empty_adaptation_field_then_payload() {
    // af_len = 0 packet still has a payload (the PAT here). Hits the
    // `af_len < 1` branch where no PCR is read.
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet_empty_af(true, 0x0000, &pat_payload(1, 0x0100))));
    let result = scan_m2ts_from_reader(data.as_slice()).expect("scan succeeds");
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
    assert!(result.pcr_pid.is_none());
  }

  #[test]
  fn pes_spans_multiple_ts_packets_via_continuation() {
    // A PES that starts in one packet (PUSI set) and continues in the next
    // (PUSI clear) must be reassembled across both.
    let pid = 0x1011u16;
    let pes = pes_payload(&[0x01, 0x02, 0x03]);
    // A distinctive continuation payload (no PES start code, PUSI clear).
    let continuation = vec![0x04u8; 184];
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes)));
    // Continuation packet: PUSI clear.
    data.extend_from_slice(&m2ts(&ts_packet(false, pid, &continuation)));

    let mut dispatched: Vec<u8> = Vec::new();
    scan_m2ts_streaming_from_reader(data.as_slice(), |p, _st, payload, _pmt| {
      if p == pid {
        dispatched = payload.to_vec();
      }
      PesAction::Continue
    })
    .expect("scan succeeds");
    // First three ES bytes from the PUSI packet (ts_packet pads the rest
    // with 0xFF), then the continuation bytes were appended, so the total
    // length exceeds a single TS payload and the 0x04 run is present.
    assert_eq!(&dispatched[..3], &[0x01, 0x02, 0x03]);
    assert!(dispatched.len() > TS_PACKET_SIZE, "continuation appended bytes");
    assert!(dispatched.windows(4).any(|w| w == [0x04, 0x04, 0x04, 0x04]));
  }

  #[test]
  fn second_pes_flushes_first_and_callback_can_stop() {
    // Two PES on the same PID: the second PUSI flushes the first, which is
    // dispatched. Returning Stop must abort the scan immediately.
    let pid = 0x1011u16;
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes_payload(&[0x11]))));
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes_payload(&[0x22]))));
    // This packet must never be reached because the callback stops first.
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes_payload(&[0x33]))));

    let mut payloads: Vec<Vec<u8>> = Vec::new();
    scan_m2ts_streaming_from_reader(data.as_slice(), |p, _st, payload, _pmt| {
      if p == pid {
        payloads.push(payload.to_vec());
      }
      PesAction::Stop
    })
    .expect("scan succeeds");
    // Only the first PES got flushed before Stop aborted the loop. The
    // ts_packet payload is padded with 0xFF, so check the leading ES byte.
    assert_eq!(payloads.len(), 1);
    assert_eq!(payloads[0][0], 0x11);
  }

  #[test]
  fn skip_pid_stops_reassembly_but_keeps_byte_accounting() {
    // SkipPid: after the first PES dispatch the PID is skipped, so later
    // PES are not dispatched, but per-PID byte accounting continues.
    let pid = 0x1011u16;
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    // First PES, then a second PUSI which flushes the first and triggers
    // SkipPid, then more packets that must still be byte-counted.
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes_payload(&[0xA0]))));
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes_payload(&[0xB0]))));
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes_payload(&[0xC0]))));

    let mut count = 0u32;
    let result = scan_m2ts_streaming_from_reader(data.as_slice(), |p, _st, _payload, _pmt| {
      if p == pid {
        count += 1;
        PesAction::SkipPid
      } else {
        PesAction::Continue
      }
    })
    .expect("scan succeeds");
    // Exactly one dispatch happened (the flush of the first PES); after
    // SkipPid no further dispatch occurs.
    assert_eq!(count, 1);
    let stats = result.streams.get(&pid).expect("PID seen");
    // All three PES packets were byte-counted regardless of skipping.
    assert_eq!(stats.packet_count, 3);
    assert!(stats.total_bytes > 0);
  }

  #[test]
  fn bitrate_samples_roll_with_arrival_timecode() {
    // Feed packets across more than one second of arrival time so the
    // one-second bitrate window rolls and emits samples.
    let mut data = Vec::new();
    let ticks_per_sec = 27_000_000u32;
    // 0s, 0.5s, 1.0s, 1.5s, 2.1s
    for (i, frac) in [0.0f64, 0.5, 1.0, 1.5, 2.1].iter().enumerate() {
      let atc = (frac * ticks_per_sec as f64) as u32;
      let pkt = ts_packet(true, 0x1011, &pes_payload(&[i as u8]));
      data.extend_from_slice(&m2ts_atc(&pkt, atc));
    }
    let result = scan_m2ts_from_reader(data.as_slice()).expect("scan succeeds");
    // Crossing 1s and 2s boundaries yields at least two rolled windows.
    assert!(
      result.bitrate_samples.len() >= 2,
      "expected rolled windows, got {:?}",
      result.bitrate_samples
    );
    assert!(result.duration_seconds >= 2.0);
  }

  #[test]
  fn pes_header_data_length_bytes_are_skipped() {
    // A PES with a non-zero PES_header_data_length must have those filler
    // bytes skipped before the ES payload reaches the callback.
    let pid = 0x1011u16;
    let pes = pes_payload_with_header(5, &[0xCA, 0xFE]);
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes)));

    let mut dispatched: Vec<u8> = Vec::new();
    scan_m2ts_streaming_from_reader(data.as_slice(), |p, _st, payload, _pmt| {
      if p == pid {
        dispatched = payload.to_vec();
      }
      PesAction::Continue
    })
    .expect("scan succeeds");
    // The 5 header filler bytes (0x55) are gone; only the ES payload remains.
    assert_eq!(&dispatched[..2], &[0xCA, 0xFE]);
    assert!(!dispatched.contains(&0x55));
  }

  #[test]
  fn multiple_elementary_streams_video_audio_and_pgs() {
    // A PMT with video + audio + PGS, each on its own PID, must map all
    // three stream types and dispatch PES for each elementary PID.
    let pmt = vec![
      0x00, // pointer
      0x02, // table_id (PMT)
      0xB0, 0x1C, // section_length = 28
      0x00, 0x01, // program_number
      0x01, 0x00, 0x00, // version / section / last
      0xE0, 0x00, // PCR PID
      0xF0, 0x00, // program_info_length = 0
      // video AVC (0x1b) PID 0x1011
      0x1b, 0xF0, 0x11, 0xF0, 0x00, // audio AC3 (0x81) PID 0x1100
      0x81, 0xF1, 0x00, 0xF0, 0x00, // PGS subtitle (0x90) PID 0x1200
      0x90, 0xF2, 0x00, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, // CRC
    ];
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt)));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x1011, &pes_payload(&[0x10]))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x1100, &pes_payload(&[0x20]))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x1200, &pes_payload(&[0x30]))));

    let mut seen: HashMap<u16, u8> = HashMap::new();
    let result = scan_m2ts_streaming_from_reader(data.as_slice(), |p, st, _payload, _pmt| {
      seen.insert(p, st);
      PesAction::Continue
    })
    .expect("scan succeeds");

    assert_eq!(result.streams.get(&0x1011).unwrap().stream_type, 0x1b);
    assert_eq!(result.streams.get(&0x1100).unwrap().stream_type, 0x81);
    assert_eq!(result.streams.get(&0x1200).unwrap().stream_type, 0x90);
    // PES dispatched for all three elementary PIDs.
    assert_eq!(seen.get(&0x1011), Some(&0x1b));
    assert_eq!(seen.get(&0x1100), Some(&0x81));
    assert_eq!(seen.get(&0x1200), Some(&0x90));
  }

  #[test]
  fn streaming_with_progress_callback_builds_snapshot() {
    // The with-progress entry point wires a progress callback; even if the
    // 1-second throttle never fires, the path and snapshot helpers compile
    // and run, and the final result is correct.
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x1011, &pes_payload(&[0x77]))));

    let mut progress_snapshots = 0u32;
    let result = scan_m2ts_streaming_from_reader_with_progress(
      data.as_slice(),
      |_p, _st, _payload, _pmt| PesAction::Continue,
      |_progress| progress_snapshots += 1,
    )
    .expect("scan succeeds");
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
    // The snapshot builder is also exercised directly to assert its shape.
    let mut stbp = [0u8; MAX_PID];
    stbp[0x1011] = 0x1b;
    let mut tbbp = [0u64; MAX_PID];
    tbbp[0x1011] = 123;
    let mut pcbp = [0u64; MAX_PID];
    pcbp[0x1011] = 4;
    let snap = build_progress_snapshot(
      456,
      &[0x1011u16],
      &stbp,
      &tbbp,
      &pcbp,
      Some(0),
      Some(27_000_000),
      None,
      None,
    );
    assert_eq!(snap.bytes, 456);
    assert!((snap.duration_seconds - 1.0).abs() < 1e-9);
    let s = snap.streams.get(&0x1011).unwrap();
    assert_eq!(s.stream_type, 0x1b);
    assert_eq!(s.total_bytes, 123);
    assert_eq!(s.packet_count, 4);
    // Snapshots may or may not have fired depending on timing; the counter
    // is only referenced so the closure has an observable effect.
    let _ = progress_snapshots;
  }

  #[test]
  fn streaming_from_path_reads_a_temp_file() {
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x1011, &pes_payload(&[0x9A]))));
    let file = TempFile::new("bdmaster-m2ts-streaming", &data);

    let mut dispatched = 0u32;
    let result = scan_m2ts_streaming(&file.path, |_p, _st, _payload, _pmt| {
      dispatched += 1;
      PesAction::Continue
    })
    .expect("scan succeeds");
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
    assert!(dispatched >= 1);
  }

  #[test]
  fn scan_m2ts_path_variant_full_pipeline() {
    // Exercises the separate path-based scan_m2ts function end to end:
    // PAT/PMT discovery, PCR-driven duration, PES sample capture, and
    // bitrate windows from the arrival timecode.
    let pid = 0x1011u16;
    let ticks_per_sec = 27_000_000u32;
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    // First PCR sample (adaptation + payload so it also reaches the
    // bitrate window) at t=0; PUSI begins a PES for sample capture.
    data.extend_from_slice(&m2ts_atc(
      &ts_packet_pcr_pusi(true, pid, 1_000, 0, true, &pes_payload(&[0xEE, 0xFF])),
      0,
    ));
    // A payload-bearing packet at t=0.5s keeps the window fed.
    data.extend_from_slice(&m2ts_atc(&ts_packet(false, pid, &[0x00u8; 184]), ticks_per_sec / 2));
    // Second PCR sample 1s after the first (for duration) on a packet that
    // also carries a payload so it advances the bitrate window past 1s.
    data.extend_from_slice(&m2ts_atc(
      &ts_packet_pcr(pid, 1_000 + 90_000, 0, true, &[0x00u8; 100]),
      ticks_per_sec + ticks_per_sec / 2,
    ));

    let file = TempFile::new("bdmaster-m2ts-path", &data);
    let result = scan_m2ts(&file.path).expect("scan succeeds");

    assert_eq!(result.program_pmt_pids, vec![0x0100]);
    assert_eq!(result.pcr_pid, Some(pid));
    assert!(
      (result.duration_seconds - 1.0).abs() < 1e-3,
      "expected ~1s, got {}",
      result.duration_seconds
    );
    let stats = result.streams.get(&pid).expect("PID seen");
    assert_eq!(stats.stream_type, 0x1b);
    // The flushed PES sample carries the ES payload.
    assert_eq!(&stats.pes_sample[..2], &[0xEE, 0xFF]);
    assert!(!result.bitrate_samples.is_empty());
  }

  #[test]
  fn scan_m2ts_path_continuation_and_64k_sample_flush() {
    // Path variant: a PES spanning many continuation packets accumulates
    // past the 64KB threshold so pes_sample is taken mid-stream and the
    // `pes_started` reset branch runs.
    let pid = 0x1011u16;
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    // Start a PES with a small ES chunk.
    data.extend_from_slice(&m2ts(&ts_packet(true, pid, &pes_payload(&[0x01; 100]))));
    // ~64KB / ~184 bytes per packet => well over 360 continuation packets.
    for _ in 0..400 {
      data.extend_from_slice(&m2ts(&ts_packet(false, pid, &[0x02u8; 184])));
    }
    let file = TempFile::new("bdmaster-m2ts-64k", &data);
    let result = scan_m2ts(&file.path).expect("scan succeeds");
    let stats = result.streams.get(&pid).expect("PID seen");
    assert!(
      stats.pes_sample.len() >= 64 * 1024,
      "expected >=64KB sample, got {}",
      stats.pes_sample.len()
    );
  }

  #[test]
  fn scan_m2ts_path_empty_file_falls_back_to_metadata_size() {
    // An empty file: total_bytes stays 0, so scan_m2ts substitutes the
    // file's metadata length (also 0 here) and returns no streams.
    let file = TempFile::new("bdmaster-m2ts-empty", &[]);
    let result = scan_m2ts(&file.path).expect("scan succeeds");
    assert_eq!(result.bytes, 0);
    assert!(result.streams.is_empty());
    assert!(result.program_pmt_pids.is_empty());
    assert_eq!(result.duration_seconds, 0.0);
  }

  #[test]
  fn scan_m2ts_path_short_trailing_frame_is_dropped() {
    // Path variant: a full packet then a partial frame. The partial frame
    // is read but `filled < M2TS_PACKET_SIZE`, so the loop breaks.
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&[0u8; 30]);
    let file = TempFile::new("bdmaster-m2ts-short", &data);
    let result = scan_m2ts(&file.path).expect("scan succeeds");
    assert_eq!(result.bytes, M2TS_PACKET_SIZE as u64);
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
  }

  #[test]
  fn scan_m2ts_path_skips_non_sync_packet() {
    // Path variant resync branch: a frame with the wrong sync byte is
    // counted but skipped, then a good PAT frame still parses.
    let mut bad = vec![0u8; M2TS_PACKET_SIZE];
    bad[4] = 0x00; // not the sync byte
    let mut data = Vec::new();
    data.extend_from_slice(&bad);
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    let file = TempFile::new("bdmaster-m2ts-nosync", &data);
    let result = scan_m2ts(&file.path).expect("scan succeeds");
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
    assert_eq!(result.bytes, 2 * M2TS_PACKET_SIZE as u64);
  }

  /// A reader that sleeps before serving its bytes so that, by the time the
  /// scanner finishes processing that first chunk, the 1-second progress
  /// throttle has elapsed and the progress snapshot is emitted.
  struct SlowReader {
    data: Vec<u8>,
    served: bool,
  }
  impl Read for SlowReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
      if self.served {
        return Ok(0);
      }
      // Sleep so `last_progress_at.elapsed()` exceeds one second once this
      // chunk is processed, then hand over all the bytes at once.
      std::thread::sleep(Duration::from_millis(1100));
      let n = self.data.len().min(buf.len());
      buf[..n].copy_from_slice(&self.data[..n]);
      self.served = true;
      Ok(n)
    }
  }

  #[test]
  fn slow_reader_triggers_progress_snapshot() {
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x1011, &pes_payload(&[0x42]))));
    let reader = SlowReader { data, served: false };

    let mut progress_count = 0u32;
    let result = scan_m2ts_streaming_from_reader_with_progress(
      reader,
      |_p, _st, _payload, _pmt| PesAction::Continue,
      |progress| {
        progress_count += 1;
        // The snapshot carries cumulative bytes (zero on the first,
        // pre-data tick) and a stream table.
        let _ = progress.bytes;
        let _ = progress.duration_seconds;
      },
    )
    .expect("scan succeeds");
    assert!(progress_count >= 1, "progress callback should have fired");
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
  }

  #[test]
  fn slow_reader_drives_no_progress_entry_point() {
    // Same slow reader, but through scan_m2ts_streaming_from_reader, whose
    // internal `|_| {}` progress closure must be exercised when the 1s
    // throttle fires.
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0100, &pmt_payload())));
    let reader = SlowReader { data, served: false };
    let result =
      scan_m2ts_streaming_from_reader(reader, |_p, _st, _payload, _pmt| PesAction::Continue).expect("scan succeeds");
    assert_eq!(result.program_pmt_pids, vec![0x0100]);
  }

  /// A reader that returns an I/O error so the scanner propagates it. Used to
  /// cover the `Err(e) => return Err(e.into())` arm in scan_m2ts_from_reader.
  struct ErrReader;
  impl Read for ErrReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
      Err(std::io::Error::new(std::io::ErrorKind::Other, "boom"))
    }
  }

  #[test]
  fn reader_io_error_is_propagated() {
    let err = scan_m2ts_from_reader(ErrReader);
    assert!(err.is_err(), "I/O error should propagate");
  }

  #[test]
  fn scan_m2ts_path_adaptation_only_packet_has_no_payload() {
    // Path variant: an adaptation-only packet (control 0x20, no payload)
    // hits the `!has_payload` continue branch.
    let pid = 0x1011u16;
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    // Adaptation-only PCR packet: no payload byte to inspect.
    data.extend_from_slice(&m2ts(&ts_packet_pcr(pid, 1_000, 0, false, &[])));
    let file = TempFile::new("bdmaster-m2ts-afonly", &data);
    let result = scan_m2ts(&file.path).expect("scan succeeds");
    assert_eq!(result.pcr_pid, Some(pid));
    // The adaptation-only packet contributed no per-PID payload bytes.
    assert!(result.streams.get(&pid).is_none());
  }

  #[test]
  fn scan_m2ts_path_unwraps_atc_wraparound() {
    // Path variant ATC wraparound: a high then low arrival timecode.
    let high = (1u32 << 30) - 1;
    let low = 5u32;
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts_atc(&ts_packet(true, 0x1011, &pes_payload(&[0x01])), high));
    data.extend_from_slice(&m2ts_atc(&ts_packet(true, 0x1011, &pes_payload(&[0x02])), low));
    let file = TempFile::new("bdmaster-m2ts-wrap", &data);
    let result = scan_m2ts(&file.path).expect("scan succeeds");
    assert!(result.duration_seconds > 0.0);
  }

  #[test]
  fn parse_pat_rejects_malformed_sections() {
    let mut pmt_pids = Vec::new();
    let mut set = std::collections::HashSet::new();
    let mut flags = [false; MAX_PID];

    // Empty payload.
    parse_pat(&[], &mut pmt_pids, &mut set, &mut flags);
    assert!(pmt_pids.is_empty());

    // Wrong table_id (not 0x00).
    let mut wrong = pat_payload(1, 0x0100);
    wrong[1] = 0x42;
    parse_pat(&wrong, &mut pmt_pids, &mut set, &mut flags);
    assert!(pmt_pids.is_empty());

    // Section header claims more bytes than the payload holds.
    let mut truncated = pat_payload(1, 0x0100);
    truncated[3] = 0x7F; // huge section_length
    parse_pat(&truncated, &mut pmt_pids, &mut set, &mut flags);
    assert!(pmt_pids.is_empty());

    // program_number == 0 entries (the NIT) are ignored.
    let nit = vec![
      0x00, 0x00, 0xB0, 0x0D, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, // program_number = 0
      0xE0, 0x10, // pid
      0x00, 0x00, 0x00, 0x00,
    ];
    parse_pat(&nit, &mut pmt_pids, &mut set, &mut flags);
    assert!(pmt_pids.is_empty());

    // A too-short payload (header start beyond available bytes).
    parse_pat(&[0x05, 0x00], &mut pmt_pids, &mut set, &mut flags);
    assert!(pmt_pids.is_empty());
  }

  #[test]
  fn parse_pmt_rejects_malformed_sections() {
    let mut map = HashMap::new();
    let mut by_pid = [0u8; MAX_PID];

    // Empty payload.
    parse_pmt(&[], &mut map, &mut by_pid);
    assert!(map.is_empty());

    // Wrong table_id.
    let mut wrong = pmt_payload();
    wrong[1] = 0x10;
    parse_pmt(&wrong, &mut map, &mut by_pid);
    assert!(map.is_empty());

    // Section claims more bytes than present.
    let mut truncated = pmt_payload();
    truncated[3] = 0x7F;
    parse_pmt(&truncated, &mut map, &mut by_pid);
    assert!(map.is_empty());

    // Too short to even hold the 12-byte fixed header.
    parse_pmt(&[0x00, 0x02, 0xB0], &mut map, &mut by_pid);
    assert!(map.is_empty());
  }

  #[test]
  fn pmt_with_program_info_length_skips_descriptors() {
    // A PMT whose program_info_length is non-zero must skip those
    // descriptor bytes before reading the ES loop.
    let pmt = vec![
      0x00, // pointer
      0x02, // table_id
      0xB0, 0x15, // section_length = 21
      0x00, 0x01, // program_number
      0x01, 0x00, 0x00, // version / section / last
      0xE0, 0x00, // PCR PID
      0xF0, 0x03, // program_info_length = 3
      0xAA, 0xBB, 0xCC, // 3 descriptor bytes
      // ES: AVC PID 0x1011
      0x1b, 0xF0, 0x11, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, // CRC
    ];
    let mut map = HashMap::new();
    let mut by_pid = [0u8; MAX_PID];
    parse_pmt(&pmt, &mut map, &mut by_pid);
    assert_eq!(map.get(&0x1011), Some(&0x1b));
  }

  #[test]
  fn atc_wraparound_is_unwrapped_monotonically() {
    // Two packets where the second ATC is smaller than the first (a 30-bit
    // wraparound). The unwrap keeps the duration positive.
    let high = (1u32 << 30) - 1; // near the 30-bit max
    let low = 10u32; // wrapped value
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts_atc(&ts_packet(true, 0x1011, &pes_payload(&[0x01])), high));
    data.extend_from_slice(&m2ts_atc(&ts_packet(true, 0x1011, &pes_payload(&[0x02])), low));
    let result = scan_m2ts_from_reader(data.as_slice()).expect("scan succeeds");
    // After unwrap the second timecode is greater than the first.
    assert!(result.duration_seconds > 0.0);
  }

  #[test]
  fn pcr_present_but_adaptation_field_too_short_is_ignored() {
    // PCR_flag set but af_len < 7: the PCR must not be read.
    let pid = 0x1011u16;
    let mut ts = vec![0xFFu8; TS_PACKET_SIZE];
    ts[0] = SYNC_BYTE;
    ts[1] = (pid >> 8) as u8 & 0x1F;
    ts[2] = (pid & 0xFF) as u8;
    ts[3] = 0x20; // adaptation only
    ts[4] = 0x01; // af_len = 1 (only the flags byte fits)
    ts[5] = 0x10; // PCR_flag set, but no room for PCR bytes
    let data = m2ts(&ts);
    let result = scan_m2ts_from_reader(data.as_slice()).expect("scan succeeds");
    assert!(result.pcr_pid.is_none());
  }
}
