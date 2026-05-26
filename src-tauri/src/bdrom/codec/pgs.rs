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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bdrom::types::TSStreamType;

    fn pgs_stream() -> TSStreamInfo {
        TSStreamInfo::new(0x1200, TSStreamType::PresentationGraphics as u8)
    }

    /// Build a Presentation Composition Segment with one composition object.
    fn pcs(width: u16, height: u16, forced: bool, comp_num: u16) -> Vec<u8> {
        let mut v = vec![0x16];
        v.extend_from_slice(&[0x00, 0x13]); // segment size
        v.extend_from_slice(&width.to_be_bytes());
        v.extend_from_slice(&height.to_be_bytes());
        v.push(0x10); // frame rate / reserved
        v.extend_from_slice(&comp_num.to_be_bytes());
        v.push(0x80); // composition state
        v.extend_from_slice(&[0x00, 0x00]); // palette update / id
        v.push(0x01); // num composition objects
        v.extend_from_slice(&[0x00, 0x00]); // object id
        v.push(0x00); // window id
        v.push(if forced { 0x40 } else { 0x00 }); // cropped/forced flag
        v.extend(std::iter::repeat(0u8).take(12)); // 6 x 16-bit position/crop fields
        v
    }

    /// Build an Object Definition Segment.
    fn ods(object_id: u16) -> Vec<u8> {
        let mut v = vec![0x15];
        v.extend_from_slice(&[0x00, 0x10]); // segment size
        v.extend_from_slice(&object_id.to_be_bytes());
        v
    }

    fn run(stream: &mut TSStreamInfo, state: &mut PgsState, data: &[u8]) {
        let mut buffer = TSStreamBuffer::new(data);
        scan(stream, &mut buffer, state);
    }

    #[test]
    fn pcs_sets_dimensions_and_initializes() {
        let mut stream = pgs_stream();
        let mut state = PgsState::default();
        run(&mut stream, &mut state, &pcs(1920, 1080, false, 1));
        assert!(stream.is_initialized);
        assert!(stream.is_vbr);
        assert_eq!(stream.width, 1920);
        assert_eq!(stream.height, 1080);
    }

    #[test]
    fn forced_composition_then_ods_counts_forced_caption() {
        let mut stream = pgs_stream();
        let mut state = PgsState::default();
        run(&mut stream, &mut state, &pcs(1920, 1080, true, 1));
        assert!(state.last_frame.forced);
        run(&mut stream, &mut state, &ods(0));
        assert_eq!(stream.forced_captions, 1);
        assert_eq!(stream.captions, 0);
    }

    #[test]
    fn normal_composition_then_ods_counts_caption() {
        let mut stream = pgs_stream();
        let mut state = PgsState::default();
        run(&mut stream, &mut state, &pcs(1920, 1080, false, 1));
        run(&mut stream, &mut state, &ods(0));
        assert_eq!(stream.captions, 1);
        assert_eq!(stream.forced_captions, 0);
    }

    #[test]
    fn end_of_display_stops_further_counting() {
        let mut stream = pgs_stream();
        let mut state = PgsState::default();
        run(&mut stream, &mut state, &pcs(1920, 1080, false, 1));
        run(&mut stream, &mut state, &ods(0)); // counts -> 1
        run(&mut stream, &mut state, &[0x80]); // end-of-display marker
        assert!(state.last_frame.finished);
        run(&mut stream, &mut state, &ods(1)); // must not count after finish
        assert_eq!(stream.captions, 1);
    }

    #[test]
    fn unknown_segment_type_is_ignored() {
        let mut stream = pgs_stream();
        let mut state = PgsState::default();
        run(&mut stream, &mut state, &[0x99, 0x00, 0x00]);
        assert!(stream.is_vbr);
        assert!(!stream.is_initialized);
        assert_eq!(stream.captions, 0);
    }
}
