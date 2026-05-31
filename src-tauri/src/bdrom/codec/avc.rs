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
 * Faithful port of TSCodecAVC.cs.
 *
 * Note: BDInfo's AVC parser only extracts profile + level from the SPS — it
 * does *not* parse picture width/height/cropping. Picture dimensions come
 * from the MPLS video_format byte. This port preserves that behavior.
 */

use super::stream_buffer::TSStreamBuffer;
use crate::protocol::TSStreamInfo;

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer) {
  let mut parse: u32 = 0;
  let mut access_unit_delimiter_parse: u8 = 0;
  let mut sequence_parameter_set_parse: u8 = 0;
  let mut profile: String = String::new();
  let mut level: String;
  let mut constraint_set3_flag: u8 = 0;

  for _ in 0..buffer.len() {
    let byte = buffer.read_byte(true) as u32;
    parse = (parse << 8).wrapping_add(byte);

    if parse == 0x00000109 {
      access_unit_delimiter_parse = 1;
    } else if access_unit_delimiter_parse > 0 {
      access_unit_delimiter_parse -= 1;
      if access_unit_delimiter_parse == 0 {
        let _ = (parse & 0xFF) >> 5;
        if stream.is_initialized {
          return;
        }
      }
    } else if parse == 0x00000127 || parse == 0x00000167 {
      sequence_parameter_set_parse = 3;
    } else if sequence_parameter_set_parse > 0 {
      sequence_parameter_set_parse -= 1;
      if !stream.is_initialized {
        match sequence_parameter_set_parse {
          2 => {
            profile = match parse & 0xFF {
              66 => "Baseline Profile".to_string(),
              77 => "Main Profile".to_string(),
              88 => "Extended Profile".to_string(),
              100 => "High Profile".to_string(),
              110 => "High 10 Profile".to_string(),
              122 => "High 4:2:2 Profile".to_string(),
              144 => "High 4:4:4 Profile".to_string(),
              _ => "Unknown Profile".to_string(),
            };
          }
          1 => {
            // constraintSet0..3 flags
            let _cs0 = ((parse & 0x80) >> 7) as u8;
            let _cs1 = ((parse & 0x40) >> 6) as u8;
            let _cs2 = ((parse & 0x20) >> 5) as u8;
            constraint_set3_flag = ((parse & 0x10) >> 4) as u8;
          }
          0 => {
            let b = (parse & 0xFF) as u8;
            level = if b == 11 && constraint_set3_flag == 1 {
              "1b".to_string()
            } else {
              format!("{}.{}", b / 10, b - ((b / 10) * 10))
            };
            stream.encoding_profile = format!("{} {}", profile, level);
            stream.is_vbr = true;
            stream.is_initialized = true;
          }
          _ => {}
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::bdrom::types::TSStreamType;

  fn avc_stream() -> TSStreamInfo {
    TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8)
  }

  /// Build an SPS NAL: start code (0x00000127), profile_idc, constraint byte,
  /// level_idc.
  fn sps(profile_idc: u8, constraint: u8, level_idc: u8) -> Vec<u8> {
    vec![0x00, 0x00, 0x01, 0x27, profile_idc, constraint, level_idc]
  }

  #[test]
  fn empty_buffer_leaves_stream_uninitialized() {
    let mut stream = avc_stream();
    let data: Vec<u8> = Vec::new();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert!(!stream.is_initialized);
  }

  #[test]
  fn high_profile_level_4_0() {
    let data = sps(100, 0x00, 40);
    let mut stream = avc_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert!(stream.is_initialized);
    assert!(stream.is_vbr);
    assert_eq!(stream.encoding_profile, "High Profile 4.0");
  }

  #[test]
  fn baseline_level_1b_uses_constraint_set3() {
    // level_idc 11 with constraint_set3 set decodes as the special "1b" level.
    let data = sps(66, 0x10, 11);
    let mut stream = avc_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert_eq!(stream.encoding_profile, "Baseline Profile 1b");
  }

  #[test]
  fn unknown_profile_idc_is_reported() {
    let data = sps(200, 0x00, 31);
    let mut stream = avc_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert_eq!(stream.encoding_profile, "Unknown Profile 3.1");
  }

  #[test]
  fn does_not_panic_on_garbage() {
    let mut stream = avc_stream();
    let data: Vec<u8> = (0..512u32).map(|i| (i % 256) as u8).collect();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
  }

  #[test]
  fn all_profile_idc_labels() {
    for (idc, name) in [
      (66u8, "Baseline Profile"),
      (77, "Main Profile"),
      (88, "Extended Profile"),
      (100, "High Profile"),
      (110, "High 10 Profile"),
      (122, "High 4:2:2 Profile"),
      (144, "High 4:4:4 Profile"),
    ] {
      let data = sps(idc, 0x00, 40);
      let mut stream = avc_stream();
      let mut buffer = TSStreamBuffer::new(&data);
      scan(&mut stream, &mut buffer);
      assert!(stream.encoding_profile.starts_with(name), "{}", stream.encoding_profile);
    }
  }

  #[test]
  fn sps_alternate_start_code_0x67() {
    // 0x00000167 is the other accepted SPS NAL start code.
    let data = vec![0x00, 0x00, 0x01, 0x67, 77, 0x00, 41];
    let mut stream = avc_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert_eq!(stream.encoding_profile, "Main Profile 4.1");
  }

  #[test]
  fn aud_after_init_returns_early() {
    // SPS initializes the stream, then an access-unit delimiter (0x00000109)
    // triggers the initialized early-return branch.
    let mut data = sps(100, 0x00, 40);
    data.extend_from_slice(&[0x00, 0x00, 0x01, 0x09, 0x10, 0x00, 0x00]);
    let mut stream = avc_stream();
    let mut buffer = TSStreamBuffer::new(&data);
    scan(&mut stream, &mut buffer);
    assert!(stream.is_initialized);
    assert_eq!(stream.encoding_profile, "High Profile 4.0");
  }
}
