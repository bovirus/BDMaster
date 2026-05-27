/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecTrueHD.cs.
 */

use super::ac3;
use super::stream_buffer::TSStreamBuffer;
use crate::bdrom::types::TSStreamType;
use crate::protocol::TSStreamInfo;

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
    if stream.is_initialized
        && stream
            .core
            .as_ref()
            .map(|c| c.is_initialized)
            .unwrap_or(false)
    {
        return;
    }

    let mut sync: u32 = 0;
    let mut sync_found = false;
    for _ in 0..buffer.len() {
        sync = sync.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);
        if sync == 0xF8726FBA {
            sync_found = true;
            break;
        }
    }

    if !sync_found {
        if stream.core.is_none() {
            let core = TSStreamInfo::new(stream.pid, TSStreamType::AC3Audio as u8);
            stream.core = Some(Box::new(core));
        }
        let mut needs_init = true;
        if let Some(c) = &stream.core {
            needs_init = !c.is_initialized;
        }
        if needs_init {
            buffer.begin_read();
            if let Some(core) = stream.core.as_deref_mut() {
                ac3::scan(core, buffer);
            }
        }
        return;
    }

    let ratebits = buffer.read_bits2_default(4) as u32;
    if ratebits != 0xF {
        stream.sample_rate =
            (if (ratebits & 8) > 0 { 44100u32 } else { 48000u32 }) << (ratebits & 7);
    }
    buffer.bs_skip_bits_default(15);

    stream.channel_count = 0;
    stream.lfe = 0;
    if buffer.read_bool_default() {
        stream.lfe += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }
    if buffer.read_bool_default() {
        stream.lfe += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 1;
    }
    if buffer.read_bool_default() {
        stream.channel_count += 2;
    }

    buffer.bs_skip_bits_default(49);

    let mut peak_bitrate = buffer.read_bits4_default(15);
    peak_bitrate = (peak_bitrate.wrapping_mul(stream.sample_rate)) >> 4;

    let denom = (stream.channel_count + stream.lfe).max(1) as f64 * stream.sample_rate.max(1) as f64;
    let peak_bitdepth = peak_bitrate as f64 / denom;

    stream.bit_depth = if peak_bitdepth > 14.0 { 24 } else { 16 };

    buffer.bs_skip_bits_default(79);

    let has_extensions = buffer.read_bool_default();
    let num_extensions = (buffer.read_bits2_default(4) as u32 * 2) + 1;
    let mut has_content = buffer.read_bits4_default(4) != 0;

    if has_extensions {
        for _ in 0..num_extensions {
            if buffer.read_bits2_default(8) != 0 {
                has_content = true;
            }
        }
        if has_content {
            stream.has_extensions = true;
        }
    }

    // BDInfo keeps the TrueHD-metadata-dialnorm copy disabled (the block is
    // commented out in TSCodecTrueHD.cs with `// TODO: Get THD dialnorm from
    // metadata`); we mirror that to preserve parity:
    //   if let Some(core) = &stream.core {
    //       if core.dial_norm != 0 { stream.dial_norm = core.dial_norm; }
    //   }

    stream.is_vbr = true;
    if let Some(c) = &stream.core {
        if c.is_initialized {
            stream.is_initialized = true;
        }
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
                // `i` may exceed 31 for the wide zero-fill skips below; a shift
                // of >= 32 panics in debug builds, so treat those bits as 0.
                let bit = i < 32 && (val >> i) & 1 == 1;
                self.bits.push(bit);
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

    /// A valid legacy AC3 (bsid 8) sync frame, used to exercise the core
    /// fallback. Contains no TrueHD sync word.
    fn ac3_core_frame() -> Vec<u8> {
        let mut bits = BitWriter::default();
        bits.put(7, 3); // acmod
        bits.put(0, 2); // cmixlev
        bits.put(0, 2); // surmixlev
        bits.put(1, 1); // lfeon
        bits.put(27, 5); // dialnorm
        bits.put(0, 1); // compre
        bits.put(0, 1); // langcode
        bits.put(0, 1); // audprodie
        bits.put(0, 2); // copyright + original
        bits.put(0, 8); // padding
        let mut data = vec![0x0B, 0x77, 0x00, 0x00, 0x00, 0x40];
        data.extend(bits.bytes());
        data
    }

    fn thd_stream() -> TSStreamInfo {
        TSStreamInfo::new(0x1100, TSStreamType::AC3TrueHDAudio as u8)
    }

    #[test]
    fn no_truehd_sync_parses_ac3_core() {
        let data = ac3_core_frame();
        let mut stream = thd_stream();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        let core = stream.core.as_ref().expect("AC3 core created");
        assert_eq!(core.stream_type, TSStreamType::AC3Audio as u8);
        assert!(core.is_initialized);
        assert_eq!(core.sample_rate, 48000);
    }

    #[test]
    fn empty_buffer_creates_uninitialized_core() {
        let data: Vec<u8> = Vec::new();
        let mut stream = thd_stream();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);
        assert!(stream.core.is_some());
        assert!(!stream.is_initialized);
    }

    #[test]
    fn truehd_sync_parses_sample_rate_channels_and_bit_depth() {
        let mut bits = BitWriter::default();
        bits.put(0, 4); // ratebits 0 -> 48000 Hz
        bits.put(0, 15); // skip
        bits.put(1, 1); // LFE present (+1)
        bits.put(1, 1); // channel (+1)
        for _ in 0..11 {
            bits.put(0, 1); // remaining channel-present flags off
        }
        bits.put(0, 49); // skip
        bits.put(480, 15); // peak bitrate -> peak bit depth 15 -> 24-bit
        bits.put(0, 79); // skip
        bits.put(0, 1); // has_extensions = false
        bits.put(0, 4); // num_extensions field
        bits.put(0, 4); // has_content field
        bits.put(0, 16); // padding

        let mut data = vec![0xF8, 0x72, 0x6F, 0xBA]; // TrueHD sync
        data.extend(bits.bytes());

        let mut stream = thd_stream();
        let mut buffer = TSStreamBuffer::new(&data);
        scan(&mut stream, &mut buffer);

        assert_eq!(stream.sample_rate, 48000);
        assert_eq!(stream.channel_count, 1);
        assert_eq!(stream.lfe, 1);
        assert_eq!(stream.bit_depth, 24);
        assert!(stream.is_vbr);
        // Without an initialized core, the parent stays uninitialized (parity
        // with BDInfo's two-call pattern).
        assert!(!stream.is_initialized);
    }

    #[test]
    fn does_not_panic_on_garbage() {
        for len in 0..48usize {
            let mut data = vec![0xF8, 0x72, 0x6F, 0xBA];
            data.extend((0..len).map(|i| (i as u8).wrapping_mul(29) | 0x21));
            let mut stream = thd_stream();
            let mut buffer = TSStreamBuffer::new(&data);
            scan(&mut stream, &mut buffer);
        }
    }
}
