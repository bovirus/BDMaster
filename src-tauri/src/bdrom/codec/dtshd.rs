/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecDTSHD.cs.
 */

use super::dts;
use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::{TSAudioMode, TSStreamType};
use crate::protocol::TSStreamInfo;

const SAMPLE_RATES: [u32; 16] = [
    0x1F40, 0x3E80, 0x7D00, 0x0FA00, 0x1F400, 0x5622, 0x0AC44, 0x15888,
    0x2B110, 0x56220, 0x2EE0, 0x5DC0, 0x0BB80, 0x17700, 0x2EE00, 0x5DC00,
];

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer, bitrate: i64) {
    let st = TSStreamType::from_u8(stream.stream_type);
    if stream.is_initialized
        && (st == TSStreamType::DTSHDSecondaryAudio
            || stream
                .core
                .as_ref()
                .map(|c| c.is_initialized)
                .unwrap_or(false))
    {
        return;
    }

    let mut sync: u32 = 0;
    let mut sync_found = false;
    for _ in 0..buffer.len() {
        sync = sync.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);
        if sync == 0x64582025 {
            sync_found = true;
            break;
        }
    }

    if !sync_found {
        // Fallback: parse the DTS Core sync portion if no HD payload was seen.
        if stream.core.is_none() {
            let core = TSStreamInfo::new(stream.pid, TSStreamType::DTSAudio as u8);
            stream.core = Some(Box::new(core));
        }
        let mut needs_init = true;
        if let Some(c) = &stream.core {
            needs_init = !c.is_initialized;
        }
        if needs_init {
            buffer.begin_read();
            if let Some(core) = stream.core.as_deref_mut() {
                dts::scan(core, buffer, bitrate);
            }
        }
        return;
    }

    buffer.bs_skip_bits_default(8);
    let nu_sub_stream_index = buffer.read_bits4_default(2) as u32;
    let b_blown_up_header = buffer.read_bool_default();
    buffer.bs_skip_bits_default(if b_blown_up_header { 32 } else { 24 });

    let mut nu_num_assets: u32 = 1;
    let b_static_fields_present = buffer.read_bool_default();
    if b_static_fields_present {
        buffer.bs_skip_bits_default(5);
        if buffer.read_bool_default() {
            buffer.bs_skip_bits_default(36);
        }
        let nu_num_audio_present = (buffer.read_bits2_default(3) as u32) + 1;
        nu_num_assets = (buffer.read_bits2_default(3) as u32) + 1;
        let mut _nu_active_ex_ss_mask: Vec<u32> = vec![0; nu_num_audio_present as usize];
        for i in 0..nu_num_audio_present as usize {
            _nu_active_ex_ss_mask[i] = buffer.read_bits4_default(nu_sub_stream_index + 1);
        }
        for _ in 0..nu_num_audio_present {
            for j in 0..(nu_sub_stream_index + 1) {
                if ((j + 1) % 2) == 1 {
                    buffer.bs_skip_bits_default(8);
                }
            }
        }
        if buffer.read_bool_default() {
            buffer.bs_skip_bits_default(2);
            let nu_bits4_mix_out_mask = (buffer.read_bits2_default(2) as u32) * 4 + 4;
            let nu_num_mix_out_configs = (buffer.read_bits2_default(2) as u32) + 1;
            let mut _nu_mix_out_ch_mask: Vec<u32> = vec![0; nu_num_mix_out_configs as usize];
            for i in 0..nu_num_mix_out_configs as usize {
                _nu_mix_out_ch_mask[i] = buffer.read_bits4_default(nu_bits4_mix_out_mask);
            }
        }
    }

    let mut asset_sizes: Vec<u32> = vec![0; nu_num_assets as usize];
    for i in 0..nu_num_assets as usize {
        asset_sizes[i] = if b_blown_up_header {
            buffer.read_bits4_default(20) + 1
        } else {
            buffer.read_bits4_default(16) + 1
        };
    }

    for i in 0..nu_num_assets as usize {
        buffer.bs_skip_bits_default(12);
        if b_static_fields_present {
            if buffer.read_bool_default() {
                buffer.bs_skip_bits_default(4);
            }
            if buffer.read_bool_default() {
                buffer.bs_skip_bits_default(24);
            }
            if buffer.read_bool_default() {
                let nu_info_text_byte_size = (buffer.read_bits2_default(10) as u32) + 1;
                for _ in 0..nu_info_text_byte_size {
                    buffer.read_bits2_default(8);
                }
            }
            let nu_bit_resolution = (buffer.read_bits2_default(5) as u32) + 1;
            let nu_max_sample_rate = buffer.read_bits2_default(4) as u32;
            let nu_total_num_chs = (buffer.read_bits2_default(8) as u32) + 1;
            let mut nu_spkr_activity_mask: u32 = 0;
            if buffer.read_bool_default() {
                if nu_total_num_chs > 2 {
                    buffer.bs_skip_bits_default(1);
                }
                if nu_total_num_chs > 6 {
                    buffer.bs_skip_bits_default(1);
                }
                if buffer.read_bool_default() {
                    let mut nu_num_bits4_sa_mask = buffer.read_bits2_default(2) as u32;
                    nu_num_bits4_sa_mask = nu_num_bits4_sa_mask * 4 + 4;
                    nu_spkr_activity_mask = buffer.read_bits4_default(nu_num_bits4_sa_mask);
                }
            }
            stream.sample_rate = SAMPLE_RATES[nu_max_sample_rate as usize];
            stream.bit_depth = nu_bit_resolution;

            stream.lfe = 0;
            if (nu_spkr_activity_mask & 0x8) == 0x8 {
                stream.lfe += 1;
            }
            if (nu_spkr_activity_mask & 0x1000) == 0x1000 {
                stream.lfe += 1;
            }
            stream.channel_count = nu_total_num_chs.saturating_sub(stream.lfe);
        }
        if nu_num_assets > 1 {
            // TODO mirror BDInfo
            break;
        }
        let _ = i;
    }

    let mut temp2: u32 = 0;
    while buffer.position() < buffer.len() {
        temp2 = temp2.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);
        match temp2 {
            0x41A29547 | 0x655E315E | 0x0A801921 | 0x1D95F262 | 0x47004A03 | 0x5A5A5A5A => {
                let mut temp3: u32 = 0;
                while buffer.position() < buffer.len() {
                    temp3 = temp3.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);
                    if temp3 == 0x02000850 {
                        stream.has_extensions = true;
                        break;
                    }
                }
            }
            _ => {}
        }
        if stream.has_extensions {
            break;
        }
    }

    if let Some(core) = &stream.core {
        if core.audio_mode == TSAudioMode::Extended.label() && stream.channel_count == 5 {
            stream.audio_mode = TSAudioMode::Extended.label().to_string();
        }
    }

    if st == TSStreamType::DTSHDMasterAudio {
        stream.is_vbr = true;
        stream.is_initialized = true;
    } else if bitrate > 0 {
        stream.is_vbr = false;
        stream.bit_rate = bitrate as u64;
        if let Some(core) = &stream.core {
            stream.bit_rate += core.bit_rate;
        }
        stream.is_initialized = stream.bit_rate > 0;
    }
}
