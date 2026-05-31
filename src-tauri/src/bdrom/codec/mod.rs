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
 * Codec dispatcher. Each codec is invoked on a reassembled PES payload via
 * the TSStreamBuffer abstraction. Parsers may be called multiple times for
 * the same stream until is_initialized turns true.
 */

pub mod aac;
pub mod ac3;
pub mod avc;
pub mod dts;
pub mod dtshd;
pub mod hevc;
pub mod lpcm;
pub mod mpa;
pub mod mpeg2;
pub mod mvc;
pub mod pgs;
pub mod stream_buffer;
pub mod truehd;
pub mod vc1;

use crate::bdrom::types::TSStreamType;
use crate::protocol::TSStreamInfo;

pub use pgs::PgsState;
pub use stream_buffer::TSStreamBuffer;

/// Per-PID codec scanning state held across PES invocations. This mirrors
/// the per-stream state TSStreamFile.cs maintains in the C# original.
#[derive(Default)]
pub struct CodecScanState {
  pub pgs: PgsState,
  pub hevc: hevc::PersistentHevc,
}

pub fn scan_stream(
  stream: &mut TSStreamInfo,
  state: &mut CodecScanState,
  payload: &[u8],
  bitrate: i64,
  extended_diagnostics: bool,
  is_full_scan: bool,
) {
  let mut buffer = TSStreamBuffer::new(payload);
  let st = TSStreamType::from_u8(stream.stream_type);

  match st {
    TSStreamType::MPEG2Video => {
      mpeg2::scan(stream, &mut buffer);
    }
    TSStreamType::AVCVideo => {
      avc::scan(stream, &mut buffer);
    }
    TSStreamType::MVCVideo => {
      mvc::scan(stream, &mut buffer);
    }
    TSStreamType::HEVCVideo => {
      hevc::scan(stream, &mut buffer, extended_diagnostics, &mut state.hevc);
    }
    TSStreamType::VC1Video => {
      vc1::scan(stream, &mut buffer);
    }
    TSStreamType::MPEG1Audio | TSStreamType::MPEG2Audio => {
      mpa::scan(stream, &mut buffer);
    }
    TSStreamType::MPEG2AacAudio | TSStreamType::MPEG4AacAudio => {
      aac::scan(stream, &mut buffer);
    }
    TSStreamType::AC3Audio | TSStreamType::AC3PlusAudio | TSStreamType::AC3PlusSecondaryAudio => {
      ac3::scan(stream, &mut buffer);
    }
    TSStreamType::AC3TrueHDAudio => {
      truehd::scan(stream, &mut buffer);
    }
    TSStreamType::LpcmAudio => {
      if let Some(p) = lpcm::parse(payload) {
        stream.channel_count = p.channels;
        stream.lfe = p.lfe;
        stream.sample_rate = p.sample_rate;
        stream.bit_depth = p.bit_depth;
        let total = p.channels + p.lfe;
        stream.bit_rate = p.sample_rate as u64 * p.bit_depth as u64 * total as u64;
        stream.is_vbr = false;
        stream.is_initialized = true;
      }
    }
    TSStreamType::DTSAudio => {
      dts::scan(stream, &mut buffer, bitrate);
    }
    TSStreamType::DTSHDAudio | TSStreamType::DTSHDMasterAudio | TSStreamType::DTSHDSecondaryAudio => {
      dtshd::scan(stream, &mut buffer, bitrate);
    }
    TSStreamType::PresentationGraphics => {
      if is_full_scan {
        pgs::scan(stream, &mut buffer, &mut state.pgs);
      } else {
        stream.is_initialized = true;
      }
    }
    _ => {
      stream.is_initialized = true;
    }
  }
}

/// Update audio/video description strings using the populated parameters.
pub fn finalize_description(stream: &mut TSStreamInfo) {
  let st = TSStreamType::from_u8(stream.stream_type);

  // Refine codec names now that extension/audio-mode flags are known, so
  // Atmos / DTS:X / Dolby Digital EX / DTS-ES labels appear like BDInfo.
  if st.is_audio() {
    let extended_mode = stream.audio_mode == "Extended";
    // TSStream.CodecName returns the parser-derived ExtendedData string for
    // MPEG-1/2 and AAC audio (e.g. "MPEG 1 Layer III", "MPEG-4 AAC LC"), so
    // preserve whatever the codec parser populated rather than overwriting
    // it with the static stream-type name. CodecShortName for those types is
    // still the static label, so it is always (re)derived below.
    if !matches!(
      st,
      TSStreamType::MPEG1Audio | TSStreamType::MPEG2Audio | TSStreamType::MPEG2AacAudio | TSStreamType::MPEG4AacAudio
    ) {
      stream.codec_name = st.codec_name_dynamic(stream.has_extensions, extended_mode).to_string();
    }
    stream.codec_short_name = st
      .codec_short_name_dynamic(stream.has_extensions, extended_mode)
      .to_string();
  }

  if st.is_video() {
    let mut parts: Vec<String> = Vec::new();
    if let Some(bv) = stream.base_view {
      parts.push(if bv {
        "Right Eye".to_string()
      } else {
        "Left Eye".to_string()
      });
    }
    if stream.height > 0 {
      parts.push(format!(
        "{}{}",
        stream.height,
        if stream.is_interlaced { "i" } else { "p" }
      ));
    }
    if stream.frame_rate_enumerator > 0 && stream.frame_rate_denominator > 0 {
      if stream.frame_rate_enumerator % stream.frame_rate_denominator == 0 {
        parts.push(format!(
          "{} fps",
          stream.frame_rate_enumerator / stream.frame_rate_denominator
        ));
      } else {
        parts.push(format!(
          "{:.3} fps",
          stream.frame_rate_enumerator as f64 / stream.frame_rate_denominator as f64
        ));
      }
    } else if !stream.framerate.is_empty() {
      parts.push(format!("{} fps", stream.framerate));
    }
    if !stream.aspect_ratio.is_empty() {
      parts.push(stream.aspect_ratio.clone());
    }
    if !stream.encoding_profile.is_empty() {
      parts.push(stream.encoding_profile.clone());
    }
    if !stream.extended_format_info.is_empty() {
      parts.push(stream.extended_format_info.join(" / "));
    }
    stream.description = parts.join(" / ");
  } else if st.is_audio() {
    let mut parts: Vec<String> = Vec::new();
    let mut channels = if stream.channel_count > 0 {
      format!("{}.{}", stream.channel_count, stream.lfe)
    } else if !stream.channel_layout.is_empty() {
      stream.channel_layout.clone()
    } else {
      String::new()
    };
    // TSStream.ChannelDescription appends -EX/-ES for Extended audio mode.
    if stream.audio_mode == "Extended" {
      match st {
        TSStreamType::AC3Audio => channels.push_str("-EX"),
        TSStreamType::DTSAudio | TSStreamType::DTSHDAudio | TSStreamType::DTSHDMasterAudio => channels.push_str("-ES"),
        _ => {}
      }
    }
    if !channels.is_empty() {
      parts.push(channels);
    }
    if stream.sample_rate > 0 {
      parts.push(format!("{} kHz", stream.sample_rate / 1000));
    }
    if stream.bit_rate > 0 {
      // BDInfo only subtracts the embedded core's bitrate for TrueHD; for
      // DTS-HD HR/MA and DD+ the displayed rate includes the core.
      let core_br = if st == TSStreamType::AC3TrueHDAudio {
        stream.core.as_ref().map(|c| c.bit_rate).unwrap_or(0)
      } else {
        0
      };
      let net = stream.bit_rate.saturating_sub(core_br);
      parts.push(format!("{} kbps", (net + 500) / 1000));
    }
    if stream.bit_depth > 0 {
      parts.push(format!("{}-bit", stream.bit_depth));
    }
    if stream.dial_norm != 0 {
      parts.push(format!("DN {}dB", stream.dial_norm));
    }
    if stream.channel_count == 2 {
      match stream.audio_mode.as_str() {
        "DualMono" => parts.push("Dual Mono".to_string()),
        "Surround" => parts.push("Dolby Surround".to_string()),
        "JointStereo" => parts.push("Joint Stereo".to_string()),
        _ => {}
      }
    }
    let mut desc = parts.join(" / ");
    if let Some(core) = &stream.core {
      let core_st = TSStreamType::from_u8(core.stream_type);
      let codec = match core_st {
        TSStreamType::AC3Audio => "AC3 Embedded",
        TSStreamType::DTSAudio => "DTS Core",
        TSStreamType::AC3PlusAudio => "DD+ Embedded",
        _ => "",
      };
      if !codec.is_empty() {
        desc = format!("{} ({}: {})", desc, codec, core.description);
      }
    }
    stream.description = desc;
  } else if st.is_graphics() {
    // Mirror TSGraphicsStream.Description exactly, including the
    // "( + N Forced Caption)" bracketed form when normal captions are also
    // present (BDInfo commit 2581e58 "Fix PGS Caption count reporting").
    let mut description = String::new();
    if stream.width > 0 || stream.height > 0 {
      description = format!("{}x{}", stream.width, stream.height);
    }
    if stream.captions > 0 || stream.forced_captions > 0 {
      if stream.captions > 0 {
        description.push_str(&format!(
          " / {} Caption{}",
          stream.captions,
          if stream.captions > 1 { "s" } else { "" }
        ));
      }
      if stream.forced_captions > 0 {
        let (prefix, suffix) = if stream.captions > 0 {
          (" ( + ", ")")
        } else {
          (" / ", "")
        };
        description.push_str(&format!(
          "{}{} Forced Caption{}{}",
          prefix,
          stream.forced_captions,
          if stream.forced_captions > 1 { "s" } else { "" },
          suffix
        ));
      }
    }
    stream.description = description;
  }
}

/// Convenience to refine an entire stream from a single PES sample. Used by
/// the lightweight enrichment path before deep per-PES scanning.
pub fn refine_from_pes(stream: &mut TSStreamInfo, sample: &[u8]) {
  let mut state = CodecScanState::default();
  scan_stream(stream, &mut state, sample, 0, false, false);
  finalize_description(stream);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lpcm_dispatch_sets_fields_and_bit_rate() {
    // Channel code 9 (5ch + LFE), sample-rate code 4 (96 kHz), depth code 3 (24-bit).
    let payload = vec![0x00, 0x00, (9 << 4) | 4, 3 << 6];
    let mut stream = TSStreamInfo::new(0x1100, TSStreamType::LpcmAudio as u8);
    let mut state = CodecScanState::default();
    scan_stream(&mut stream, &mut state, &payload, 0, false, false);
    assert!(stream.is_initialized);
    assert_eq!(stream.channel_count, 5);
    assert_eq!(stream.lfe, 1);
    assert_eq!(stream.sample_rate, 96000);
    assert_eq!(stream.bit_depth, 24);
    assert_eq!(stream.bit_rate, 96000 * 24 * 6);
  }

  #[test]
  fn unknown_stream_type_is_marked_initialized() {
    let mut stream = TSStreamInfo::new(0x1FFF, 0xFF);
    let mut state = CodecScanState::default();
    scan_stream(&mut stream, &mut state, &[0u8; 4], 0, false, false);
    assert!(stream.is_initialized);
  }

  #[test]
  fn quick_init_marks_pgs_initialized_without_counting() {
    let mut stream = TSStreamInfo::new(0x1200, TSStreamType::PresentationGraphics as u8);
    let mut state = CodecScanState::default();
    // is_full_scan = false -> quick init path.
    scan_stream(&mut stream, &mut state, &[0x16, 0x00], 0, false, false);
    assert!(stream.is_initialized);
    assert_eq!(stream.captions, 0);
  }

  #[test]
  fn finalize_audio_description() {
    let mut stream = TSStreamInfo::new(0x1100, TSStreamType::AC3Audio as u8);
    stream.channel_count = 5;
    stream.lfe = 1;
    stream.sample_rate = 48000;
    stream.bit_rate = 1_500_000;
    stream.bit_depth = 24;
    stream.dial_norm = -27;
    finalize_description(&mut stream);
    assert_eq!(stream.description, "5.1 / 48 kHz / 1500 kbps / 24-bit / DN -27dB");
  }

  #[test]
  fn finalize_video_description() {
    let mut stream = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
    stream.height = 1080;
    stream.is_interlaced = false;
    stream.frame_rate_enumerator = 24000;
    stream.frame_rate_denominator = 1001;
    stream.aspect_ratio = "16:9".to_string();
    stream.encoding_profile = "High Profile 4.0".to_string();
    finalize_description(&mut stream);
    assert_eq!(stream.description, "1080p / 23.976 fps / 16:9 / High Profile 4.0");
  }

  #[test]
  fn finalize_graphics_description() {
    // With normal captions present, forced captions render in the bracketed
    // "( + N Forced Caption)" form (BDInfo commit 2581e58).
    let mut stream = TSStreamInfo::new(0x1200, TSStreamType::PresentationGraphics as u8);
    stream.width = 1920;
    stream.height = 1080;
    stream.captions = 3;
    stream.forced_captions = 1;
    finalize_description(&mut stream);
    assert_eq!(stream.description, "1920x1080 / 3 Captions ( + 1 Forced Caption)");
  }

  #[test]
  fn finalize_graphics_description_forced_only() {
    // Without normal captions, forced captions use the plain " / " separator.
    let mut stream = TSStreamInfo::new(0x1200, TSStreamType::PresentationGraphics as u8);
    stream.width = 1920;
    stream.height = 1080;
    stream.captions = 0;
    stream.forced_captions = 2;
    finalize_description(&mut stream);
    assert_eq!(stream.description, "1920x1080 / 2 Forced Captions");
  }

  #[test]
  fn finalize_graphics_description_captions_only() {
    let mut stream = TSStreamInfo::new(0x1200, TSStreamType::PresentationGraphics as u8);
    stream.width = 1920;
    stream.height = 1080;
    stream.captions = 1;
    finalize_description(&mut stream);
    assert_eq!(stream.description, "1920x1080 / 1 Caption");
  }

  #[test]
  fn finalize_audio_extended_mode_appends_es_suffix() {
    // DTS in Extended mode renders the channel layout with the "-ES" suffix
    // (TSStream.ChannelDescription) and keeps the full bitrate (core only
    // subtracted for TrueHD).
    let mut stream = TSStreamInfo::new(0x1100, TSStreamType::DTSAudio as u8);
    stream.channel_count = 5;
    stream.lfe = 1;
    stream.sample_rate = 48000;
    stream.audio_mode = "Extended".to_string();
    finalize_description(&mut stream);
    assert!(stream.description.starts_with("5.1-ES /"));
  }

  #[test]
  fn refine_from_pes_runs_dispatch_and_description() {
    let payload = vec![0x00, 0x00, (1 << 4) | 1, 1 << 6]; // mono, 48k, 16-bit
    let mut stream = TSStreamInfo::new(0x1100, TSStreamType::LpcmAudio as u8);
    refine_from_pes(&mut stream, &payload);
    assert!(stream.is_initialized);
    assert_eq!(stream.channel_count, 1);
    assert!(!stream.description.is_empty());
  }

  #[test]
  fn scan_stream_dispatches_every_codec_arm_without_panic() {
    use TSStreamType::*;
    let types = [
      MPEG2Video,
      AVCVideo,
      MVCVideo,
      HEVCVideo,
      VC1Video,
      MPEG1Audio,
      MPEG2Audio,
      MPEG2AacAudio,
      MPEG4AacAudio,
      AC3Audio,
      AC3PlusAudio,
      AC3PlusSecondaryAudio,
      AC3TrueHDAudio,
      LpcmAudio,
      DTSAudio,
      DTSHDAudio,
      DTSHDMasterAudio,
      DTSHDSecondaryAudio,
      PresentationGraphics,
      InteractiveGraphics,
      Subtitle,
      Unknown,
    ];
    let payload = [0u8; 32];
    for st in types {
      let mut a = TSStreamInfo::new(0x1100, st as u8);
      let mut sa = CodecScanState::default();
      scan_stream(&mut a, &mut sa, &payload, 768_000, true, true);
      let mut b = TSStreamInfo::new(0x1100, st as u8);
      let mut sb = CodecScanState::default();
      scan_stream(&mut b, &mut sb, &payload, 768_000, false, false);
    }
  }

  #[test]
  fn pgs_full_scan_path_is_routed_to_counter() {
    let mut stream = TSStreamInfo::new(0x1200, TSStreamType::PresentationGraphics as u8);
    let mut state = CodecScanState::default();
    scan_stream(&mut stream, &mut state, &[0x16, 0x00, 0x00, 0x00], 0, false, true);
    let _ = stream.is_initialized;
  }

  #[test]
  fn finalize_audio_embedded_core_labels() {
    for (core_type, label) in [
      (TSStreamType::AC3Audio, "AC3 Embedded"),
      (TSStreamType::DTSAudio, "DTS Core"),
      (TSStreamType::AC3PlusAudio, "DD+ Embedded"),
    ] {
      let mut stream = TSStreamInfo::new(0x1100, TSStreamType::AC3TrueHDAudio as u8);
      stream.channel_count = 8;
      stream.lfe = 1;
      stream.sample_rate = 48000;
      stream.bit_rate = 5_000_000;
      let mut core = Box::new(TSStreamInfo::new(0x1100, core_type as u8));
      core.bit_rate = 640_000;
      core.description = "5.1 / 48 kHz / 640 kbps".to_string();
      stream.core = Some(core);
      finalize_description(&mut stream);
      assert!(
        stream.description.contains(label),
        "missing {label} in {}",
        stream.description
      );
    }
  }

  #[test]
  fn finalize_audio_truehd_subtracts_only_core_bitrate() {
    let mut stream = TSStreamInfo::new(0x1100, TSStreamType::AC3TrueHDAudio as u8);
    stream.channel_count = 8;
    stream.lfe = 1;
    stream.sample_rate = 48000;
    stream.bit_rate = 5_000_000;
    let mut core = Box::new(TSStreamInfo::new(0x1100, TSStreamType::AC3Audio as u8));
    core.bit_rate = 640_000;
    stream.core = Some(core);
    finalize_description(&mut stream);
    // (5_000_000 - 640_000 + 500) / 1000 = 4360 kbps.
    assert!(stream.description.contains("4360 kbps"), "{}", stream.description);
  }

  #[test]
  fn finalize_video_base_view_integer_fps_and_extended_info() {
    let mut stream = TSStreamInfo::new(0x1011, TSStreamType::MVCVideo as u8);
    stream.base_view = Some(true); // Right Eye
    stream.height = 1080;
    stream.frame_rate_enumerator = 24;
    stream.frame_rate_denominator = 1; // integer -> "24 fps"
    stream.aspect_ratio = "16:9".to_string();
    stream.encoding_profile = "High".to_string();
    stream.extended_format_info = vec!["HDR10".to_string(), "BT.2020".to_string()];
    finalize_description(&mut stream);
    assert!(
      stream
        .description
        .starts_with("Right Eye / 1080p / 24 fps / 16:9 / High")
    );
    assert!(stream.description.contains("HDR10 / BT.2020"));

    let mut left = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
    left.base_view = Some(false);
    left.height = 1080;
    finalize_description(&mut left);
    assert!(left.description.starts_with("Left Eye"));
  }

  #[test]
  fn finalize_video_framerate_string_fallback() {
    let mut stream = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
    stream.height = 720;
    stream.frame_rate_enumerator = 0; // forces the label-string fallback
    stream.framerate = "59.94".to_string();
    finalize_description(&mut stream);
    assert!(stream.description.contains("59.94 fps"), "{}", stream.description);
  }

  #[test]
  fn finalize_audio_channel_layout_fallback_and_stereo_modes() {
    let mut s = TSStreamInfo::new(0x1100, TSStreamType::AC3Audio as u8);
    s.channel_count = 0;
    s.channel_layout = "5.1".to_string();
    s.sample_rate = 48000;
    finalize_description(&mut s);
    assert!(s.description.starts_with("5.1 / 48 kHz"));

    for (mode, label) in [
      ("DualMono", "Dual Mono"),
      ("Surround", "Dolby Surround"),
      ("JointStereo", "Joint Stereo"),
    ] {
      let mut a = TSStreamInfo::new(0x1100, TSStreamType::AC3Audio as u8);
      a.channel_count = 2;
      a.sample_rate = 48000;
      a.audio_mode = mode.to_string();
      finalize_description(&mut a);
      assert!(a.description.contains(label), "missing {label}: {}", a.description);
    }
  }
}
