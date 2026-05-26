/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * CLPI (Clip Information) reader. Faithful port of TSStreamClipFile.cs: it
 * validates the `HDMV0100`/`HDMV0200`/`HDMV0300` signature, walks the clip
 * info / program info section, and parses each stream's PID, coding type, and
 * type-specific attributes (video format/frame rate/aspect, audio channel
 * layout/sample rate/language, graphics & subtitle languages).
 */

use anyhow::Result;
use std::path::Path;

use crate::bdrom::types::{convert_sample_rate, TSStreamType};

/// One stream entry from the CLPI program-info table.
#[derive(Debug, Clone, Default)]
pub struct ClpiStream {
    pub pid: u16,
    pub stream_type: u8,
    pub video_format: u8,
    pub frame_rate: u8,
    pub aspect_ratio: u8,
    pub channel_layout: u8,
    pub sample_rate: u32,
    pub language_code: String,
}

#[derive(Debug, Clone, Default)]
pub struct StreamClipFile {
    pub name: String,
    pub size: u64,
    /// 8-byte signature, e.g. `HDMV0200`. Empty if the file was too short.
    pub file_type: String,
    /// True once the signature and clip-info section parsed successfully.
    pub is_valid: bool,
    /// Streams keyed by appearance order (also carries PID).
    pub streams: Vec<ClpiStream>,
}

fn be_u32(data: &[u8], offset: usize) -> u32 {
    ((data[offset] as u32) << 24)
        | ((data[offset + 1] as u32) << 16)
        | ((data[offset + 2] as u32) << 8)
        | (data[offset + 3] as u32)
}

fn ascii3(data: &[u8], offset: usize) -> String {
    if offset + 3 > data.len() {
        return String::new();
    }
    String::from_utf8_lossy(&data[offset..offset + 3])
        .trim_end_matches('\0')
        .to_string()
}

pub fn parse_clpi(path: &Path) -> Result<StreamClipFile> {
    let data = std::fs::read(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    Ok(parse_clpi_bytes(name, data.len() as u64, &data))
}

/// Parse a CLPI file already loaded into memory. Returns a `StreamClipFile`
/// with `is_valid = false` (and no streams) when the signature is wrong or the
/// clip-info section is malformed, rather than panicking — mirroring BDInfo's
/// "unknown file type" rejection while staying panic-free.
pub fn parse_clpi_bytes(name: String, size: u64, data: &[u8]) -> StreamClipFile {
    let mut scf = StreamClipFile {
        name,
        size,
        ..Default::default()
    };

    if data.len() < 16 {
        return scf;
    }
    scf.file_type = String::from_utf8_lossy(&data[0..8]).to_string();
    if !matches!(scf.file_type.as_str(), "HDMV0100" | "HDMV0200" | "HDMV0300") {
        return scf;
    }

    // ClipInfo section: 4-byte big-endian offset at byte 12, then a 4-byte
    // length prefix; the program-info stream table follows.
    let clip_index = be_u32(data, 12) as usize;
    if clip_index + 4 > data.len() {
        return scf;
    }
    let clip_length = be_u32(data, clip_index) as usize;
    let start = clip_index + 4;
    if clip_length < 10 || start + clip_length > data.len() {
        return scf;
    }
    let clip = &data[start..start + clip_length];

    let stream_count = clip[8] as usize;
    let mut off = 10usize;
    for _ in 0..stream_count {
        if off + 2 > clip.len() {
            break;
        }
        let pid = ((clip[off] as u16) << 8) + clip[off + 1] as u16;
        off += 2;
        // clip[off] is the stream_coding_info length; clip[off+1] is the type.
        if off + 1 >= clip.len() {
            break;
        }
        let info_len = clip[off] as usize;
        let type_raw = clip[off + 1];
        let st = TSStreamType::from_u8(type_raw);

        let mut stream = ClpiStream {
            pid,
            stream_type: type_raw,
            ..Default::default()
        };

        let added = if st == TSStreamType::MVCVideo {
            // BDInfo leaves MVC as a TODO and does not register a stream.
            false
        } else if st.is_video() {
            if off + 3 < clip.len() {
                stream.video_format = clip[off + 2] >> 4;
                stream.frame_rate = clip[off + 2] & 0x0F;
                stream.aspect_ratio = clip[off + 3] >> 4;
            }
            true
        } else if st.is_audio() {
            if off + 5 < clip.len() {
                stream.channel_layout = clip[off + 2] >> 4;
                stream.sample_rate = convert_sample_rate(clip[off + 2] & 0x0F);
                stream.language_code = ascii3(clip, off + 3);
            }
            true
        } else if st.is_graphics() {
            stream.language_code = ascii3(clip, off + 2);
            true
        } else if st.is_text() {
            // Subtitle: a character-code byte precedes the language code.
            stream.language_code = ascii3(clip, off + 3);
            true
        } else {
            false
        };

        if added {
            scf.streams.push(stream);
        }

        // Advance past this stream's coding-info block.
        off += info_len + 1;
    }

    scf.is_valid = true;
    scf
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic CLPI image: an 8-byte signature, a 4-byte clip-info
    /// offset at byte 12, and a clip-info block (4-byte length prefix) carrying
    /// a stream-count byte and the per-stream coding-info entries.
    fn build_clpi(file_type: &str, streams: &[Vec<u8>]) -> Vec<u8> {
        // Stream table starts at byte 10 of the clip block (bytes 0..8 reserved,
        // byte 8 = stream count, byte 9 reserved).
        let mut clip = vec![0u8; 10];
        clip[8] = streams.len() as u8;
        for s in streams {
            clip.extend_from_slice(s);
        }
        let clip_len = clip.len() as u32;

        // Header: 8-byte type, 4 bytes pad, 4-byte clip-info offset, then the
        // clip block prefixed by its 4-byte length.
        let mut data = Vec::new();
        data.extend_from_slice(file_type.as_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]); // bytes 8..12
        let clip_index = 16u32;
        data.extend_from_slice(&clip_index.to_be_bytes()); // bytes 12..16
        data.extend_from_slice(&clip_len.to_be_bytes()); // clip length prefix
        data.extend_from_slice(&clip); // clip block
        data
    }

    /// A coding-info entry: PID (2 bytes), length byte, coding type, attributes.
    fn stream_entry(pid: u16, coding_type: u8, attrs: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&pid.to_be_bytes());
        // length byte counts coding type + attributes.
        v.push((1 + attrs.len()) as u8);
        v.push(coding_type);
        v.extend_from_slice(attrs);
        v
    }

    #[test]
    fn rejects_unknown_signature() {
        let data = build_clpi("XXXX0000", &[]);
        let scf = parse_clpi_bytes("00000.CLPI".into(), data.len() as u64, &data);
        assert!(!scf.is_valid);
        assert!(scf.streams.is_empty());
    }

    #[test]
    fn too_short_file_is_invalid() {
        let scf = parse_clpi_bytes("X.CLPI".into(), 4, &[0u8; 4]);
        assert!(!scf.is_valid);
        assert_eq!(scf.file_type, "");
    }

    #[test]
    fn parses_video_audio_and_pgs_streams() {
        // Video (HEVC 0x24): byte0 = video_format<<4 | frame_rate,
        //                    byte1 = aspect<<4.
        let video = stream_entry(0x1011, 0x24, &[(6 << 4) | 1, 3 << 4]);
        // Audio (TrueHD 0x83): byte0 = channel_layout<<4 | sr_code, then 3-byte lang.
        let audio = stream_entry(
            0x1100,
            0x83,
            &[(6 << 4) | 1, b'e', b'n', b'g'],
        );
        // PGS (0x90): 3-byte language immediately.
        let pgs = stream_entry(0x1200, 0x90, &[b'j', b'p', b'n']);

        let data = build_clpi("HDMV0200", &[video, audio, pgs]);
        let scf = parse_clpi_bytes("00001.CLPI".into(), data.len() as u64, &data);

        assert!(scf.is_valid);
        assert_eq!(scf.file_type, "HDMV0200");
        assert_eq!(scf.streams.len(), 3);

        let v = &scf.streams[0];
        assert_eq!(v.pid, 0x1011);
        assert_eq!(v.stream_type, 0x24);
        assert_eq!(v.video_format, 6); // 1080p
        assert_eq!(v.frame_rate, 1); // 23.976
        assert_eq!(v.aspect_ratio, 3); // 16:9

        let a = &scf.streams[1];
        assert_eq!(a.pid, 0x1100);
        assert_eq!(a.channel_layout, 6); // 5.1
        assert_eq!(a.sample_rate, 48000); // sr_code 1
        assert_eq!(a.language_code, "eng");

        let g = &scf.streams[2];
        assert_eq!(g.pid, 0x1200);
        assert_eq!(g.language_code, "jpn");
    }

    #[test]
    fn mvc_stream_is_not_registered_but_offset_advances() {
        // An MVC entry (0x20) is skipped; the following audio entry must still parse.
        let mvc = stream_entry(0x1012, 0x20, &[0x00, 0x00]);
        let audio = stream_entry(0x1100, 0x81, &[(3 << 4) | 1, b'f', b'r', b'a']);
        let data = build_clpi("HDMV0300", &[mvc, audio]);
        let scf = parse_clpi_bytes("00002.CLPI".into(), data.len() as u64, &data);
        assert!(scf.is_valid);
        // Only the audio stream is registered (MVC skipped, like BDInfo).
        assert_eq!(scf.streams.len(), 1);
        assert_eq!(scf.streams[0].pid, 0x1100);
        assert_eq!(scf.streams[0].language_code, "fra");
    }

    #[test]
    fn truncated_stream_table_does_not_panic() {
        // stream_count claims 4 but the data is short.
        let mut data = build_clpi("HDMV0100", &[stream_entry(0x1011, 0x1b, &[0x61, 0x00])]);
        // Bump the stream count byte (clip block starts at 20, +8 = count).
        data[20 + 8] = 4;
        let scf = parse_clpi_bytes("00003.CLPI".into(), data.len() as u64, &data);
        // No panic; whatever parsed cleanly is valid.
        assert!(scf.is_valid);
    }
}
