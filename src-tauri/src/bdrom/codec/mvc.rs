/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecMVC.cs.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::protocol::TSStreamInfo;

pub fn scan(stream: &mut TSStreamInfo, _buffer: &mut TSStreamBuffer) {
    stream.is_vbr = true;
    stream.is_initialized = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bdrom::types::TSStreamType;

    #[test]
    fn scan_marks_stream_vbr_and_initialized() {
        // Mirrors BDInfo's TSCodecMVC.Scan, which only sets these two flags.
        let mut stream = TSStreamInfo::new(0x1012, TSStreamType::MVCVideo as u8);
        let data: Vec<u8> = Vec::new();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(stream.is_vbr);
        assert!(stream.is_initialized);
    }
}
