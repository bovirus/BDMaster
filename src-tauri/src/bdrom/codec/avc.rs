/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecAVC.cs.
 *
 * Note: BDInfo's AVC parser only extracts profile + level from the SPS — it
 * does *not* parse picture width/height/cropping. Picture dimensions come
 * from the MPLS video_format byte. This port preserves that behavior.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::protocol::TSStreamInfo;

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
    let mut parse: u32 = 0;
    let mut access_unit_delimiter_parse: u8 = 0;
    let mut sequence_parameter_set_parse: u8 = 0;
    let mut profile: String = String::new();
    let mut level: String;
    let mut constraint_set3_flag: u8 = 0;

    for _ in 0..buffer.len() {
        let byte = buffer.read_byte(true) as u32;
        parse = (parse << 8).wrapping_add(byte);

        if parse == 0x00000109 {
            access_unit_delimiter_parse = 1;
        } else if access_unit_delimiter_parse > 0 {
            access_unit_delimiter_parse -= 1;
            if access_unit_delimiter_parse == 0 {
                let _ = (parse & 0xFF) >> 5;
                if stream.is_initialized {
                    return;
                }
            }
        } else if parse == 0x00000127 || parse == 0x00000167 {
            sequence_parameter_set_parse = 3;
        } else if sequence_parameter_set_parse > 0 {
            sequence_parameter_set_parse -= 1;
            if !stream.is_initialized {
                match sequence_parameter_set_parse {
                    2 => {
                        profile = match parse & 0xFF {
                            66 => "Baseline Profile".to_string(),
                            77 => "Main Profile".to_string(),
                            88 => "Extended Profile".to_string(),
                            100 => "High Profile".to_string(),
                            110 => "High 10 Profile".to_string(),
                            122 => "High 4:2:2 Profile".to_string(),
                            144 => "High 4:4:4 Profile".to_string(),
                            _ => "Unknown Profile".to_string(),
                        };
                    }
                    1 => {
                        // constraintSet0..3 flags
                        let _cs0 = ((parse & 0x80) >> 7) as u8;
                        let _cs1 = ((parse & 0x40) >> 6) as u8;
                        let _cs2 = ((parse & 0x20) >> 5) as u8;
                        constraint_set3_flag = ((parse & 0x10) >> 4) as u8;
                    }
                    0 => {
                        let b = (parse & 0xFF) as u8;
                        level = if b == 11 && constraint_set3_flag == 1 {
                            "1b".to_string()
                        } else {
                            format!("{}.{}", b / 10, b - ((b / 10) * 10))
                        };
                        stream.encoding_profile = format!("{} {}", profile, level);
                        stream.is_vbr = true;
                        stream.is_initialized = true;
                    }
                    _ => {}
                }
            }
        }
    }
}
