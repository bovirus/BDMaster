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
 * Full scan worker. Mirrors BDInfo's `ScanBDROM` background flow:
 * sequentially reads every M2TS file referenced by any playlist, dispatches
 * PES payloads through the codec parsers in full-scan mode (so PGS caption
 * counts are accumulated), and writes back per-clip / per-stream measured
 * sizes plus refined bit rates. The shared `FullScanState` is updated as the
 * worker progresses so the polling frontend can render updates in real time.
 */

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::protocol::{
  ChapterMetricsInfo, ChartSample, DiscInfo, FullScanState, ScanFileErrorInfo, ScanProgressInfo, TSStreamInfo,
};

use super::codec::{self, CodecScanState};
use super::m2ts;
use super::types::TSStreamType;
use super::{
  BDRom, StreamSource, cache_estimated_stream_sizes, effective_stream_source, estimate_stream_size, is_ssif_mvc_stream,
  open_bdrom, open_stream_reader_raw, recompute_mvc_extension, refresh_ssif_derived_metadata,
};

#[derive(Debug, Clone, Copy)]
struct CachedStreamEstimate {
  bit_rate: u64,
  active_bit_rate: u64,
  estimated_size: u64,
}

type StreamMeasurementKey = (usize, u32, u16);

/// Read wrapper that reports cumulative bytes consumed at most once per
/// `min_interval` AND short-circuits to EOF the moment the scan's cancel
/// flag is raised. Returning `Ok(0)` looks like end-of-file to the m2ts
/// scanner, which finishes its current packet, flushes the in-progress PES
/// to its callback, and returns the partial scan result. The worker then
/// discards that result (no measured-size writes) and exits the loop.
struct ProgressReader<R: Read> {
  inner: R,
  bytes_read: u64,
  last_report: Instant,
  min_interval: Duration,
  state: Arc<FullScanState>,
  base_completed: u64,
}

impl<R: Read> ProgressReader<R> {
  fn new(inner: R, state: Arc<FullScanState>, base_completed: u64) -> Self {
    Self {
      inner,
      bytes_read: 0,
      last_report: Instant::now(),
      min_interval: Duration::from_millis(250),
      state,
      base_completed,
    }
  }
}

impl<R: Read> Read for ProgressReader<R> {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    if self.state.cancel.load(Ordering::SeqCst) {
      return Ok(0);
    }
    let n = self.inner.read(buf)?;
    self.bytes_read += n as u64;
    if self.last_report.elapsed() >= self.min_interval {
      let mut p = self.state.progress.lock().unwrap_or_else(|e| e.into_inner());
      p.finished_bytes = self.base_completed + self.bytes_read;
      self.last_report = Instant::now();
    }
    Ok(n)
  }
}

/// Kicks off a background worker that performs the full scan. Returns
/// immediately. If a scan is already running the call is a no-op so the UI's
/// disabled-button guard isn't strictly required.
pub fn start(path: String, state: Arc<FullScanState>) {
  if state
    .running
    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
    .is_err()
  {
    return;
  }
  // Reset the cancel flag from any previous scan run.
  state.cancel.store(false, Ordering::SeqCst);

  let started_at_ms = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_millis() as u64)
    .unwrap_or(0);

  {
    let mut p = state.progress.lock().unwrap_or_else(|e| e.into_inner());
    *p = ScanProgressInfo {
      path: path.clone(),
      total_bytes: 0,
      finished_bytes: 0,
      is_running: true,
      is_completed: false,
      is_cancelled: false,
      error: None,
      file_errors: Vec::new(),
      current_file: None,
      started_at_ms,
      disc: None,
      version: 1,
    };
  }

  let state_for_thread = state.clone();
  std::thread::spawn(move || {
    let result = run_worker(path, state_for_thread.clone());
    let cancelled = state_for_thread.cancel.load(Ordering::SeqCst);
    let mut p = state_for_thread.progress.lock().unwrap_or_else(|e| e.into_inner());
    p.is_running = false;
    match result {
      Ok(()) => {
        if cancelled {
          p.is_cancelled = true;
        } else {
          p.is_completed = true;
          p.finished_bytes = p.total_bytes;
        }
      }
      Err(err) => {
        if cancelled {
          p.is_cancelled = true;
        } else {
          p.error = Some(err.to_string());
          log::error!("Full scan failed: {}", err);
        }
      }
    }
    p.version += 1;
    drop(p);
    state_for_thread.running.store(false, Ordering::SeqCst);
    // Leave the cancel flag set so any in-flight reads still see it; a
    // subsequent `start()` call resets it. No-op when never raised.
  });
}

/// Request cancellation of the running scan. Idempotent; safe to call when
/// no scan is running. The worker honours the flag at the next file boundary
/// and within the `ProgressReader::read` short-circuit.
pub fn cancel(state: &FullScanState) {
  state.cancel.store(true, Ordering::SeqCst);
}

pub fn snapshot(state: &FullScanState) -> ScanProgressInfo {
  state.progress.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn run_worker(path: String, state: Arc<FullScanState>) -> Result<()> {
  // 1. Open the disc once and build its structural metadata. Codec
  //    initialization happens inline while the exhaustive pass reads each
  //    file, so this phase starts reporting full-scan progress immediately
  //    and never spends the configured fast-scan budget a second time.
  let use_ssif = crate::config::get_config().scan.enable_ssif_support;
  let bdrom = open_bdrom(Path::new(&path), use_ssif)?;
  let mut disc = super::to_disc_info(&bdrom);
  refresh_ssif_derived_metadata(&mut disc, &bdrom);
  cache_estimated_stream_sizes(&mut disc);
  let cached_estimates = capture_stream_estimates(&disc);

  // 2. Collect every main/angle (clip-name → playlist-indices) pair. This
  //    is the same union BDInfo builds in PlaylistMap.
  let mut clip_to_pls: HashMap<String, Vec<usize>> = HashMap::new();
  for (pli, pl) in disc.playlists.iter().enumerate() {
    for clip in &pl.stream_clips {
      let entry = clip_to_pls.entry(clip.name.clone()).or_default();
      if !entry.contains(&pli) {
        entry.push(pli);
      }
    }
  }

  // Stable iteration order — handy for both the UI (predictable progress)
  // and for tests.
  let mut clip_names: Vec<String> = clip_to_pls.keys().cloned().collect();
  clip_names.sort();

  // 3. Compute total scan bytes upfront so the progress bar's max is fixed.
  //    `effective_stream_source` returns SSIF sizes when SSIF mode is on,
  //    so the progress bar reflects the actual bytes we'll read.
  let total_bytes: u64 = clip_names
    .iter()
    .filter_map(|name| effective_stream_source(&bdrom, name).map(|(_, s)| *s))
    .sum();

  {
    let mut p = state.progress.lock().unwrap_or_else(|e| e.into_inner());
    p.total_bytes = total_bytes;
    p.disc = Some(disc.clone());
    p.version += 1;
  }

  // Reset measured fields. The disc may carry stale measurements from a
  // previous scan in the same session.
  for pl in disc.playlists.iter_mut() {
    pl.measured_size = 0;
    pl.bitrate_samples.clear();
    pl.chapter_metrics.clear();
    for clip in pl.stream_clips.iter_mut() {
      clip.measured_size = 0;
    }
    for s in pl
      .video_streams
      .iter_mut()
      .chain(pl.audio_streams.iter_mut())
      .chain(pl.graphics_streams.iter_mut())
      .chain(pl.text_streams.iter_mut())
    {
      s.measured_size = 0;
      s.captions = 0;
      s.forced_captions = 0;
    }
    for angle_streams in &mut pl.angle_streams {
      for stream in angle_streams {
        stream.measured_size = 0;
      }
    }
  }

  let mut completed_bytes: u64 = 0;
  let mut measured_seconds: HashMap<(String, u32), f64> = HashMap::new();
  let mut playlist_diagnostics: HashMap<String, Vec<m2ts::StreamDiagnostic>> = HashMap::new();

  // 4. Iterate clips in stable order, scanning each file once.
  for clip_name in &clip_names {
    if state.cancel.load(Ordering::SeqCst) {
      break;
    }

    let entry = match effective_stream_source(&bdrom, clip_name) {
      Some(e) => e,
      None => continue,
    };
    let file_size = entry.1;

    {
      let mut p = state.progress.lock().unwrap_or_else(|e| e.into_inner());
      p.current_file = Some(clip_name.clone());
      p.version += 1;
    }

    match scan_one_file(
      &bdrom,
      &entry.0,
      &mut disc,
      clip_name,
      &state,
      completed_bytes,
      &cached_estimates,
      &mut measured_seconds,
      &mut playlist_diagnostics,
    ) {
      Ok(()) => {}
      Err(err) => {
        // A cancel triggers a clean Ok(0) EOF return from the
        // ProgressReader, so the m2ts scan will succeed (with a
        // partial result that scan_one_file already discards via
        // its own cancel check). A real I/O error here is logged
        // and we keep going to the next file — same policy BDInfo
        // applies via scanState.Exception.
        if !state.cancel.load(Ordering::SeqCst) {
          log::warn!("Full scan: failed to scan {}: {}", clip_name, err);
          let mut progress = state.progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
          progress.file_errors.push(ScanFileErrorInfo {
            file: clip_name.clone(),
            message: err.to_string(),
          });
          progress.version += 1;
        }
      }
    }

    if state.cancel.load(Ordering::SeqCst) {
      // Don't advance the byte counter or refresh derived fields when
      // the file was interrupted partway — the partial measurement is
      // either zero (scan_one_file returned early) or already written
      // before the EOF short-circuit, in which case finalize would
      // produce misleading bit rates. Stop here.
      break;
    }

    completed_bytes += file_size;

    // Refresh derived fields (playlist measured size, VBR bit rates, and
    // descriptions) so the snapshot we publish reflects what we know so
    // far. Doing this per-file means every poll the user sees up-to-date
    // numbers.
    restore_estimated_sizes(&mut disc, &cached_estimates);
    finalize_after_file(&mut disc);

    let mut p = state.progress.lock().unwrap_or_else(|e| e.into_inner());
    p.finished_bytes = completed_bytes;
    p.disc = Some(disc.clone());
    p.version += 1;
  }

  // 5. Final pass — only if we weren't cancelled. On cancel we leave the
  // partial disc snapshot in place; the frontend reverts to the un-scanned
  // state by re-issuing a basic scan_disc when it sees is_cancelled.
  if !state.cancel.load(Ordering::SeqCst) {
    restore_estimated_sizes(&mut disc, &cached_estimates);
    finalize_after_file(&mut disc);
    // The full pass now owns the definitive bit rates. Rebuild estimates
    // from those measured values instead of retaining the structural
    // pre-scan zeros.
    cache_estimated_stream_sizes(&mut disc);
    let mut p = state.progress.lock().unwrap_or_else(|e| e.into_inner());
    p.disc = Some(disc);
    p.current_file = None;
    p.finished_bytes = total_bytes;
    p.version += 1;
  } else {
    let mut p = state.progress.lock().unwrap_or_else(|e| e.into_inner());
    p.current_file = None;
    p.version += 1;
  }

  Ok(())
}

/// Scan a single M2TS file end-to-end. Builds a temporary `pid → *mut stream`
/// table that spans every playlist referencing this file, dispatches each
/// reassembled PES payload to the matching codec parser (full-scan mode), and
/// finally writes per-clip / per-stream measured-size deltas back to `disc`
/// using the m2ts scanner's per-PID totals and per-second bitrate samples.
fn scan_one_file(
  bd: &BDRom,
  src: &StreamSource,
  disc: &mut DiscInfo,
  clip_name: &str,
  state: &Arc<FullScanState>,
  base_completed: u64,
  cached_estimates: &HashMap<(String, u16), CachedStreamEstimate>,
  measured_seconds: &mut HashMap<(String, u32), f64>,
  playlist_diagnostics: &mut HashMap<String, Vec<m2ts::StreamDiagnostic>>,
) -> Result<()> {
  // Map of every playlist index that references this clip at any angle.
  let mut plis: Vec<usize> = Vec::new();
  for (pli, pl) in disc.playlists.iter().enumerate() {
    if pl.stream_clips.iter().any(|c| c.name == clip_name) {
      plis.push(pli);
    }
  }
  if plis.is_empty() {
    return Ok(());
  }

  // Build pid → *mut TSStreamInfo, first-playlist-wins. The codec parser
  // mutates this lead stream and we'll redistribute the codec-derived
  // fields to siblings after the scan via metadata snapshot.
  let mut pid_streams: HashMap<u16, *mut TSStreamInfo> = HashMap::new();
  for &pli in &plis {
    let pl = &mut disc.playlists[pli];
    let angle_indices: Vec<u32> = pl
      .stream_clips
      .iter()
      .filter(|clip| clip.name == clip_name)
      .map(|clip| clip.angle_index)
      .collect();
    if angle_indices.contains(&0) {
      for s in pl.video_streams.iter_mut() {
        pid_streams.entry(s.pid).or_insert(s as *mut _);
      }
      for s in pl.audio_streams.iter_mut() {
        pid_streams.entry(s.pid).or_insert(s as *mut _);
      }
      for s in pl.graphics_streams.iter_mut() {
        pid_streams.entry(s.pid).or_insert(s as *mut _);
      }
      for s in pl.text_streams.iter_mut() {
        pid_streams.entry(s.pid).or_insert(s as *mut _);
      }
    }
    for angle_index in angle_indices.into_iter().filter(|index| *index > 0) {
      if let Some(streams) = pl.angle_streams.get_mut(angle_index as usize - 1) {
        for stream in streams {
          pid_streams.entry(stream.pid).or_insert(stream as *mut _);
        }
      }
    }
  }
  if pid_streams.is_empty() {
    return Ok(());
  }

  // Bitrate hint passed to DTS / DTS-HD parsers. Seed with current values
  // so successive full-scan invocations get the refined hint.
  let bitrate_hint: HashMap<u16, i64> = pid_streams
    .iter()
    .map(|(pid, p)| unsafe { (*pid, (**p).bit_rate as i64) })
    .collect();

  let base_stream_bytes = capture_stream_measurement_base(disc, &plis);

  // The m2ts scanner reads BDInfo-sized chunks internally, so progress
  // reporting sits directly below it and fires once per chunk refill.
  //
  // Formerly this function added an extra BufReader layer here; that was
  // useful when m2ts read one 192-byte packet at a time, but is redundant
  // after the scanner moved to chunked reads.
  let raw_reader = open_stream_reader_raw(bd, src)?;
  let progress_reader = ProgressReader::new(raw_reader, state.clone(), base_completed);

  let mut pid_state: HashMap<u16, CodecScanState> = HashMap::new();
  let mut synthetic_holders: HashMap<u16, Box<TSStreamInfo>> = HashMap::new();

  let result = m2ts::scan_m2ts_streaming_from_reader_with_progress(
    progress_reader,
    |pid, _stream_type, payload, pmt| {
      // Cancellation: short-circuit the entire scan immediately.
      if state.cancel.load(Ordering::SeqCst) {
        return m2ts::PesAction::Stop;
      }
      let target_ptr: Option<*mut TSStreamInfo> = if let Some(&ptr) = pid_streams.get(&pid) {
        Some(ptr)
      } else if let Some(&stream_type) = pmt.get(&pid) {
        let mut stub = TSStreamInfo::new(pid, stream_type);
        let st = TSStreamType::from_u8(stream_type);
        stub.stream_type_text = st.type_text().to_string();
        stub.codec_name = st.codec_name().to_string();
        stub.codec_short_name = st.codec_short_name().to_string();
        stub.is_video_stream = st.is_video();
        stub.is_audio_stream = st.is_audio();
        stub.is_graphics_stream = st.is_graphics();
        stub.is_text_stream = st.is_text();
        let mut boxed = Box::new(stub);
        let ptr = &mut *boxed as *mut TSStreamInfo;
        synthetic_holders.insert(pid, boxed);
        pid_streams.insert(pid, ptr);
        Some(ptr)
      } else {
        None
      };

      let Some(ptr) = target_ptr else {
        // PID isn't in any playlist's MPLS and isn't even in the
        // PMT — pure noise. Skip its PES forever to avoid the
        // per-packet reassembly cost.
        return m2ts::PesAction::SkipPid;
      };

      let stream = unsafe { &mut *ptr };
      let st = TSStreamType::from_u8(stream.stream_type);
      // PGS streams are special: their codec parser keeps counting
      // captions across the whole file, so we never skip them.
      if st == TSStreamType::PresentationGraphics {
        let cs = pid_state.entry(pid).or_default();
        let bitrate = bitrate_hint.get(&pid).copied().unwrap_or(0);
        codec::scan_stream(stream, cs, payload, bitrate, true, true);
        return m2ts::PesAction::Continue;
      }
      // For non-PGS streams: dispatch the codec parser only while the
      // stream is still uninitialized. Once it reports initialized,
      // tell the m2ts scanner to stop reassembling its PES — byte
      // counting continues unaffected. This is the dominant per-byte
      // CPU saving on a full disc scan.
      if !stream.is_initialized {
        let cs = pid_state.entry(pid).or_default();
        let bitrate = bitrate_hint.get(&pid).copied().unwrap_or(0);
        codec::scan_stream(stream, cs, payload, bitrate, true, true);
        if !stream.is_initialized {
          return m2ts::PesAction::Continue;
        }
      }
      m2ts::PesAction::SkipPid
    },
    |progress| {
      publish_partial_file_snapshot(
        disc,
        &plis,
        clip_name,
        &progress,
        &base_stream_bytes,
        cached_estimates,
        state,
        base_completed,
      );
    },
  )?;

  // If the scan was cancelled mid-file, drop the partial result without
  // applying any per-clip / per-stream measured-size deltas.
  if state.cancel.load(Ordering::SeqCst) {
    return Ok(());
  }

  let file_total_bytes = result.bytes;
  let file_duration_s = result.duration_seconds;
  let per_pid_bytes: HashMap<u16, u64> = result.streams.iter().map(|(pid, stats)| (*pid, stats.total_bytes)).collect();

  // The m2ts scanner returns per-PID byte totals and per-second bitrate
  // samples. We attribute those to clips proportionally to each clip's
  // [time_in, time_out] window vs. the file's full duration — accurate for
  // CBR and a close approximation for VBR.
  // Snapshot codec metadata via the same raw pointers (captures synthetic
  // hidden streams, plus any codec changes the full scan made).
  let mut codec_metadata: HashMap<u16, TSStreamInfo> = HashMap::new();
  for (pid, ptr) in &pid_streams {
    unsafe {
      codec_metadata.insert(*pid, (**ptr).clone());
    }
  }

  // pid_streams holds raw pointers into disc.playlists; drop them before
  // we mutate the playlists by index in the loop below to keep miri-style
  // aliasing rules satisfied (no two `&mut` to overlapping data live at
  // once).
  drop(pid_streams);
  drop(synthetic_holders);

  for &pli in &plis {
    let pl = &mut disc.playlists[pli];
    // Track which PIDs the playlist already knows about so we can either
    // update them in-place or attach hidden synthetic streams below.
    let mut declared_pids: HashSet<u16> = pl
      .video_streams
      .iter()
      .chain(pl.audio_streams.iter())
      .chain(pl.graphics_streams.iter())
      .chain(pl.text_streams.iter())
      .map(|s| s.pid)
      .collect();

    let mut clip_ratios_by_angle: HashMap<u32, f64> = HashMap::new();
    for clip in pl.stream_clips.iter_mut() {
      if clip.name != clip_name {
        continue;
      }
      let clip_duration_s = clip.length as f64 / 45000.0;
      let ratio = if file_duration_s > 0.0 {
        (clip_duration_s / file_duration_s).clamp(0.0, 1.0)
      } else {
        1.0
      };
      // For clips that span the full file, ratio ≈ 1 and the clip's
      // measured size is the file's total bytes. Partial clips get
      // a proportional share.
      clip.measured_size = (file_total_bytes as f64 * ratio).round() as u64;
      *clip_ratios_by_angle.entry(clip.angle_index).or_default() += ratio;
    }

    // Distribute per-PID bytes to each declared stream of the playlist.
    // The same pro-rata factor used for the clip applies to its streams.
    let total_clip_ratio = clip_ratios_by_angle.get(&0).copied().unwrap_or_default();

    if total_clip_ratio > 0.0 {
      for s in pl
        .video_streams
        .iter_mut()
        .chain(pl.audio_streams.iter_mut())
        .chain(pl.graphics_streams.iter_mut())
        .chain(pl.text_streams.iter_mut())
      {
        if let Some(b) = per_pid_bytes.get(&s.pid) {
          let base = base_stream_bytes.get(&(pli, 0, s.pid)).copied().unwrap_or(s.measured_size);
          s.measured_size = base + (*b as f64 * total_clip_ratio).round() as u64;
        }
        // Copy codec-derived fields if the codec parser touched them
        // during this file's full scan (PGS captions, refined HEVC
        // metadata, etc.).
        if let Some(meta) = codec_metadata.get(&s.pid) {
          if meta.captions > s.captions {
            s.captions = meta.captions;
          }
          if meta.forced_captions > s.forced_captions {
            s.forced_captions = meta.forced_captions;
          }
          if !s.is_initialized && meta.is_initialized {
            copy_codec_metadata(s, meta);
          }
        }
      }
    }

    // Alternate angles have their own video-stream dictionaries in BDInfo.
    // Timestamp-free files retain the same proportional fallback as angle 0.
    for (angle_index, ratio) in clip_ratios_by_angle.iter().filter(|(angle, _)| **angle > 0) {
      if let Some(streams) = pl.angle_streams.get_mut(*angle_index as usize - 1) {
        for stream in streams {
          if let Some(bytes) = per_pid_bytes.get(&stream.pid) {
            let base = base_stream_bytes
              .get(&(pli, *angle_index, stream.pid))
              .copied()
              .unwrap_or(stream.measured_size);
            stream.measured_size = base.saturating_add((*bytes as f64 * *ratio).round() as u64);
          }
          if let Some(meta) = codec_metadata.get(&stream.pid) {
            if !stream.is_initialized && meta.is_initialized {
              copy_codec_metadata(stream, meta);
            }
          }
        }
      }
    }

    // Hidden tracks: PIDs that appear in the file's PMT but not in the
    // playlist's MPLS. We attach a copy with is_hidden=true once.
    if total_clip_ratio <= 0.0 {
      continue;
    }
    for (pid, meta) in &codec_metadata {
      if declared_pids.contains(pid) {
        continue;
      }
      if is_ssif_mvc_stream(bd, clip_name, *pid, meta) {
        let mut mvc = meta.clone();
        mvc.is_hidden = false;
        if let Some(b) = per_pid_bytes.get(pid) {
          mvc.measured_size = (*b as f64 * total_clip_ratio).round() as u64;
        }
        mvc.estimated_size = estimate_stream_size(&mvc, pl.total_length as f64 / 45000.0);
        if mvc.is_video_stream {
          pl.video_streams.push(mvc);
          declared_pids.insert(*pid);
        }
        continue;
      }
      let mut hidden = meta.clone();
      hidden.is_hidden = true;
      // Hidden tracks accumulate their own measured size based on the
      // same per-PID byte total.
      if let Some(b) = per_pid_bytes.get(pid) {
        hidden.measured_size = (*b as f64 * total_clip_ratio).round() as u64;
      }
      hidden.estimated_size = estimate_stream_size(&hidden, pl.total_length as f64 / 45000.0);
      pl.has_hidden_tracks = true;
      if hidden.is_video_stream {
        pl.video_streams.push(hidden);
      } else if hidden.is_audio_stream {
        pl.audio_streams.push(hidden);
      } else if hidden.is_graphics_stream {
        pl.graphics_streams.push(hidden);
      } else if hidden.is_text_stream {
        pl.text_streams.push(hidden);
      } else {
        continue;
      }
      declared_pids.insert(*pid);
    }
  }

  // Replace the proportional fallback above with PTS/DTS-window attribution
  // whenever the stream supplied usable timing diagnostics. Keeping the
  // fallback path makes malformed, timestamp-free files remain reportable.
  for &pli in &plis {
    apply_exact_file_measurements(
      &mut disc.playlists[pli],
      pli,
      clip_name,
      &result,
      &codec_metadata,
      &base_stream_bytes,
      measured_seconds,
      playlist_diagnostics,
    );
  }
  if result.streams.values().all(|stream| stream.diagnostics.is_empty()) {
    append_bitrate_samples_and_refresh_chapters(disc, &plis, clip_name, &result.bitrate_samples);
  }

  restore_estimated_sizes(disc, cached_estimates);
  refresh_ssif_derived_metadata(disc, bd);

  Ok(())
}

fn diagnostic_in_clip(diagnostic: &m2ts::StreamDiagnostic, time_in_45k: u64, time_out_45k: u64) -> bool {
  let time_in = time_in_45k as f64 / 45_000.0;
  let time_out = time_out_45k as f64 / 45_000.0;
  diagnostic.marker == 0.0 || (diagnostic.marker >= time_in && diagnostic.marker <= time_out)
}

#[allow(clippy::too_many_arguments)]
fn apply_exact_file_measurements(
  pl: &mut crate::protocol::PlaylistInfo,
  playlist_index: usize,
  clip_name: &str,
  result: &m2ts::M2tsScanResult,
  codec_metadata: &HashMap<u16, TSStreamInfo>,
  base_stream_bytes: &HashMap<StreamMeasurementKey, u64>,
  measured_seconds: &mut HashMap<(String, u32), f64>,
  playlist_diagnostics: &mut HashMap<String, Vec<m2ts::StreamDiagnostic>>,
) {
  let matching_indices: Vec<usize> = pl
    .stream_clips
    .iter()
    .enumerate()
    .filter_map(|(index, clip)| (clip.name == clip_name).then_some(index))
    .collect();
  if matching_indices.is_empty() {
    return;
  }

  let mut attributed_by_angle: HashMap<u32, HashMap<u16, u64>> = HashMap::new();
  let mut seconds_by_angle: HashMap<u32, f64> = HashMap::new();
  let mut transformed_diagnostics = Vec::new();

  for clip_index in matching_indices {
    let clip = pl.stream_clips[clip_index].clone();
    let mut clip_packets = 0u64;
    let mut has_timing = false;

    for (pid, stats) in &result.streams {
      let mut stream_bytes = 0u64;
      for diagnostic in &stats.diagnostics {
        if diagnostic_in_clip(diagnostic, clip.time_in, clip.time_out) {
          has_timing = true;
          clip_packets = clip_packets.saturating_add(diagnostic.packets);
          stream_bytes = stream_bytes.saturating_add(diagnostic.bytes);
        }
      }
      if stream_bytes > 0 {
        *attributed_by_angle
          .entry(clip.angle_index)
          .or_default()
          .entry(*pid)
          .or_default() += stream_bytes;
      }
    }

    if has_timing {
      pl.stream_clips[clip_index].measured_size = clip_packets.saturating_mul(192);
    }

    let primary_pid = if clip.angle_index == 0 {
      pl.video_streams.first().map(|stream| stream.pid)
    } else {
      pl.angle_streams
        .get(clip.angle_index as usize - 1)
        .and_then(|streams| streams.first())
        .map(|stream| stream.pid)
    };
    if let Some(stats) = primary_pid.and_then(|pid| result.streams.get(&pid)) {
      for diagnostic in &stats.diagnostics {
        if !diagnostic_in_clip(diagnostic, clip.time_in, clip.time_out) {
          continue;
        }
        *seconds_by_angle.entry(clip.angle_index).or_default() += diagnostic.interval;
        if clip.angle_index == 0 {
          let mut transformed = diagnostic.clone();
          transformed.marker =
            clip.relative_time_in as f64 / 45_000.0 + diagnostic.marker - clip.time_in as f64 / 45_000.0;
          transformed_diagnostics.push(transformed);
        }
      }
    }
  }

  if let Some(per_pid_bytes) = attributed_by_angle.get(&0) {
    for stream in pl
      .video_streams
      .iter_mut()
      .chain(pl.audio_streams.iter_mut())
      .chain(pl.graphics_streams.iter_mut())
      .chain(pl.text_streams.iter_mut())
    {
      if let Some(bytes) = per_pid_bytes.get(&stream.pid) {
        let base = base_stream_bytes
          .get(&(playlist_index, 0, stream.pid))
          .copied()
          .unwrap_or_default();
        stream.measured_size = base.saturating_add(*bytes);
      }
    }
  }

  for (angle_index, per_pid_bytes) in attributed_by_angle.iter().filter(|(angle, _)| **angle > 0) {
    if let Some(streams) = pl.angle_streams.get_mut(*angle_index as usize - 1) {
      for stream in streams {
        if let Some(bytes) = per_pid_bytes.get(&stream.pid) {
          let base = base_stream_bytes
            .get(&(playlist_index, *angle_index, stream.pid))
            .copied()
            .unwrap_or_default();
          stream.measured_size = base.saturating_add(*bytes);
        }
        if let Some(meta) = codec_metadata.get(&stream.pid) {
          if !stream.is_initialized && meta.is_initialized {
            copy_codec_metadata(stream, meta);
          }
        }
      }
    }
  }

  for (angle_index, seconds) in seconds_by_angle {
    *measured_seconds.entry((pl.name.clone(), angle_index)).or_default() += seconds;
  }
  update_measured_bitrates(pl, measured_seconds);

  if !transformed_diagnostics.is_empty() {
    let diagnostics = playlist_diagnostics.entry(pl.name.clone()).or_default();
    diagnostics.extend(transformed_diagnostics);
    diagnostics.sort_by(|a, b| {
      a.marker
        .partial_cmp(&b.marker)
        .unwrap_or(std::cmp::Ordering::Equal)
    });
    rebuild_chart_samples(pl, diagnostics);
    refresh_chapter_metrics_from_diagnostics(pl, diagnostics);
  }
}

fn update_measured_bitrates(
  pl: &mut crate::protocol::PlaylistInfo,
  measured_seconds: &HashMap<(String, u32), f64>,
) {
  if let Some(seconds) = measured_seconds.get(&(pl.name.clone(), 0)).copied().filter(|seconds| *seconds > 0.0) {
    for stream in pl
      .video_streams
      .iter_mut()
      .chain(pl.audio_streams.iter_mut())
      .chain(pl.graphics_streams.iter_mut())
      .chain(pl.text_streams.iter_mut())
    {
      let active = (stream.measured_size as f64 * 8.0 / seconds).round() as u64;
      if stream.is_video_stream {
        stream.active_bit_rate = active;
      }
      if stream.is_vbr {
        stream.bit_rate = active;
      }
    }
  }
  for (index, streams) in pl.angle_streams.iter_mut().enumerate() {
    let angle_index = index as u32 + 1;
    let Some(seconds) = measured_seconds
      .get(&(pl.name.clone(), angle_index))
      .copied()
      .filter(|seconds| *seconds > 0.0)
    else {
      continue;
    };
    for stream in streams {
      let active = (stream.measured_size as f64 * 8.0 / seconds).round() as u64;
      stream.active_bit_rate = active;
      if stream.is_vbr {
        stream.bit_rate = active;
      }
    }
  }
}

fn rebuild_chart_samples(pl: &mut crate::protocol::PlaylistInfo, diagnostics: &[m2ts::StreamDiagnostic]) {
  let mut buckets: std::collections::BTreeMap<u64, (u64, f64)> = std::collections::BTreeMap::new();
  for diagnostic in diagnostics {
    let second = diagnostic.marker.max(0.0).floor() as u64;
    let bucket = buckets.entry(second).or_default();
    bucket.0 = bucket.0.saturating_add(diagnostic.bytes);
    bucket.1 += diagnostic.interval;
  }
  pl.bitrate_samples = buckets
    .into_iter()
    .map(|(second, (bytes, seconds))| ChartSample {
      time: second as f64,
      bit_rate: if seconds > 0.0 {
        (bytes as f64 * 8.0 / seconds).round() as u64
      } else {
        0
      },
    })
    .collect();
}

fn refresh_chapter_metrics_from_diagnostics(
  pl: &mut crate::protocol::PlaylistInfo,
  diagnostics: &[m2ts::StreamDiagnostic],
) {
  pl.chapter_metrics.clear();
  let total_length = pl.total_length as f64 / 45_000.0;
  for chapter_index in 0..pl.chapters.len() {
    let start = pl.chapters[chapter_index];
    let end = pl.chapters.get(chapter_index + 1).copied().unwrap_or(total_length);
    let chapter: Vec<&m2ts::StreamDiagnostic> = diagnostics
      .iter()
      .filter(|diagnostic| diagnostic.marker >= start && diagnostic.marker < end)
      .collect();
    if chapter.is_empty() {
      pl.chapter_metrics.push(ChapterMetricsInfo::default());
      continue;
    }

    let total_bytes: u64 = chapter.iter().map(|diagnostic| diagnostic.bytes).sum();
    let chapter_length = (end - start).max(0.0);
    let avg_video_rate = if chapter_length > 0.0 {
      (total_bytes as f64 * 8.0 / chapter_length).round() as u64
    } else {
      0
    };
    let (max_1_sec_rate, max_1_sec_time) = diagnostic_peak_window(&chapter, 1.0);
    let (max_5_sec_rate, max_5_sec_time) = diagnostic_peak_window(&chapter, 5.0);
    let (max_10_sec_rate, max_10_sec_time) = diagnostic_peak_window(&chapter, 10.0);
    let frame_count = chapter.iter().filter(|diagnostic| diagnostic.has_frame).count() as u64;
    let (max_frame_size, max_frame_time) = chapter
      .iter()
      .filter(|diagnostic| diagnostic.has_frame)
      .max_by_key(|diagnostic| diagnostic.bytes)
      .map(|diagnostic| (diagnostic.bytes, diagnostic.marker))
      .unwrap_or((0, 0.0));

    pl.chapter_metrics.push(ChapterMetricsInfo {
      avg_video_rate,
      max_1_sec_rate,
      max_1_sec_time,
      max_5_sec_rate,
      max_5_sec_time,
      max_10_sec_rate,
      max_10_sec_time,
      avg_frame_size: if frame_count > 0 { total_bytes / frame_count } else { 0 },
      max_frame_size,
      max_frame_time,
    });
  }
}

fn diagnostic_peak_window(diagnostics: &[&m2ts::StreamDiagnostic], window_seconds: f64) -> (u64, f64) {
  let mut queue: std::collections::VecDeque<&m2ts::StreamDiagnostic> = std::collections::VecDeque::new();
  let mut bytes = 0u64;
  let mut seconds = 0.0;
  let mut best_rate = 0u64;
  let mut best_time = 0.0;
  for diagnostic in diagnostics {
    queue.push_back(diagnostic);
    bytes = bytes.saturating_add(diagnostic.bytes);
    seconds += diagnostic.interval;
    if seconds > window_seconds {
      let rate = (bytes as f64 * 8.0 / seconds).round() as u64;
      if rate > best_rate {
        best_rate = rate;
        best_time = (diagnostic.marker - seconds).max(0.0);
      }
      if let Some(front) = queue.pop_front() {
        bytes = bytes.saturating_sub(front.bytes);
        seconds = (seconds - front.interval).max(0.0);
      }
    }
  }
  (best_rate, best_time)
}

fn capture_stream_measurement_base(disc: &DiscInfo, plis: &[usize]) -> HashMap<StreamMeasurementKey, u64> {
  let mut base = HashMap::new();
  for &pli in plis {
    let Some(pl) = disc.playlists.get(pli) else {
      continue;
    };
    for s in pl
      .video_streams
      .iter()
      .chain(pl.audio_streams.iter())
      .chain(pl.graphics_streams.iter())
      .chain(pl.text_streams.iter())
    {
      base.insert((pli, 0, s.pid), s.measured_size);
    }
    for (angle_index, streams) in pl.angle_streams.iter().enumerate() {
      for stream in streams {
        base.insert((pli, angle_index as u32 + 1, stream.pid), stream.measured_size);
      }
    }
  }
  base
}

fn capture_stream_estimates(disc: &DiscInfo) -> HashMap<(String, u16), CachedStreamEstimate> {
  let mut cached = HashMap::new();
  for pl in &disc.playlists {
    for s in pl
      .video_streams
      .iter()
      .chain(pl.audio_streams.iter())
      .chain(pl.graphics_streams.iter())
      .chain(pl.text_streams.iter())
    {
      cached.insert(
        (pl.name.clone(), s.pid),
        CachedStreamEstimate {
          bit_rate: s.bit_rate,
          active_bit_rate: s.active_bit_rate,
          estimated_size: s.estimated_size,
        },
      );
    }
  }
  cached
}

fn restore_stream_estimates(disc: &mut DiscInfo, cached: &HashMap<(String, u16), CachedStreamEstimate>) {
  for pl in disc.playlists.iter_mut() {
    for s in pl
      .video_streams
      .iter_mut()
      .chain(pl.audio_streams.iter_mut())
      .chain(pl.graphics_streams.iter_mut())
      .chain(pl.text_streams.iter_mut())
      .chain(pl.angle_streams.iter_mut().flatten())
    {
      if let Some(estimate) = cached.get(&(pl.name.clone(), s.pid)) {
        s.estimated_size = estimate.estimated_size;
        s.bit_rate = estimate.bit_rate;
        s.active_bit_rate = estimate.active_bit_rate;
      }
    }
  }
}

fn restore_estimated_sizes(
  disc: &mut DiscInfo,
  cached: &HashMap<(String, u16), CachedStreamEstimate>,
) {
  for pl in disc.playlists.iter_mut() {
    for s in pl
      .video_streams
      .iter_mut()
      .chain(pl.audio_streams.iter_mut())
      .chain(pl.graphics_streams.iter_mut())
      .chain(pl.text_streams.iter_mut())
      .chain(pl.angle_streams.iter_mut().flatten())
    {
      if let Some(estimate) = cached.get(&(pl.name.clone(), s.pid)) {
        s.estimated_size = estimate.estimated_size;
      }
    }
  }
}

fn publish_partial_file_snapshot(
  disc: &DiscInfo,
  plis: &[usize],
  clip_name: &str,
  progress: &m2ts::M2tsScanProgress,
  base_stream_bytes: &HashMap<StreamMeasurementKey, u64>,
  cached_estimates: &HashMap<(String, u16), CachedStreamEstimate>,
  state: &Arc<FullScanState>,
  base_completed: u64,
) {
  // Never apply provisional measurements or cached rates to the live disc:
  // codec parsers are mutating that same object during the scan. Publishing
  // from a clone prevents a progress tick from undoing metadata discovered
  // earlier in the current file.
  let mut snapshot = disc.clone();
  apply_partial_file_measurements(&mut snapshot, plis, clip_name, progress, base_stream_bytes);
  restore_stream_estimates(&mut snapshot, cached_estimates);
  finalize_after_file(&mut snapshot);

  let mut p = state.progress.lock().unwrap_or_else(|e| e.into_inner());
  p.finished_bytes = base_completed + progress.bytes;
  p.disc = Some(snapshot);
  p.version += 1;
}

fn apply_partial_file_measurements(
  disc: &mut DiscInfo,
  plis: &[usize],
  clip_name: &str,
  progress: &m2ts::M2tsScanProgress,
  base_stream_bytes: &HashMap<StreamMeasurementKey, u64>,
) {
  let file_duration_s = progress.duration_seconds;
  for &pli in plis {
    let Some(pl) = disc.playlists.get_mut(pli) else {
      continue;
    };

    let mut clip_ratios_by_angle: HashMap<u32, f64> = HashMap::new();
    for clip in pl.stream_clips.iter_mut() {
      if clip.name != clip_name {
        continue;
      }
      let clip_duration_s = clip.length as f64 / 45000.0;
      let ratio = if file_duration_s > 0.0 {
        (clip_duration_s / file_duration_s).clamp(0.0, 1.0)
      } else {
        1.0
      };
      clip.measured_size = (progress.bytes as f64 * ratio).round() as u64;
      *clip_ratios_by_angle.entry(clip.angle_index).or_default() += ratio;
    }

    let total_clip_ratio = clip_ratios_by_angle.get(&0).copied().unwrap_or_default();
    if total_clip_ratio > 0.0 {
      for s in pl
        .video_streams
        .iter_mut()
        .chain(pl.audio_streams.iter_mut())
        .chain(pl.graphics_streams.iter_mut())
        .chain(pl.text_streams.iter_mut())
      {
        if let Some(stat) = progress.streams.get(&s.pid) {
          let base = base_stream_bytes.get(&(pli, 0, s.pid)).copied().unwrap_or(s.measured_size);
          s.measured_size = base + (stat.total_bytes as f64 * total_clip_ratio).round() as u64;
        }
      }
    }
    for (angle_index, ratio) in clip_ratios_by_angle.iter().filter(|(angle, _)| **angle > 0) {
      if let Some(streams) = pl.angle_streams.get_mut(*angle_index as usize - 1) {
        for stream in streams {
          if let Some(stat) = progress.streams.get(&stream.pid) {
            let base = base_stream_bytes
              .get(&(pli, *angle_index, stream.pid))
              .copied()
              .unwrap_or(stream.measured_size);
            stream.measured_size = base + (stat.total_bytes as f64 * *ratio).round() as u64;
          }
        }
      }
    }
  }
  recompute_mvc_extension(disc);
}

fn append_bitrate_samples_and_refresh_chapters(
  disc: &mut DiscInfo,
  plis: &[usize],
  clip_name: &str,
  file_samples: &[(f64, u64)],
) {
  for &pli in plis {
    let Some(pl) = disc.playlists.get_mut(pli) else {
      continue;
    };

    for clip in pl
      .stream_clips
      .iter()
      .filter(|c| c.angle_index == 0 && c.name == clip_name)
    {
      let clip_in_s = clip.time_in as f64 / 45000.0;
      let clip_out_s = clip.time_out as f64 / 45000.0;
      let playlist_offset_s = clip.relative_time_in as f64 / 45000.0;

      for &(file_time_s, bit_rate) in file_samples {
        if file_time_s < clip_in_s {
          continue;
        }
        if file_time_s > clip_out_s {
          break;
        }
        pl.bitrate_samples.push(ChartSample {
          time: playlist_offset_s + (file_time_s - clip_in_s),
          bit_rate,
        });
      }
    }

    pl.bitrate_samples
      .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    refresh_chapter_metrics(pl);
  }
}

fn refresh_chapter_metrics(pl: &mut crate::protocol::PlaylistInfo) {
  pl.chapter_metrics.clear();
  if pl.chapters.is_empty() {
    return;
  }

  let video_ratio = measured_video_ratio(pl);
  let total_length_s = pl.total_length as f64 / 45000.0;

  for i in 0..pl.chapters.len() {
    let start = pl.chapters[i];
    let end = if i + 1 < pl.chapters.len() {
      pl.chapters[i + 1]
    } else {
      total_length_s
    };
    let samples: Vec<ChartSample> = pl
      .bitrate_samples
      .iter()
      .filter(|s| s.time >= start && s.time < end)
      .cloned()
      .collect();

    if samples.is_empty() {
      pl.chapter_metrics.push(ChapterMetricsInfo::default());
      continue;
    }

    let avg = samples.iter().map(|s| s.bit_rate as f64).sum::<f64>() / samples.len() as f64;
    let (max_1_sec_rate, max_1_sec_time) = peak_window(&samples, start, end, 1.0);
    let (max_5_sec_rate, max_5_sec_time) = peak_window(&samples, start, end, 5.0);
    let (max_10_sec_rate, max_10_sec_time) = peak_window(&samples, start, end, 10.0);

    pl.chapter_metrics.push(ChapterMetricsInfo {
      avg_video_rate: scale_rate(avg, video_ratio),
      max_1_sec_rate: scale_rate(max_1_sec_rate, video_ratio),
      max_1_sec_time,
      max_5_sec_rate: scale_rate(max_5_sec_rate, video_ratio),
      max_5_sec_time,
      max_10_sec_rate: scale_rate(max_10_sec_rate, video_ratio),
      max_10_sec_time,
      avg_frame_size: 0,
      max_frame_size: 0,
      max_frame_time: 0.0,
    });
  }
}

fn measured_video_ratio(pl: &crate::protocol::PlaylistInfo) -> f64 {
  let video_bytes: u64 = pl.video_streams.iter().map(|s| s.measured_size).sum();
  let playlist_bytes: u64 = pl
    .stream_clips
    .iter()
    .filter(|c| c.angle_index == 0)
    .map(|c| c.measured_size)
    .sum();
  if video_bytes > 0 && playlist_bytes > 0 {
    return (video_bytes as f64 / playlist_bytes as f64).clamp(0.0, 1.0);
  }
  1.0
}

fn peak_window(samples: &[ChartSample], start: f64, end: f64, window_s: f64) -> (f64, f64) {
  let mut best_rate = 0.0;
  let mut best_time = start;
  for sample in samples {
    let window_end = (sample.time + window_s).min(end);
    let window: Vec<&ChartSample> = samples
      .iter()
      .filter(|s| s.time >= sample.time && s.time < window_end)
      .collect();
    if window.is_empty() {
      continue;
    }
    let rate = window.iter().map(|s| s.bit_rate as f64).sum::<f64>() / window.len() as f64;
    if rate > best_rate {
      best_rate = rate;
      best_time = sample.time;
    }
  }
  (best_rate, best_time)
}

fn scale_rate(rate: f64, ratio: f64) -> u64 {
  (rate * ratio).max(0.0).round() as u64
}

/// Refresh playlist-level aggregates after each file finishes. Estimated
/// bitrate/size fields are intentionally left alone; they are cached before
/// the full scan starts and remain the pre-scan estimate for the whole run.
fn finalize_after_file(disc: &mut DiscInfo) {
  for pl in disc.playlists.iter_mut() {
    let mut total: u64 = 0;
    for clip in &pl.stream_clips {
      if clip.angle_index == 0 {
        total += clip.measured_size;
      }
    }
    pl.measured_size = total;

    // Recompute description so newly populated PGS caption counts and
    // other codec metadata surface in the UI without rewriting the
    // cached pre-scan bitrate estimate.
    for s in pl
      .video_streams
      .iter_mut()
      .chain(pl.audio_streams.iter_mut())
      .chain(pl.graphics_streams.iter_mut())
      .chain(pl.text_streams.iter_mut())
    {
      codec::finalize_description(s);
    }
    for angle_streams in &mut pl.angle_streams {
      for stream in angle_streams {
        codec::finalize_description(stream);
      }
    }
  }
  recompute_mvc_extension(disc);
}

fn copy_codec_metadata(dst: &mut TSStreamInfo, src: &TSStreamInfo) {
  if !src.is_initialized {
    return;
  }
  dst.is_initialized = true;
  dst.is_vbr = src.is_vbr;
  dst.codec_name = src.codec_name.clone();
  dst.codec_short_name = src.codec_short_name.clone();
  dst.stream_type_text = src.stream_type_text.clone();
  dst.description = src.description.clone();
  dst.width = src.width;
  dst.height = src.height;
  dst.framerate = src.framerate.clone();
  dst.frame_rate_enumerator = src.frame_rate_enumerator;
  dst.frame_rate_denominator = src.frame_rate_denominator;
  dst.aspect_ratio = src.aspect_ratio.clone();
  dst.aspect_ratio_code = src.aspect_ratio_code;
  dst.video_format = src.video_format.clone();
  dst.is_interlaced = src.is_interlaced;
  dst.encoding_profile = src.encoding_profile.clone();
  dst.extended_format_info = src.extended_format_info.clone();
  dst.base_view = src.base_view;
  dst.channel_count = src.channel_count;
  dst.lfe = src.lfe;
  dst.sample_rate = src.sample_rate;
  dst.bit_depth = src.bit_depth;
  dst.channel_layout = src.channel_layout.clone();
  dst.audio_mode = src.audio_mode.clone();
  dst.dial_norm = src.dial_norm;
  dst.has_extensions = src.has_extensions;
  dst.core = src.core.clone();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::protocol::{ChartSample, DiscInfo, PlaylistInfo, PlaylistStreamClipInfo, ScanProgressInfo};
  use std::io::Cursor;
  use std::path::PathBuf;
  use std::sync::atomic::AtomicU32;

  // ---------- Temp-dir guard (no tempfile crate available) ----------

  static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

  struct TempDir {
    path: PathBuf,
  }

  impl TempDir {
    fn new(tag: &str) -> Self {
      let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
      let pid = std::process::id();
      let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
      let mut path = std::env::temp_dir();
      path.push(format!("bdmaster_fullscan_{}_{}_{}_{}", tag, pid, nanos, n));
      std::fs::create_dir_all(&path).expect("create temp dir");
      Self { path }
    }

    fn path(&self) -> &Path {
      &self.path
    }
  }

  impl Drop for TempDir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.path);
    }
  }

  // ---------- DiscInfo / PlaylistInfo builders ----------

  fn empty_disc() -> DiscInfo {
    DiscInfo {
      path: String::new(),
      disc_name: String::new(),
      disc_title: String::new(),
      volume_label: String::new(),
      size: 0,
      is_bd_plus: false,
      is_bd_java: false,
      is_3d: false,
      is_4k: false,
      is_50hz: false,
      is_dbox: false,
      is_psp: false,
      is_uhd: false,
      has_mvc_extension: false,
      has_hevc_streams: false,
      has_uhd_disc_marker: false,
      meta_title: None,
      meta_disc_number: None,
      file_set_identifier: None,
      playlists: Vec::new(),
      stream_files: Vec::new(),
      stream_clip_files: Vec::new(),
    }
  }

  fn empty_playlist(name: &str) -> PlaylistInfo {
    PlaylistInfo {
      name: name.to_string(),
      group_index: 1,
      file_size: 0,
      measured_size: 0,
      total_length: 0,
      has_hidden_tracks: false,
      has_loops: false,
      is_custom: false,
      chapters: Vec::new(),
      chapter_metrics: Vec::new(),
      bitrate_samples: Vec::new(),
      stream_clips: Vec::new(),
      video_streams: Vec::new(),
      audio_streams: Vec::new(),
      graphics_streams: Vec::new(),
      text_streams: Vec::new(),
      angle_streams: Vec::new(),
      total_angles: 0,
    }
  }

  fn clip(name: &str, time_in: u64, time_out: u64) -> PlaylistStreamClipInfo {
    let length = time_out.saturating_sub(time_in);
    PlaylistStreamClipInfo {
      name: name.to_string(),
      display_name: name.to_string(),
      time_in,
      time_out,
      relative_time_in: 0,
      relative_time_out: length,
      length,
      file_size: 0,
      measured_size: 0,
      interleaved_file_size: 0,
      angle_index: 0,
    }
  }

  fn video_stream(pid: u16) -> TSStreamInfo {
    let mut s = TSStreamInfo::new(pid, 0x1b);
    s.is_video_stream = true;
    s.codec_name = "MPEG-4 AVC Video".into();
    s.bit_rate = 20_000_000;
    s.active_bit_rate = 20_000_000;
    s.estimated_size = 1_000_000;
    s
  }

  fn audio_stream(pid: u16) -> TSStreamInfo {
    let mut s = TSStreamInfo::new(pid, 0x81);
    s.is_audio_stream = true;
    s.codec_name = "Dolby Digital Audio".into();
    s.bit_rate = 640_000;
    s.active_bit_rate = 640_000;
    s.estimated_size = 100_000;
    s
  }

  fn sample(time: f64, bit_rate: u64) -> ChartSample {
    ChartSample { time, bit_rate }
  }

  // ---------- m2ts byte builders (copied from m2ts.rs tests) ----------

  const TS_PACKET_SIZE_T: usize = 188;
  const SYNC: u8 = 0x47;

  fn ts_packet(pusi: bool, pid: u16, payload: &[u8]) -> Vec<u8> {
    let mut ts = vec![0xFFu8; TS_PACKET_SIZE_T];
    ts[0] = SYNC;
    ts[1] = ((pid >> 8) as u8 & 0x1F) | if pusi { 0x40 } else { 0 };
    ts[2] = (pid & 0xFF) as u8;
    ts[3] = 0x10; // payload only
    let n = payload.len().min(TS_PACKET_SIZE_T - 4);
    ts[4..4 + n].copy_from_slice(&payload[..n]);
    ts
  }

  /// TS packet with an adaptation field carrying a PCR, plus payload.
  fn ts_packet_pcr(pid: u16, pcr_27mhz: i128) -> Vec<u8> {
    let mut ts = vec![0xFFu8; TS_PACKET_SIZE_T];
    ts[0] = SYNC;
    ts[1] = (pid >> 8) as u8 & 0x1F; // no PUSI
    ts[2] = (pid & 0xFF) as u8;
    ts[3] = 0x30; // adaptation + payload
    ts[4] = 7; // adaptation_field_length
    ts[5] = 0x10; // PCR flag
    let base = (pcr_27mhz / 300) as u64;
    let ext = (pcr_27mhz % 300) as u64;
    ts[6] = (base >> 25) as u8;
    ts[7] = (base >> 17) as u8;
    ts[8] = (base >> 9) as u8;
    ts[9] = (base >> 1) as u8;
    ts[10] = (((base & 0x1) as u8) << 7) | 0x7E | ((ext >> 8) as u8 & 0x01);
    ts[11] = (ext & 0xFF) as u8;
    ts
  }

  /// Wrap a TS packet in a 192-byte M2TS frame with the given ATC prefix.
  fn m2ts_atc(ts: &[u8], atc: u32) -> Vec<u8> {
    let mut p = vec![
      ((atc >> 24) & 0x3F) as u8,
      ((atc >> 16) & 0xFF) as u8,
      ((atc >> 8) & 0xFF) as u8,
      (atc & 0xFF) as u8,
    ];
    p.extend_from_slice(ts);
    p
  }

  fn pat_payload(program: u16, pmt_pid: u16) -> Vec<u8> {
    vec![
      0x00,
      0x00,
      0xB0,
      0x0D,
      0x00,
      0x01,
      0x01,
      0x00,
      0x00,
      (program >> 8) as u8,
      (program & 0xFF) as u8,
      0xE0 | (pmt_pid >> 8) as u8,
      (pmt_pid & 0xFF) as u8,
      0x00,
      0x00,
      0x00,
      0x00,
    ]
  }

  /// PMT declaring AVC on 0x1011, AC3 on 0x1100, and a PGS on 0x1200. The
  /// PGS PID is *not* present in the MPLS, so the full scan attaches it as a
  /// hidden / synthetic track. section_length = 28 (0x1C): 3 ES entries.
  fn pmt_payload() -> Vec<u8> {
    vec![
      0x00, 0x02, 0xB0, 0x1C, 0x00, 0x01, 0x01, 0x00, 0x00, 0xE0, 0x00, 0xF0, 0x00, 0x1b, 0xF0, 0x11, 0xF0,
      0x00, // AVC 0x1011
      0x81, 0xF1, 0x00, 0xF0, 0x00, // AC3 0x1100
      0x90, 0xF2, 0x00, 0xF0, 0x00, // PGS 0x1200 (hidden, not in MPLS)
      0x00, 0x00, 0x00, 0x00,
    ]
  }

  fn pes_payload(stream_id: u8, es: &[u8]) -> Vec<u8> {
    let mut v = vec![0x00, 0x00, 0x01, stream_id, 0x00, 0x00, 0x80, 0x00, 0x00];
    v.extend_from_slice(es);
    v
  }

  /// Build a small but structurally complete M2TS byte stream:
  /// PAT, PMT, several PES for the AVC and AC3 PIDs, and a couple of PCR
  /// packets spaced ~2 s apart so a non-zero duration is reported.
  fn build_m2ts_bytes() -> Vec<u8> {
    let mut data = Vec::new();
    let mut atc: u32 = 0;
    let push = |frame: Vec<u8>, data: &mut Vec<u8>| {
      data.extend_from_slice(&frame);
    };
    push(
      m2ts_atc(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100)), atc),
      &mut data,
    );
    atc += 27_000; // ~1ms
    push(m2ts_atc(&ts_packet(true, 0x0100, &pmt_payload()), atc), &mut data);
    // PCR at t=0 (27MHz units).
    atc += 27_000;
    push(m2ts_atc(&ts_packet_pcr(0x1011, 0), atc), &mut data);
    // A handful of PES payloads on both PIDs, each ATC step ~0.5 s so the
    // per-second bitrate sampler crosses several 1-second boundaries.
    const STEP_27MHZ: u32 = 13_500_000; // 0.5 s at 27 MHz
    for i in 0..8u8 {
      atc += STEP_27MHZ;
      push(
        m2ts_atc(&ts_packet(true, 0x1011, &pes_payload(0xE0, &[0xAA, 0xBB, i])), atc),
        &mut data,
      );
      atc += STEP_27MHZ;
      push(
        m2ts_atc(&ts_packet(true, 0x1100, &pes_payload(0xBD, &[0x0B, 0x77, i])), atc),
        &mut data,
      );
      // PGS PES on the hidden PID so the synthetic + hidden-track path runs.
      atc += 27_000;
      push(
        m2ts_atc(&ts_packet(true, 0x1200, &pes_payload(0xBD, &[0x50, 0x47, i])), atc),
        &mut data,
      );
    }
    // PCR at ~8 s for a non-zero PCR-derived duration.
    atc += STEP_27MHZ;
    push(m2ts_atc(&ts_packet_pcr(0x1011, 8 * 27_000_000), atc), &mut data);
    data
  }

  // ---------- minimal native BDMV disc on disk ----------

  fn build_mpls() -> Vec<u8> {
    // Same layout as mpls.rs test builder: one play item (00001.M2TS), one
    // AVC video stream (0x1011), one AC3 audio stream (0x1100), one chapter.
    let mut d: Vec<u8> = Vec::new();
    d.extend_from_slice(b"MPLS0200");
    d.extend_from_slice(&[0u8; 4]); // playlist_offset @8
    d.extend_from_slice(&[0u8; 4]); // chapters_offset @12
    d.extend_from_slice(&[0u8; 4]); // extensions_offset @16
    while d.len() < 0x38 {
      d.push(0);
    }
    d.push(0x10); // mvc_base_view_r

    let playlist_offset = d.len() as u32;
    d[8..12].copy_from_slice(&playlist_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes()); // playlist_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.extend_from_slice(&1u16.to_be_bytes()); // item_count
    d.extend_from_slice(&0u16.to_be_bytes()); // subitem_count

    let item_start = d.len();
    d.extend_from_slice(&0u16.to_be_bytes()); // item_length placeholder
    d.extend_from_slice(b"00001");
    d.extend_from_slice(b"M2TS");
    d.push(0x00);
    d.push(0x00);
    d.push(0x00);
    d.extend_from_slice(&0u32.to_be_bytes()); // in_time
    d.extend_from_slice(&4_500_000u32.to_be_bytes()); // out_time (100 s)
    d.extend_from_slice(&[0u8; 12]);

    d.extend_from_slice(&0u16.to_be_bytes()); // stn_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.push(1); // video count
    d.push(1); // audio count
    d.push(0);
    d.push(0);
    d.push(0);
    d.push(0);
    d.push(0);
    d.extend_from_slice(&[0u8; 5]);

    // Video stream entry.
    d.push(3);
    d.push(1);
    d.extend_from_slice(&0x1011u16.to_be_bytes());
    d.push(3);
    d.push(0x1b); // AVC
    d.push((6 << 4) | 1);
    d.push(3 << 4);

    // Audio stream entry.
    d.push(3);
    d.push(1);
    d.extend_from_slice(&0x1100u16.to_be_bytes());
    d.push(5);
    d.push(0x81); // AC3
    d.push((6 << 4) | 1);
    d.extend_from_slice(b"eng");

    let item_len = (d.len() - item_start - 2) as u16;
    d[item_start..item_start + 2].copy_from_slice(&item_len.to_be_bytes());

    let chapters_offset = d.len() as u32;
    d[12..16].copy_from_slice(&chapters_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes());
    d.extend_from_slice(&1u16.to_be_bytes());
    let mut chapter = vec![0u8; 14];
    chapter[1] = 1;
    chapter[4..8].copy_from_slice(&(45000u32 * 10).to_be_bytes());
    d.extend_from_slice(&chapter);

    d
  }

  fn build_clpi() -> Vec<u8> {
    // Minimal valid CLPI: AVC video (0x1011) + AC3 audio (0x1100).
    fn stream_entry(pid: u16, coding_type: u8, attrs: &[u8]) -> Vec<u8> {
      let mut v = Vec::new();
      v.extend_from_slice(&pid.to_be_bytes());
      v.push((1 + attrs.len()) as u8);
      v.push(coding_type);
      v.extend_from_slice(attrs);
      v
    }
    let video = stream_entry(0x1011, 0x1b, &[(6 << 4) | 1, 3 << 4]);
    let audio = stream_entry(0x1100, 0x81, &[(6 << 4) | 1, b'e', b'n', b'g']);
    let streams = [video, audio];

    let mut clip = vec![0u8; 10];
    clip[8] = streams.len() as u8;
    for s in &streams {
      clip.extend_from_slice(s);
    }
    let clip_len = clip.len() as u32;

    let mut data = Vec::new();
    data.extend_from_slice(b"HDMV0200");
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&16u32.to_be_bytes());
    data.extend_from_slice(&clip_len.to_be_bytes());
    data.extend_from_slice(&clip);
    data
  }

  /// Lay out BDMV/PLAYLIST + CLIPINF + STREAM under `root`. Returns the root
  /// path string to pass to the worker.
  fn build_native_disc(root: &Path) {
    let bdmv = root.join("BDMV");
    let playlist = bdmv.join("PLAYLIST");
    let clipinf = bdmv.join("CLIPINF");
    let stream = bdmv.join("STREAM");
    std::fs::create_dir_all(&playlist).unwrap();
    std::fs::create_dir_all(&clipinf).unwrap();
    std::fs::create_dir_all(&stream).unwrap();
    std::fs::write(bdmv.join("index.bdmv"), b"INDX0200").unwrap();
    std::fs::write(playlist.join("00800.mpls"), build_mpls()).unwrap();
    std::fs::write(clipinf.join("00001.clpi"), build_clpi()).unwrap();
    std::fs::write(stream.join("00001.m2ts"), build_m2ts_bytes()).unwrap();
  }

  // ---------- Pure-function unit tests ----------

  #[test]
  fn scale_rate_scales_and_clamps() {
    assert_eq!(scale_rate(100.0, 0.5), 50);
    assert_eq!(scale_rate(0.0, 1.0), 0);
    assert_eq!(scale_rate(-10.0, 1.0), 0); // clamped to 0
    assert_eq!(scale_rate(33.4, 1.0), 33); // rounds
    assert_eq!(scale_rate(33.6, 1.0), 34);
  }

  #[test]
  fn peak_window_finds_busiest_region_and_handles_empty() {
    let empty: Vec<ChartSample> = Vec::new();
    let (rate, time) = peak_window(&empty, 0.0, 10.0, 1.0);
    assert_eq!(rate, 0.0);
    assert_eq!(time, 0.0);

    let samples = vec![sample(0.0, 100), sample(1.0, 200), sample(2.0, 900), sample(3.0, 800)];
    let (rate, time) = peak_window(&samples, 0.0, 4.0, 1.0);
    // 1-second window: peak is the single highest sample (2.0 -> 900).
    assert_eq!(rate, 900.0);
    assert_eq!(time, 2.0);

    let (rate5, _) = peak_window(&samples, 0.0, 4.0, 5.0);
    // 5-second window averages everything; lower than the single peak.
    assert!(rate5 > 0.0 && rate5 < 900.0);
  }

  #[test]
  fn measured_video_ratio_handles_zero_and_normal() {
    let mut pl = empty_playlist("00800.MPLS");
    // No video bytes -> ratio 1.0
    assert_eq!(measured_video_ratio(&pl), 1.0);

    let mut v = video_stream(0x1011);
    v.measured_size = 800;
    pl.video_streams.push(v);
    let mut c = clip("00001.M2TS", 0, 4_500_000);
    c.measured_size = 1000;
    pl.stream_clips.push(c);
    let ratio = measured_video_ratio(&pl);
    assert!((ratio - 0.8).abs() < 1e-9);

    // video bytes exceeding playlist bytes clamps to 1.0
    pl.video_streams[0].measured_size = 5000;
    assert_eq!(measured_video_ratio(&pl), 1.0);
  }

  #[test]
  fn refresh_chapter_metrics_empty_and_populated() {
    // No chapters: clears and returns.
    let mut pl = empty_playlist("p");
    pl.chapter_metrics.push(ChapterMetricsInfo::default());
    refresh_chapter_metrics(&mut pl);
    assert!(pl.chapter_metrics.is_empty());

    // With chapters + samples.
    let mut pl = empty_playlist("p");
    pl.total_length = 45000 * 20; // 20 s
    pl.chapters = vec![0.0, 10.0];
    let mut v = video_stream(0x1011);
    v.measured_size = 900;
    pl.video_streams.push(v);
    let mut c = clip("00001.M2TS", 0, 45000 * 20);
    c.measured_size = 1000;
    pl.stream_clips.push(c);
    pl.bitrate_samples = vec![sample(0.0, 100), sample(5.0, 200), sample(11.0, 300), sample(15.0, 400)];
    refresh_chapter_metrics(&mut pl);
    assert_eq!(pl.chapter_metrics.len(), 2);
    // First chapter [0,10) has 2 samples -> nonzero avg.
    assert!(pl.chapter_metrics[0].avg_video_rate > 0);
    // Second chapter [10,20) has 2 samples -> nonzero avg.
    assert!(pl.chapter_metrics[1].avg_video_rate > 0);

    // A chapter range with no samples gets a default metric.
    let mut pl = empty_playlist("p");
    pl.total_length = 45000 * 100;
    pl.chapters = vec![0.0, 50.0];
    pl.bitrate_samples = vec![sample(0.0, 100)]; // only in first chapter
    refresh_chapter_metrics(&mut pl);
    assert_eq!(pl.chapter_metrics.len(), 2);
    assert_eq!(pl.chapter_metrics[1].avg_video_rate, 0);
  }

  #[test]
  fn append_bitrate_samples_filters_by_clip_window_and_sorts() {
    let mut disc = empty_disc();
    let mut pl = empty_playlist("00800.MPLS");
    // clip covering file-time [2s, 8s], placed at playlist offset 0.
    let mut c = clip("00001.M2TS", 45000 * 2, 45000 * 8);
    c.relative_time_in = 0;
    pl.total_length = 45000 * 6;
    pl.stream_clips.push(c);
    disc.playlists.push(pl);

    // file samples: some before clip_in, some within, some after clip_out.
    let file_samples = vec![
      (1.0, 100u64), // before window -> skipped
      (2.0, 200),    // at clip_in
      (5.0, 300),    // within
      (8.0, 400),    // at clip_out (<= so included)
      (9.0, 500),    // after window -> break
    ];
    append_bitrate_samples_and_refresh_chapters(&mut disc, &[0], "00001.M2TS", &file_samples);
    let pl = &disc.playlists[0];
    // 3 samples kept (2.0, 5.0, 8.0); times re-based to playlist offset.
    assert_eq!(pl.bitrate_samples.len(), 3);
    // First kept sample is file_time 2.0 - clip_in 2.0 + offset 0 = 0.0.
    assert!((pl.bitrate_samples[0].time - 0.0).abs() < 1e-9);
    // sorted ascending
    for w in pl.bitrate_samples.windows(2) {
      assert!(w[0].time <= w[1].time);
    }
  }

  #[test]
  fn append_bitrate_samples_skips_missing_playlist_index() {
    let mut disc = empty_disc();
    disc.playlists.push(empty_playlist("p"));
    // index 5 doesn't exist -> no panic, no change.
    append_bitrate_samples_and_refresh_chapters(&mut disc, &[5], "x", &[(0.0, 1)]);
    assert!(disc.playlists[0].bitrate_samples.is_empty());
  }

  #[test]
  fn exact_timing_windows_drive_payload_bitrates_and_chapter_metrics() {
    let mut pl = empty_playlist("00001.MPLS");
    pl.total_length = 180_000;
    pl.chapters = vec![0.0];
    pl.stream_clips.push(clip("00001.M2TS", 0, 180_000));
    let mut video = video_stream(0x1011);
    video.is_vbr = true;
    video.measured_size = 0;
    let mut audio = audio_stream(0x1100);
    audio.is_vbr = true;
    audio.measured_size = 0;
    pl.video_streams.push(video);
    pl.audio_streams.push(audio);

    let diagnostics = |bytes, packets, has_frame| {
      [1.0, 2.0, 3.0]
        .into_iter()
        .map(|marker| m2ts::StreamDiagnostic {
          marker,
          interval: 1.0,
          bytes,
          packets,
          has_frame,
        })
        .collect()
    };
    let mut streams = HashMap::new();
    streams.insert(
      0x1011,
      m2ts::StreamStats {
        pid: 0x1011,
        stream_type: 0x1b,
        total_bytes: 300,
        packet_count: 6,
        packet_seconds: 3.0,
        diagnostics: diagnostics(100, 2, true),
        pes_sample: Vec::new(),
        pes_in_progress: Vec::new(),
        pes_started: false,
      },
    );
    streams.insert(
      0x1100,
      m2ts::StreamStats {
        pid: 0x1100,
        stream_type: 0x81,
        total_bytes: 60,
        packet_count: 3,
        packet_seconds: 0.0,
        diagnostics: diagnostics(20, 1, false),
        pes_sample: Vec::new(),
        pes_in_progress: Vec::new(),
        pes_started: false,
      },
    );
    let result = m2ts::M2tsScanResult {
      bytes: 9 * 192,
      duration_seconds: 3.0,
      streams,
      bitrate_samples: Vec::new(),
      program_pmt_pids: Vec::new(),
      pcr_pid: None,
    };
    let mut measured_seconds = HashMap::new();
    let mut playlist_diagnostics = HashMap::new();
    apply_exact_file_measurements(
      &mut pl,
      0,
      "00001.M2TS",
      &result,
      &HashMap::new(),
      &HashMap::new(),
      &mut measured_seconds,
      &mut playlist_diagnostics,
    );

    assert_eq!(pl.stream_clips[0].measured_size, 9 * 192);
    assert_eq!(pl.video_streams[0].measured_size, 300);
    assert_eq!(pl.audio_streams[0].measured_size, 60);
    assert_eq!(pl.video_streams[0].active_bit_rate, 800);
    assert_eq!(pl.video_streams[0].bit_rate, 800);
    assert_eq!(pl.audio_streams[0].bit_rate, 160);
    assert_eq!(pl.bitrate_samples.len(), 3);
    assert_eq!(pl.chapter_metrics.len(), 1);
    assert_eq!(pl.chapter_metrics[0].avg_video_rate, 600);
    assert_eq!(pl.chapter_metrics[0].avg_frame_size, 100);
    assert_eq!(pl.chapter_metrics[0].max_frame_size, 100);
  }

  #[test]
  fn exact_angle_measurement_replaces_the_live_proportional_snapshot() {
    let mut pl = empty_playlist("00001.MPLS");
    let mut angle_clip = clip("00101.M2TS", 0, 90_000);
    angle_clip.angle_index = 1;
    pl.stream_clips.push(angle_clip);
    let mut angle_video = video_stream(0x1011);
    angle_video.measured_size = 999;
    pl.angle_streams.push(vec![angle_video]);

    let mut streams = HashMap::new();
    streams.insert(
      0x1011,
      m2ts::StreamStats {
        pid: 0x1011,
        stream_type: 0x1b,
        total_bytes: 100,
        packet_count: 2,
        packet_seconds: 1.0,
        diagnostics: vec![m2ts::StreamDiagnostic {
          marker: 1.0,
          interval: 1.0,
          bytes: 100,
          packets: 2,
          has_frame: true,
        }],
        pes_sample: Vec::new(),
        pes_in_progress: Vec::new(),
        pes_started: false,
      },
    );
    let result = m2ts::M2tsScanResult {
      bytes: 384,
      duration_seconds: 1.0,
      streams,
      bitrate_samples: Vec::new(),
      program_pmt_pids: Vec::new(),
      pcr_pid: None,
    };
    let mut base = HashMap::new();
    base.insert((0, 1, 0x1011), 50);
    apply_exact_file_measurements(
      &mut pl,
      0,
      "00101.M2TS",
      &result,
      &HashMap::new(),
      &base,
      &mut HashMap::new(),
      &mut HashMap::new(),
    );

    assert_eq!(pl.stream_clips[0].measured_size, 384);
    assert_eq!(pl.angle_streams[0][0].measured_size, 150);
  }

  #[test]
  fn capture_and_restore_stream_estimates_roundtrip() {
    let mut disc = empty_disc();
    let mut pl = empty_playlist("00800.MPLS");
    pl.video_streams.push(video_stream(0x1011));
    pl.audio_streams.push(audio_stream(0x1100));
    disc.playlists.push(pl);

    let cached = capture_stream_estimates(&disc);
    assert_eq!(cached.len(), 2);
    let v = cached.get(&("00800.MPLS".to_string(), 0x1011)).unwrap();
    assert_eq!(v.bit_rate, 20_000_000);

    // Clobber and then restore.
    {
      let pl = &mut disc.playlists[0];
      for s in pl.video_streams.iter_mut().chain(pl.audio_streams.iter_mut()) {
        s.bit_rate = 1;
        s.active_bit_rate = 1;
        s.estimated_size = 1;
      }
    }
    restore_stream_estimates(&mut disc, &cached);
    assert_eq!(disc.playlists[0].video_streams[0].bit_rate, 20_000_000);
    assert_eq!(disc.playlists[0].audio_streams[0].estimated_size, 100_000);
  }

  #[test]
  fn capture_stream_measurement_base_picks_only_listed_plis() {
    let mut disc = empty_disc();
    let mut pl0 = empty_playlist("a");
    let mut v = video_stream(0x1011);
    v.measured_size = 1234;
    pl0.video_streams.push(v);
    disc.playlists.push(pl0);
    disc.playlists.push(empty_playlist("b"));

    let base = capture_stream_measurement_base(&disc, &[0, 99]);
    assert_eq!(base.get(&(0, 0, 0x1011)).copied(), Some(1234));
    // index 99 doesn't exist; skipped without panic.
    assert_eq!(base.len(), 1);
  }

  #[test]
  fn apply_partial_file_measurements_distributes_bytes() {
    let mut disc = empty_disc();
    let mut pl = empty_playlist("00800.MPLS");
    let mut c = clip("00001.M2TS", 0, 45000 * 10); // 10 s clip
    c.measured_size = 0;
    pl.stream_clips.push(c);
    pl.video_streams.push(video_stream(0x1011));
    pl.audio_streams.push(audio_stream(0x1100));
    disc.playlists.push(pl);

    let mut streams: HashMap<u16, m2ts::StreamStats> = HashMap::new();
    streams.insert(
      0x1011,
      m2ts::StreamStats {
        pid: 0x1011,
        stream_type: 0x1b,
        total_bytes: 8000,
        packet_count: 1,
        packet_seconds: 0.0,
        diagnostics: Vec::new(),
        pes_sample: Vec::new(),
        pes_in_progress: Vec::new(),
        pes_started: false,
      },
    );
    let progress = m2ts::M2tsScanProgress {
      bytes: 10_000,
      duration_seconds: 10.0,
      streams,
    };
    let base = capture_stream_measurement_base(&disc, &[0]);
    apply_partial_file_measurements(&mut disc, &[0], "00001.M2TS", &progress, &base);
    // clip ratio = 10/10 = 1 -> clip measured = file bytes.
    assert_eq!(disc.playlists[0].stream_clips[0].measured_size, 10_000);
    // video stream got its per-PID bytes; audio (no stats) unchanged.
    assert_eq!(disc.playlists[0].video_streams[0].measured_size, 8000);
    assert_eq!(disc.playlists[0].audio_streams[0].measured_size, 0);
  }

  #[test]
  fn apply_partial_file_measurements_zero_duration_uses_full_ratio() {
    let mut disc = empty_disc();
    let mut pl = empty_playlist("00800.MPLS");
    pl.stream_clips.push(clip("00001.M2TS", 0, 45000 * 10));
    disc.playlists.push(pl);
    let progress = m2ts::M2tsScanProgress {
      bytes: 500,
      duration_seconds: 0.0,
      streams: HashMap::new(),
    };
    apply_partial_file_measurements(&mut disc, &[0], "00001.M2TS", &progress, &HashMap::new());
    // ratio defaults to 1.0 when file_duration is 0.
    assert_eq!(disc.playlists[0].stream_clips[0].measured_size, 500);
  }

  #[test]
  fn finalize_after_file_sums_clip_sizes() {
    let mut disc = empty_disc();
    let mut pl = empty_playlist("00800.MPLS");
    let mut c1 = clip("00001.M2TS", 0, 100);
    c1.measured_size = 300;
    let mut c2 = clip("00002.M2TS", 0, 100);
    c2.measured_size = 700;
    let mut c3 = clip("00003.M2TS", 0, 100);
    c3.measured_size = 999;
    c3.angle_index = 1; // non-angle-0, must be ignored.
    pl.stream_clips.push(c1);
    pl.stream_clips.push(c2);
    pl.stream_clips.push(c3);
    pl.video_streams.push(video_stream(0x1011));
    disc.playlists.push(pl);

    finalize_after_file(&mut disc);
    assert_eq!(disc.playlists[0].measured_size, 1000);
  }

  #[test]
  fn copy_codec_metadata_copies_only_when_src_initialized() {
    let mut dst = TSStreamInfo::new(0x1011, 0x1b);
    let mut src = TSStreamInfo::new(0x1011, 0x1b);
    src.is_initialized = false;
    src.width = 1920;
    copy_codec_metadata(&mut dst, &src);
    // src not initialized -> no copy.
    assert_eq!(dst.width, 0);
    assert!(!dst.is_initialized);

    src.is_initialized = true;
    src.width = 1920;
    src.height = 1080;
    src.codec_name = "MPEG-4 AVC Video".into();
    src.channel_count = 6;
    src.sample_rate = 48000;
    src.core = Some(Box::new(TSStreamInfo::new(0x1100, 0x81)));
    copy_codec_metadata(&mut dst, &src);
    assert!(dst.is_initialized);
    assert_eq!(dst.width, 1920);
    assert_eq!(dst.height, 1080);
    assert_eq!(dst.codec_name, "MPEG-4 AVC Video");
    assert_eq!(dst.channel_count, 6);
    assert_eq!(dst.sample_rate, 48000);
    assert!(dst.core.is_some());
  }

  // ---------- ProgressReader ----------

  fn make_state() -> Arc<FullScanState> {
    Arc::new(FullScanState::new())
  }

  #[test]
  fn progress_reader_passes_through_bytes() {
    let state = make_state();
    let data = vec![1u8; 4096];
    let mut reader = ProgressReader::new(Cursor::new(data.clone()), state.clone(), 100);
    // Force a report on the first read by backdating last_report.
    reader.last_report = Instant::now() - Duration::from_secs(1);
    let mut buf = [0u8; 1024];
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 1024);
    assert_eq!(reader.bytes_read, 1024);
    // The progress snapshot reflects base_completed + bytes_read.
    let p = snapshot(&state);
    assert_eq!(p.finished_bytes, 100 + 1024);

    // Read the rest.
    let mut total = n;
    loop {
      let m = reader.read(&mut buf).unwrap();
      if m == 0 {
        break;
      }
      total += m;
    }
    assert_eq!(total, 4096);
  }

  #[test]
  fn progress_reader_short_circuits_on_cancel() {
    let state = make_state();
    let data = vec![7u8; 2048];
    let mut reader = ProgressReader::new(Cursor::new(data), state.clone(), 0);
    // Raise cancel before reading -> immediate EOF.
    state.cancel.store(true, Ordering::SeqCst);
    let mut buf = [0u8; 512];
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 0);
    assert_eq!(reader.bytes_read, 0);
  }

  // ---------- cancel / snapshot ----------

  #[test]
  fn cancel_sets_flag_and_snapshot_clones() {
    let state = make_state();
    assert!(!state.cancel.load(Ordering::SeqCst));
    cancel(&state);
    assert!(state.cancel.load(Ordering::SeqCst));

    {
      let mut p = state.progress.lock().unwrap();
      p.path = "abc".into();
      p.version = 42;
    }
    let snap = snapshot(&state);
    assert_eq!(snap.path, "abc");
    assert_eq!(snap.version, 42);
  }

  // ---------- publish_partial_file_snapshot ----------

  #[test]
  fn publish_partial_file_snapshot_updates_progress_and_disc() {
    let state = make_state();
    let mut disc = empty_disc();
    let mut pl = empty_playlist("00800.MPLS");
    pl.stream_clips.push(clip("00001.M2TS", 0, 45000 * 10));
    pl.video_streams.push(video_stream(0x1011));
    disc.playlists.push(pl);
    let base = capture_stream_measurement_base(&disc, &[0]);
    let cached = capture_stream_estimates(&disc);

    let progress = m2ts::M2tsScanProgress {
      bytes: 4000,
      duration_seconds: 10.0,
      streams: HashMap::new(),
    };
    publish_partial_file_snapshot(&disc, &[0], "00001.M2TS", &progress, &base, &cached, &state, 1000);
    let p = snapshot(&state);
    assert_eq!(p.finished_bytes, 1000 + 4000);
    let published = p.disc.expect("partial disc snapshot");
    assert!(published.playlists[0].stream_clips[0].measured_size > 0);
    assert!(p.version >= 1);
    // The live codec target remains untouched by provisional snapshots.
    assert_eq!(disc.playlists[0].stream_clips[0].measured_size, 0);
    assert_eq!(disc.playlists[0].video_streams[0].bit_rate, 20_000_000);
  }

  // ---------- Heavy worker path ----------

  #[test]
  fn run_worker_scans_native_disc_to_completion() {
    let tmp = TempDir::new("run_worker");
    build_native_disc(tmp.path());
    let state = make_state();

    // Seed progress as start() would.
    {
      let mut p = state.progress.lock().unwrap();
      *p = ScanProgressInfo {
        path: tmp.path().to_string_lossy().to_string(),
        is_running: true,
        ..Default::default()
      };
    }

    let path = tmp.path().to_string_lossy().to_string();
    run_worker(path, state.clone()).expect("worker succeeds");

    let snap = snapshot(&state);
    // total_bytes was computed from the m2ts file size; finished should reach it.
    assert!(snap.total_bytes > 0, "total_bytes set");
    assert_eq!(snap.finished_bytes, snap.total_bytes);
    let disc = snap.disc.expect("disc present");
    assert_eq!(disc.playlists.len(), 1);
    let pl = &disc.playlists[0];
    // The single clip's bytes were measured.
    assert!(pl.measured_size > 0, "playlist measured size populated");
    assert!(pl.stream_clips[0].measured_size > 0, "clip measured");
    // At least one bitrate sample was produced (PCR span > 1 s).
    assert!(!pl.bitrate_samples.is_empty(), "bitrate samples present");
    // chapter metrics: one chapter declared.
    assert_eq!(pl.chapter_metrics.len(), pl.chapters.len());
    // The PGS PID 0x1200 in the PMT but not the MPLS was attached as a
    // hidden track on the graphics stream list.
    assert!(pl.has_hidden_tracks, "hidden track flagged");
    assert!(
      pl.graphics_streams.iter().any(|s| s.pid == 0x1200 && s.is_hidden),
      "hidden PGS track attached"
    );
  }

  #[test]
  fn start_records_error_for_missing_disc() {
    let tmp = TempDir::new("start_err");
    // No BDMV: the worker's open_bdrom fails and start() records the error.
    let state = make_state();
    let path = tmp.path().to_string_lossy().to_string();
    start(path, state.clone());

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
      let snap = snapshot(&state);
      if !snap.is_running {
        break;
      }
      if Instant::now() > deadline {
        panic!("worker did not finish");
      }
      std::thread::sleep(Duration::from_millis(20));
    }
    let snap = snapshot(&state);
    assert!(!snap.is_completed);
    assert!(snap.error.is_some(), "error recorded for missing disc");
    assert!(!state.running.load(Ordering::SeqCst));
  }

  #[test]
  fn start_marks_cancelled_when_cancel_set() {
    let tmp = TempDir::new("start_cancel");
    build_native_disc(tmp.path());
    let state = make_state();
    let path = tmp.path().to_string_lossy().to_string();

    // Raise cancel concurrently. start() resets the flag at the top, so we
    // race the worker; to keep the test deterministic we cancel right after
    // launching and poll for either completion or the cancelled flag.
    start(path, state.clone());
    cancel(&state);

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
      let snap = snapshot(&state);
      if !snap.is_running {
        break;
      }
      if Instant::now() > deadline {
        panic!("worker did not finish");
      }
      // Keep re-raising cancel in case start() reset it before we set it.
      cancel(&state);
      std::thread::sleep(Duration::from_millis(5));
    }
    let snap = snapshot(&state);
    // Either it finished normally before we cancelled, or it was cancelled.
    assert!(snap.is_completed || snap.is_cancelled);
    assert!(!state.running.load(Ordering::SeqCst));
  }

  #[test]
  fn run_worker_cancelled_before_start_does_not_finalize() {
    let tmp = TempDir::new("cancel_pre");
    build_native_disc(tmp.path());
    let state = make_state();
    {
      let mut p = state.progress.lock().unwrap();
      p.path = tmp.path().to_string_lossy().to_string();
    }
    // Cancel before running -> the per-clip loop breaks immediately.
    state.cancel.store(true, Ordering::SeqCst);
    let path = tmp.path().to_string_lossy().to_string();
    run_worker(path, state.clone()).expect("worker returns Ok even when cancelled");

    let snap = snapshot(&state);
    // Disc snapshot exists (set before the loop) but nothing was measured.
    let disc = snap.disc.expect("disc present");
    assert_eq!(disc.playlists[0].measured_size, 0);
    // current_file cleared on the cancel branch.
    assert!(snap.current_file.is_none());
  }

  #[test]
  fn run_worker_errors_on_missing_disc() {
    let tmp = TempDir::new("missing");
    // No BDMV created -> open_bdrom fails.
    let state = make_state();
    let path = tmp.path().to_string_lossy().to_string();
    let res = run_worker(path, state.clone());
    assert!(res.is_err(), "worker errors when no BDMV present");
  }

  #[test]
  fn scan_one_file_populates_measurements() {
    let tmp = TempDir::new("scan_one");
    build_native_disc(tmp.path());
    let use_ssif = false;
    let bdrom = open_bdrom(tmp.path(), use_ssif).expect("open bdrom");
    let mut disc = super::super::to_disc_info(&bdrom);
    super::super::codec_init::codec_init(&mut disc, &bdrom);
    let cached = capture_stream_estimates(&disc);

    let state = make_state();
    let clip_name = "00001.M2TS";
    let entry = effective_stream_source(&bdrom, clip_name).expect("stream source");
    let mut measured_seconds = HashMap::new();
    let mut diagnostics = HashMap::new();
    scan_one_file(
      &bdrom,
      &entry.0,
      &mut disc,
      clip_name,
      &state,
      0,
      &cached,
      &mut measured_seconds,
      &mut diagnostics,
    )
    .expect("scan_one_file ok");

    let pl = &disc.playlists[0];
    // The clip and at least the video stream picked up measured bytes.
    assert!(pl.stream_clips[0].measured_size > 0);
    let total_stream_measured: u64 = pl
      .video_streams
      .iter()
      .chain(pl.audio_streams.iter())
      .map(|s| s.measured_size)
      .sum();
    assert!(total_stream_measured > 0, "stream measured sizes populated");
  }

  #[test]
  fn scan_one_file_cancel_discards_measurements() {
    let tmp = TempDir::new("scan_one_cancel");
    build_native_disc(tmp.path());
    let bdrom = open_bdrom(tmp.path(), false).expect("open bdrom");
    let mut disc = super::super::to_disc_info(&bdrom);
    super::super::codec_init::codec_init(&mut disc, &bdrom);
    let cached = capture_stream_estimates(&disc);

    let state = make_state();
    state.cancel.store(true, Ordering::SeqCst);
    let clip_name = "00001.M2TS";
    let entry = effective_stream_source(&bdrom, clip_name).expect("stream source");
    let mut measured_seconds = HashMap::new();
    let mut diagnostics = HashMap::new();
    scan_one_file(
      &bdrom,
      &entry.0,
      &mut disc,
      clip_name,
      &state,
      0,
      &cached,
      &mut measured_seconds,
      &mut diagnostics,
    )
    .expect("scan_one_file ok");

    // Cancelled: no per-clip measured-size deltas applied.
    assert_eq!(disc.playlists[0].stream_clips[0].measured_size, 0);
  }

  #[test]
  fn scan_one_file_unknown_clip_is_noop() {
    let tmp = TempDir::new("scan_one_unknown");
    build_native_disc(tmp.path());
    let bdrom = open_bdrom(tmp.path(), false).expect("open bdrom");
    let mut disc = super::super::to_disc_info(&bdrom);
    let cached = capture_stream_estimates(&disc);
    let state = make_state();
    // Use the real stream source but a clip name no playlist references.
    let entry = effective_stream_source(&bdrom, "00001.M2TS").expect("src");
    let mut measured_seconds = HashMap::new();
    let mut diagnostics = HashMap::new();
    scan_one_file(
      &bdrom,
      &entry.0,
      &mut disc,
      "99999.M2TS",
      &state,
      0,
      &cached,
      &mut measured_seconds,
      &mut diagnostics,
    )
    .expect("noop ok");
    // No playlist references that clip name -> nothing measured.
    assert_eq!(disc.playlists[0].stream_clips[0].measured_size, 0);
  }

  #[test]
  fn start_spawns_worker_and_completes() {
    let tmp = TempDir::new("start");
    build_native_disc(tmp.path());
    let state = make_state();
    let path = tmp.path().to_string_lossy().to_string();

    start(path, state.clone());

    // Poll until the worker finishes (no event emission; UI polls).
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
      let snap = snapshot(&state);
      if !snap.is_running && (snap.is_completed || snap.error.is_some() || snap.is_cancelled) {
        break;
      }
      if Instant::now() > deadline {
        panic!("worker did not complete in time");
      }
      std::thread::sleep(Duration::from_millis(20));
    }
    let snap = snapshot(&state);
    assert!(snap.is_completed, "scan completed: {:?}", snap.error);
    assert!(!snap.is_running);
    assert_eq!(snap.finished_bytes, snap.total_bytes);
    // running flag cleared so a subsequent start would proceed.
    assert!(!state.running.load(Ordering::SeqCst));
  }

  #[test]
  fn start_is_noop_when_already_running() {
    let state = make_state();
    // Pretend a scan is already running.
    state.running.store(true, Ordering::SeqCst);
    {
      let mut p = state.progress.lock().unwrap();
      p.version = 7;
      p.path = "original".into();
    }
    // This call must early-return without touching progress.
    start("ignored".into(), state.clone());
    let snap = snapshot(&state);
    assert_eq!(snap.version, 7);
    assert_eq!(snap.path, "original");
    state.running.store(false, Ordering::SeqCst);
  }
}
