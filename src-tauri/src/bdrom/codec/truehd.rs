/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecTrueHD.cs.
 */

use super::ac3;
use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::TSStreamType;
use crate::protocol::TSStreamInfo;

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
    if stream.is_initialized
        && stream
            .core
            .as_ref()
            .map(|c| c.is_initialized)
            .unwrap_or(false)
    {
        return;
    }

    let mut sync: u32 = 0;
    let mut sync_found = false;
    for _ in 0..buffer.len() {
        sync = sync.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);
        if sync == 0xF8726FBA {
            sync_found = true;
            break;
        }
    }

    if !sync_found {
        if stream.core.is_none() {
            let core = TSStreamInfo::new(stream.pid, TSStreamType::AC3Audio as u8);
            stream.core = Some(Box::new(core));
        }
        let mut needs_init = true;
        if let Some(c) = &stream.core {
            needs_init = !c.is_initialized;
        }
        if needs_init {
            buffer.begin_read();
            if let Some(core) = stream.core.as_deref_mut() {
                ac3::scan(core, buffer);
            }
        }
        return;
    }

    let ratebits = buffer.read_bits2_default(4) as u32;
    if ratebits != 0xF {
        stream.sample_rate =
            (if (ratebits & 8) > 0 { 44100u32 } else { 48000u32 }) << (ratebits & 7);
    }
    buffer.bs_skip_bits_default(15);

    stream.channel_count = 0;
    stream.lfe = 0;
    if buffer.read_bool_default() {
        stream.lfe += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.lfe += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }

    buffer.bs_skip_bits_default(49);

    let mut peak_bitrate = buffer.read_bits4_default(15);
    peak_bitrate = (peak_bitrate.wrapping_mul(stream.sample_rate)) >> 4;

    let denom = (stream.channel_count + stream.lfe).max(1) as f64 * stream.sample_rate.max(1) as f64;
    let peak_bitdepth = peak_bitrate as f64 / denom;

    stream.bit_depth = if peak_bitdepth > 14.0 { 24 } else { 16 };

    buffer.bs_skip_bits_default(79);

    let has_extensions = buffer.read_bool_default();
    let num_extensions = (buffer.read_bits2_default(4) as u32 * 2) + 1;
    let mut has_content = buffer.read_bits4_default(4) != 0;

    if has_extensions {
        for _ in 0..num_extensions {
            if buffer.read_bits2_default(8) != 0 {
                has_content = true;
            }
        }
        if has_content {
            stream.has_extensions = true;
        }
    }

    stream.is_vbr = true;
    if let Some(c) = &stream.core {
        if c.is_initialized {
            stream.is_initialized = true;
        }
    }
}
