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
 * Faithful port of TSStreamBuffer.cs. The codec parsers consume bytes via
 * this abstraction, including the H.26x emulation-prevention-byte skip.
 */

#[derive(Debug, Clone, Copy)]
pub enum SeekOrigin {
  Begin,
  Current,
  End,
}

pub struct TSStreamBuffer<'a> {
  data: &'a [u8],
  pos: usize,
  skip_bits: u32,
  skipped_bytes: u32,
}

impl<'a> TSStreamBuffer<'a> {
  pub fn new(data: &'a [u8]) -> Self {
    Self {
      data,
      pos: 0,
      skip_bits: 0,
      skipped_bytes: 0,
    }
  }

  pub fn len(&self) -> usize {
    self.data.len()
  }

  pub fn position(&self) -> usize {
    self.pos
  }

  pub fn seek(&mut self, offset: i64, origin: SeekOrigin) {
    let base = match origin {
      SeekOrigin::Begin => 0,
      SeekOrigin::Current => self.pos as i64,
      SeekOrigin::End => self.data.len() as i64,
    };
    let target = base + offset;
    self.pos = target.max(0).min(self.data.len() as i64) as usize;
  }

  pub fn begin_read(&mut self) {
    self.skip_bits = 0;
    self.skipped_bytes = 0;
    self.pos = 0;
  }

  pub fn read_bytes(&mut self, n: usize) -> Option<Vec<u8>> {
    // Mirror C# semantics: returns null if pos + bytes >= length.
    if self.pos + n >= self.data.len() {
      return None;
    }
    let v = self.data[self.pos..self.pos + n].to_vec();
    self.pos += n;
    Some(v)
  }

  pub fn read_byte(&mut self, skip_h26x: bool) -> u8 {
    if self.pos >= self.data.len() {
      return 0;
    }
    let mut b = self.data[self.pos];
    let saved_pos = self.pos;
    self.pos += 1;

    if skip_h26x && b == 0x03 {
      // Look back at the two bytes prior to the byte we just read.
      if saved_pos >= 2 && self.data[saved_pos - 2] == 0x00 && self.data[saved_pos - 1] == 0x00 {
        if self.pos < self.data.len() {
          b = self.data[self.pos];
          self.pos += 1;
          self.skipped_bytes += 1;
        }
      }
    }
    b
  }

  pub fn read_byte_default(&mut self) -> u8 {
    self.read_byte(false)
  }

  pub fn read_bool(&mut self, skip_h26x: bool) -> bool {
    let pos = self.pos;
    self.skipped_bytes = 0;
    if pos == self.data.len() {
      return false;
    }
    let data = self.read_byte(skip_h26x);
    let value = (data >> (8 - self.skip_bits as i32 - 1)) & 1 != 0;
    self.skip_bits += 1;
    self.pos = pos + (self.skip_bits >> 3) as usize + self.skipped_bytes as usize;
    self.skip_bits %= 8;
    value
  }

  pub fn read_bool_default(&mut self) -> bool {
    self.read_bool(false)
  }

  pub fn read_bits2(&mut self, bits: u32, skip_h26x: bool) -> u16 {
    let pos = self.pos;
    self.skipped_bytes = 0;

    let mut shift: i32 = 8;
    let mut data: u32 = 0;
    for i in 0..2 {
      if pos + i >= self.data.len() {
        break;
      }
      data += (self.read_byte(skip_h26x) as u32) << shift;
      shift -= 8;
    }

    let mut value: u16 = 0;
    let from = self.skip_bits;
    let to = self.skip_bits + bits;
    for i in from..to {
      value <<= 1;
      let shift_amount = 16i32 - i as i32 - 1;
      let bit = if shift_amount < 0 {
        0
      } else {
        (data >> shift_amount) & 1
      };
      value += bit as u16;
    }
    self.skip_bits += bits;
    self.pos = pos + (self.skip_bits >> 3) as usize + self.skipped_bytes as usize;
    self.skip_bits %= 8;
    value
  }

  pub fn read_bits2_default(&mut self, bits: u32) -> u16 {
    self.read_bits2(bits, false)
  }

  pub fn read_bits4(&mut self, bits: u32, skip_h26x: bool) -> u32 {
    let pos = self.pos;
    self.skipped_bytes = 0;

    let mut shift: i32 = 24;
    let mut data: u32 = 0;
    for i in 0..4 {
      if pos + i >= self.data.len() {
        break;
      }
      data += (self.read_byte(skip_h26x) as u32) << shift;
      shift -= 8;
    }

    let mut value: u32 = 0;
    let from = self.skip_bits;
    let to = self.skip_bits + bits;
    for i in from..to {
      value <<= 1;
      let shift_amount = 32i32 - i as i32 - 1;
      let bit = if shift_amount < 0 {
        0
      } else {
        (data >> shift_amount) & 1
      };
      value += bit;
    }
    self.skip_bits += bits;
    self.pos = pos + (self.skip_bits >> 3) as usize + self.skipped_bytes as usize;
    self.skip_bits %= 8;
    value
  }

  pub fn read_bits4_default(&mut self, bits: u32) -> u32 {
    self.read_bits4(bits, false)
  }

  pub fn read_bits8(&mut self, bits: u32, skip_h26x: bool) -> u64 {
    let pos = self.pos;
    self.skipped_bytes = 0;

    // First 4 bytes
    let mut shift: i32 = 24;
    let mut data1: u32 = 0;
    for i in 0..4 {
      if pos + i >= self.data.len() {
        break;
      }
      data1 += (self.read_byte(skip_h26x) as u32) << shift;
      shift -= 8;
    }
    // Next 4 bytes. C# checks `pos + i` against the length using the
    // *original* captured position (not the advanced one), so the second
    // half is read whenever the first half was in bounds. Mirror that.
    shift = 24;
    let mut data2: u32 = 0;
    for i in 0..4 {
      if pos + i >= self.data.len() {
        break;
      }
      data2 += (self.read_byte(skip_h26x) as u32) << shift;
      shift -= 8;
    }
    let combined: u64 = ((data1 as u64) << 32) | (data2 as u64);

    let mut value: u64 = 0;
    let from = self.skip_bits;
    let to = self.skip_bits + bits;
    for i in from..to {
      value <<= 1;
      let shift_amount = 64i32 - i as i32 - 1;
      let bit = if shift_amount < 0 {
        0
      } else {
        (combined >> shift_amount) & 1
      };
      value += bit;
    }
    self.skip_bits += bits;
    self.pos = pos + (self.skip_bits >> 3) as usize + self.skipped_bytes as usize;
    self.skip_bits %= 8;
    value
  }

  pub fn read_bits8_default(&mut self, bits: u32) -> u64 {
    self.read_bits8(bits, false)
  }

  pub fn bs_skip_bits(&mut self, bits: u32, skip_h26x: bool) {
    let count = bits / 16 + if bits % 16 > 0 { 1 } else { 0 };
    let mut bits_read: u32 = 0;
    for _ in 0..count {
      let mut to_read = bits - bits_read;
      if to_read > 16 {
        to_read = 16;
      }
      self.read_bits2(to_read, skip_h26x);
      bits_read += to_read;
    }
  }

  pub fn bs_skip_bits_default(&mut self, bits: u32) {
    self.bs_skip_bits(bits, false);
  }

  pub fn bs_skip_next_byte(&mut self) {
    if self.skip_bits > 0 {
      self.bs_skip_bits(8 - self.skip_bits, false);
    }
  }

  pub fn bs_reset_bits(&mut self) {
    self.skip_bits = 0;
  }

  pub fn bs_skip_bytes(&mut self, bytes: i32, skip_h26x: bool) {
    if bytes > 0 {
      for _ in 0..bytes {
        self.read_byte(skip_h26x);
      }
    } else {
      // C# semantics: position = pos + (skipBits >> 3) + bytes
      let pos = self.pos as i64;
      let new_pos = pos + (self.skip_bits as i64 >> 3) + bytes as i64;
      self.pos = new_pos.max(0).min(self.data.len() as i64) as usize;
    }
  }

  pub fn bs_skip_bytes_default(&mut self, bytes: i32) {
    self.bs_skip_bytes(bytes, false);
  }

  pub fn read_exp(&mut self, skip_h26x: bool) -> u32 {
    let mut leading_zeros: u32 = 0;
    while self.data_bit_stream_remain() > 0 && !self.read_bool(skip_h26x) {
      leading_zeros += 1;
      if leading_zeros > 32 {
        break;
      }
    }
    let info_d = 1u64 << leading_zeros as u64;
    let extra = self.read_bits4(leading_zeros, skip_h26x);
    (info_d as u32).wrapping_sub(1).wrapping_add(extra)
  }

  pub fn read_exp_default(&mut self) -> u32 {
    self.read_exp(false)
  }

  pub fn skip_exp(&mut self, skip_h26x: bool) {
    let mut leading_zeros: u32 = 0;
    while self.data_bit_stream_remain() > 0 && !self.read_bool(skip_h26x) {
      leading_zeros += 1;
      if leading_zeros > 32 {
        break;
      }
    }
    self.bs_skip_bits(leading_zeros, skip_h26x);
  }

  pub fn skip_exp_default(&mut self) {
    self.skip_exp(false);
  }

  pub fn skip_exp_multi(&mut self, num: u32, skip_h26x: bool) {
    for _ in 0..num {
      self.skip_exp(skip_h26x);
    }
  }

  pub fn skip_exp_multi_default(&mut self, num: u32) {
    self.skip_exp_multi(num, false);
  }

  /// Signed exp-golomb (se(v) in H.26x).
  pub fn read_se(&mut self, skip_h26x: bool) -> i32 {
    let ue = self.read_exp(skip_h26x) as i32;
    if ue & 1 == 1 { (ue + 1) / 2 } else { -(ue / 2) }
  }

  pub fn read_se_default(&mut self) -> i32 {
    self.read_se(false)
  }

  pub fn data_bit_stream_remain(&self) -> i64 {
    (self.data.len() as i64 - self.pos as i64) * 8 - self.skip_bits as i64
  }

  pub fn data_bit_stream_remain_bytes(&self) -> i64 {
    self.data.len() as i64 - self.pos as i64
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn read_bits2_reads_msb_first() {
    let data = [0xAB, 0xCD, 0xEF, 0x12];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_bits2_default(16), 0xABCD);
    assert_eq!(b.read_bits2_default(16), 0xEF12);
  }

  #[test]
  fn read_bits2_sub_byte_fields() {
    // 0xA = 1010, 0x5 = 0101 -> nibble reads.
    let data = [0xA5];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_bits2_default(4), 0xA);
    assert_eq!(b.read_bits2_default(4), 0x5);
  }

  #[test]
  fn read_bits4_reads_32_bits() {
    let data = [0xDE, 0xAD, 0xBE, 0xEF];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_bits4_default(32), 0xDEAD_BEEF);
  }

  #[test]
  fn read_bits8_reads_64_bits_and_high_halves() {
    let data = [0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78, 0x9A];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_bits8_default(64), 0xABCD_EF12_3456_789A);

    let mut b2 = TSStreamBuffer::new(&data);
    // The top 32 bits are the first four bytes.
    assert_eq!(b2.read_bits8_default(32), 0xABCD_EF12);

    let mut b3 = TSStreamBuffer::new(&data);
    // 40 bits -> first five bytes worth of MSB-aligned data.
    assert_eq!(b3.read_bits8_default(40), 0xABCD_EF12_34);
  }

  #[test]
  fn read_bool_walks_bits_msb_first() {
    let data = [0b1010_0000];
    let mut b = TSStreamBuffer::new(&data);
    assert!(b.read_bool_default());
    assert!(!b.read_bool_default());
    assert!(b.read_bool_default());
    assert!(!b.read_bool_default());
  }

  #[test]
  fn h26x_emulation_byte_is_skipped_after_two_zero_bytes() {
    // 0x00 0x00 0x03 0x42 -> the 0x03 emulation-prevention byte is dropped.
    let data = [0x00, 0x00, 0x03, 0x42];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_byte(true), 0x00);
    assert_eq!(b.read_byte(true), 0x00);
    assert_eq!(b.read_byte(true), 0x42);
    assert_eq!(b.position(), 4);
  }

  #[test]
  fn h26x_emulation_byte_kept_when_not_preceded_by_two_zeros() {
    // 0x03 preceded by 0x01 0x00 is real data, not an emulation byte.
    let data = [0x01, 0x00, 0x03, 0x42];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_byte(true), 0x01);
    assert_eq!(b.read_byte(true), 0x00);
    assert_eq!(b.read_byte(true), 0x03);
    assert_eq!(b.read_byte(true), 0x42);
  }

  #[test]
  fn h26x_disabled_keeps_emulation_byte() {
    let data = [0x00, 0x00, 0x03, 0x42];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_byte(false), 0x00);
    assert_eq!(b.read_byte(false), 0x00);
    assert_eq!(b.read_byte(false), 0x03);
  }

  #[test]
  fn exp_golomb_decodes_sequence() {
    // ue(v): "1"=0, "010"=1, "011"=2 -> bits 1 010 011 = 0b1010_0110.
    let data = [0b1010_0110];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_exp_default(), 0);
    assert_eq!(b.read_exp_default(), 1);
    assert_eq!(b.read_exp_default(), 2);
  }

  #[test]
  fn signed_exp_golomb_maps_codes() {
    // se(v): ue 0 -> 0, ue 1 -> +1, ue 2 -> -1.
    let data = [0b1010_0110];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_se_default(), 0);
    assert_eq!(b.read_se_default(), 1);
    assert_eq!(b.read_se_default(), -1);
  }

  #[test]
  fn read_bytes_returns_none_at_or_past_end() {
    // Mirrors BDInfo: returns None when pos + n >= length.
    let data = [0x01, 0x02, 0x03, 0x04];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.read_bytes(2), Some(vec![0x01, 0x02]));
    // pos=2, requesting 2 -> 2+2 >= 4 -> None.
    assert_eq!(b.read_bytes(2), None);
  }

  #[test]
  fn seek_clamps_within_bounds() {
    let data = [0u8; 8];
    let mut b = TSStreamBuffer::new(&data);
    b.seek(4, SeekOrigin::Begin);
    assert_eq!(b.position(), 4);
    b.seek(-2, SeekOrigin::Current);
    assert_eq!(b.position(), 2);
    b.seek(0, SeekOrigin::End);
    assert_eq!(b.position(), 8);
    b.seek(100, SeekOrigin::Begin); // clamps to len
    assert_eq!(b.position(), 8);
    b.seek(-100, SeekOrigin::Begin); // clamps to 0
    assert_eq!(b.position(), 0);
  }

  #[test]
  fn begin_read_resets_cursor_and_bit_state() {
    let data = [0xFF, 0x00, 0xFF];
    let mut b = TSStreamBuffer::new(&data);
    b.read_bits2_default(4);
    b.read_byte_default();
    b.begin_read();
    assert_eq!(b.position(), 0);
    assert_eq!(b.read_bits2_default(8), 0xFF);
  }

  #[test]
  fn remaining_counters_track_position() {
    let data = [0u8; 4];
    let mut b = TSStreamBuffer::new(&data);
    assert_eq!(b.data_bit_stream_remain(), 32);
    assert_eq!(b.data_bit_stream_remain_bytes(), 4);
    b.read_byte_default();
    assert_eq!(b.data_bit_stream_remain(), 24);
    assert_eq!(b.data_bit_stream_remain_bytes(), 3);
  }

  #[test]
  fn under_reads_default_without_panicking() {
    let data = [0xFF];
    let mut b = TSStreamBuffer::new(&data);
    // Reading well past the end returns zero/false, never panics.
    let _ = b.read_bits8_default(64);
    let _ = b.read_bits4_default(32);
    assert!(!b.read_bool_default());
    assert_eq!(b.read_byte_default(), 0);
  }
}
