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

use std::collections::{HashMap, HashSet};
use std::io::{Read, Result as IoResult};
use std::time::{Duration, Instant};

use crate::bdrom::codec;
use crate::bdrom::codec_cache::{clpi_stream_descriptor, codec_cache};
use crate::bdrom::disc_info::{
  clpi_file_for_clip, clpi_stream_to_info, is_ssif_mvc_stream, reference_clip_name_for_playlist,
};
use crate::bdrom::m2ts;
use crate::bdrom::model::{BDRom, effective_stream_source, open_stream_reader};
use crate::bdrom::types::*;
use crate::protocol::{DiscInfo, TSStreamInfo};

/// Reader used by the lightweight phase. The deadline is shared by every
/// clip on the disc, so a pathological or slow stream cannot turn the fast
/// scan into an accidental full scan.
struct DeadlineReader<R> {
  inner: R,
  deadline: Instant,
}

impl<R> DeadlineReader<R> {
  fn new(inner: R, deadline: Instant) -> Self {
    Self { inner, deadline }
  }
}

impl<R: Read> Read for DeadlineReader<R> {
  fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
    if Instant::now() >= self.deadline {
      return Ok(0);
    }
    // Keep each grant small enough that scan_inner returns to the deadline
    // gate frequently even though it normally asks for a 5 MiB chunk.
    const DEADLINE_CHUNK: usize = 192 * 256;
    let limit = buf.len().min(DEADLINE_CHUNK);
    self.inner.read(&mut buf[..limit])
  }
}

/// Run a one-shot codec init pass over every unique clip on the disc,
/// including alternate-angle files.
/// For each clip we open the M2TS reader, dispatch reassembled PES payloads
/// to the matching codec parser, and stop reading the moment every PMT-
/// listed PID has reported `is_initialized` (mirrors BDInfo's
/// `ScanStream` finish condition over `Streams.Values`). Codec-derived
/// fields populated during the scan are then snapshotted and copied to
/// every other playlist that references the same clip.
pub(crate) fn codec_init(disc: &mut DiscInfo, bd: &BDRom) {
  use codec::CodecScanState;

  let fast_scan_seconds = crate::config::get_config().scan.fast_scan_seconds.clamp(1, 3600);
  let deadline = Instant::now() + Duration::from_secs(fast_scan_seconds as u64);

  /// Codec-init result captured per unique clip. `codec_metadata` is the
  /// snapshot of every PID's TSStreamInfo after the codec parsers ran
  /// (taken via the same raw pointers used during the scan, so it always
  /// reflects the mutated state). `per_pid_bytes` and `duration_seconds`
  /// are the partial-scan running totals used to estimate bit rate for
  /// VBR streams the codec parser can't pin down.
  struct ClipInitCache {
    codec_metadata: HashMap<u16, TSStreamInfo>,
    per_pid_bytes: HashMap<u16, u64>,
    duration_seconds: f64,
  }

  // Phase A.1: collect every playlist index that references each unique
  // main- or alternate-angle clip. We need the union (not just one "lead") because
  // playlists can subset streams differently — a PID present in this
  // clip's PMT might only appear in a non-lead playlist's MPLS.
  let mut clip_referencing_plis: HashMap<String, Vec<usize>> = HashMap::new();
  for (pli, pl) in disc.playlists.iter().enumerate() {
    for clip in &pl.stream_clips {
      let entry = clip_referencing_plis.entry(clip.name.clone()).or_default();
      if !entry.contains(&pli) {
        entry.push(pli);
      }
    }
  }

  // Phase A.2: scan each unique clip until codecs are initialized, reusing
  // the per-disc codec cache so each distinct stream is read only once.
  //
  // A "play-all" / menu-loop playlist can reference hundreds of clips that
  // all carry the same streams; reading every one of them just to re-confirm
  // a codec we already identified meant reading the better part of a gigabyte
  // during what should be a lightweight open. Before reading a clip we look
  // up its streams (from its CLPI, which is already parsed, so no M2TS read)
  // in the cache: if every one is present, we serve the clip from the cache.
  // After reading a clip we store every one of its streams, so a sibling clip
  // that only repeats them is free — even streams whose codec never
  // initialized are cached, so a padding/fake file can't force re-reads.
  let disc_path = disc.path.clone();
  // Scan reference clips first. They are the source BDInfo uses when it
  // merges codec-level metadata into a playlist, and prioritizing them also
  // makes a short deadline deterministic and useful.
  let reference_clips: HashSet<String> = bd
    .playlists
    .values()
    .filter_map(|pl| reference_clip_name_for_playlist(pl, bd))
    .collect();
  let mut clips_to_scan: Vec<(String, Vec<usize>)> = clip_referencing_plis.into_iter().collect();
  clips_to_scan.sort_by(|(a, _), (b, _)| {
    let a_reference = reference_clips.contains(a);
    let b_reference = reference_clips.contains(b);
    b_reference.cmp(&a_reference).then_with(|| a.cmp(b))
  });

  let mut clip_cache: HashMap<String, ClipInitCache> = HashMap::new();
  for (clip_name, plis) in &clips_to_scan {
    if Instant::now() >= deadline {
      break;
    }
    // Cache lookup: if this clip's CLPI lists only streams we've already
    // scanned (in the same playlist set), reuse their cached codec metadata
    // and don't read the M2TS.
    if let Some(scf) = clpi_file_for_clip(bd, clip_name) {
      if !scf.streams.is_empty() {
        let cached = {
          let cache = codec_cache().lock().unwrap_or_else(|e| e.into_inner());
          if scf
            .streams
            .iter()
            .all(|s| cache.contains_key(&(disc_path.clone(), plis.clone(), clpi_stream_descriptor(s))))
          {
            let meta: HashMap<u16, TSStreamInfo> = scf
              .streams
              .iter()
              .filter_map(|s| {
                cache
                  .get(&(disc_path.clone(), plis.clone(), clpi_stream_descriptor(s)))
                  .map(|m| (s.pid, m.clone()))
              })
              .collect();
            Some(meta)
          } else {
            None
          }
        };
        if let Some(codec_metadata) = cached {
          clip_cache.insert(
            clip_name.clone(),
            ClipInitCache {
              codec_metadata,
              per_pid_bytes: HashMap::new(),
              duration_seconds: 0.0,
            },
          );
          continue;
        }
      }
    }

    let entry = match effective_stream_source(bd, clip_name) {
      Some(e) => e,
      None => continue,
    };

    // Build a single PID -> *mut TSStreamInfo table merged across every
    // playlist that references this clip. First playlist with a given
    // PID wins; the codec parser will mutate that one stream and we'll
    // distribute its codec metadata to all other playlists in Phase B.
    let mut pid_state: HashMap<u16, CodecScanState> = HashMap::new();
    let mut pid_streams: HashMap<u16, *mut TSStreamInfo> = HashMap::new();
    for &pli in plis {
      let pl = &mut disc.playlists[pli];
      let angle_indices: Vec<u32> = pl
        .stream_clips
        .iter()
        .filter(|clip| clip.name == *clip_name)
        .map(|clip| clip.angle_index)
        .collect();
      if angle_indices.contains(&0) {
        for s in pl.video_streams.iter_mut() {
          pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
        }
        for s in pl.audio_streams.iter_mut() {
          pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
        }
        for s in pl.graphics_streams.iter_mut() {
          pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
        }
        for s in pl.text_streams.iter_mut() {
          pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
        }
      }
      for angle_index in angle_indices.into_iter().filter(|index| *index > 0) {
        if let Some(streams) = pl.angle_streams.get_mut(angle_index as usize - 1) {
          for s in streams {
            pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
          }
        }
      }
    }
    if pid_streams.is_empty() {
      continue;
    }

    // BitRate hint passed to DTS / DTS-HD parsers (they accept a running
    // bitrate computed by the host). Seeded with the MPLS-derived value.
    let bitrate_hint: HashMap<u16, i64> = pid_streams
      .iter()
      .map(|(pid, p)| unsafe { (*pid, (**p).bit_rate as i64) })
      .collect();

    let reader = match open_stream_reader(bd, &entry.0) {
      Ok(r) => r,
      Err(err) => {
        log::warn!("codec scan {}: {}", clip_name, err);
        continue;
      }
    };

    let reader = DeadlineReader::new(reader, deadline);

    // PMT may declare PIDs that no playlist's MPLS references — those
    // are "hidden" tracks (BDInfo's TSPlaylistFile.cs sets IsHidden=true
    // for any clip stream not in PlaylistStreams). We allocate synthetic
    // TSStreamInfo entries for them on first PES so the codec parser
    // can populate their format fields the same way it does for the
    // real ones. Phase B then attaches a copy to every playlist that
    // doesn't declare the PID.
    let mut synthetic_holders: HashMap<u16, Box<TSStreamInfo>> = HashMap::new();

    let res = m2ts::scan_m2ts_streaming_from_reader(reader, |pid, _stream_type, payload, pmt| {
      let target_ptr: Option<*mut TSStreamInfo> = if let Some(&ptr) = pid_streams.get(&pid) {
        Some(ptr)
      } else if let Some(&stream_type) = pmt.get(&pid) {
        // PMT-declared but not in any MPLS — synthesize.
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

      if let Some(ptr) = target_ptr {
        let stream = unsafe { &mut *ptr };
        if !stream.is_initialized {
          let state = pid_state.entry(pid).or_default();
          let bitrate = bitrate_hint.get(&pid).copied().unwrap_or(0);
          codec::scan_stream(stream, state, payload, bitrate, true, false);
        }
      }

      // BDInfo-style early-stop: terminate the moment every PMT-
      // listed PID has reported initialized — including hidden
      // ones we synthesized above (so their codec details get
      // captured before we exit). PIDs in PMT that haven't yet
      // delivered a PES are still pending; keep scanning.
      if pmt.is_empty() {
        return m2ts::PesAction::Continue;
      }
      let any_uninit = pmt.keys().any(|p| {
        pid_streams
          .get(p)
          .map(|ptr| unsafe { !(**ptr).is_initialized })
          .unwrap_or(true)
      });
      if any_uninit {
        m2ts::PesAction::Continue
      } else {
        m2ts::PesAction::Stop
      }
    });

    match res {
      Ok(r) => {
        let mut by_pid: HashMap<u16, u64> = HashMap::new();
        for (pid, stat) in &r.streams {
          by_pid.insert(*pid, stat.total_bytes);
        }
        let duration = r.duration_seconds;

        // Estimate bit_rate for VBR streams from running totals.
        // We mutate the very streams pid_streams pointed at, so the
        // snapshot taken below reflects these updates too.
        if duration > 0.0 {
          for (pid, ptr) in &pid_streams {
            if let Some(b) = by_pid.get(pid) {
              let active = (*b as f64 * 8.0 / duration).round() as u64;
              unsafe {
                let s = &mut **ptr;
                s.active_bit_rate = active;
                if s.is_vbr || s.bit_rate == 0 {
                  s.bit_rate = active;
                }
              }
            }
          }
        }

        // Snapshot codec metadata via the same raw pointers so we
        // capture whichever playlist owned the mutated stream.
        let mut codec_metadata: HashMap<u16, TSStreamInfo> = HashMap::new();
        for (pid, ptr) in &pid_streams {
          unsafe {
            codec_metadata.insert(*pid, (**ptr).clone());
          }
        }

        // Populate the per-disc cache with every stream this clip's CLPI
        // lists, so a sibling clip that only repeats them is served without a
        // read. Use the freshly scanned metadata when we have it, else a
        // CLPI-derived placeholder — caching even an uninitialized stream so
        // it can't force the clip to be rescanned by a sibling.
        if let Some(scf) = clpi_file_for_clip(bd, clip_name) {
          let mut cache = codec_cache().lock().unwrap_or_else(|e| e.into_inner());
          for s in &scf.streams {
            cache
              .entry((disc_path.clone(), plis.clone(), clpi_stream_descriptor(s)))
              .or_insert_with(|| {
                codec_metadata
                  .get(&s.pid)
                  .cloned()
                  .or_else(|| clpi_stream_to_info(s))
                  .unwrap_or_else(|| TSStreamInfo::new(s.pid, s.stream_type))
              });
          }
        }

        clip_cache.insert(
          clip_name.clone(),
          ClipInitCache {
            codec_metadata,
            per_pid_bytes: by_pid,
            duration_seconds: duration,
          },
        );
      }
      Err(err) => {
        log::warn!("codec scan {}: {}", clip_name, err);
      }
    }
  }

  // Phase B: distribute codec metadata. For PIDs the playlist already
  // declares in MPLS, copy codec details into the existing stream. For
  // PIDs that appeared in the clip's PMT but not in this playlist's MPLS
  // (BDInfo's "hidden" tracks), append a new is_hidden=true stream.
  for pl in disc.playlists.iter_mut() {
    // PIDs the playlist already has from MPLS (used to detect hidden).
    let mut declared_pids: HashSet<u16> = pl
      .video_streams
      .iter()
      .chain(pl.audio_streams.iter())
      .chain(pl.graphics_streams.iter())
      .chain(pl.text_streams.iter())
      .map(|s| s.pid)
      .collect();

    for clip in &pl.stream_clips {
      let cached = match clip_cache.get(&clip.name) {
        Some(c) => c,
        None => continue,
      };

      if clip.angle_index > 0 {
        if let Some(angle_streams) = pl.angle_streams.get_mut(clip.angle_index as usize - 1) {
          for stream in angle_streams {
            if stream.is_initialized {
              continue;
            }
            if let Some(meta) = cached.codec_metadata.get(&stream.pid) {
              if meta.is_initialized {
                copy_codec_metadata(stream, meta);
              }
            }
          }
        }
        continue;
      }

      // Update existing streams with codec details.
      for s in pl
        .video_streams
        .iter_mut()
        .chain(pl.audio_streams.iter_mut())
        .chain(pl.graphics_streams.iter_mut())
        .chain(pl.text_streams.iter_mut())
      {
        if s.is_initialized {
          continue;
        }
        if let Some(meta) = cached.codec_metadata.get(&s.pid) {
          if meta.is_initialized {
            copy_codec_metadata(s, meta);
          }
        }
      }

      // Add hidden streams for PMT PIDs not in this playlist's MPLS.
      for (pid, meta) in &cached.codec_metadata {
        if declared_pids.contains(pid) {
          continue;
        }
        if is_ssif_mvc_stream(bd, &clip.name, *pid, meta) {
          let mut mvc = meta.clone();
          mvc.is_hidden = false;
          if mvc.is_video_stream {
            pl.video_streams.push(mvc);
            declared_pids.insert(*pid);
          }
          continue;
        }
        let mut hidden = meta.clone();
        hidden.is_hidden = true;
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
          // Unknown stream type — drop.
          continue;
        }
        // Don't add the same hidden PID twice if multiple clips of
        // the playlist contain it.
        declared_pids.insert(*pid);
      }
    }
  }

  // For VBR streams that didn't get a fixed bit rate from the codec
  // parser, accumulate per-PID bytes across all clips of the playlist and
  // divide by total scanned seconds — gives a more representative running
  // average than any single clip's first few seconds.
  for pl in disc.playlists.iter_mut() {
    let mut per_pid_total_bytes: HashMap<u16, u64> = HashMap::new();
    let mut total_seconds: f64 = 0.0;
    for clip in &pl.stream_clips {
      if clip.angle_index != 0 {
        continue;
      }
      if let Some(cached) = clip_cache.get(&clip.name) {
        total_seconds += cached.duration_seconds;
        for (pid, bytes) in &cached.per_pid_bytes {
          *per_pid_total_bytes.entry(*pid).or_insert(0) += *bytes;
        }
      }
    }
    if total_seconds > 0.0 {
      for s in pl
        .video_streams
        .iter_mut()
        .chain(pl.audio_streams.iter_mut())
        .chain(pl.graphics_streams.iter_mut())
        .chain(pl.text_streams.iter_mut())
      {
        if let Some(b) = per_pid_total_bytes.get(&s.pid) {
          let active = (*b as f64 * 8.0 / total_seconds).round() as u64;
          s.active_bit_rate = active;
          if s.is_vbr || s.bit_rate == 0 {
            s.bit_rate = active;
          }
        }
      }
    }

    // Refine VBR video bit_rate using the playlist's total bandwidth.
    // The deadline-bounded codec-init sample can be biased toward whatever
    // happens in the first few seconds. Total bandwidth (angle-0 clip bytes ×
    // 8 / total length) is exact, and audio bit rates are mostly
    // codec-fixed and accurate — so the residual is a much better
    // estimate of the dominant VBR video stream's actual average.
    let total_length_s = pl.total_length as f64 / 45000.0;
    if total_length_s > 0.0 && !pl.video_streams.is_empty() {
      let mut angle0_bytes: u64 = 0;
      for c in &pl.stream_clips {
        if c.angle_index == 0 {
          angle0_bytes += c.file_size;
        }
      }
      if angle0_bytes > 0 {
        let total_bps = angle0_bytes as f64 * 8.0 / total_length_s;
        let non_video_bps: f64 = pl
          .audio_streams
          .iter()
          .chain(pl.graphics_streams.iter())
          .chain(pl.text_streams.iter())
          .map(|s| s.bit_rate as f64)
          .sum();
        let video_residual = total_bps - non_video_bps;
        if video_residual > 0.0 {
          let total_video_partial: f64 = pl.video_streams.iter().map(|s| s.bit_rate as f64).sum();
          if total_video_partial > 0.0 {
            // Multiple video streams (e.g. MVC + AVC for 3D):
            // split the residual proportionally to their
            // partial-scan averages.
            for s in pl.video_streams.iter_mut() {
              let proportion = s.bit_rate as f64 / total_video_partial;
              s.bit_rate = (video_residual * proportion).max(0.0) as u64;
            }
          } else {
            // Single uninitialized video stream — give it the
            // entire residual (still better than 0).
            let per_video = video_residual / pl.video_streams.len() as f64;
            for s in pl.video_streams.iter_mut() {
              s.bit_rate = per_video.max(0.0) as u64;
            }
          }
        }
      }
    }

    // Description is recomputed once all underlying fields are populated
    // so it reflects codec init + audio CoreStream linkage.
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
}

/// Copy codec-derived fields from the lead playlist's snapshot into a
/// sibling stream on a different playlist that shares the same underlying
/// clip + PID. Leaves measurement and language fields alone.
pub(crate) fn copy_codec_metadata(dst: &mut TSStreamInfo, src: &TSStreamInfo) {
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
  dst.captions = src.captions;
  dst.forced_captions = src.forced_captions;
  if dst.bit_rate == 0 && src.bit_rate > 0 {
    dst.bit_rate = src.bit_rate;
  }
  if dst.active_bit_rate == 0 && src.active_bit_rate > 0 {
    dst.active_bit_rate = src.active_bit_rate;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Cursor;

  #[test]
  fn deadline_reader_stops_before_reading_after_deadline() {
    let mut reader = DeadlineReader::new(Cursor::new(vec![1, 2, 3]), Instant::now());
    let mut buffer = [0u8; 3];
    assert_eq!(reader.read(&mut buffer).expect("deadline is EOF"), 0);
  }

  #[test]
  fn deadline_reader_delegates_before_deadline() {
    let mut reader = DeadlineReader::new(
      Cursor::new(vec![1, 2, 3]),
      Instant::now() + Duration::from_secs(1),
    );
    let mut buffer = [0u8; 3];
    assert_eq!(reader.read(&mut buffer).expect("read succeeds"), 3);
    assert_eq!(buffer, [1, 2, 3]);
  }
}
