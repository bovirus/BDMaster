/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * MPLS (Movie Playlist) parser. Port of TSPlaylistFile.cs Scan().
 */

use anyhow::{anyhow, Result};
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
        let v = *self
            .data
            .get(self.pos)
            .ok_or_else(|| anyhow!("eof at {}", self.pos))?;
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
}

pub fn parse_mpls(path: &Path) -> Result<PlaylistFile> {
    let data = std::fs::read(path)?;
    parse_mpls_bytes(
        path.file_name()
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
    r.pos = 0x38;
    let misc_flags = r.read_u8()?;
    let mvc_base_view_r = (misc_flags & 0x10) != 0;

    // Playlist
    r.pos = playlist_offset;
    let _playlist_length = r.read_u32()?;
    let _reserved = r.read_u16()?;
    let item_count = r.read_u16()?;
    let _subitem_count = r.read_u16()?;

    let mut stream_clips: Vec<PlaylistStreamClip> = Vec::new();
    let mut playlist_streams: Vec<PlaylistStream> = Vec::new();
    let mut angle_count: u32 = 0;

    for _ in 0..item_count {
        let item_start = r.pos;
        let item_length = r.read_u16()? as usize;
        let item_name = r.read_string(5)?;
        let _item_type = r.read_string(4)?;

        // skip 1 byte
        r.pos += 1;
        let multiangle = (data[r.pos] >> 4) & 0x01;
        let _condition = data[r.pos] & 0x0F;
        r.pos += 2;

        let in_time = r.read_u32()? as i64;
        let in_time = if (in_time as i32) < 0 { in_time & 0x7FFFFFFF } else { in_time };

        let out_time = r.read_u32()? as i64;
        let out_time = if (out_time as i32) < 0 { out_time & 0x7FFFFFFF } else { out_time };

        let stream_clip = PlaylistStreamClip {
            name: format!("{}.M2TS", item_name.trim_end_matches('\0')),
            time_in: in_time,
            time_out: out_time,
            angle_index: 0,
        };
        stream_clips.push(stream_clip.clone());

        // skip 12 bytes
        r.pos += 12;
        if multiangle > 0 {
            let angles = data[r.pos] as i32;
            r.pos += 2;
            for angle in 0..(angles - 1).max(0) {
                let angle_name = r.read_string(5)?;
                let _angle_type = r.read_string(4)?;
                r.pos += 1;
                let angle_clip = PlaylistStreamClip {
                    name: format!("{}.M2TS", angle_name.trim_end_matches('\0')),
                    time_in: in_time,
                    time_out: out_time,
                    angle_index: (angle + 1) as u32,
                };
                stream_clips.push(angle_clip);
            }
            if (angles - 1) as u32 > angle_count {
                angle_count = (angles - 1) as u32;
            }
        }

        // STN_table
        let _stn_length = r.read_u16()?;
        r.pos += 2;
        let stream_count_video = r.read_u8()? as i32;
        let stream_count_audio = r.read_u8()? as i32;
        let stream_count_pg = r.read_u8()? as i32;
        let stream_count_ig = r.read_u8()? as i32;
        let stream_count_secondary_audio = r.read_u8()? as i32;
        let stream_count_secondary_video = r.read_u8()? as i32;
        let _stream_count_pip = r.read_u8()? as i32;
        r.pos += 5;

        for _ in 0..stream_count_video {
            if let Some(s) = create_stream(data, &mut r.pos, 0)? {
                add_unique(&mut playlist_streams, s);
            }
        }
        for _ in 0..stream_count_audio {
            if let Some(s) = create_stream(data, &mut r.pos, 0)? {
                add_unique(&mut playlist_streams, s);
            }
        }
        for _ in 0..stream_count_pg {
            if let Some(s) = create_stream(data, &mut r.pos, 0)? {
                add_unique(&mut playlist_streams, s);
            }
        }
        for _ in 0..stream_count_ig {
            if let Some(s) = create_stream(data, &mut r.pos, 0)? {
                add_unique(&mut playlist_streams, s);
            }
        }
        for _ in 0..stream_count_secondary_audio {
            if let Some(s) = create_stream(data, &mut r.pos, 2)? {
                add_unique(&mut playlist_streams, s);
            }
        }
        for _ in 0..stream_count_secondary_video {
            if let Some(s) = create_stream(data, &mut r.pos, 6)? {
                add_unique(&mut playlist_streams, s);
            }
        }

        // Skip rest of item
        let consumed = r.pos - item_start;
        let total = item_length + 2;
        if total > consumed {
            r.pos += total - consumed;
        }
    }

    // Chapters
    let mut chapters: Vec<f64> = Vec::new();
    if chapters_offset + 4 <= data.len() {
        r.pos = chapters_offset + 4;
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
                let secs = chapter_time as f64 / 45000.0;
                chapters.push(secs);
            }
            r.pos += 14;
        }
    }

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

fn add_unique(list: &mut Vec<PlaylistStream>, s: PlaylistStream) {
    if !list.iter().any(|x| x.pid == s.pid) {
        list.push(s);
    }
}

fn create_stream(data: &[u8], pos: &mut usize, post_extra: usize) -> Result<Option<PlaylistStream>> {
    if *pos >= data.len() {
        return Ok(None);
    }
    let header_length = data[*pos] as usize;
    *pos += 1;
    let header_pos = *pos;
    if header_pos >= data.len() {
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
            *pos += 2; // subpathid + subclipid
            if *pos + 2 > data.len() {
                return Ok(None);
            }
            pid = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
            *pos += 2;
        }
        3 => {
            *pos += 1; // subpathid
            if *pos + 2 > data.len() {
                return Ok(None);
            }
            pid = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
            *pos += 2;
        }
        4 => {
            *pos += 2;
            if *pos + 2 > data.len() {
                return Ok(None);
            }
            pid = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
            *pos += 2;
        }
        _ => {}
    }

    *pos = header_pos + header_length;

    if *pos >= data.len() {
        return Ok(None);
    }
    let stream_length = data[*pos] as usize;
    *pos += 1;
    let stream_pos = *pos;
    if stream_pos >= data.len() {
        return Ok(None);
    }

    let stream_type = TSStreamType::from_u8(data[*pos]);
    *pos += 1;

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
                    stream.language_code =
                        String::from_utf8_lossy(&data[*pos..*pos + 3]).to_string();
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
            *pos += 1;
            if *pos + 3 <= data.len() {
                stream.language_code = String::from_utf8_lossy(&data[*pos..*pos + 3]).to_string();
                *pos += 3;
            }
        }
        _ => {}
    }

    *pos = stream_pos + stream_length;
    *pos += post_extra;

    Ok(Some(stream))
}
