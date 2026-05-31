/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

/*
 * Faithful port of TSCodecDTSHD.cs.
 */

use super::dts;
use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::{TSAudioMode, TSStreamType};
use crate::protocol::TSStreamInfo;

const SAMPLE_RATES: [u32; 16] = [
  0x1F40, 0x3E80, 0x7D00, 0x0FA00, 0x1F400, 0x5622, 0x0AC44, 0x15888, 0x2B110, 0x56220, 0x2EE0, 0x5DC0, 0x0BB80,
  0x17700, 0x2EE00, 0x5DC00,
];

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer, bitrate: i64) {
  let st = TSStreamType::from_u8(stream.stream_type);
  if stream.is_initialized
    && (st == TSStreamType::DTSHDSecondaryAudio || stream.core.as_ref().map(|c| c.is_initialized).unwrap_or(false))
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
      // `nu_max_sample_rate` is a 4-bit field (0..=15) and SAMPLE_RATES has
      // 16 entries, so this is always in range; the defensive lookup keeps
      // the parser panic-free even on a malformed bitstream.
      stream.sample_rate = SAMPLE_RATES.get(nu_max_sample_rate as usize).copied().unwrap_or(0);
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
    // BDInfo deliberately leaves the core dialnorm copy disabled (the block is
    // commented out in TSCodecDTSHD.cs); we mirror that to keep parity:
    //   if core.dial_norm != 0 { stream.dial_norm = core.dial_norm; }
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

#[cfg(test)]
mod tests {
  use super::*;

  fn ma_stream() -> TSStreamInfo {
    TSStreamInfo::new(0x1100, TSStreamType::DTSHDMasterAudio as u8)
  }

  #[test]
  fn empty_buffer_leaves_stream_uninitialized() {
    let mut stream = ma_stream();
    let data: Vec<u8> = Vec::new();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(!stream.is_initialized);
  }

  #[test]
  fn master_audio_with_sync_initializes_as_vbr() {
    // DTS-HD sync 0x64582025, then a minimal header with static fields absent.
    let mut data = vec![0x64, 0x58, 0x20, 0x25];
    // Pad with zero bytes so the substream/header/asset-size reads succeed.
    data.extend(std::iter::repeat(0u8).take(32));
    let mut stream = ma_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(stream.is_initialized, "MA streams init even without bitrate");
    assert!(stream.is_vbr);
  }

  #[test]
  fn no_hd_sync_falls_back_to_core_dts() {
    // No DTS-HD sync present; a DTS core sync (0x7FFE8001) should be parsed
    // into stream.core via the fallback path.
    let mut data = vec![0x7F, 0xFE, 0x80, 0x01];
    data.extend(std::iter::repeat(0u8).take(64));
    let mut stream = TSStreamInfo::new(0x1100, TSStreamType::DTSHDAudio as u8);
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 1_500_000);
    assert!(stream.core.is_some(), "core stream should be created");
  }

  #[test]
  fn does_not_panic_on_truncated_or_garbage_input() {
    // The sample-rate index is a 4-bit field into a 16-entry table and every
    // read defaults on under-run, so no input should panic the parser.
    for len in 4..48usize {
      let mut data = vec![0x64, 0x58, 0x20, 0x25];
      data.extend((0..len).map(|i| (i as u8).wrapping_mul(37) | 0x80));
      let mut stream = ma_stream();
      let mut buffer = TSStreamBuffer::new(&data);
      scan(&mut stream, &mut buffer, 768_000);
    }
  }

  #[test]
  fn already_initialized_secondary_audio_returns_early() {
    let mut stream = TSStreamInfo::new(0x1100, TSStreamType::DTSHDSecondaryAudio as u8);
    stream.is_initialized = true;
    stream.sample_rate = 48000;
    let data = vec![0u8; 16];
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    // Unchanged because the early-return guard fired.
    assert_eq!(stream.sample_rate, 48000);
  }

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

  /// DTS-HD frame with the static-fields header + one asset describing 6
  /// channels (5.1), 48 kHz, 24-bit.
  fn build_static_fields_frame() -> Vec<u8> {
    let mut b = BitWriter::default();
    b.put(0xFF, 8); // skip 8
    b.put(0, 2); // nu_sub_stream_index = 0
    b.put(0, 1); // b_blown_up_header = 0
    b.put(0xFFFFFF, 24); // skip 24
    b.put(1, 1); // b_static_fields_present = 1
    b.put(0, 5); // skip 5
    b.put(0, 1); // (no 36-bit timestamp skip)
    b.put(0, 3); // nu_num_audio_present field -> 1
    b.put(0, 3); // nu_num_assets field -> 1
    b.put(0, 1); // nu_active_ex_ss_mask[0] (sub_stream_index+1 bits)
    b.put(0xFF, 8); // (j+1)%2==1 skip 8
    b.put(0, 1); // mixer metadata absent
    b.put(100, 16); // asset_sizes[0] field
    // asset 0:
    b.put(0xFFF, 12); // skip 12
    b.put(0, 1); // (no 4-bit skip)
    b.put(0, 1); // (no 24-bit skip)
    b.put(0, 1); // info text absent
    b.put(23, 5); // nu_bit_resolution -> 24
    b.put(12, 4); // nu_max_sample_rate -> 48000
    b.put(5, 8); // nu_total_num_chs -> 6
    b.put(1, 1); // speaker-activity present
    b.put(0, 1); // (>2 chs) skip 1
    b.put(1, 1); // sa-mask present
    b.put(0, 2); // nu_num_bits4_sa_mask -> 4-bit mask
    b.put(0x8, 4); // speaker mask: LFE bit set
    let mut data = vec![0x64, 0x58, 0x20, 0x25];
    data.extend(b.bytes());
    data.extend(std::iter::repeat(0u8).take(8)); // no extension sync
    data
  }

  #[test]
  fn static_fields_extract_channels_rate_and_depth() {
    let data = build_static_fields_frame();
    let mut stream = ma_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(stream.is_initialized);
    assert!(stream.is_vbr); // MA is VBR
    assert_eq!(stream.sample_rate, 48000);
    assert_eq!(stream.bit_depth, 24);
    assert_eq!(stream.lfe, 1);
    assert_eq!(stream.channel_count, 5);
  }

  #[test]
  fn hd_hr_with_bitrate_hint_sets_cbr_bitrate() {
    // A non-MA HD stream with a bitrate hint reports CBR at that bitrate.
    let data = build_static_fields_frame();
    let mut stream = TSStreamInfo::new(0x1100, TSStreamType::DTSHDAudio as u8);
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 1_500_000);
    assert!(stream.is_initialized);
    assert!(!stream.is_vbr);
    assert_eq!(stream.bit_rate, 1_500_000);
  }

  #[test]
  fn dtsx_extension_marker_sets_has_extensions() {
    // Static fields absent so the header ends byte-aligned at 8 bytes; the
    // DTS:X extension sync (0x41A29547 ... 0x02000850) then follows.
    let mut b = BitWriter::default();
    b.put(0xFF, 8); // skip 8
    b.put(0, 2); // nu_sub_stream_index = 0
    b.put(0, 1); // b_blown_up_header = 0
    b.put(0xFFFFFF, 24); // skip 24
    b.put(0, 1); // b_static_fields_present = 0
    b.put(100, 16); // asset_sizes[0]
    b.put(0xFFF, 12); // asset skip 12
    let mut data = vec![0x64, 0x58, 0x20, 0x25];
    data.extend(b.bytes()); // exactly 8 bytes -> byte aligned
    data.extend_from_slice(&[0x41, 0xA2, 0x95, 0x47, 0x02, 0x00, 0x08, 0x50]);

    let mut stream = ma_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(stream.has_extensions, "DTS:X extension marker detected");
  }
}
