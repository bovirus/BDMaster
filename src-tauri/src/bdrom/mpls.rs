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
 * MPLS (Movie Playlist) parser. Port of TSPlaylistFile.cs Scan().
 */

use anyhow::{Result, anyhow};
use std::path::Path;

use super::types::*;

#[derive(Debug, Clone)]
pub struct PlaylistFile {
  pub name: String,
  pub file_type: String,
  pub mvc_base_view_r: bool,
  pub stream_clips: Vec<PlaylistStreamClip>,
  pub chapters: Vec<f64>,
  pub angle_count: u32,
  pub playlist_streams: Vec<PlaylistStream>,
}

#[derive(Debug, Clone)]
pub struct PlaylistStreamClip {
  pub name: String,
  pub time_in: i64,  // 45kHz units
  pub time_out: i64, // 45kHz units
  pub angle_index: u32,
}

#[derive(Debug, Clone)]
pub struct PlaylistStream {
  pub pid: u16,
  pub stream_type: TSStreamType,
  pub video_format: TSVideoFormat,
  pub frame_rate: TSFrameRate,
  pub aspect_ratio: TSAspectRatio,
  pub channel_layout: TSChannelLayout,
  pub sample_rate_hz: u32,
  pub language_code: String,
}

impl PlaylistStream {
  fn new(pid: u16, stream_type: TSStreamType) -> Self {
    Self {
      pid,
      stream_type,
      video_format: TSVideoFormat::Unknown,
      frame_rate: TSFrameRate::Unknown,
      aspect_ratio: TSAspectRatio::Unknown,
      channel_layout: TSChannelLayout::Unknown,
      sample_rate_hz: 0,
      language_code: String::new(),
    }
  }
}

struct Reader<'a> {
  data: &'a [u8],
  pos: usize,
}

impl<'a> Reader<'a> {
  fn new(data: &'a [u8]) -> Self {
    Self { data, pos: 0 }
  }

  fn read_u8(&mut self) -> Result<u8> {
    let v = *self.data.get(self.pos).ok_or_else(|| anyhow!("eof at {}", self.pos))?;
    self.pos += 1;
    Ok(v)
  }

  fn read_u16(&mut self) -> Result<u16> {
    if self.pos + 2 > self.data.len() {
      return Err(anyhow!("eof"));
    }
    let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
    self.pos += 2;
    Ok(v)
  }

  fn read_u32(&mut self) -> Result<u32> {
    if self.pos + 4 > self.data.len() {
      return Err(anyhow!("eof"));
    }
    let v = u32::from_be_bytes([
      self.data[self.pos],
      self.data[self.pos + 1],
      self.data[self.pos + 2],
      self.data[self.pos + 3],
    ]);
    self.pos += 4;
    Ok(v)
  }

  fn read_string(&mut self, len: usize) -> Result<String> {
    if self.pos + len > self.data.len() {
      return Err(anyhow!("eof"));
    }
    let s = String::from_utf8_lossy(&self.data[self.pos..self.pos + len]).to_string();
    self.pos += len;
    Ok(s)
  }

  fn seek(&mut self, pos: usize) -> Result<()> {
    if pos > self.data.len() {
      return Err(anyhow!("eof at {}", pos));
    }
    self.pos = pos;
    Ok(())
  }

  fn skip(&mut self, len: usize) -> Result<()> {
    self.seek(self.pos.saturating_add(len))
  }
}

pub fn parse_mpls(path: &Path) -> Result<PlaylistFile> {
  let data = std::fs::read(path)?;
  parse_mpls_bytes(
    path
      .file_name()
      .map(|n| n.to_string_lossy().to_uppercase())
      .unwrap_or_default(),
    &data,
  )
}

pub fn parse_mpls_bytes(name: String, data: &[u8]) -> Result<PlaylistFile> {
  let mut r = Reader::new(data);
  let file_type = r.read_string(8)?;
  if file_type != "MPLS0100" && file_type != "MPLS0200" && file_type != "MPLS0300" {
    return Err(anyhow!("unknown MPLS file type: {}", file_type));
  }

  let playlist_offset = r.read_u32()? as usize;
  let chapters_offset = r.read_u32()? as usize;
  let _extensions_offset = r.read_u32()? as usize;

  // misc flags @ 0x38
  r.seek(0x38)?;
  let misc_flags = r.read_u8()?;
  let mvc_base_view_r = (misc_flags & 0x10) != 0;

  // Playlist
  r.seek(playlist_offset)?;
  let _playlist_length = r.read_u32()?;
  let _reserved = r.read_u16()?;
  let item_count = r.read_u16()?;
  let _subitem_count = r.read_u16()?;

  let mut stream_clips: Vec<PlaylistStreamClip> = Vec::new();
  let mut playlist_streams: Vec<PlaylistStream> = Vec::new();
  let mut angle_count: u32 = 0;
  let mut cumulative_main_length: i64 = 0;

  for _ in 0..item_count {
    let item_start = r.pos;
    let item_length = r.read_u16()? as usize;
    let item_name = r.read_string(5)?;
    let _item_type = r.read_string(4)?;

    // skip 1 byte
    r.skip(1)?;
    let flags = r.read_u8()?;
    let multiangle = (flags >> 4) & 0x01;
    let _condition = flags & 0x0F;
    r.skip(1)?;

    let in_time = r.read_u32()? as i64;
    let in_time = if (in_time as i32) < 0 {
      in_time & 0x7FFFFFFF
    } else {
      in_time
    };

    let out_time = r.read_u32()? as i64;
    let out_time = if (out_time as i32) < 0 {
      out_time & 0x7FFFFFFF
    } else {
      out_time
    };

    let stream_clip = PlaylistStreamClip {
      name: format!("{}.M2TS", item_name.trim_end_matches('\0')),
      time_in: in_time,
      time_out: out_time,
      angle_index: 0,
    };
    stream_clips.push(stream_clip.clone());

    // skip 12 bytes
    r.skip(12)?;
    if multiangle > 0 {
      let angles = r.read_u8()? as i32;
      r.skip(1)?;
      let extra_angles = (angles - 1).max(0) as u32;
      for angle in 0..extra_angles {
        let angle_name = r.read_string(5)?;
        let _angle_type = r.read_string(4)?;
        r.skip(1)?;
        let angle_clip = PlaylistStreamClip {
          name: format!("{}.M2TS", angle_name.trim_end_matches('\0')),
          time_in: in_time,
          time_out: out_time,
          angle_index: angle + 1,
        };
        stream_clips.push(angle_clip);
      }
      angle_count = angle_count.max(extra_angles);
    }

    // STN_table
    let _stn_length = r.read_u16()?;
    r.skip(2)?;
    let stream_count_video = r.read_u8()? as i32;
    let stream_count_audio = r.read_u8()? as i32;
    let stream_count_pg = r.read_u8()? as i32;
    let stream_count_ig = r.read_u8()? as i32;
    let stream_count_secondary_audio = r.read_u8()? as i32;
    let stream_count_secondary_video = r.read_u8()? as i32;
    let _stream_count_pip = r.read_u8()? as i32;
    r.skip(5)?;

    // BDInfo's `RelativeLength` is the current play item's duration divided
    // by the preceding angle-0 playlist duration at this point. Metadata
    // from an already-known PID is replaced only when that ratio is > 1%.
    let clip_length = (out_time - in_time).max(0);
    let significant_clip = clip_is_significant(clip_length, cumulative_main_length);
    cumulative_main_length = cumulative_main_length.saturating_add(clip_length);

    for _ in 0..stream_count_video {
      if let Some(s) = create_stream(data, &mut r.pos)? {
        add_stream_metadata(&mut playlist_streams, s, significant_clip);
      }
    }
    for _ in 0..stream_count_audio {
      if let Some(s) = create_stream(data, &mut r.pos)? {
        add_stream_metadata(&mut playlist_streams, s, significant_clip);
      }
    }
    for _ in 0..stream_count_pg {
      if let Some(s) = create_stream(data, &mut r.pos)? {
        add_stream_metadata(&mut playlist_streams, s, significant_clip);
      }
    }
    for _ in 0..stream_count_ig {
      if let Some(s) = create_stream(data, &mut r.pos)? {
        add_stream_metadata(&mut playlist_streams, s, significant_clip);
      }
    }
    for _ in 0..stream_count_secondary_audio {
      let s = create_stream(data, &mut r.pos)?;
      // BDInfo skips the secondary-audio extension (pos += 2) after every
      // entry, whether or not a stream was produced.
      r.skip(2)?;
      if let Some(s) = s {
        add_stream_metadata(&mut playlist_streams, s, significant_clip);
      }
    }
    for _ in 0..stream_count_secondary_video {
      let s = create_stream(data, &mut r.pos)?;
      // Likewise the secondary-video extension is always skipped (pos += 6).
      r.skip(6)?;
      if let Some(s) = s {
        add_stream_metadata(&mut playlist_streams, s, significant_clip);
      }
    }

    // Skip rest of item
    let consumed = r.pos - item_start;
    let total = item_length + 2;
    if total > consumed {
      r.skip(total - consumed)?;
    }
  }

  // Chapters
  let mut raw_chapters: Vec<(usize, u64)> = Vec::new();
  if chapters_offset + 6 <= data.len() {
    r.seek(chapters_offset + 4)?;
    let chapter_count = r.read_u16()? as usize;
    for _ in 0..chapter_count {
      if r.pos + 14 > data.len() {
        break;
      }
      let chapter_type = data[r.pos + 1];
      if chapter_type == 1 {
        let _stream_file_index = ((data[r.pos + 2] as u16) << 8) | data[r.pos + 3] as u16;
        let chapter_time: u64 = ((data[r.pos + 4] as u64) << 24)
          | ((data[r.pos + 5] as u64) << 16)
          | ((data[r.pos + 6] as u64) << 8)
          | (data[r.pos + 7] as u64);
        raw_chapters.push((_stream_file_index as usize, chapter_time));
      }
      r.skip(14)?;
    }
  }
  let chapters = playlist_relative_chapters(&stream_clips, &raw_chapters);

  Ok(PlaylistFile {
    name,
    file_type,
    mvc_base_view_r,
    stream_clips,
    chapters,
    angle_count,
    playlist_streams,
  })
}

fn clip_is_significant(clip_length: i64, cumulative_main_length: i64) -> bool {
  cumulative_main_length == 0 || clip_length as f64 / cumulative_main_length as f64 > 0.01
}

fn add_stream_metadata(list: &mut Vec<PlaylistStream>, s: PlaylistStream, significant_clip: bool) {
  if let Some(existing) = list.iter_mut().find(|x| x.pid == s.pid) {
    if significant_clip {
      *existing = s;
    }
  } else {
    list.push(s);
  }
}

fn create_stream(data: &[u8], pos: &mut usize) -> Result<Option<PlaylistStream>> {
  if *pos >= data.len() {
    return Ok(None);
  }
  let header_length = data[*pos] as usize;
  *pos += 1;
  let header_pos = *pos;
  let Some(header_end) = header_pos.checked_add(header_length) else {
    *pos = data.len();
    return Ok(None);
  };
  if header_pos >= data.len() || header_end > data.len() {
    *pos = data.len();
    return Ok(None);
  }
  let header_type = data[*pos];
  *pos += 1;

  let mut pid: u16 = 0;
  match header_type {
    1 => {
      if *pos + 2 > data.len() {
        return Ok(None);
      }
      pid = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
      *pos += 2;
    }
    2 => {
      if !checked_skip(pos, 2, data.len()) {
        return Ok(None);
      }
      if *pos + 2 > data.len() {
        return Ok(None);
      }
      pid = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
      *pos += 2;
    }
    3 => {
      if !checked_skip(pos, 1, data.len()) {
        return Ok(None);
      }
      if *pos + 2 > data.len() {
        return Ok(None);
      }
      pid = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
      *pos += 2;
    }
    4 => {
      if !checked_skip(pos, 2, data.len()) {
        return Ok(None);
      }
      if *pos + 2 > data.len() {
        return Ok(None);
      }
      pid = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
      *pos += 2;
    }
    _ => {}
  }

  *pos = header_end;

  if *pos >= data.len() {
    return Ok(None);
  }
  let stream_length = data[*pos] as usize;
  *pos += 1;
  let stream_pos = *pos;
  let Some(stream_end) = stream_pos.checked_add(stream_length) else {
    *pos = data.len();
    return Ok(None);
  };
  if stream_pos >= data.len() || stream_end > data.len() {
    *pos = data.len();
    return Ok(None);
  }

  let stream_type = TSStreamType::from_u8(data[*pos]);
  *pos += 1;

  if stream_type == TSStreamType::MVCVideo {
    *pos = stream_end;
    return Ok(None);
  }

  let mut stream = PlaylistStream::new(pid, stream_type);

  match stream_type {
    TSStreamType::HEVCVideo
    | TSStreamType::AVCVideo
    | TSStreamType::MPEG1Video
    | TSStreamType::MPEG2Video
    | TSStreamType::VC1Video
    | TSStreamType::MVCVideo => {
      if *pos + 1 < data.len() {
        let video_format = TSVideoFormat::from_u8(data[*pos] >> 4);
        let frame_rate = TSFrameRate::from_u8(data[*pos] & 0xF);
        let aspect_ratio = TSAspectRatio::from_u8(data[*pos + 1] >> 4);
        stream.video_format = video_format;
        stream.frame_rate = frame_rate;
        stream.aspect_ratio = aspect_ratio;
      }
    }
    TSStreamType::AC3Audio
    | TSStreamType::AC3PlusAudio
    | TSStreamType::AC3PlusSecondaryAudio
    | TSStreamType::AC3TrueHDAudio
    | TSStreamType::DTSAudio
    | TSStreamType::DTSHDAudio
    | TSStreamType::DTSHDMasterAudio
    | TSStreamType::DTSHDSecondaryAudio
    | TSStreamType::LpcmAudio
    | TSStreamType::MPEG1Audio
    | TSStreamType::MPEG2Audio
    | TSStreamType::MPEG2AacAudio
    | TSStreamType::MPEG4AacAudio => {
      if *pos < data.len() {
        let audio_format = data[*pos];
        *pos += 1;
        stream.channel_layout = TSChannelLayout::from_u8(audio_format >> 4);
        stream.sample_rate_hz = convert_sample_rate(audio_format & 0xF);
        if *pos + 3 <= data.len() {
          stream.language_code = String::from_utf8_lossy(&data[*pos..*pos + 3]).to_string();
          *pos += 3;
        }
      }
    }
    TSStreamType::InteractiveGraphics | TSStreamType::PresentationGraphics => {
      if *pos + 3 <= data.len() {
        stream.language_code = String::from_utf8_lossy(&data[*pos..*pos + 3]).to_string();
        *pos += 3;
      }
    }
    TSStreamType::Subtitle => {
      if !checked_skip(pos, 1, data.len()) {
        return Ok(None);
      }
      if *pos + 3 <= data.len() {
        stream.language_code = String::from_utf8_lossy(&data[*pos..*pos + 3]).to_string();
        *pos += 3;
      }
    }
    _ => {}
  }

  *pos = stream_end;

  Ok(Some(stream))
}

fn checked_skip(pos: &mut usize, amount: usize, len: usize) -> bool {
  let Some(next) = pos.checked_add(amount) else {
    *pos = len;
    return false;
  };
  if next > len {
    *pos = len;
    return false;
  }
  *pos = next;
  true
}

fn playlist_relative_chapters(clips: &[PlaylistStreamClip], raw_chapters: &[(usize, u64)]) -> Vec<f64> {
  let angle0: Vec<&PlaylistStreamClip> = clips.iter().filter(|c| c.angle_index == 0).collect();
  let total_45k: i64 = angle0.iter().map(|c| (c.time_out - c.time_in).max(0)).sum();
  let total_seconds = total_45k as f64 / 45000.0;

  let mut chapters = Vec::new();
  for &(stream_file_index, chapter_time) in raw_chapters {
    let Some(clip) = angle0.get(stream_file_index).copied() else {
      continue;
    };
    let prior_45k: i64 = angle0
      .iter()
      .take(stream_file_index)
      .map(|c| (c.time_out - c.time_in).max(0))
      .sum();
    let relative_45k = prior_45k + chapter_time as i64 - clip.time_in;
    if relative_45k < 0 {
      continue;
    }
    let secs = relative_45k as f64 / 45000.0;
    if total_seconds - secs < 1.0 {
      continue;
    }
    chapters.push(secs);
  }
  chapters
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Build a minimal but structurally valid MPLS: one play item (clip
  /// 00001.M2TS) with one AVC video stream and one AC3 audio stream, plus one
  /// chapter. The MVC base-view-R flag is set.
  fn build_mpls() -> Vec<u8> {
    let mut d: Vec<u8> = Vec::new();
    d.extend_from_slice(b"MPLS0200");
    d.extend_from_slice(&[0u8; 4]); // playlist_offset @8 (patched later)
    d.extend_from_slice(&[0u8; 4]); // chapters_offset @12 (patched later)
    d.extend_from_slice(&[0u8; 4]); // extensions_offset @16
    while d.len() < 0x38 {
      d.push(0);
    }
    d.push(0x10); // misc flags @0x38 -> mvc_base_view_r = true

    let playlist_offset = d.len() as u32;
    d[8..12].copy_from_slice(&playlist_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes()); // playlist_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.extend_from_slice(&1u16.to_be_bytes()); // item_count
    d.extend_from_slice(&0u16.to_be_bytes()); // subitem_count

    let item_start = d.len();
    d.extend_from_slice(&0u16.to_be_bytes()); // item_length placeholder
    d.extend_from_slice(b"00001"); // item name
    d.extend_from_slice(b"M2TS"); // item type
    d.push(0x00); // skip 1
    d.push(0x00); // multiangle/condition (no multiangle)
    d.push(0x00); // +1 (r.pos += 2)
    d.extend_from_slice(&0u32.to_be_bytes()); // in_time
    d.extend_from_slice(&4_500_000u32.to_be_bytes()); // out_time (100 s)
    d.extend_from_slice(&[0u8; 12]); // reserved skip

    // STN table header.
    d.extend_from_slice(&0u16.to_be_bytes()); // stn_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved (parser skips 2)
    d.push(1); // video count
    d.push(1); // audio count
    d.push(0); // pg
    d.push(0); // ig
    d.push(0); // secondary audio
    d.push(0); // secondary video
    d.push(0); // pip
    d.extend_from_slice(&[0u8; 5]); // reserved skip

    // Video stream entry.
    d.push(3); // stream-entry header length
    d.push(1); // header type 1 -> PID follows
    d.extend_from_slice(&0x1011u16.to_be_bytes()); // PID
    d.push(3); // stream coding info length
    d.push(0x1b); // AVC
    d.push((6 << 4) | 1); // video format 1080p, frame rate 23.976
    d.push(3 << 4); // aspect 16:9

    // Audio stream entry.
    d.push(3);
    d.push(1);
    d.extend_from_slice(&0x1100u16.to_be_bytes()); // PID
    d.push(5); // stream coding info length
    d.push(0x81); // AC3
    d.push((6 << 4) | 1); // channel layout 5.1, sample-rate code 48 kHz
    d.extend_from_slice(b"eng");

    let item_len = (d.len() - item_start - 2) as u16;
    d[item_start..item_start + 2].copy_from_slice(&item_len.to_be_bytes());

    // Chapters section: parser reads from chapters_offset + 4.
    let chapters_offset = d.len() as u32;
    d[12..16].copy_from_slice(&chapters_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes()); // length (skipped)
    d.extend_from_slice(&1u16.to_be_bytes()); // chapter count
    let mut chapter = vec![0u8; 14];
    chapter[1] = 1; // chapter type 1
    chapter[4..8].copy_from_slice(&(45000u32 * 10).to_be_bytes()); // 10 s
    d.extend_from_slice(&chapter);

    d
  }

  /// Build one STN stream entry: `[header_length][header...][stream_length][coding...]`.
  /// `header_type` selects how the PID is located (matching `create_stream`).
  fn stream_entry(header_type: u8, pid: u16, coding: &[u8]) -> Vec<u8> {
    let mut header = vec![header_type];
    match header_type {
      1 => header.extend_from_slice(&pid.to_be_bytes()),
      2 | 4 => {
        header.extend_from_slice(&[0u8, 0u8]);
        header.extend_from_slice(&pid.to_be_bytes());
      }
      3 => {
        header.push(0);
        header.extend_from_slice(&pid.to_be_bytes());
      }
      _ => {}
    }
    let mut out = vec![header.len() as u8];
    out.extend_from_slice(&header);
    out.push(coding.len() as u8);
    out.extend_from_slice(coding);
    out
  }

  fn mpls_header(signature: &[u8], misc_flags: u8) -> Vec<u8> {
    let mut d: Vec<u8> = Vec::new();
    d.extend_from_slice(signature);
    d.extend_from_slice(&[0u8; 4]); // playlist_offset @8 (patched later)
    d.extend_from_slice(&[0u8; 4]); // chapters_offset @12 (patched later)
    d.extend_from_slice(&[0u8; 4]); // extensions_offset @16
    while d.len() < 0x38 {
      d.push(0);
    }
    d.push(misc_flags); // @0x38
    d
  }

  /// MPLS exercising every stream category and header-entry type, plus the
  /// secondary-audio (+2) and secondary-video (+6) skip bytes.
  fn build_rich_mpls() -> Vec<u8> {
    let mut d = mpls_header(b"MPLS0100", 0x00);

    let playlist_offset = d.len() as u32;
    d[8..12].copy_from_slice(&playlist_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes()); // playlist_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.extend_from_slice(&1u16.to_be_bytes()); // item_count
    d.extend_from_slice(&0u16.to_be_bytes()); // subitem_count

    let item_start = d.len();
    d.extend_from_slice(&0u16.to_be_bytes()); // item_length placeholder
    d.extend_from_slice(b"00002"); // item name
    d.extend_from_slice(b"M2TS");
    d.push(0x00); // skip 1
    d.push(0x00); // multiangle = 0
    d.push(0x00); // +1
    d.extend_from_slice(&0u32.to_be_bytes()); // in_time
    d.extend_from_slice(&9_000_000u32.to_be_bytes()); // out_time
    d.extend_from_slice(&[0u8; 12]); // reserved skip

    // STN table.
    d.extend_from_slice(&0u16.to_be_bytes()); // stn_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.push(1); // video
    d.push(1); // audio
    d.push(2); // pg (PG + subtitle)
    d.push(1); // ig
    d.push(1); // secondary audio
    d.push(1); // secondary video
    d.push(0); // pip
    d.extend_from_slice(&[0u8; 5]); // reserved skip

    d.extend(stream_entry(1, 0x1011, &[0x1b, (6 << 4) | 1, 3 << 4])); // AVC video
    d.extend(stream_entry(1, 0x1100, &[0x82, (6 << 4) | 1, b's', b'p', b'a'])); // DTS audio
    d.extend(stream_entry(2, 0x1200, &[0x90, b'e', b'n', b'g'])); // PG (header type 2)
    d.extend(stream_entry(3, 0x1201, &[0x92, 0x00, b'f', b'r', b'a'])); // subtitle (type 3)
    d.extend(stream_entry(4, 0x1400, &[0x91, b'j', b'p', b'n'])); // IG (type 4)
    d.extend(stream_entry(1, 0x1A00, &[0x81, (1 << 4) | 1, b'd', b'e', b'u'])); // secondary audio
    d.extend_from_slice(&[0u8, 0u8]); // secondary-audio extension (+2)
    d.extend(stream_entry(1, 0x1B00, &[0x20, (6 << 4) | 1, 3 << 4])); // secondary video (MVC)
    d.extend_from_slice(&[0u8; 6]); // secondary-video extension (+6)

    let item_len = (d.len() - item_start - 2) as u16;
    d[item_start..item_start + 2].copy_from_slice(&item_len.to_be_bytes());

    // Two chapters: one type-1 (kept) and one type-2 (ignored).
    let chapters_offset = d.len() as u32;
    d[12..16].copy_from_slice(&chapters_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes()); // length
    d.extend_from_slice(&2u16.to_be_bytes()); // chapter count
    let mut ch1 = vec![0u8; 14];
    ch1[1] = 1;
    ch1[4..8].copy_from_slice(&(45000u32 * 5).to_be_bytes());
    d.extend_from_slice(&ch1);
    let mut ch2 = vec![0u8; 14];
    ch2[1] = 2; // non-type-1 -> skipped
    d.extend_from_slice(&ch2);
    d
  }

  #[test]
  fn rejects_unknown_signature() {
    let data = b"XXXX0000".to_vec();
    assert!(parse_mpls_bytes("00000.MPLS".into(), &data).is_err());
  }

  #[test]
  fn parses_all_stream_categories_and_header_types() {
    let pl = parse_mpls_bytes("00002.MPLS".into(), &build_rich_mpls()).expect("parses");
    assert_eq!(pl.file_type, "MPLS0100");
    assert!(!pl.mvc_base_view_r);
    assert_eq!(pl.angle_count, 0);

    assert_eq!(pl.stream_clips.len(), 1);
    assert_eq!(pl.stream_clips[0].name, "00002.M2TS");
    assert_eq!(pl.stream_clips[0].time_out, 9_000_000);

    // video, audio, PG, subtitle, IG, secondary audio. MPLS-declared MVC
    // is skipped to mirror BDInfo's CreatePlaylistStream TODO.
    assert_eq!(pl.playlist_streams.len(), 6);
    let by_pid = |pid: u16| pl.playlist_streams.iter().find(|s| s.pid == pid).unwrap();
    assert_eq!(by_pid(0x1011).stream_type, TSStreamType::AVCVideo);
    let dts = by_pid(0x1100);
    assert_eq!(dts.stream_type, TSStreamType::DTSAudio);
    assert_eq!(dts.channel_layout, TSChannelLayout::Multi);
    assert_eq!(dts.sample_rate_hz, 48000);
    assert_eq!(dts.language_code, "spa");
    assert_eq!(by_pid(0x1200).stream_type, TSStreamType::PresentationGraphics);
    assert_eq!(by_pid(0x1200).language_code, "eng");
    assert_eq!(by_pid(0x1201).stream_type, TSStreamType::Subtitle);
    assert_eq!(by_pid(0x1201).language_code, "fra");
    assert_eq!(by_pid(0x1400).stream_type, TSStreamType::InteractiveGraphics);
    assert_eq!(by_pid(0x1400).language_code, "jpn");
    assert_eq!(by_pid(0x1A00).stream_type, TSStreamType::AC3Audio);
    assert_eq!(by_pid(0x1A00).channel_layout, TSChannelLayout::Mono);
    assert!(pl.playlist_streams.iter().all(|s| s.pid != 0x1B00));

    // Only the type-1 chapter survives.
    assert_eq!(pl.chapters.len(), 1);
    assert!((pl.chapters[0] - 5.0).abs() < 1e-6);
  }

  #[test]
  fn parses_multiangle_clips() {
    let mut d = mpls_header(b"MPLS0300", 0x00);
    let playlist_offset = d.len() as u32;
    d[8..12].copy_from_slice(&playlist_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes());
    d.extend_from_slice(&0u16.to_be_bytes());
    d.extend_from_slice(&1u16.to_be_bytes()); // item_count
    d.extend_from_slice(&0u16.to_be_bytes());

    let item_start = d.len();
    d.extend_from_slice(&0u16.to_be_bytes()); // item_length
    d.extend_from_slice(b"00100");
    d.extend_from_slice(b"M2TS");
    d.push(0x00); // skip 1
    d.push(0x10); // multiangle flag set (bit4)
    d.push(0x00); // +1
    d.extend_from_slice(&0u32.to_be_bytes()); // in_time
    d.extend_from_slice(&4_500_000u32.to_be_bytes()); // out_time
    d.extend_from_slice(&[0u8; 12]); // reserved
    // multi-angle: 3 angles -> 2 extra angle clips.
    d.push(3); // angle count
    d.push(0x00); // +1 (r.pos += 2)
    for name in ["00101", "00102"] {
      d.extend_from_slice(name.as_bytes());
      d.extend_from_slice(b"M2TS");
      d.push(0x00); // +1
    }
    // STN table with no streams.
    d.extend_from_slice(&0u16.to_be_bytes()); // stn_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.extend_from_slice(&[0u8; 7]); // counts (all zero)
    d.extend_from_slice(&[0u8; 5]); // reserved
    let item_len = (d.len() - item_start - 2) as u16;
    d[item_start..item_start + 2].copy_from_slice(&item_len.to_be_bytes());

    // Empty chapter section.
    let chapters_offset = d.len() as u32;
    d[12..16].copy_from_slice(&chapters_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes());
    d.extend_from_slice(&0u16.to_be_bytes()); // chapter count 0

    let pl = parse_mpls_bytes("00100.MPLS".into(), &d).expect("parses");
    assert_eq!(pl.angle_count, 2);
    assert_eq!(pl.stream_clips.len(), 3);
    assert_eq!(pl.stream_clips[0].angle_index, 0);
    assert_eq!(pl.stream_clips[1].angle_index, 1);
    assert_eq!(pl.stream_clips[1].name, "00101.M2TS");
    assert_eq!(pl.stream_clips[2].angle_index, 2);
    assert_eq!(pl.playlist_streams.len(), 0);
  }

  #[test]
  fn duplicate_pid_metadata_uses_the_last_significant_clip() {
    let mut list = vec![PlaylistStream {
      pid: 0x1100,
      stream_type: TSStreamType::AC3Audio,
      video_format: TSVideoFormat::Unknown,
      frame_rate: TSFrameRate::Unknown,
      aspect_ratio: TSAspectRatio::Unknown,
      channel_layout: TSChannelLayout::Unknown,
      sample_rate_hz: 0,
      language_code: String::new(),
    }];

    let richer = PlaylistStream {
      pid: 0x1100,
      stream_type: TSStreamType::AC3Audio,
      video_format: TSVideoFormat::Unknown,
      frame_rate: TSFrameRate::Unknown,
      aspect_ratio: TSAspectRatio::Unknown,
      channel_layout: TSChannelLayout::Multi,
      sample_rate_hz: 48_000,
      language_code: "eng".to_string(),
    };
    add_stream_metadata(&mut list, richer, true);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].language_code, "eng");
    assert_eq!(list[0].sample_rate_hz, 48_000);

    let poorer = PlaylistStream {
      pid: 0x1100,
      stream_type: TSStreamType::AC3Audio,
      video_format: TSVideoFormat::Unknown,
      frame_rate: TSFrameRate::Unknown,
      aspect_ratio: TSAspectRatio::Unknown,
      channel_layout: TSChannelLayout::Unknown,
      sample_rate_hz: 0,
      language_code: "fra".to_string(),
    };
    add_stream_metadata(&mut list, poorer, true);
    assert_eq!(list[0].language_code, "fra");

    let insignificant = PlaylistStream {
      pid: 0x1100,
      stream_type: TSStreamType::AC3Audio,
      video_format: TSVideoFormat::Unknown,
      frame_rate: TSFrameRate::Unknown,
      aspect_ratio: TSAspectRatio::Unknown,
      channel_layout: TSChannelLayout::Combo,
      sample_rate_hz: 192_000,
      language_code: "jpn".to_string(),
    };
    add_stream_metadata(&mut list, insignificant, false);
    assert_eq!(list[0].language_code, "fra");
  }

  #[test]
  fn clip_significance_is_relative_to_accumulated_playlist_length() {
    assert!(clip_is_significant(45_000, 45_000));
    assert!(clip_is_significant(1_001, 100_000));
    assert!(!clip_is_significant(1_000, 100_000));
    assert!(!clip_is_significant(450, 4_500_450));
  }

  #[test]
  fn chapters_are_playlist_relative_and_final_marks_are_dropped() {
    let clips = vec![
      PlaylistStreamClip {
        name: "00001.M2TS".to_string(),
        time_in: 0,
        time_out: 450_000,
        angle_index: 0,
      },
      PlaylistStreamClip {
        name: "00002.M2TS".to_string(),
        time_in: 900_000,
        time_out: 1_800_000,
        angle_index: 0,
      },
      PlaylistStreamClip {
        name: "00003.M2TS".to_string(),
        time_in: 0,
        time_out: 1_000_000,
        angle_index: 1,
      },
    ];
    let chapters = playlist_relative_chapters(
      &clips,
      &[
        (0, 90_000),    // 2 seconds into first clip.
        (1, 1_125_000), // 5 seconds into second clip, after 10s first clip.
        (1, 1_780_000), // Less than 1 second from playlist end -> drop.
        (4, 0),         // Invalid stream-file index -> drop.
      ],
    );
    assert_eq!(chapters.len(), 2);
    assert!((chapters[0] - 2.0).abs() < 1e-6);
    assert!((chapters[1] - 15.0).abs() < 1e-6);
  }

  #[test]
  fn truncated_input_is_rejected_gracefully() {
    // Valid signature but no room for the offset fields -> Err, not panic.
    let data = b"MPLS0200".to_vec();
    assert!(parse_mpls_bytes("x.MPLS".into(), &data).is_err());
  }

  #[test]
  fn parses_clips_streams_and_chapters() {
    let data = build_mpls();
    let pl = parse_mpls_bytes("00800.MPLS".into(), &data).expect("parses");

    assert_eq!(pl.file_type, "MPLS0200");
    assert!(pl.mvc_base_view_r);
    assert_eq!(pl.angle_count, 0);

    assert_eq!(pl.stream_clips.len(), 1);
    assert_eq!(pl.stream_clips[0].name, "00001.M2TS");
    assert_eq!(pl.stream_clips[0].time_in, 0);
    assert_eq!(pl.stream_clips[0].time_out, 4_500_000);
    assert_eq!(pl.stream_clips[0].angle_index, 0);

    assert_eq!(pl.playlist_streams.len(), 2);
    let video = &pl.playlist_streams[0];
    assert_eq!(video.pid, 0x1011);
    assert_eq!(video.stream_type, TSStreamType::AVCVideo);
    assert_eq!(video.video_format, TSVideoFormat::Video1080p);
    assert_eq!(video.frame_rate, TSFrameRate::F23_976);
    assert_eq!(video.aspect_ratio, TSAspectRatio::Aspect16_9);

    let audio = &pl.playlist_streams[1];
    assert_eq!(audio.pid, 0x1100);
    assert_eq!(audio.stream_type, TSStreamType::AC3Audio);
    assert_eq!(audio.channel_layout, TSChannelLayout::Multi);
    assert_eq!(audio.sample_rate_hz, 48000);
    assert_eq!(audio.language_code, "eng");

    assert_eq!(pl.chapters.len(), 1);
    assert!((pl.chapters[0] - 10.0).abs() < 1e-6);
  }
}
