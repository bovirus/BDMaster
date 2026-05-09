/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecAAC.cs.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::TSAudioMode;
use crate::protocol::TSStreamInfo;

const AAC_ID: [&str; 2] = ["MPEG-4", "MPEG-2"];

fn aac_profile(profile_type: i32) -> &'static str {
    match profile_type {
        0 => "AAC Main",
        1 => "AAC LC",
        2 => "AAC SSR",
        3 => "AAC LTP",
        16 => "ER AAC LC",
        18 => "ER AAC LTP",
        36 => "SLS",
        _ => "",
    }
}

const AAC_SAMPLE_RATES: [u32; 31] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050,
    16000, 12000, 11025, 8000, 7350, 0, 0, 57600,
    51200, 40000, 38400, 34150, 28800, 25600, 20000, 19200,
    17075, 14400, 12800, 9600, 0, 0, 0,
];

const AAC_CHANNELS: [u32; 8] = [0, 1, 2, 3, 4, 5, 6, 8];

const AAC_CHANNEL_MODES: [TSAudioMode; 8] = [
    TSAudioMode::Unknown,
    TSAudioMode::Mono,
    TSAudioMode::Stereo,
    TSAudioMode::Extended,
    TSAudioMode::Surround,
    TSAudioMode::Surround,
    TSAudioMode::Surround,
    TSAudioMode::Surround,
];

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
    if stream.is_initialized {
        return;
    }

    let sync_word = buffer.read_bits2_default(12);
    if sync_word != 0b1111_1111_1111 {
        return;
    }

    let audio_version_id = buffer.read_bits2_default(1) as usize;
    let _layer_index = buffer.read_bits2_default(2);
    let _protection_absent = buffer.read_bool_default();
    let profile_object_type = buffer.read_bits2_default(2) as i32;
    let sampling_rate_index = buffer.read_bits2_default(4) as usize;
    let _private_bit = buffer.read_bool_default();
    let channel_mode = buffer.read_bits2_default(3) as usize;
    let _original_bit = buffer.read_bool_default();
    let _home = buffer.read_bool_default();

    if sampling_rate_index <= 13 {
        stream.sample_rate = AAC_SAMPLE_RATES[sampling_rate_index];
    } else {
        stream.sample_rate = 0;
    }

    if channel_mode < 8 {
        stream.audio_mode = AAC_CHANNEL_MODES[channel_mode].label().to_string();
        stream.channel_count = AAC_CHANNELS[channel_mode];
    } else {
        stream.channel_count = 0;
        stream.audio_mode = TSAudioMode::Unknown.label().to_string();
    }

    if channel_mode >= 7 && channel_mode <= 8 {
        if stream.channel_count > 0 {
            stream.channel_count -= 1;
        }
        stream.lfe = 1;
    } else {
        stream.lfe = 0;
    }

    let id = AAC_ID.get(audio_version_id).copied().unwrap_or("");
    stream.codec_name = format!("{} {}", id, aac_profile(profile_object_type));
    stream.is_vbr = true;
    stream.is_initialized = true;
}
