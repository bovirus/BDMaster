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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bdrom::types::TSStreamType;

    /// Pack an ADTS fixed header (28 bits, sync forced valid) MSB-aligned into 4
    /// bytes, matching the parser's read order.
    fn adts_header(version: u32, profile: u32, sr_idx: u32, channel_mode: u32) -> Vec<u8> {
        let mut h: u32 = 0;
        h = (h << 12) | 0xFFF; // sync (12 bits)
        h = (h << 1) | (version & 0x1);
        h = (h << 2) | 0; // layer
        h = (h << 1) | 1; // protection_absent
        h = (h << 2) | (profile & 0x3);
        h = (h << 4) | (sr_idx & 0xF);
        h = (h << 1) | 0; // private
        h = (h << 3) | (channel_mode & 0x7);
        h = (h << 1) | 0; // original
        h = (h << 1) | 0; // home
        (h << 4).to_be_bytes().to_vec()
    }

    fn aac_stream() -> TSStreamInfo {
        TSStreamInfo::new(0x1100, TSStreamType::MPEG2Video as u8)
    }

    #[test]
    fn invalid_sync_word_is_rejected() {
        let mut stream = aac_stream();
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(!stream.is_initialized);
    }

    #[test]
    fn aac_profile_table_matches_bdinfo() {
        assert_eq!(aac_profile(0), "AAC Main");
        assert_eq!(aac_profile(1), "AAC LC");
        assert_eq!(aac_profile(2), "AAC SSR");
        assert_eq!(aac_profile(3), "AAC LTP");
        assert_eq!(aac_profile(16), "ER AAC LC");
        assert_eq!(aac_profile(18), "ER AAC LTP");
        assert_eq!(aac_profile(36), "SLS");
        assert_eq!(aac_profile(99), "");
    }

    #[test]
    fn full_samplerate_and_channel_matrix_matches_tables() {
        for version in 0..2u32 {
            for profile in 0..4u32 {
                for sr_idx in 0..16u32 {
                    for cm in 0..8u32 {
                        let data = adts_header(version, profile, sr_idx, cm);
                        let mut stream = aac_stream();
                        let mut buffer = TSStreamBuffer::new(&data);
                        scan(&mut stream, &mut buffer);
                        assert!(stream.is_initialized);
                        assert!(stream.is_vbr);

                        let expected_sr = if sr_idx <= 13 {
                            AAC_SAMPLE_RATES[sr_idx as usize]
                        } else {
                            0
                        };
                        assert_eq!(stream.sample_rate, expected_sr, "sr_idx {sr_idx}");

                        let mut expected_ch = AAC_CHANNELS[cm as usize];
                        let expected_lfe = if cm == 7 {
                            if expected_ch > 0 {
                                expected_ch -= 1;
                            }
                            1
                        } else {
                            0
                        };
                        assert_eq!(stream.channel_count, expected_ch, "cm {cm}");
                        assert_eq!(stream.lfe, expected_lfe, "cm {cm}");
                        assert_eq!(
                            stream.audio_mode,
                            AAC_CHANNEL_MODES[cm as usize].label(),
                            "cm {cm}"
                        );
                        assert_eq!(
                            stream.codec_name,
                            format!("{} {}", AAC_ID[version as usize], aac_profile(profile as i32))
                        );
                    }
                }
            }
        }
    }
}
