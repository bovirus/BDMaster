/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecAC3.cs.
 */

use super::stream_buffer::{SeekOrigin, TSStreamBuffer};
use crate::bdrom::types::{TSAudioMode, TSStreamType};
use crate::protocol::TSStreamInfo;

const AC3_BITRATE: [i32; 19] = [
    32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640,
];

const AC3_CHANNELS: [u8; 8] = [2, 1, 2, 3, 3, 4, 4, 5];

fn ac3_chan_map(chan_map: u32) -> u32 {
    let mut channels: u32 = 0;
    for i in 0u8..16 {
        if (chan_map & (1 << (15 - i))) != 0 {
            match i {
                5 | 6 | 9 | 10 | 11 => channels += 2,
                _ => {}
            }
        }
    }
    channels
}

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
    if stream.is_initialized {
        return;
    }

    let sync = match buffer.read_bytes(2) {
        Some(s) => s,
        None => return,
    };
    if sync[0] != 0x0B || sync[1] != 0x77 {
        return;
    }

    let second_frame = stream.channel_count > 0;

    let mut sr_code: u32;
    let mut frame_size: u32 = 0;
    let mut frame_size_code: u32 = 0;
    let channel_mode: u32;
    let mut lfe_on: u32;
    let mut dial_norm: u32 = 0;
    let mut dial_norm_ext: u32 = 0;
    let mut num_blocks: u32 = 0;

    let hdr = match buffer.read_bytes(4) {
        Some(h) => h,
        None => return,
    };
    let mut bsid: u32 = ((hdr[3] & 0xF8) >> 3) as u32;
    buffer.seek(-4, SeekOrigin::Current);

    let st = TSStreamType::from_u8(stream.stream_type);

    let mut audio_mode = parse_audio_mode_from_string(&stream.audio_mode);

    if bsid <= 10 {
        buffer.bs_skip_bytes_default(2);
        sr_code = buffer.read_bits2_default(2) as u32;
        frame_size_code = buffer.read_bits2_default(6) as u32;
        bsid = buffer.read_bits2_default(5) as u32;
        buffer.bs_skip_bits_default(3);

        channel_mode = buffer.read_bits2_default(3) as u32;
        if (channel_mode & 0x1) > 0 && channel_mode != 0x1 {
            buffer.bs_skip_bits_default(2);
        }
        if (channel_mode & 0x4) > 0 {
            buffer.bs_skip_bits_default(2);
        }
        if channel_mode == 0x2 {
            let dsurmod = buffer.read_bits2_default(2);
            if dsurmod == 0x2 {
                audio_mode = TSAudioMode::Surround;
            }
        }
        lfe_on = buffer.read_bits2_default(1) as u32;
        dial_norm = buffer.read_bits2_default(5) as u32;
        if buffer.read_bool_default() {
            buffer.bs_skip_bits_default(8);
        }
        if buffer.read_bool_default() {
            buffer.bs_skip_bits_default(8);
        }
        if buffer.read_bool_default() {
            buffer.bs_skip_bits_default(7);
        }
        if channel_mode == 0 {
            buffer.bs_skip_bits_default(5);
            if buffer.read_bool_default() {
                buffer.bs_skip_bits_default(8);
            }
            if buffer.read_bool_default() {
                buffer.bs_skip_bits_default(8);
            }
            if buffer.read_bool_default() {
                buffer.bs_skip_bits_default(7);
            }
        }
        buffer.bs_skip_bits_default(2);
        if bsid == 6 {
            if buffer.read_bool_default() {
                buffer.bs_skip_bits_default(14);
            }
            if buffer.read_bool_default() {
                let dsurexmod = buffer.read_bits2_default(2);
                let dheadphonmod = buffer.read_bits2_default(2);
                if dheadphonmod == 0x2 {
                    // TODO (per BDInfo)
                }
                buffer.bs_skip_bits_default(10);
                if dsurexmod == 2 {
                    audio_mode = TSAudioMode::Extended;
                }
            }
        }
    } else {
        let frame_type = buffer.read_bits2_default(2) as u32;
        buffer.bs_skip_bits_default(3);

        frame_size = (buffer.read_bits4_default(11) + 1) << 1;

        sr_code = buffer.read_bits2_default(2) as u32;
        if sr_code == 3 {
            sr_code = buffer.read_bits2_default(2) as u32;
            num_blocks = 3;
        } else {
            num_blocks = buffer.read_bits2_default(2) as u32;
        }
        channel_mode = buffer.read_bits2_default(3) as u32;
        lfe_on = buffer.read_bits2_default(1) as u32;
        bsid = buffer.read_bits2_default(5) as u32;
        dial_norm_ext = buffer.read_bits2_default(5) as u32;

        if buffer.read_bool_default() {
            buffer.bs_skip_bits_default(8);
        }
        if channel_mode == 0 {
            buffer.bs_skip_bits_default(5);
            if buffer.read_bool_default() {
                buffer.bs_skip_bits_default(8);
            }
        }
        if frame_type == 1 {
            // dependent stream — clone current state as the core
            let mut core = Box::new(stream.clone());
            core.stream_type = TSStreamType::AC3Audio as u8;
            stream.core = Some(core);

            if buffer.read_bool_default() {
                let chanmap = buffer.read_bits4_default(16);
                if let Some(core) = &stream.core {
                    stream.channel_count = core.channel_count;
                }
                stream.channel_count += ac3_chan_map(chanmap);
                if let Some(core) = &stream.core {
                    lfe_on = core.lfe;
                }
            }
        }

        // EMDF (Atmos JOC detection)
        let mut emdf_found = false;
        loop {
            if buffer.position() >= buffer.len() {
                break;
            }
            let emdf_sync = buffer.read_bits4_default(16);
            if emdf_sync == 0x5838 {
                emdf_found = true;
                break;
            }
            buffer.seek(-2, SeekOrigin::Current);
            buffer.bs_skip_bits_default(1);
            if buffer.position() >= buffer.len() {
                break;
            }
        }

        if emdf_found {
            let emdf_container_size = buffer.read_bits4_default(16);
            let remain_after_emdf = buffer.data_bit_stream_remain() - (emdf_container_size as i64) * 8;

            let mut emdf_version = buffer.read_bits2_default(2) as u32;
            if emdf_version == 3 {
                emdf_version += buffer.read_bits2_default(2) as u32;
            }

            if emdf_version > 0 {
                let to_skip = (buffer.data_bit_stream_remain() - remain_after_emdf) as u32;
                buffer.bs_skip_bits_default(to_skip);
            } else {
                let temp = buffer.read_bits2_default(3);
                if temp == 0x7 {
                    buffer.bs_skip_bits_default(2);
                }
                let mut emdf_payload_id = buffer.read_bits2_default(5);

                if emdf_payload_id > 0 && emdf_payload_id < 16 {
                    if emdf_payload_id == 0x1F {
                        buffer.bs_skip_bits_default(5);
                    }
                    emdf_payload_config(buffer);
                    let emdf_payload_size = buffer.read_bits2_default(8) as u32 * 8;
                    buffer.bs_skip_bits_default(emdf_payload_size + 1);
                }

                while {
                    emdf_payload_id = buffer.read_bits2_default(5);
                    emdf_payload_id != 14 && buffer.position() < buffer.len()
                } {
                    if emdf_payload_id == 0x1F {
                        buffer.bs_skip_bits_default(5);
                    }
                    emdf_payload_config(buffer);
                    let emdf_payload_size = buffer.read_bits2_default(8) as u32 * 8;
                    buffer.read_bits4_default(emdf_payload_size + 1);
                }

                if buffer.position() < buffer.len() && emdf_payload_id == 14 {
                    emdf_payload_config(buffer);
                    buffer.bs_skip_bits_default(12);
                    let joc_num_objects_bits = buffer.read_bits2_default(6);
                    if joc_num_objects_bits > 0 {
                        stream.has_extensions = true;
                    }
                }
            }
        }
    }

    if channel_mode < 8 && stream.channel_count == 0 {
        stream.channel_count = AC3_CHANNELS[channel_mode as usize] as u32;
    }

    if audio_mode == TSAudioMode::Unknown {
        audio_mode = match channel_mode {
            0 => TSAudioMode::DualMono,
            2 => TSAudioMode::Stereo,
            _ => TSAudioMode::Unknown,
        };
    }
    stream.audio_mode = audio_mode.label().to_string();

    stream.sample_rate = match sr_code {
        0 => 48000,
        1 => 44100,
        2 => 32000,
        _ => 0,
    };

    if bsid <= 10 {
        let f_size = frame_size_code >> 1;
        if f_size < 19 {
            stream.bit_rate = AC3_BITRATE[f_size as usize] as u64 * 1000;
        }
    } else {
        if num_blocks > 0 {
            stream.bit_rate = (4.0 * frame_size as f64 * stream.sample_rate as f64
                / (num_blocks as f64 * 256.0)) as u64;
        }
        if let Some(core) = &stream.core {
            stream.bit_rate += core.bit_rate;
        }
    }

    stream.lfe = lfe_on;

    if st != TSStreamType::AC3PlusSecondaryAudio {
        if (st == TSStreamType::AC3PlusAudio && bsid == 6) || st == TSStreamType::AC3Audio {
            stream.dial_norm = -(dial_norm as i32);
        } else if st == TSStreamType::AC3PlusAudio && second_frame {
            stream.dial_norm = -(dial_norm_ext as i32);
        }
    }
    stream.is_vbr = false;
    if st == TSStreamType::AC3PlusAudio && bsid == 6 && !second_frame {
        stream.is_initialized = false;
    } else {
        stream.is_initialized = true;
    }
}

fn emdf_payload_config(buffer: &mut TSStreamBuffer) {
    let sample_offset_e = buffer.read_bool_default();
    if sample_offset_e {
        buffer.bs_skip_bits_default(12);
    }
    if buffer.read_bool_default() {
        buffer.bs_skip_bits_default(11);
    }
    if buffer.read_bool_default() {
        buffer.bs_skip_bits_default(2);
    }
    if buffer.read_bool_default() {
        buffer.bs_skip_bits_default(8);
    }
    if !buffer.read_bool_default() {
        buffer.bs_skip_bits_default(1);
        if !sample_offset_e {
            if buffer.read_bool_default() {
                buffer.bs_skip_bits_default(9);
            }
        }
    }
}

fn parse_audio_mode_from_string(s: &str) -> TSAudioMode {
    match s {
        "DualMono" => TSAudioMode::DualMono,
        "Stereo" => TSAudioMode::Stereo,
        "Surround" => TSAudioMode::Surround,
        "Extended" => TSAudioMode::Extended,
        "JointStereo" => TSAudioMode::JointStereo,
        "Mono" => TSAudioMode::Mono,
        _ => TSAudioMode::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MSB-first bit accumulator matching the parser's read order.
    #[derive(Default)]
    struct BitWriter {
        bits: Vec<bool>,
    }
    impl BitWriter {
        fn put(&mut self, val: u32, n: u32) {
            for i in (0..n).rev() {
                self.bits.push((val >> i) & 1 == 1);
            }
        }
        fn bytes(&self) -> Vec<u8> {
            let mut out = vec![0u8; (self.bits.len() + 7) / 8];
            for (i, b) in self.bits.iter().enumerate() {
                if *b {
                    out[i / 8] |= 1 << (7 - (i % 8));
                }
            }
            out
        }
    }

    fn ac3_stream() -> TSStreamInfo {
        TSStreamInfo::new(0x1100, TSStreamType::AC3Audio as u8)
    }

    #[test]
    fn invalid_sync_word_is_rejected() {
        let mut stream = ac3_stream();
        let data = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(!stream.is_initialized);
    }

    #[test]
    fn chan_map_helper_matches_bdinfo() {
        assert_eq!(ac3_chan_map(0), 0);
        // Bit 15-5 == bit 10 maps (i == 5) -> +2.
        assert_eq!(ac3_chan_map(1 << 10), 2);
        // All five contributing positions (i = 5,6,9,10,11) -> +2 each = 10.
        let mask = (1 << 10) | (1 << 9) | (1 << 6) | (1 << 5) | (1 << 4);
        assert_eq!(ac3_chan_map(mask), 10);
    }

    #[test]
    fn legacy_ac3_frame_is_parsed() {
        // Build a legacy AC3 (bsid 8) sync frame: 48 kHz, frmsizecod 0 ->
        // 32 kbit/s, acmod 7 (3/2 -> 5 channels), LFE on, dialnorm 27.
        let mut bits = BitWriter::default();
        bits.put(7, 3); // acmod
        bits.put(0, 2); // cmixlev (acmod has bit0 and != 1)
        bits.put(0, 2); // surmixlev (acmod has bit2)
        bits.put(1, 1); // lfeon
        bits.put(27, 5); // dialnorm
        bits.put(0, 1); // compre absent
        bits.put(0, 1); // langcode absent
        bits.put(0, 1); // audprodie absent
        bits.put(0, 2); // copyright + original
        bits.put(0, 8); // trailing padding

        let mut data = vec![
            0x0B, 0x77, // sync
            0x00, 0x00, // crc1 (skipped)
            0x00, // fscod=0, frmsizecod=0
            0x40, // bsid=8, bsmod=0
        ];
        data.extend(bits.bytes());

        let mut stream = ac3_stream();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);

        assert!(stream.is_initialized);
        assert!(!stream.is_vbr);
        assert_eq!(stream.sample_rate, 48000);
        assert_eq!(stream.bit_rate, 32_000);
        assert_eq!(stream.channel_count, 5);
        assert_eq!(stream.lfe, 1);
        assert_eq!(stream.dial_norm, -27);
    }

    #[test]
    fn does_not_panic_on_truncated_or_garbage_input() {
        for len in 0..40usize {
            let mut data = vec![0x0B, 0x77];
            data.extend((0..len).map(|i| (i as u8).wrapping_mul(53) | 0x11));
            let mut stream = ac3_stream();
            let mut buffer = TSStreamBuffer::new(&data);
            scan(&mut stream, &mut buffer);
        }
    }
}
