/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecMPA.cs.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::TSAudioMode;
use crate::protocol::TSStreamInfo;

const MPA_BITRATE: [[[u32; 16]; 4]; 4] = [
    // MPEG Version 2.5
    [
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0],
    ],
    // reserved
    [
        [0; 16], [0; 16], [0; 16], [0; 16],
    ],
    // MPEG Version 2
    [
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0],
    ],
    // MPEG Version 1
    [
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0],
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0],
    ],
];

const MPA_SAMPLE_RATE: [[u32; 4]; 4] = [
    [11025, 12000, 8000, 0],
    [0, 0, 0, 0],
    [22050, 24000, 16000, 0],
    [44100, 48000, 32000, 0],
];

const MPA_CHANNEL_MODES: [TSAudioMode; 4] = [
    TSAudioMode::Stereo,
    TSAudioMode::JointStereo,
    TSAudioMode::DualMono,
    TSAudioMode::Mono,
];

const MPA_VERSION: [&str; 4] = ["MPEG 2.5", "Unknown MPEG", "MPEG 2", "MPEG 1"];

const MPA_LAYER: [&str; 4] = ["Unknown Layer", "Layer III", "Layer II", "Layer I"];

const MPA_CHANNELS: [u32; 4] = [2, 2, 2, 1];

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
    if stream.is_initialized {
        return;
    }

    let sync_word = (buffer.read_bits2_default(11) as u32) << 5;
    if sync_word != 0b1111_1111_1110_0000 {
        return;
    }

    let audio_version_id = buffer.read_bits2_default(2) as usize;
    let layer_index = buffer.read_bits2_default(2) as usize;
    let _protection_bit = buffer.read_bool_default();
    let bitrate_index = buffer.read_bits2_default(4) as usize;
    let sampling_rate_index = buffer.read_bits2_default(2) as usize;
    let _padding = buffer.read_bool_default();
    let _private_bit = buffer.read_bool_default();
    let channel_mode = buffer.read_bits2_default(2) as usize;
    let _mode_extension = buffer.read_bits2_default(2);
    let _copyright_bit = buffer.read_bool_default();
    let _original_bit = buffer.read_bool_default();
    let _emphasis = buffer.read_bits2_default(2);

    stream.bit_rate = MPA_BITRATE[audio_version_id][layer_index][bitrate_index] as u64 * 1000;
    stream.sample_rate = MPA_SAMPLE_RATE[audio_version_id][sampling_rate_index];
    stream.audio_mode = MPA_CHANNEL_MODES[channel_mode].label().to_string();
    stream.channel_count = MPA_CHANNELS[channel_mode];
    stream.lfe = 0;

    stream.codec_name = format!("{} {}", MPA_VERSION[audio_version_id], MPA_LAYER[layer_index]);
    stream.is_vbr = false;
    stream.is_initialized = true;
}
