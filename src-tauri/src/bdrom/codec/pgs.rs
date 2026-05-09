/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecPGS.cs.
 */

use std::collections::HashMap;

use super::stream_buffer::TSStreamBuffer;
use crate::protocol::TSStreamInfo;

#[derive(Debug, Clone, Copy, Default)]
pub struct Frame {
    pub started: bool,
    pub forced: bool,
    pub finished: bool,
}

#[derive(Debug, Default)]
pub struct PgsState {
    pub last_frame: Frame,
    pub caption_ids: HashMap<i32, Frame>,
}

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer, state: &mut PgsState) {
    let segment_type = buffer.read_byte(false);
    match segment_type {
        0x15 => {
            read_ods(stream, buffer, state);
        }
        0x16 => {
            read_pcs(stream, buffer, state);
        }
        0x80 => {
            if !state.last_frame.finished {
                state.last_frame.finished = true;
            }
        }
        _ => {}
    }
    stream.is_vbr = true;
}

fn read_ods(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer, state: &mut PgsState) {
    let _segment_size = buffer.read_bits2(16, false);
    let _object_id = buffer.read_bits2(16, false);

    if !state.last_frame.finished {
        if state.last_frame.forced {
            stream.forced_captions += 1;
        } else {
            stream.captions += 1;
        }
    }
}

fn read_pcs(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer, state: &mut PgsState) {
    let _segment_size = buffer.read_bits2(16, false);
    if !stream.is_initialized {
        stream.width = buffer.read_bits2(16, false) as u32;
        stream.height = buffer.read_bits2(16, false) as u32;
        stream.is_initialized = true;
    } else {
        let _ = buffer.read_bits2(16, false);
        let _ = buffer.read_bits2(16, false);
    }

    let _ = buffer.read_byte_default();
    let composition_number = buffer.read_bits2(16, false) as i32;
    let _composition_state = buffer.read_byte(false);
    let _ = buffer.read_bits2(16, false);
    let num_composition_objects = buffer.read_byte(false) as i32;

    for _ in 0..num_composition_objects {
        let _object_id = buffer.read_bits2(16, false);
        let _window_id = buffer.read_byte(false);
        let forced = buffer.read_byte(false);
        let _ = buffer.read_bits2(16, false);
        let _ = buffer.read_bits2(16, false);
        let _ = buffer.read_bits2(16, false);
        let _ = buffer.read_bits2(16, false);
        let _ = buffer.read_bits2(16, false);
        let _ = buffer.read_bits2(16, false);

        state.last_frame = Frame {
            started: true,
            forced: (forced & 0x40) == 0x40,
            finished: false,
        };

        state
            .caption_ids
            .entry(composition_number)
            .or_insert(state.last_frame);
    }
}
