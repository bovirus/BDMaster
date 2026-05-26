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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bdrom::types::TSStreamType;

    /// Pack a 32-bit MPEG audio frame header (sync word forced valid) into 4
    /// big-endian bytes, in the field order the parser consumes.
    fn mpa_header(version: u32, layer: u32, bitrate_idx: u32, sr_idx: u32, channel_mode: u32) -> Vec<u8> {
        let mut h: u32 = 0;
        h = (h << 11) | 0x7FF; // sync (11 bits)
        h = (h << 2) | (version & 0x3);
        h = (h << 2) | (layer & 0x3);
        h = (h << 1) | 1; // protection bit
        h = (h << 4) | (bitrate_idx & 0xF);
        h = (h << 2) | (sr_idx & 0x3);
        h = (h << 1) | 0; // padding
        h = (h << 1) | 0; // private
        h = (h << 2) | (channel_mode & 0x3);
        h = (h << 2) | 0; // mode extension
        h = (h << 1) | 0; // copyright
        h = (h << 1) | 0; // original
        h = (h << 2) | 0; // emphasis
        h.to_be_bytes().to_vec()
    }

    fn mpa_stream() -> TSStreamInfo {
        TSStreamInfo::new(0x1100, TSStreamType::MPEG2Video as u8)
    }

    #[test]
    fn invalid_sync_word_is_rejected() {
        let mut stream = mpa_stream();
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(!stream.is_initialized);
    }

    #[test]
    fn mpeg1_layer3_stereo_128k_48k() {
        let data = mpa_header(3, 1, 9, 1, 0);
        let mut stream = mpa_stream();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(stream.is_initialized);
        assert!(!stream.is_vbr);
        assert_eq!(stream.bit_rate, 128_000);
        assert_eq!(stream.sample_rate, 48_000);
        assert_eq!(stream.channel_count, 2);
        assert_eq!(stream.codec_name, "MPEG 1 Layer III");
        assert_eq!(stream.audio_mode, TSAudioMode::Stereo.label());
    }

    #[test]
    fn mpeg1_layer1_mono_448k_44k() {
        let data = mpa_header(3, 3, 14, 0, 3);
        let mut stream = mpa_stream();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert_eq!(stream.bit_rate, 448_000);
        assert_eq!(stream.sample_rate, 44_100);
        assert_eq!(stream.channel_count, 1);
        assert_eq!(stream.codec_name, "MPEG 1 Layer I");
        assert_eq!(stream.audio_mode, TSAudioMode::Mono.label());
    }

    #[test]
    fn full_version_layer_bitrate_matrix_matches_tables() {
        // Exercises every version/layer/bitrate/sample-rate/channel-mode cell and
        // confirms the parser reproduces the lookup tables exactly.
        for version in 0..4u32 {
            for layer in 0..4u32 {
                for br in 0..16u32 {
                    for sr in 0..4u32 {
                        for ch in 0..4u32 {
                            let data = mpa_header(version, layer, br, sr, ch);
                            let mut stream = mpa_stream();
                            let mut buffer = TSStreamBuffer::new(&data);
                            scan(&mut stream, &mut buffer);
                            assert!(stream.is_initialized);
                            assert_eq!(
                                stream.bit_rate,
                                MPA_BITRATE[version as usize][layer as usize][br as usize] as u64
                                    * 1000
                            );
                            assert_eq!(
                                stream.sample_rate,
                                MPA_SAMPLE_RATE[version as usize][sr as usize]
                            );
                            assert_eq!(stream.channel_count, MPA_CHANNELS[ch as usize]);
                            assert_eq!(
                                stream.audio_mode,
                                MPA_CHANNEL_MODES[ch as usize].label()
                            );
                            assert_eq!(
                                stream.codec_name,
                                format!(
                                    "{} {}",
                                    MPA_VERSION[version as usize], MPA_LAYER[layer as usize]
                                )
                            );
                        }
                    }
                }
            }
        }
    }
}
