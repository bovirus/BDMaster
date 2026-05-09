/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
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
            if saved_pos >= 2
                && self.data[saved_pos - 2] == 0x00
                && self.data[saved_pos - 1] == 0x00
            {
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
            value += if (data >> (16 - i as i32 - 1)) & 1 != 0 { 1 } else { 0 };
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
            value += if (data >> (32 - i as i32 - 1)) & 1 != 0 { 1 } else { 0 };
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
        // Next 4 bytes
        shift = 24;
        let mut data2: u32 = 0;
        for i in 0..4 {
            if self.pos + i >= self.data.len() {
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
            value += if (combined >> (64 - i as i32 - 1)) & 1 != 0 { 1 } else { 0 };
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
        if ue & 1 == 1 {
            (ue + 1) / 2
        } else {
            -(ue / 2)
        }
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
