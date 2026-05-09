/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecDTS.cs.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::TSAudioMode;
use crate::protocol::TSStreamInfo;

const DCA_SAMPLE_RATES: [u32; 16] = [
    0, 8000, 16000, 32000, 0, 0, 11025, 22050, 44100, 0, 0, 12000, 24000, 48000, 96000, 192000,
];

const DCA_BIT_RATES: [i64; 32] = [
    32000, 56000, 64000, 96000, 112000, 128000,
    192000, 224000, 256000, 320000, 384000,
    448000, 512000, 576000, 640000, 768000,
    896000, 1024000, 1152000, 1280000, 1344000,
    1408000, 1411200, 1472000, 1509000, 1920000,
    2048000, 3072000, 3840000,
    1,  // open
    2,  // variable
    3,  // lossless
];

const DCA_BITS_PER_SAMPLE: [u32; 7] = [16, 16, 20, 20, 0, 24, 24];

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer, bitrate: i64) {
    if stream.is_initialized {
        return;
    }

    let mut sync: u32 = 0;
    let mut sync_found = false;
    for _ in 0..buffer.len() {
        sync = sync.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);
        if sync == 0x7FFE8001 {
            sync_found = true;
            break;
        }
    }
    if !sync_found {
        return;
    }

    buffer.bs_skip_bits_default(6);
    let crc_present = buffer.read_bits4_default(1);
    buffer.bs_skip_bits_default(7);
    let frame_size = buffer.read_bits4_default(14);
    if frame_size < 95 {
        return;
    }
    buffer.bs_skip_bits_default(6);
    let sample_rate = buffer.read_bits4_default(4);
    if (sample_rate as usize) >= DCA_SAMPLE_RATES.len() {
        return;
    }
    let bit_rate = buffer.read_bits4_default(5);
    if (bit_rate as usize) >= DCA_BIT_RATES.len() {
        return;
    }
    buffer.bs_skip_bits_default(8);
    let ext_coding = buffer.read_bits4_default(1);
    buffer.bs_skip_bits_default(1);
    let lfe = buffer.read_bits4_default(2);
    buffer.bs_skip_bits_default(1);
    if crc_present == 1 {
        buffer.bs_skip_bits_default(16);
    }
    buffer.bs_skip_bits_default(7);
    let source_pcm_res = buffer.read_bits4_default(3);
    buffer.bs_skip_bits_default(2);
    let dialog_norm = buffer.read_bits4_default(4);
    if (source_pcm_res as usize) >= DCA_BITS_PER_SAMPLE.len() {
        return;
    }
    buffer.bs_skip_bits_default(4);
    let total_channels = buffer.read_bits4_default(3) + 1 + ext_coding;

    stream.sample_rate = DCA_SAMPLE_RATES[sample_rate as usize];
    stream.channel_count = total_channels;
    stream.lfe = if lfe > 0 { 1 } else { 0 };
    stream.bit_depth = DCA_BITS_PER_SAMPLE[source_pcm_res as usize];
    stream.dial_norm = -(dialog_norm as i32);
    if (source_pcm_res & 0x1) == 0x1 {
        stream.audio_mode = TSAudioMode::Extended.label().to_string();
    }

    stream.bit_rate = DCA_BIT_RATES[bit_rate as usize] as u64;
    match stream.bit_rate {
        1 => {
            if bitrate > 0 {
                stream.bit_rate = bitrate as u64;
                stream.is_vbr = false;
                stream.is_initialized = true;
            } else {
                stream.bit_rate = 0;
            }
        }
        2 | 3 => {
            stream.is_vbr = true;
            stream.is_initialized = true;
        }
        _ => {
            stream.is_vbr = false;
            stream.is_initialized = true;
        }
    }
}
