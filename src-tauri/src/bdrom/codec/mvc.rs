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
