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
 * Faithful port of TSCodecMPEG2.cs.
 *
 * BDInfo gates several stream-property assignments behind #if DEBUG. We mirror
 * the behavior with a `debug_mode` flag controlled at compile time; in release
 * builds (debug_assertions == false) the gating matches the upstream binary.
 */

use super::stream_buffer::TSStreamBuffer;
#[cfg(debug_assertions)]
use crate::bdrom::types::TSAspectRatio;
use crate::protocol::TSStreamInfo;

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
  let mut parse: u32 = 0;
  let mut picture_parse: u32 = 0;
  let mut sequence_header_parse: u32 = 0;
  let mut extension_parse: u32 = 0;
  let mut sequence_extension_parse: u32 = 0;

  for _ in 0..buffer.len() {
    parse = parse.wrapping_shl(8).wrapping_add(buffer.read_byte_default() as u32);

    if parse == 0x00000100 {
      picture_parse = 2;
    } else if parse == 0x000001B3 {
      sequence_header_parse = 7;
    } else if sequence_header_parse > 0 {
      sequence_header_parse -= 1;
      match sequence_header_parse {
        4 => {
          #[cfg(debug_assertions)]
          {
            stream.width = (parse & 0xFFF000) >> 12;
            stream.height = parse & 0xFFF;
          }
        }
        3 => {
          #[cfg(debug_assertions)]
          {
            let ar = ((parse & 0xF0) >> 4) as u8;
            let aspect = TSAspectRatio::from_u8(ar);
            stream.aspect_ratio = aspect.label().to_string();
            stream.aspect_ratio_code = ar as u32;

            match parse & 0xF {
              1 => {
                stream.frame_rate_enumerator = 24000;
                stream.frame_rate_denominator = 1001;
              }
              2 => {
                stream.frame_rate_enumerator = 24000;
                stream.frame_rate_denominator = 1000;
              }
              3 => {
                stream.frame_rate_enumerator = 25000;
                stream.frame_rate_denominator = 1000;
              }
              4 => {
                stream.frame_rate_enumerator = 30000;
                stream.frame_rate_denominator = 1001;
              }
              5 => {
                stream.frame_rate_enumerator = 30000;
                stream.frame_rate_denominator = 1000;
              }
              6 => {
                stream.frame_rate_enumerator = 50000;
                stream.frame_rate_denominator = 1000;
              }
              7 => {
                stream.frame_rate_enumerator = 60000;
                stream.frame_rate_denominator = 1001;
              }
              8 => {
                stream.frame_rate_enumerator = 60000;
                stream.frame_rate_denominator = 1000;
              }
              _ => {
                stream.frame_rate_enumerator = 0;
                stream.frame_rate_denominator = 0;
              }
            }
          }
        }
        0 => {
          #[cfg(debug_assertions)]
          {
            stream.bit_rate = ((parse & 0xFFFFC0) >> 6) as u64 * 200;
          }
          stream.is_vbr = true;
          stream.is_initialized = true;
        }
        _ => {}
      }
    } else if picture_parse > 0 {
      picture_parse -= 1;
      if picture_parse == 0 {
        let _picture_coding = (parse & 0x38) >> 3;
        if stream.is_initialized {
          return;
        }
      }
    } else if parse == 0x000001B5 {
      extension_parse = 1;
    } else if extension_parse > 0 {
      extension_parse -= 1;
      if extension_parse == 0 && (parse & 0xF0) == 0x10 {
        sequence_extension_parse = 1;
      }
    } else if sequence_extension_parse > 0 {
      sequence_extension_parse -= 1;
      if sequence_extension_parse == 0 {
        #[cfg(debug_assertions)]
        {
          let sequence_extension = (parse & 0x8) >> 3;
          stream.is_interlaced = sequence_extension == 0;
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::bdrom::types::TSStreamType;

  fn mpeg2_stream() -> TSStreamInfo {
    TSStreamInfo::new(0x1011, TSStreamType::MPEG2Video as u8)
  }

  /// A sequence header (0x000001B3) whose dimension bytes encode 1920x1080.
  fn sequence_header_1920x1080() -> Vec<u8> {
    vec![
      0x00, 0x00, 0x01, 0xB3, // sequence header start code
      0x78, 0x04, 0x38, // width=1920, height=1080 (read in DEBUG case 4)
      0x00, 0x00, 0x00, 0x00, // aspect/frame-rate/bit-rate bytes
    ]
  }

  #[test]
  fn empty_buffer_leaves_stream_uninitialized() {
    let mut stream = mpeg2_stream();
    let data: Vec<u8> = Vec::new();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert!(!stream.is_initialized);
  }

  #[test]
  fn sequence_header_marks_vbr_and_initialized() {
    // This is the only behavior present in BDInfo's shipping (`#undef DEBUG`)
    // build, so it must hold regardless of Rust build profile.
    let data = sequence_header_1920x1080();
    let mut stream = mpeg2_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert!(stream.is_initialized);
    assert!(stream.is_vbr);
  }

  #[test]
  fn dimension_extraction_is_profile_gated() {
    let data = sequence_header_1920x1080();
    let mut stream = mpeg2_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    #[cfg(debug_assertions)]
    {
      // Debug builds additionally populate dimensions from the ES.
      assert_eq!(stream.width, 1920);
      assert_eq!(stream.height, 1080);
    }
    #[cfg(not(debug_assertions))]
    {
      // Release matches BDInfo's `#undef DEBUG`: dimensions stay unset.
      assert_eq!(stream.width, 0);
      assert_eq!(stream.height, 0);
    }
  }

  #[test]
  fn does_not_panic_on_garbage() {
    let mut stream = mpeg2_stream();
    let data: Vec<u8> = (0..512u32).map(|i| (i % 256) as u8).collect();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
  }

  #[test]
  #[cfg(debug_assertions)]
  fn frame_rate_and_aspect_codes() {
    // These elementary-stream fields are only populated in debug builds,
    // mirroring BDInfo's `#undef DEBUG` release behavior.
    let cases = [
      (1u8, 24000u32, 1001u32),
      (2, 24000, 1000),
      (3, 25000, 1000),
      (4, 30000, 1001),
      (5, 30000, 1000),
      (6, 50000, 1000),
      (7, 60000, 1001),
      (8, 60000, 1000),
    ];
    for (code, num, den) in cases {
      // 4th byte after B3 carries aspect (high nibble) + frame-rate (low).
      let data = vec![
        0x00,
        0x00,
        0x01,
        0xB3,
        0x78,
        0x04,
        0x38,
        (3 << 4) | code,
        0x12,
        0x34,
        0x56,
      ];
      let mut stream = mpeg2_stream();
      let mut buffer = TSStreamBuffer::new(&data);
      scan(&mut stream, &mut buffer);
      assert_eq!(stream.frame_rate_enumerator, num, "code {code}");
      assert_eq!(stream.frame_rate_denominator, den, "code {code}");
      assert_eq!(stream.aspect_ratio, "16:9");
      assert!(stream.bit_rate > 0);
    }
    // Codes 0 and >8 leave the frame rate unset.
    for code in [0u8, 9, 15] {
      let data = vec![0x00, 0x00, 0x01, 0xB3, 0x78, 0x04, 0x38, code, 0x00, 0x00, 0x00];
      let mut stream = mpeg2_stream();
      let mut buffer = TSStreamBuffer::new(&data);
      scan(&mut stream, &mut buffer);
      assert_eq!(stream.frame_rate_enumerator, 0);
    }
  }

  #[test]
  fn sequence_extension_picture_and_interlace() {
    // Sequence header (inits), then a sequence extension carrying the
    // interlace flag, then a picture start code that triggers the
    // already-initialized early return.
    let mut data = vec![0x00, 0x00, 0x01, 0xB3, 0x78, 0x04, 0x38, 0x31, 0x12, 0x34, 0x56];
    // Sequence extension: 0x000001B5, extension id nibble 0x1, then a byte
    // whose bit 3 clear => interlaced source.
    data.extend_from_slice(&[0x00, 0x00, 0x01, 0xB5, 0x10, 0x00]);
    // Picture start code + 2 bytes => picture_coding read, init => return.
    data.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x00, 0x00]);

    let mut stream = mpeg2_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert!(stream.is_initialized);
    assert!(stream.is_vbr);
    #[cfg(debug_assertions)]
    {
      assert_eq!(stream.width, 1920);
      assert_eq!(stream.height, 1080);
      assert!(stream.is_interlaced);
    }
  }
}
