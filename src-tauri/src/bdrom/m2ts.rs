/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
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
    scan_inner(reader, |pid, st, payload, pmt| on_pes(pid, st, payload, pmt))
}

pub fn scan_m2ts_from_reader<R: Read>(reader: R) -> Result<M2tsScanResult> {
    scan_inner(reader, |_, _, _, _| PesAction::Continue)
}

const TS_PACKET_SIZE: usize = 188;
const M2TS_PACKET_SIZE: usize = 192;
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
    scan_inner(reader, on_pes)
}

fn scan_inner<R, F>(reader: R, mut on_pes: F) -> Result<M2tsScanResult>
where
    R: Read,
    F: FnMut(u16, u8, &[u8], &HashMap<u16, u8>) -> PesAction,
{
    let mut reader = reader;
    let mut packet = [0u8; M2TS_PACKET_SIZE];
    let mut pmt_pid_set = std::collections::HashSet::<u16>::new();
    let mut pmt_pids: Vec<u16> = Vec::new();
    let mut pid_to_stream_type: HashMap<u16, u8> = HashMap::new();
    let mut stats: HashMap<u16, StreamStats> = HashMap::new();
    let mut pending_pes: HashMap<u16, Vec<u8>> = HashMap::new();
    // PIDs whose PES we no longer need to reassemble. Once a stream's codec
    // has been initialized (and it's not PGS, which keeps accumulating
    // caption counts), the callback returns `SkipPid` and we skip the
    // expensive per-packet payload extend for that PID.
    let mut skip_pids = std::collections::HashSet::<u16>::new();
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

    'outer: loop {
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

        let st_byte = *pid_to_stream_type.get(&pid).unwrap_or(&0);
        let entry = stats.entry(pid).or_insert(StreamStats {
            pid,
            stream_type: st_byte,
            total_bytes: 0,
            packet_count: 0,
            pes_sample: Vec::new(),
            pes_in_progress: Vec::new(),
            pes_started: false,
        });
        entry.total_bytes += payload.len() as u64;
        entry.packet_count += 1;

        if pid == 0 && payload_unit_start {
            parse_pat(payload, &mut pmt_pids, &mut pmt_pid_set);
        } else if pmt_pid_set.contains(&pid) && payload_unit_start {
            parse_pmt(payload, &mut pid_to_stream_type);
        } else if pid != 0 && !pmt_pid_set.contains(&pid) && !skip_pids.contains(&pid) {
            // PES reassembly + dispatch
            if payload_unit_start && payload.len() >= 9 && payload[0] == 0x00
                && payload[1] == 0x00 && payload[2] == 0x01
            {
                // Flush previous PES for this PID, if any.
                let mut start_new_pes = true;
                if let Some(prev) = pending_pes.remove(&pid) {
                    if !prev.is_empty() {
                        let stream_type = *pid_to_stream_type.get(&pid).unwrap_or(&0);
                        match on_pes(pid, stream_type, &prev, &pid_to_stream_type) {
                            PesAction::Continue => {}
                            PesAction::Stop => break 'outer,
                            PesAction::SkipPid => {
                                skip_pids.insert(pid);
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

    // Flush any remaining accumulated PES so codec parsers get a final shot.
    for (pid, buf) in pending_pes.into_iter() {
        if !buf.is_empty() && !skip_pids.contains(&pid) {
            let stream_type = *pid_to_stream_type.get(&pid).unwrap_or(&0);
            let _ = on_pes(pid, stream_type, &buf, &pid_to_stream_type);
        }
    }

    for stat in stats.values_mut() {
        if stat.stream_type == 0 {
            stat.stream_type = *pid_to_stream_type.get(&stat.pid).unwrap_or(&0);
        }
    }

    let duration_seconds = match (first_pcr_27mhz, last_pcr_27mhz) {
        (Some(a), Some(b)) if b > a => (b - a) as f64 / 27_000_000.0,
        _ => match (first_atc_27mhz, prev_atc_27mhz) {
            (Some(a), Some(b)) if b > a => (b - a) as f64 / 27_000_000.0,
            _ => 0.0,
        },
    };

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
    let mut pid_to_stream_type: HashMap<u16, u8> = HashMap::new();
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
            if payload_unit_start && payload.len() >= 9 && payload[0] == 0x00
                && payload[1] == 0x00 && payload[2] == 0x01
            {
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
            parse_pat(payload, &mut pmt_pids, &mut pmt_pid_set);
        } else if pmt_pid_set.contains(&pid) && payload_unit_start {
            parse_pmt(payload, &mut pid_to_stream_type);
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

fn parse_pat(payload: &[u8], pmt_pids: &mut Vec<u16>, pmt_pid_set: &mut std::collections::HashSet<u16>) {
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
    let section_length =
        ((payload[start + 1] as usize & 0x0F) << 8) | payload[start + 2] as usize;
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
                pmt_pids.push(pid);
            }
        }
    }
}

fn parse_pmt(payload: &[u8], pid_to_stream_type: &mut HashMap<u16, u8>) {
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
    let section_length =
        ((payload[start + 1] as usize & 0x0F) << 8) | payload[start + 2] as usize;
    let section_end = start + 3 + section_length;
    if section_end > payload.len() {
        return;
    }
    let program_info_length =
        ((payload[start + 10] as usize & 0x0F) << 8) | payload[start + 11] as usize;
    let mut i = start + 12 + program_info_length;
    let table_end = section_end.saturating_sub(4);
    while i + 5 <= table_end {
        let stream_type = payload[i];
        let elem_pid = (((payload[i + 1] as u16) & 0x1F) << 8) | payload[i + 2] as u16;
        let es_info_length =
            ((payload[i + 3] as usize & 0x0F) << 8) | payload[i + 4] as usize;
        pid_to_stream_type.insert(elem_pid, stream_type);
        i += 5 + es_info_length;
    }
}
