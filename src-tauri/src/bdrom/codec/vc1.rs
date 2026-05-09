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
