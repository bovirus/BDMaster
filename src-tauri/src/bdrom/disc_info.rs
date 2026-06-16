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

use std::collections::HashSet;

use crate::bdrom::clpi::{ClpiStream, StreamClipFile};
use crate::bdrom::codec;
use crate::bdrom::lang::language_name;
use crate::bdrom::model::{BDRom, SSIF_MVC_PID};
use crate::bdrom::mpls::{PlaylistFile, PlaylistStream};
use crate::bdrom::types::*;
use crate::protocol::{
  DiscInfo, PlaylistInfo, PlaylistStreamClipInfo, StreamClipFileInfo, StreamFileInfo, TSStreamInfo,
};

pub(crate) fn to_disc_info(bd: &BDRom) -> DiscInfo {
  let scan_config = crate::config::get_config().scan;
  let path_str = bd.path.to_string_lossy().to_string();
  let disc_name = bd
    .path
    .file_name()
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_default();

  let has_hevc_streams = bd.playlists.values().any(|pl| {
    pl.playlist_streams
      .iter()
      .any(|s| s.stream_type == TSStreamType::HEVCVideo)
  });
  let has_mvc = bd.playlists.values().any(|pl| {
    pl.playlist_streams
      .iter()
      .any(|s| s.stream_type == TSStreamType::MVCVideo)
  });

  // Sort playlists by name and assign group indices. Two playlists belong
  // to the same group if they share at least one stream-clip name —
  // mirroring BDInfo's playlist grouping in FormMain.cs.
  let mut playlist_names: Vec<&String> = bd
    .playlists
    .iter()
    .filter_map(|(name, pl)| {
      if playlist_is_valid_for_scan(
        pl,
        scan_config.filter_looping_playlists,
        scan_config.filter_short_playlists,
        scan_config.filter_short_playlists_value,
      ) {
        Some(name)
      } else {
        None
      }
    })
    .collect();
  playlist_names.sort();
  let mut groups: Vec<Vec<&String>> = Vec::new();
  let mut group_index_by_name: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
  for name in &playlist_names {
    let pl = match bd.playlists.get(*name) {
      Some(p) => p,
      None => continue,
    };
    let mut matched: Option<usize> = None;
    'outer: for (gi, group) in groups.iter().enumerate() {
      for other_name in group {
        if let Some(other) = bd.playlists.get(*other_name) {
          for c1 in &pl.stream_clips {
            for c2 in &other.stream_clips {
              if c1.name == c2.name {
                matched = Some(gi);
                break 'outer;
              }
            }
          }
        }
      }
    }
    match matched {
      Some(gi) => groups[gi].push(*name),
      None => groups.push(vec![*name]),
    }
  }
  for (gi, group) in groups.iter().enumerate() {
    for name in group {
      group_index_by_name.insert((*name).clone(), (gi + 1) as u32);
    }
  }

  let playlists: Vec<PlaylistInfo> = playlist_names
    .iter()
    .map(|name| {
      let group = group_index_by_name.get(*name).copied().unwrap_or(0);
      build_playlist_info(bd.playlists.get(*name).unwrap(), &bd, group)
    })
    .collect();

  // Stream files (sorted). `interleaved=true` marks clips with an SSIF
  // counterpart so the UI can flag them regardless of whether SSIF mode is
  // currently active.
  let mut stream_files: Vec<StreamFileInfo> = bd
    .stream_files
    .iter()
    .map(|(name, (_, size))| {
      let interleaved_file_size = bd.interleaved_files.get(name).map(|(_, s)| *s).unwrap_or(0);
      StreamFileInfo {
        name: name.clone(),
        display_name: stream_display_name(bd, name),
        size: *size,
        interleaved_file_size,
        duration: 0,
        interleaved: interleaved_file_size > 0,
      }
    })
    .collect();
  stream_files.sort_by(|a, b| a.name.cmp(&b.name));

  let mut stream_clip_files: Vec<StreamClipFileInfo> = bd
    .stream_clip_files
    .values()
    .map(|c| StreamClipFileInfo {
      name: c.name.clone(),
      size: c.size,
    })
    .collect();
  stream_clip_files.sort_by(|a, b| a.name.cmp(&b.name));

  let is_4k = bd.is_uhd
    || bd.playlists.values().any(|pl| {
      pl.playlist_streams
        .iter()
        .any(|s| s.video_format == TSVideoFormat::Video2160p)
    });

  DiscInfo {
    path: path_str,
    disc_name,
    disc_title: bd.disc_title.clone().unwrap_or_default(),
    volume_label: bd.volume_label.clone(),
    size: bd.size,
    is_bd_plus: bd.is_bd_plus,
    is_bd_java: bd.is_bd_java,
    is_3d: bd.is_3d,
    is_4k,
    is_50hz: bd.is_50_hz,
    is_dbox: bd.is_dbox,
    is_psp: bd.is_psp,
    is_uhd: bd.is_uhd,
    has_mvc_extension: has_mvc,
    has_hevc_streams: has_hevc_streams,
    has_uhd_disc_marker: bd.is_uhd,
    meta_title: bd.disc_title.clone(),
    meta_disc_number: None,
    file_set_identifier: None,
    playlists,
    stream_files,
    stream_clip_files,
  }
}

pub(crate) fn build_playlist_info(pl: &PlaylistFile, bd: &BDRom, group_index: u32) -> PlaylistInfo {
  // Compute clip lengths and total length using only angle 0 clips.
  let mut total_length_45k: i64 = 0;
  let mut total_file_size: u64 = 0;
  let mut clips: Vec<PlaylistStreamClipInfo> = Vec::new();

  let mut relative_time_in: i64 = 0;
  for c in &pl.stream_clips {
    let length = (c.time_out - c.time_in).max(0);
    let m2ts_size = bd.stream_files.get(&c.name).map(|(_, s)| *s).unwrap_or(0);
    let interleaved_file_size = bd.interleaved_files.get(&c.name).map(|(_, s)| *s).unwrap_or(0);
    // When SSIF mode is on and the clip has an interleaved counterpart,
    // the "scanned size" is the SSIF — that's what `effective_stream_source`
    // hands back, that's what gets measured during the full scan, and
    // that's what the BDInfo "Size" column shows. Fall back to the M2TS
    // size in every other case.
    let file_size = if bd.use_ssif && interleaved_file_size > 0 {
      interleaved_file_size
    } else {
      m2ts_size
    };
    total_file_size += file_size;
    let info = PlaylistStreamClipInfo {
      name: c.name.clone(),
      display_name: stream_display_name(bd, &c.name),
      time_in: c.time_in as u64,
      time_out: c.time_out as u64,
      relative_time_in: relative_time_in.max(0) as u64,
      relative_time_out: (relative_time_in + length).max(0) as u64,
      length: length as u64,
      file_size,
      measured_size: 0,
      interleaved_file_size,
      angle_index: c.angle_index,
    };
    if c.angle_index == 0 {
      total_length_45k += length;
      relative_time_in += length;
    }
    clips.push(info);
  }

  let mut video_streams = Vec::new();
  let mut audio_streams = Vec::new();
  let mut graphics_streams = Vec::new();
  let mut text_streams = Vec::new();
  // Reference angle-0 clip, used to cross-check stream language codes and
  // hidden streams against CLPI when MPLS leaves metadata out. BDInfo
  // chooses the richest/longest clip rather than blindly using the first.
  let reference_clip = reference_clip_name_for_playlist(pl, bd);
  let mut has_hidden_tracks = false;

  for s in &pl.playlist_streams {
    let mut info = playlist_stream_to_info(s);
    if info.language_code.is_empty() {
      if let Some(clip_name) = &reference_clip {
        if let Some(code) = clpi_language_for(bd, clip_name, s.pid) {
          info.language_name = language_name(&code);
          info.language_code = code;
        }
      }
    }
    if s.stream_type.is_video() {
      video_streams.push(info);
    } else if s.stream_type.is_audio() {
      audio_streams.push(info);
    } else if s.stream_type.is_graphics() {
      graphics_streams.push(info);
    } else if s.stream_type.is_text() {
      text_streams.push(info);
    }
  }

  let declared_pids: HashSet<u16> = pl.playlist_streams.iter().map(|s| s.pid).collect();
  if let Some(clip_name) = &reference_clip {
    if let Some(clpi) = clpi_file_for_clip(bd, clip_name) {
      for stream in &clpi.streams {
        if declared_pids.contains(&stream.pid) {
          continue;
        }
        let Some(mut info) = clpi_stream_to_info(stream) else {
          continue;
        };
        info.is_hidden = true;
        has_hidden_tracks = true;
        if info.is_video_stream {
          video_streams.push(info);
        } else if info.is_audio_stream {
          audio_streams.push(info);
        } else if info.is_graphics_stream {
          graphics_streams.push(info);
        } else if info.is_text_stream {
          text_streams.push(info);
        }
      }
    }
  }

  PlaylistInfo {
    name: pl.name.clone(),
    group_index,
    file_size: total_file_size,
    measured_size: 0,
    total_length: total_length_45k.max(0) as u64,
    has_hidden_tracks,
    has_loops: playlist_has_loops(pl),
    is_custom: false,
    chapters: pl.chapters.clone(),
    chapter_metrics: Vec::new(),
    bitrate_samples: Vec::new(),
    stream_clips: clips,
    video_streams,
    audio_streams,
    graphics_streams,
    text_streams,
    total_angles: pl.angle_count,
  }
}

pub(crate) fn playlist_is_valid_for_scan(
  pl: &PlaylistFile,
  filter_looping_playlists: bool,
  filter_short_playlists: bool,
  filter_short_playlists_value: u32,
) -> bool {
  if filter_short_playlists {
    let total_seconds = playlist_total_length_45k(pl) as f64 / 45000.0;
    if total_seconds < filter_short_playlists_value as f64 {
      return false;
    }
  }

  if filter_looping_playlists && playlist_has_loops(pl) {
    return false;
  }

  true
}

pub(crate) fn playlist_total_length_45k(pl: &PlaylistFile) -> i64 {
  pl.stream_clips
    .iter()
    .filter(|c| c.angle_index == 0)
    .map(|c| (c.time_out - c.time_in).max(0))
    .sum()
}

pub(crate) fn playlist_has_loops(pl: &PlaylistFile) -> bool {
  let mut clip_times: HashSet<(String, i64)> = HashSet::new();
  for clip in pl.stream_clips.iter().filter(|c| c.angle_index == 0) {
    if !clip_times.insert((clip.name.clone(), clip.time_in)) {
      return true;
    }
  }
  false
}

/// Look up a stream's language code from the matching CLPI clip's program-info
/// table by PID. Used only as a fallback when MPLS supplies no language code.
pub(crate) fn clpi_language_for(bd: &BDRom, clip_name: &str, pid: u16) -> Option<String> {
  let scf = clpi_file_for_clip(bd, clip_name)?;
  scf
    .streams
    .iter()
    .find(|s| s.pid == pid && !s.language_code.is_empty())
    .map(|s| s.language_code.clone())
}

pub(crate) fn clpi_file_for_clip<'a>(bd: &'a BDRom, clip_name: &str) -> Option<&'a StreamClipFile> {
  let stem = clip_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(clip_name);
  let clpi_name = format!("{}.CLPI", stem.to_uppercase());
  let scf = bd.stream_clip_files.get(&clpi_name)?;
  scf.is_valid.then_some(scf)
}

pub(crate) fn reference_clip_name_for_playlist(pl: &PlaylistFile, bd: &BDRom) -> Option<String> {
  let mut best: Option<(String, usize, i64)> = None;
  for clip in pl.stream_clips.iter().filter(|c| c.angle_index == 0) {
    let stream_count = clpi_file_for_clip(bd, &clip.name).map(|c| c.streams.len()).unwrap_or(0);
    let length = (clip.time_out - clip.time_in).max(0);
    match &best {
      Some((_, best_count, best_length))
        if stream_count < *best_count || (stream_count == *best_count && length <= *best_length) => {}
      _ => best = Some((clip.name.clone(), stream_count, length)),
    }
  }
  best.map(|(name, _, _)| name)
}

pub(crate) fn clpi_stream_to_info(stream: &ClpiStream) -> Option<TSStreamInfo> {
  let stream_type = TSStreamType::from_u8(stream.stream_type);
  if stream_type == TSStreamType::Unknown {
    return None;
  }

  let playlist_stream = PlaylistStream {
    pid: stream.pid,
    stream_type,
    video_format: TSVideoFormat::from_u8(stream.video_format),
    frame_rate: TSFrameRate::from_u8(stream.frame_rate),
    aspect_ratio: TSAspectRatio::from_u8(stream.aspect_ratio),
    channel_layout: TSChannelLayout::from_u8(stream.channel_layout),
    sample_rate_hz: stream.sample_rate,
    language_code: stream.language_code.clone(),
  };
  Some(playlist_stream_to_info(&playlist_stream))
}

pub(crate) fn playlist_stream_to_info(s: &PlaylistStream) -> TSStreamInfo {
  let mut info = TSStreamInfo::new(s.pid, s.stream_type as u8);
  info.stream_type_text = s.stream_type.type_text().to_string();
  info.codec_name = s.stream_type.codec_name().to_string();
  info.codec_short_name = s.stream_type.codec_short_name().to_string();
  info.is_video_stream = s.stream_type.is_video();
  info.is_audio_stream = s.stream_type.is_audio();
  info.is_graphics_stream = s.stream_type.is_graphics();
  info.is_text_stream = s.stream_type.is_text();
  info.language_code = s.language_code.trim_end_matches('\0').to_string();
  info.language_name = language_name(&info.language_code);

  if s.stream_type.is_video() {
    info.height = s.video_format.height();
    info.is_interlaced = s.video_format.is_interlaced();
    info.framerate = s.frame_rate.label().to_string();
    info.aspect_ratio = s.aspect_ratio.label().to_string();
    info.video_format = format!("{}{}", info.height, if info.is_interlaced { "i" } else { "p" });
    // Approx widths from common heights:
    info.width = match info.height {
      480 => 720,
      576 => 720,
      720 => 1280,
      1080 => 1920,
      2160 => 3840,
      _ => 0,
    };

    let mut desc_parts: Vec<String> = Vec::new();
    if info.height > 0 {
      desc_parts.push(format!("{}{}", info.height, if info.is_interlaced { "i" } else { "p" }));
    }
    if !info.framerate.is_empty() {
      desc_parts.push(format!("{} fps", info.framerate));
    }
    if !info.aspect_ratio.is_empty() {
      desc_parts.push(info.aspect_ratio.clone());
    }
    info.description = desc_parts.join(" / ");
  }

  if s.stream_type.is_audio() {
    info.channel_layout = s.channel_layout.label().to_string();
    info.sample_rate = s.sample_rate_hz;

    let mut desc_parts: Vec<String> = Vec::new();
    if !info.channel_layout.is_empty() {
      desc_parts.push(info.channel_layout.clone());
    }
    if info.sample_rate > 0 {
      desc_parts.push(format!("{} kHz", info.sample_rate / 1000));
    }
    info.description = desc_parts.join(" / ");
  }

  info
}

pub(crate) fn stream_display_name(bd: &BDRom, clip_name: &str) -> String {
  if bd.use_ssif && bd.interleaved_files.contains_key(clip_name) {
    let stem = clip_name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(clip_name);
    format!("{}.SSIF", stem)
  } else {
    clip_name.to_string()
  }
}

pub(crate) fn is_ssif_mvc_stream(bd: &BDRom, clip_name: &str, pid: u16, stream: &TSStreamInfo) -> bool {
  bd.use_ssif
    && pid == SSIF_MVC_PID
    && bd.interleaved_files.contains_key(clip_name)
    && TSStreamType::from_u8(stream.stream_type) == TSStreamType::MVCVideo
}

pub(crate) fn refresh_ssif_derived_metadata(disc: &mut DiscInfo, bd: &BDRom) {
  if bd.is_3d {
    for pl in disc.playlists.iter_mut() {
      let Some(src) = bd.playlists.get(&pl.name) else {
        continue;
      };
      if pl.video_streams.len() <= 1 {
        continue;
      }

      for stream in pl.video_streams.iter_mut() {
        match TSStreamType::from_u8(stream.stream_type) {
          TSStreamType::AVCVideo => stream.base_view = Some(src.mvc_base_view_r),
          TSStreamType::MVCVideo => stream.base_view = Some(!src.mvc_base_view_r),
          _ => {}
        }
        codec::finalize_description(stream);
      }
    }
  }

  recompute_mvc_extension(disc);
}

pub(crate) fn recompute_mvc_extension(disc: &mut DiscInfo) {
  disc.has_mvc_extension = disc.playlists.iter().any(|pl| {
    pl.video_streams
      .iter()
      .any(|s| TSStreamType::from_u8(s.stream_type) == TSStreamType::MVCVideo)
  });
}

pub(crate) fn cache_estimated_stream_sizes(disc: &mut DiscInfo) {
  for pl in disc.playlists.iter_mut() {
    let total_seconds = pl.total_length as f64 / 45000.0;
    for stream in pl
      .video_streams
      .iter_mut()
      .chain(pl.audio_streams.iter_mut())
      .chain(pl.graphics_streams.iter_mut())
      .chain(pl.text_streams.iter_mut())
    {
      stream.estimated_size = estimate_stream_size(stream, total_seconds);
    }
  }
}

pub(crate) fn estimate_stream_size(stream: &TSStreamInfo, total_seconds: f64) -> u64 {
  let bit_rate = if stream.bit_rate > 0 {
    stream.bit_rate
  } else {
    stream.active_bit_rate
  };
  if bit_rate > 0 && total_seconds > 0.0 {
    (bit_rate as f64 * total_seconds / 8.0).round() as u64
  } else {
    0
  }
}
