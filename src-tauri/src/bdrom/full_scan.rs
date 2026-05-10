/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
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
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::protocol::{
    ChapterMetricsInfo, ChartSample, DiscInfo, FullScanState, ScanProgressInfo, TSStreamInfo,
};

use super::codec::{self, CodecScanState};
use super::m2ts;
use super::types::TSStreamType;
use super::{
    effective_stream_source, is_ssif_mvc_stream, open_bdrom, open_stream_reader_raw,
    recompute_mvc_extension, refresh_ssif_derived_metadata, BDRom, StreamSource,
};

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
            let mut p = self.state.progress.lock().unwrap();
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
        let mut p = state.progress.lock().unwrap();
        *p = ScanProgressInfo {
            path: path.clone(),
            total_bytes: 0,
            finished_bytes: 0,
            is_running: true,
            is_completed: false,
            is_cancelled: false,
            error: None,
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
        let mut p = state_for_thread.progress.lock().unwrap();
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
    state.progress.lock().unwrap().clone()
}

fn run_worker(path: String, state: Arc<FullScanState>) -> Result<()> {
    // 1. Open the disc once and build the same disc info the UI is
    //    currently displaying. The previous implementation called
    //    `super::scan` here, which re-opened the BDRom a second time
    //    internally — a measurable cost on slow drives. We now share a
    //    single BDRom across the codec-init partial pass and the
    //    subsequent full pass.
    let use_ssif = crate::config::get_config().scan.enable_ssif_support;
    let bdrom = open_bdrom(Path::new(&path), use_ssif)?;
    let mut disc = super::to_disc_info(&bdrom);
    super::codec_init(&mut disc, &bdrom);
    refresh_ssif_derived_metadata(&mut disc, &bdrom);

    // 2. Collect every (clip-name → playlist-indices) pair for angle 0. This
    //    is the same union BDInfo builds in PlaylistMap.
    let mut clip_to_pls: HashMap<String, Vec<usize>> = HashMap::new();
    for (pli, pl) in disc.playlists.iter().enumerate() {
        for clip in &pl.stream_clips {
            if clip.angle_index != 0 {
                continue;
            }
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
        let mut p = state.progress.lock().unwrap();
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
    }

    let mut completed_bytes: u64 = 0;

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
            let mut p = state.progress.lock().unwrap();
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
        finalize_after_file(&mut disc);

        let mut p = state.progress.lock().unwrap();
        p.finished_bytes = completed_bytes;
        p.disc = Some(disc.clone());
        p.version += 1;
    }

    // 5. Final pass — only if we weren't cancelled. On cancel we leave the
    // partial disc snapshot in place; the frontend reverts to the un-scanned
    // state by re-issuing a basic scan_disc when it sees is_cancelled.
    if !state.cancel.load(Ordering::SeqCst) {
        finalize_after_file(&mut disc);
        let mut p = state.progress.lock().unwrap();
        p.disc = Some(disc);
        p.current_file = None;
        p.finished_bytes = total_bytes;
        p.version += 1;
    } else {
        let mut p = state.progress.lock().unwrap();
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
) -> Result<()> {
    // Map of every playlist index that references this clip (angle 0).
    let mut plis: Vec<usize> = Vec::new();
    for (pli, pl) in disc.playlists.iter().enumerate() {
        if pl
            .stream_clips
            .iter()
            .any(|c| c.angle_index == 0 && c.name == clip_name)
        {
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

    // The m2ts scanner returns per-PID byte totals and per-second bitrate
    // samples. We attribute those to clips proportionally to each clip's
    // [time_in, time_out] window vs. the file's full duration — accurate for
    // CBR and a close approximation for VBR.
    let file_total_bytes = result.bytes;
    let file_duration_s = result.duration_seconds;
    let per_pid_bytes: HashMap<u16, u64> = result
        .streams
        .iter()
        .map(|(pid, st)| (*pid, st.total_bytes))
        .collect();

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

        for clip in pl.stream_clips.iter_mut() {
            if clip.angle_index != 0 || clip.name != clip_name {
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
        }

        // Distribute per-PID bytes to each declared stream of the playlist.
        // The same pro-rata factor used for the clip applies to its streams.
        let total_clip_ratio: f64 = pl
            .stream_clips
            .iter()
            .filter(|c| c.angle_index == 0 && c.name == clip_name)
            .map(|c| {
                let cl = c.length as f64 / 45000.0;
                if file_duration_s > 0.0 {
                    (cl / file_duration_s).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            })
            .sum();

        if total_clip_ratio > 0.0 {
            for s in pl
                .video_streams
                .iter_mut()
                .chain(pl.audio_streams.iter_mut())
                .chain(pl.graphics_streams.iter_mut())
                .chain(pl.text_streams.iter_mut())
            {
                if let Some(b) = per_pid_bytes.get(&s.pid) {
                    let base = base_stream_bytes
                        .get(&(pli, s.pid))
                        .copied()
                        .unwrap_or(s.measured_size);
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

        // Hidden tracks: PIDs that appear in the file's PMT but not in the
        // playlist's MPLS. We attach a copy with is_hidden=true once.
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

    refresh_ssif_derived_metadata(disc, bd);
    append_bitrate_samples_and_refresh_chapters(disc, &plis, clip_name, &result.bitrate_samples);

    Ok(())
}

fn capture_stream_measurement_base(disc: &DiscInfo, plis: &[usize]) -> HashMap<(usize, u16), u64> {
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
            base.insert((pli, s.pid), s.measured_size);
        }
    }
    base
}

fn publish_partial_file_snapshot(
    disc: &mut DiscInfo,
    plis: &[usize],
    clip_name: &str,
    progress: &m2ts::M2tsScanProgress,
    base_stream_bytes: &HashMap<(usize, u16), u64>,
    state: &Arc<FullScanState>,
    base_completed: u64,
) {
    apply_partial_file_measurements(disc, plis, clip_name, progress, base_stream_bytes);
    finalize_after_file(disc);

    let mut p = state.progress.lock().unwrap();
    p.finished_bytes = base_completed + progress.bytes;
    p.disc = Some(disc.clone());
    p.version += 1;
}

fn apply_partial_file_measurements(
    disc: &mut DiscInfo,
    plis: &[usize],
    clip_name: &str,
    progress: &m2ts::M2tsScanProgress,
    base_stream_bytes: &HashMap<(usize, u16), u64>,
) {
    let file_duration_s = progress.duration_seconds;
    for &pli in plis {
        let Some(pl) = disc.playlists.get_mut(pli) else {
            continue;
        };

        for clip in pl.stream_clips.iter_mut() {
            if clip.angle_index != 0 || clip.name != clip_name {
                continue;
            }
            let clip_duration_s = clip.length as f64 / 45000.0;
            let ratio = if file_duration_s > 0.0 {
                (clip_duration_s / file_duration_s).clamp(0.0, 1.0)
            } else {
                1.0
            };
            clip.measured_size = (progress.bytes as f64 * ratio).round() as u64;
        }

        let total_clip_ratio: f64 = pl
            .stream_clips
            .iter()
            .filter(|c| c.angle_index == 0 && c.name == clip_name)
            .map(|c| {
                let cl = c.length as f64 / 45000.0;
                if file_duration_s > 0.0 {
                    (cl / file_duration_s).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            })
            .sum();

        if total_clip_ratio <= 0.0 {
            continue;
        }

        for s in pl
            .video_streams
            .iter_mut()
            .chain(pl.audio_streams.iter_mut())
            .chain(pl.graphics_streams.iter_mut())
            .chain(pl.text_streams.iter_mut())
        {
            if let Some(stat) = progress.streams.get(&s.pid) {
                let base = base_stream_bytes
                    .get(&(pli, s.pid))
                    .copied()
                    .unwrap_or(s.measured_size);
                s.measured_size =
                    base + (stat.total_bytes as f64 * total_clip_ratio).round() as u64;
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

        pl.bitrate_samples.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
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

/// Refresh playlist-level aggregates after each file finishes: the playlist's
/// measured_size is the sum of its angle-0 clips', and VBR bit_rate is
/// recomputed from accumulated per-stream measured_size against playlist
/// duration. Mirrors BDInfo's UpdatePlaylistBitrates timer callback.
fn finalize_after_file(disc: &mut DiscInfo) {
    for pl in disc.playlists.iter_mut() {
        let mut total: u64 = 0;
        for clip in &pl.stream_clips {
            if clip.angle_index == 0 {
                total += clip.measured_size;
            }
        }
        pl.measured_size = total;

        let total_seconds = pl.total_length as f64 / 45000.0;
        if total_seconds > 0.0 {
            for s in pl
                .video_streams
                .iter_mut()
                .chain(pl.audio_streams.iter_mut())
                .chain(pl.graphics_streams.iter_mut())
                .chain(pl.text_streams.iter_mut())
            {
                if s.measured_size > 0 {
                    let active = (s.measured_size as f64 * 8.0 / total_seconds).round() as u64;
                    s.active_bit_rate = active;
                    if s.is_vbr || s.bit_rate == 0 {
                        s.bit_rate = active;
                    }
                }
            }
        }

        // Recompute description so newly populated PGS caption counts and
        // refined audio bit rates surface in the UI.
        for s in pl
            .video_streams
            .iter_mut()
            .chain(pl.audio_streams.iter_mut())
            .chain(pl.graphics_streams.iter_mut())
            .chain(pl.text_streams.iter_mut())
        {
            codec::finalize_description(s);
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
    if dst.bit_rate == 0 && src.bit_rate > 0 {
        dst.bit_rate = src.bit_rate;
    }
    if dst.active_bit_rate == 0 && src.active_bit_rate > 0 {
        dst.active_bit_rate = src.active_bit_rate;
    }
}
