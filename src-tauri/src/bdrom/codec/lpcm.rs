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
 * BD LPCM 4-byte audio header parser. Port of TSCodecLPCM.cs.
 */

#[derive(Debug, Clone, Copy)]
pub struct ParsedLpcm {
  pub channels: u32,
  pub lfe: u32,
  pub bit_depth: u32,
  pub sample_rate: u32,
}

pub fn parse(payload: &[u8]) -> Option<ParsedLpcm> {
  if payload.len() < 4 {
    return None;
  }
  let flags = ((payload[2] as u32) << 8) | payload[3] as u32;

  let (channels, lfe) = match (flags & 0xF000) >> 12 {
    1 => (1, 0),
    3 => (2, 0),
    4 => (3, 0),
    5 => (3, 0),
    6 => (4, 0),
    7 => (4, 0),
    8 => (5, 0),
    9 => (5, 1),
    10 => (7, 0),
    11 => (7, 1),
    _ => (0, 0),
  };

  let bit_depth = match (flags & 0xC0) >> 6 {
    1 => 16,
    2 => 20,
    3 => 24,
    _ => 0,
  };

  let sample_rate = match (flags & 0xF00) >> 8 {
    1 => 48000,
    4 => 96000,
    5 => 192000,
    _ => 0,
  };

  Some(ParsedLpcm {
    channels,
    lfe,
    bit_depth,
    sample_rate,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Build a 4-byte LPCM header. `flags = (payload[2] << 8) | payload[3]`,
  /// where the channel code is the top nibble of byte 2, the sample-rate code
  /// is the low nibble of byte 2, and the bit-depth code is bits 7..6 of byte 3.
  fn lpcm_payload(ch_code: u32, sr_code: u32, depth_code: u32) -> Vec<u8> {
    let b2 = (((ch_code & 0xF) << 4) | (sr_code & 0xF)) as u8;
    let b3 = ((depth_code & 0x3) << 6) as u8;
    vec![0x00, 0x00, b2, b3]
  }

  fn expected_channels(ch_code: u32) -> (u32, u32) {
    match ch_code {
      1 => (1, 0),
      3 => (2, 0),
      4 => (3, 0),
      5 => (3, 0),
      6 => (4, 0),
      7 => (4, 0),
      8 => (5, 0),
      9 => (5, 1),
      10 => (7, 0),
      11 => (7, 1),
      _ => (0, 0),
    }
  }

  #[test]
  fn too_short_payload_returns_none() {
    assert!(parse(&[]).is_none());
    assert!(parse(&[0x00, 0x00, 0x00]).is_none());
  }

  #[test]
  fn channel_assignment_table_matches_bdinfo() {
    for ch_code in 0..16u32 {
      let payload = lpcm_payload(ch_code, 1, 1);
      let parsed = parse(&payload).expect("4-byte header parses");
      let (channels, lfe) = expected_channels(ch_code);
      assert_eq!(parsed.channels, channels, "ch_code {ch_code}");
      assert_eq!(parsed.lfe, lfe, "ch_code {ch_code}");
    }
  }

  #[test]
  fn bit_depth_table_matches_bdinfo() {
    let expected = [0u32, 16, 20, 24];
    for depth_code in 0..4u32 {
      let payload = lpcm_payload(3, 1, depth_code);
      let parsed = parse(&payload).unwrap();
      assert_eq!(parsed.bit_depth, expected[depth_code as usize]);
    }
  }

  #[test]
  fn sample_rate_table_matches_bdinfo() {
    for sr_code in 0..16u32 {
      let payload = lpcm_payload(3, sr_code, 1);
      let parsed = parse(&payload).unwrap();
      let expected = match sr_code {
        1 => 48000,
        4 => 96000,
        5 => 192000,
        _ => 0,
      };
      assert_eq!(parsed.sample_rate, expected, "sr_code {sr_code}");
    }
  }

  #[test]
  fn typical_5_1_24bit_96k_header() {
    // Channel code 9 = 3/2/1 (5 channels + LFE), 24-bit, 96 kHz.
    let payload = lpcm_payload(9, 4, 3);
    let parsed = parse(&payload).unwrap();
    assert_eq!(parsed.channels, 5);
    assert_eq!(parsed.lfe, 1);
    assert_eq!(parsed.bit_depth, 24);
    assert_eq!(parsed.sample_rate, 96000);
    // Bit rate is computed in codec/mod.rs as sr * depth * (channels + lfe).
    let bit_rate = parsed.sample_rate as u64 * parsed.bit_depth as u64 * (parsed.channels + parsed.lfe) as u64;
    assert_eq!(bit_rate, 96000 * 24 * 6);
  }
}
