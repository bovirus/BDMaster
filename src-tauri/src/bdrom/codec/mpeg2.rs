/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecMPEG2.cs.
 *
 * BDInfo gates several stream-property assignments behind #if DEBUG. We mirror
 * the behavior with a `debug_mode` flag controlled at compile time; in release
 * builds (debug_assertions == false) the gating matches the upstream binary.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::TSAspectRatio;
use crate::protocol::TSStreamInfo;

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
    let mut parse: u32 = 0;
    let mut picture_parse: u32 = 0;
    let mut sequence_header_parse: u32 = 0;
    let mut extension_parse: u32 = 0;
    let mut sequence_extension_parse: u32 = 0;

    let debug_mode = cfg!(debug_assertions);

    for _ in 0..buffer.len() {
        parse = parse.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);

        if parse == 0x00000100 {
            picture_parse = 2;
        } else if parse == 0x000001B3 {
            sequence_header_parse = 7;
        } else if sequence_header_parse > 0 {
            sequence_header_parse -= 1;
            match sequence_header_parse {
                4 if debug_mode => {
                    stream.width = (parse & 0xFFF000) >> 12;
                    stream.height = parse & 0xFFF;
                }
                3 if debug_mode => {
                    let ar = ((parse & 0xF0) >> 4) as u8;
                    let aspect = TSAspectRatio::from_u8(ar);
                    stream.aspect_ratio = aspect.label().to_string();
                    stream.aspect_ratio_code = ar as u32;

                    match parse & 0xF {
                        1 => {
                            stream.frame_rate_enumerator = 24000;
                            stream.frame_rate_denominator = 1001;
                        }
                        2 => {
                            stream.frame_rate_enumerator = 24000;
                            stream.frame_rate_denominator = 1000;
                        }
                        3 => {
                            stream.frame_rate_enumerator = 25000;
                            stream.frame_rate_denominator = 1000;
                        }
                        4 => {
                            stream.frame_rate_enumerator = 30000;
                            stream.frame_rate_denominator = 1001;
                        }
                        5 => {
                            stream.frame_rate_enumerator = 30000;
                            stream.frame_rate_denominator = 1000;
                        }
                        6 => {
                            stream.frame_rate_enumerator = 50000;
                            stream.frame_rate_denominator = 1000;
                        }
                        7 => {
                            stream.frame_rate_enumerator = 60000;
                            stream.frame_rate_denominator = 1001;
                        }
                        8 => {
                            stream.frame_rate_enumerator = 60000;
                            stream.frame_rate_denominator = 1000;
                        }
                        _ => {
                            stream.frame_rate_enumerator = 0;
                            stream.frame_rate_denominator = 0;
                        }
                    }
                }
                0 => {
                    if debug_mode {
                        stream.bit_rate = ((parse & 0xFFFFC0) >> 6) as u64 * 200;
                    }
                    stream.is_vbr = true;
                    stream.is_initialized = true;
                }
                _ => {}
            }
        } else if picture_parse > 0 {
            picture_parse -= 1;
            if picture_parse == 0 {
                let _picture_coding = (parse & 0x38) >> 3;
                if stream.is_initialized {
                    return;
                }
            }
        } else if parse == 0x000001B5 {
            extension_parse = 1;
        } else if extension_parse > 0 {
            extension_parse -= 1;
            if extension_parse == 0 && (parse & 0xF0) == 0x10 {
                sequence_extension_parse = 1;
            }
        } else if sequence_extension_parse > 0 {
            sequence_extension_parse -= 1;
            if sequence_extension_parse == 0 && debug_mode {
                let sequence_extension = (parse & 0x8) >> 3;
                stream.is_interlaced = sequence_extension == 0;
            }
        }
    }
}
