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
 * Faithful port of TSCodecDTS.cs.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::TSAudioMode;
use crate::protocol::TSStreamInfo;

const DCA_SAMPLE_RATES: [u32; 16] = [
  0, 8000, 16000, 32000, 0, 0, 11025, 22050, 44100, 0, 0, 12000, 24000, 48000, 96000, 192000,
];

const DCA_BIT_RATES: [i64; 32] = [
  32000, 56000, 64000, 96000, 112000, 128000, 192000, 224000, 256000, 320000, 384000, 448000, 512000, 576000, 640000,
  768000, 896000, 1024000, 1152000, 1280000, 1344000, 1408000, 1411200, 1472000, 1509000, 1920000, 2048000, 3072000,
  3840000, 1, // open
  2, // variable
  3, // lossless
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::bdrom::types::TSStreamType;

  /// MSB-first bit accumulator, mirroring how `read_bits4`/`bs_skip_bits`
  /// consume the bitstream.
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

  #[allow(clippy::too_many_arguments)]
  fn dts_frame(
    crc: u32,
    frame_size: u32,
    sr: u32,
    br: u32,
    ext: u32,
    lfe: u32,
    pcm_res: u32,
    dialog_norm: u32,
    chan_base: u32,
  ) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.put(0, 6); // skip
    w.put(crc, 1); // crc present
    w.put(0, 7); // skip
    w.put(frame_size, 14);
    w.put(0, 6); // skip
    w.put(sr, 4);
    w.put(br, 5);
    w.put(0, 8); // skip
    w.put(ext, 1);
    w.put(0, 1); // skip
    w.put(lfe, 2);
    w.put(0, 1); // skip
    if crc == 1 {
      w.put(0, 16);
    }
    w.put(0, 7); // skip
    w.put(pcm_res, 3);
    w.put(0, 2); // skip
    w.put(dialog_norm, 4);
    w.put(0, 4); // skip
    w.put(chan_base, 3);
    w.put(0, 8); // trailing padding so all reads succeed
    let mut payload = vec![0x7F, 0xFE, 0x80, 0x01];
    payload.extend(w.bytes());
    payload
  }

  fn dts_stream() -> TSStreamInfo {
    TSStreamInfo::new(0x1100, TSStreamType::DTSAudio as u8)
  }

  #[test]
  fn no_sync_leaves_uninitialized() {
    let mut stream = dts_stream();
    let data = vec![0u8; 32];
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(!stream.is_initialized);
  }

  #[test]
  fn frame_size_below_95_is_rejected() {
    let data = dts_frame(0, 50, 13, 19, 0, 1, 5, 7, 5);
    let mut stream = dts_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(!stream.is_initialized);
  }

  #[test]
  fn fixed_bitrate_core_frame_is_parsed() {
    // sr=13 -> 48000, br=19 -> 1280000, lfe present, pcm_res=5 -> 24-bit +
    // Extended mode, dialog_norm=7, channel base 5 -> 6 channels.
    let data = dts_frame(0, 100, 13, 19, 0, 1, 5, 7, 5);
    let mut stream = dts_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(stream.is_initialized);
    assert!(!stream.is_vbr);
    assert_eq!(stream.sample_rate, 48000);
    assert_eq!(stream.bit_rate, 1_280_000);
    assert_eq!(stream.channel_count, 6);
    assert_eq!(stream.lfe, 1);
    assert_eq!(stream.bit_depth, 24);
    assert_eq!(stream.dial_norm, -7);
    assert_eq!(stream.audio_mode, TSAudioMode::Extended.label());
  }

  #[test]
  fn variable_bitrate_marker_sets_vbr() {
    // br=30 -> DCA_BIT_RATES[30] == 2 (variable).
    let data = dts_frame(0, 100, 13, 30, 0, 0, 1, 0, 1);
    let mut stream = dts_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(stream.is_initialized);
    assert!(stream.is_vbr);
  }

  #[test]
  fn open_bitrate_uses_caller_hint() {
    // br=29 -> DCA_BIT_RATES[29] == 1 (open). With a hint, use it; without,
    // the stream stays uninitialized.
    let data = dts_frame(0, 100, 13, 29, 0, 0, 1, 0, 1);
    let mut with_hint = dts_stream();
    let mut b1 = TSStreamBuffer::new(&data);
    scan(&mut with_hint, &mut b1, 1_500_000);
    assert!(with_hint.is_initialized);
    assert_eq!(with_hint.bit_rate, 1_500_000);

    let mut no_hint = dts_stream();
    let mut b2 = TSStreamBuffer::new(&data);
    scan(&mut no_hint, &mut b2, 0);
    assert!(!no_hint.is_initialized);
  }

  #[test]
  fn crc_present_frame_still_parses() {
    let data = dts_frame(1, 100, 13, 19, 0, 1, 5, 7, 5);
    let mut stream = dts_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer, 0);
    assert!(stream.is_initialized);
    assert_eq!(stream.sample_rate, 48000);
  }
}
