/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecVC1.cs.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::protocol::TSStreamInfo;

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
    let mut parse: u32 = 0;
    let mut frame_header_parse: u8 = 0;
    let mut sequence_header_parse: u8 = 0;
    let mut is_interlaced = false;

    for _ in 0..buffer.len() {
        parse = parse.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);

        if parse == 0x0000010D {
            frame_header_parse = 4;
        } else if frame_header_parse > 0 {
            frame_header_parse -= 1;
            if frame_header_parse == 0 {
                let _picture_type: u32 = if is_interlaced {
                    if (parse & 0x80000000) == 0 {
                        (parse & 0x78000000) >> 13
                    } else {
                        (parse & 0x3c000000) >> 12
                    }
                } else {
                    (parse & 0xf0000000) >> 14
                };
                if stream.is_initialized {
                    return;
                }
            }
        } else if parse == 0x0000010F {
            sequence_header_parse = 6;
        } else if sequence_header_parse > 0 {
            sequence_header_parse -= 1;
            match sequence_header_parse {
                5 => {
                    let profile_level = (parse & 0x38) >> 3;
                    let profile_kind = (parse & 0xC0) >> 6;
                    stream.encoding_profile = if profile_kind == 3 {
                        format!("Advanced Profile {}", profile_level)
                    } else {
                        format!("Main Profile {}", profile_level)
                    };
                }
                0 => {
                    is_interlaced = ((parse & 0x40) >> 6) > 0;
                    stream.is_interlaced = is_interlaced;
                }
                _ => {}
            }
            stream.is_vbr = true;
            stream.is_initialized = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bdrom::types::TSStreamType;

    fn vc1_stream() -> TSStreamInfo {
        TSStreamInfo::new(0x1011, TSStreamType::VC1Video as u8)
    }

    #[test]
    fn empty_buffer_leaves_stream_uninitialized() {
        let mut stream = vc1_stream();
        let data: Vec<u8> = Vec::new();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(!stream.is_initialized);
        assert!(stream.encoding_profile.is_empty());
    }

    #[test]
    fn sequence_header_sets_advanced_profile_and_interlace() {
        // 0x0000010F = sequence header start code. The byte after it carries the
        // profile (top two bits == 3 -> Advanced) and level (bits 0x38).
        // The sixth byte after the start code carries the interlace flag (0x40).
        let data = vec![
            0x00, 0x00, 0x01, 0x0F, // sequence header start code
            0xC8, // profile_kind=3 (Advanced), profile_level=1
            0x00, 0x00, 0x00, 0x00, // padding bytes
            0x40, // interlace flag set
        ];
        let mut stream = vc1_stream();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(stream.is_initialized);
        assert!(stream.is_vbr);
        assert_eq!(stream.encoding_profile, "Advanced Profile 1");
        assert!(stream.is_interlaced);
    }

    #[test]
    fn sequence_header_sets_main_profile_progressive() {
        // Profile byte 0x00 -> profile_kind=0 (Main), profile_level=0; no
        // interlace bit set in the trailing bytes -> progressive.
        let data = vec![
            0x00, 0x00, 0x01, 0x0F, // sequence header start code
            0x00, // profile_kind=0 (Main), level=0
            0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let mut stream = vc1_stream();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(stream.is_initialized);
        assert_eq!(stream.encoding_profile, "Main Profile 0");
        assert!(!stream.is_interlaced);
    }

    #[test]
    fn does_not_panic_on_garbage() {
        let mut stream = vc1_stream();
        let data: Vec<u8> = (0..256u32).map(|i| (i % 256) as u8).collect();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
    }
}
