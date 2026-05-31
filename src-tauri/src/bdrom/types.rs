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
 * Port of BDInfo TSStream.cs enums and codec name resolution.
 */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TSStreamType {
  Unknown = 0,
  MPEG1Video = 0x01,
  MPEG2Video = 0x02,
  AVCVideo = 0x1b,
  MVCVideo = 0x20,
  HEVCVideo = 0x24,
  VC1Video = 0xea,
  MPEG1Audio = 0x03,
  MPEG2Audio = 0x04,
  MPEG2AacAudio = 0x0F,
  MPEG4AacAudio = 0x11,
  LpcmAudio = 0x80,
  AC3Audio = 0x81,
  AC3PlusAudio = 0x84,
  AC3PlusSecondaryAudio = 0xA1,
  AC3TrueHDAudio = 0x83,
  DTSAudio = 0x82,
  DTSHDAudio = 0x85,
  DTSHDSecondaryAudio = 0xA2,
  DTSHDMasterAudio = 0x86,
  PresentationGraphics = 0x90,
  InteractiveGraphics = 0x91,
  Subtitle = 0x92,
}

impl TSStreamType {
  pub fn from_u8(v: u8) -> Self {
    match v {
      0x01 => Self::MPEG1Video,
      0x02 => Self::MPEG2Video,
      0x1b => Self::AVCVideo,
      0x20 => Self::MVCVideo,
      0x24 => Self::HEVCVideo,
      0xea => Self::VC1Video,
      0x03 => Self::MPEG1Audio,
      0x04 => Self::MPEG2Audio,
      0x0F => Self::MPEG2AacAudio,
      0x11 => Self::MPEG4AacAudio,
      0x80 => Self::LpcmAudio,
      0x81 => Self::AC3Audio,
      0x84 => Self::AC3PlusAudio,
      0xA1 => Self::AC3PlusSecondaryAudio,
      0x83 => Self::AC3TrueHDAudio,
      0x82 => Self::DTSAudio,
      0x85 => Self::DTSHDAudio,
      0xA2 => Self::DTSHDSecondaryAudio,
      0x86 => Self::DTSHDMasterAudio,
      0x90 => Self::PresentationGraphics,
      0x91 => Self::InteractiveGraphics,
      0x92 => Self::Subtitle,
      _ => Self::Unknown,
    }
  }

  pub fn is_video(self) -> bool {
    matches!(
      self,
      Self::MPEG1Video | Self::MPEG2Video | Self::AVCVideo | Self::MVCVideo | Self::VC1Video | Self::HEVCVideo
    )
  }

  pub fn is_audio(self) -> bool {
    matches!(
      self,
      Self::MPEG1Audio
        | Self::MPEG2Audio
        | Self::MPEG2AacAudio
        | Self::MPEG4AacAudio
        | Self::LpcmAudio
        | Self::AC3Audio
        | Self::AC3PlusAudio
        | Self::AC3PlusSecondaryAudio
        | Self::AC3TrueHDAudio
        | Self::DTSAudio
        | Self::DTSHDAudio
        | Self::DTSHDSecondaryAudio
        | Self::DTSHDMasterAudio
    )
  }

  pub fn is_graphics(self) -> bool {
    matches!(self, Self::PresentationGraphics | Self::InteractiveGraphics)
  }

  pub fn is_text(self) -> bool {
    matches!(self, Self::Subtitle)
  }

  pub fn codec_name(self) -> &'static str {
    match self {
      Self::MPEG1Video => "MPEG-1 Video",
      Self::MPEG2Video => "MPEG-2 Video",
      Self::AVCVideo => "MPEG-4 AVC Video",
      Self::MVCVideo => "MPEG-4 MVC Video",
      Self::HEVCVideo => "MPEG-H HEVC Video",
      Self::VC1Video => "VC-1 Video",
      Self::MPEG1Audio => "MP1 Audio",
      Self::MPEG2Audio => "MP2 Audio",
      Self::MPEG2AacAudio => "MPEG-2 AAC Audio",
      Self::MPEG4AacAudio => "MPEG-4 AAC Audio",
      Self::LpcmAudio => "LPCM Audio",
      Self::AC3Audio => "Dolby Digital Audio",
      Self::AC3PlusAudio => "Dolby Digital Plus Audio",
      Self::AC3PlusSecondaryAudio => "Dolby Digital Plus Audio",
      Self::AC3TrueHDAudio => "Dolby TrueHD Audio",
      Self::DTSAudio => "DTS Audio",
      Self::DTSHDAudio => "DTS-HD High-Res Audio",
      Self::DTSHDSecondaryAudio => "DTS Express",
      Self::DTSHDMasterAudio => "DTS-HD Master Audio",
      Self::PresentationGraphics => "Presentation Graphics",
      Self::InteractiveGraphics => "Interactive Graphics",
      Self::Subtitle => "Subtitle",
      Self::Unknown => "UNKNOWN",
    }
  }

  pub fn codec_short_name(self) -> &'static str {
    match self {
      Self::MPEG1Video => "MPEG-1",
      Self::MPEG2Video => "MPEG-2",
      Self::AVCVideo => "AVC",
      Self::MVCVideo => "MVC",
      Self::HEVCVideo => "HEVC",
      Self::VC1Video => "VC-1",
      Self::MPEG1Audio => "MP1",
      Self::MPEG2Audio => "MP2",
      Self::MPEG2AacAudio => "MPEG-2 AAC",
      Self::MPEG4AacAudio => "MPEG-4 AAC",
      Self::LpcmAudio => "LPCM",
      Self::AC3Audio => "AC3",
      Self::AC3PlusAudio | Self::AC3PlusSecondaryAudio => "AC3+",
      Self::AC3TrueHDAudio => "TrueHD",
      Self::DTSAudio => "DTS",
      Self::DTSHDAudio => "DTS-HD HR",
      Self::DTSHDSecondaryAudio => "DTS Express",
      Self::DTSHDMasterAudio => "DTS-HD MA",
      Self::PresentationGraphics => "PGS",
      Self::InteractiveGraphics => "IGS",
      Self::Subtitle => "SUB",
      Self::Unknown => "UNKNOWN",
    }
  }

  pub fn type_text(self) -> &'static str {
    if self.is_video() {
      "Video"
    } else if self.is_audio() {
      "Audio"
    } else if self.is_graphics() {
      "Graphics"
    } else if self.is_text() {
      "Subtitle"
    } else {
      "Other"
    }
  }

  /// BDInfo's `TSStream.CodecName`, including the Atmos / DTS:X / EX / ES
  /// variants that depend on the parsed stream's extension and audio-mode
  /// flags. `extended_mode` is `audio_mode == TSAudioMode::Extended`.
  pub fn codec_name_dynamic(self, has_extensions: bool, extended_mode: bool) -> &'static str {
    match self {
      Self::AC3Audio => {
        if extended_mode {
          "Dolby Digital EX Audio"
        } else {
          "Dolby Digital Audio"
        }
      }
      Self::AC3PlusAudio => {
        if has_extensions {
          "Dolby Digital Plus/Atmos Audio"
        } else {
          "Dolby Digital Plus Audio"
        }
      }
      Self::AC3TrueHDAudio => {
        if has_extensions {
          "Dolby TrueHD/Atmos Audio"
        } else {
          "Dolby TrueHD Audio"
        }
      }
      Self::DTSAudio => {
        if extended_mode {
          "DTS-ES Audio"
        } else {
          "DTS Audio"
        }
      }
      Self::DTSHDAudio => {
        if has_extensions {
          "DTS:X High-Res Audio"
        } else {
          "DTS-HD High-Res Audio"
        }
      }
      Self::DTSHDMasterAudio => {
        if has_extensions {
          "DTS:X Master Audio"
        } else {
          "DTS-HD Master Audio"
        }
      }
      _ => self.codec_name(),
    }
  }

  /// BDInfo's `TSStream.CodecShortName`, including dynamic variants.
  pub fn codec_short_name_dynamic(self, has_extensions: bool, extended_mode: bool) -> &'static str {
    match self {
      Self::AC3Audio => {
        if extended_mode {
          "AC3-EX"
        } else {
          "AC3"
        }
      }
      Self::AC3TrueHDAudio => {
        if has_extensions {
          "Atmos"
        } else {
          "TrueHD"
        }
      }
      Self::DTSAudio => {
        if extended_mode {
          "DTS-ES"
        } else {
          "DTS"
        }
      }
      Self::DTSHDAudio => {
        if has_extensions {
          "DTS:X HR"
        } else {
          "DTS-HD HR"
        }
      }
      Self::DTSHDMasterAudio => {
        if has_extensions {
          "DTS:X MA"
        } else {
          "DTS-HD MA"
        }
      }
      _ => self.codec_short_name(),
    }
  }

  /// BDInfo's `TSStream.CodecAltName`, used for the quick-summary report lines.
  /// Like `CodecName`/`CodecShortName`, the Atmos / DTS:X variants depend on
  /// the parsed extension flag (`has_extensions`).
  pub fn codec_alt_name(self, has_extensions: bool) -> &'static str {
    match self {
      Self::MPEG1Video => "MPEG-1",
      Self::MPEG2Video => "MPEG-2",
      Self::AVCVideo => "AVC",
      Self::MVCVideo => "MVC",
      Self::HEVCVideo => "HEVC",
      Self::VC1Video => "VC-1",
      Self::MPEG1Audio => "MP1",
      Self::MPEG2Audio => "MP2",
      Self::MPEG2AacAudio => "MPEG-2 AAC",
      Self::MPEG4AacAudio => "MPEG-4 AAC",
      Self::LpcmAudio => "LPCM",
      Self::AC3Audio => "DD AC3",
      Self::AC3PlusAudio | Self::AC3PlusSecondaryAudio => "DD AC3+",
      Self::AC3TrueHDAudio => {
        if has_extensions {
          "Dolby Atmos"
        } else {
          "Dolby TrueHD"
        }
      }
      Self::DTSAudio => "DTS",
      Self::DTSHDAudio => {
        if has_extensions {
          "DTS:X Hi-Res"
        } else {
          "DTS-HD Hi-Res"
        }
      }
      Self::DTSHDSecondaryAudio => "DTS Express",
      Self::DTSHDMasterAudio => {
        if has_extensions {
          "DTS:X Master"
        } else {
          "DTS-HD Master"
        }
      }
      Self::PresentationGraphics => "PGS",
      Self::InteractiveGraphics => "IGS",
      Self::Subtitle => "SUB",
      Self::Unknown => "UNKNOWN",
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TSVideoFormat {
  Unknown = 0,
  Video480i = 1,
  Video576i = 2,
  Video480p = 3,
  Video1080i = 4,
  Video720p = 5,
  Video1080p = 6,
  Video576p = 7,
  Video2160p = 8,
}

impl TSVideoFormat {
  pub fn from_u8(v: u8) -> Self {
    match v {
      1 => Self::Video480i,
      2 => Self::Video576i,
      3 => Self::Video480p,
      4 => Self::Video1080i,
      5 => Self::Video720p,
      6 => Self::Video1080p,
      7 => Self::Video576p,
      8 => Self::Video2160p,
      _ => Self::Unknown,
    }
  }

  pub fn height(self) -> u32 {
    match self {
      Self::Video480i | Self::Video480p => 480,
      Self::Video576i | Self::Video576p => 576,
      Self::Video720p => 720,
      Self::Video1080i | Self::Video1080p => 1080,
      Self::Video2160p => 2160,
      Self::Unknown => 0,
    }
  }

  pub fn is_interlaced(self) -> bool {
    matches!(self, Self::Video480i | Self::Video576i | Self::Video1080i)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TSFrameRate {
  Unknown = 0,
  F23_976 = 1,
  F24 = 2,
  F25 = 3,
  F29_97 = 4,
  F50 = 6,
  F59_94 = 7,
}

impl TSFrameRate {
  pub fn from_u8(v: u8) -> Self {
    match v {
      1 => Self::F23_976,
      2 => Self::F24,
      3 => Self::F25,
      4 => Self::F29_97,
      6 => Self::F50,
      7 => Self::F59_94,
      _ => Self::Unknown,
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::F23_976 => "23.976",
      Self::F24 => "24",
      Self::F25 => "25",
      Self::F29_97 => "29.97",
      Self::F50 => "50",
      Self::F59_94 => "59.94",
      Self::Unknown => "",
    }
  }

  pub fn is_50_hz(self) -> bool {
    matches!(self, Self::F25 | Self::F50)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TSAspectRatio {
  Unknown,
  Aspect4_3,
  Aspect16_9,
  Aspect2_21,
}

impl TSAspectRatio {
  pub fn from_u8(v: u8) -> Self {
    match v {
      2 => Self::Aspect4_3,
      3 => Self::Aspect16_9,
      4 => Self::Aspect2_21,
      _ => Self::Unknown,
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Aspect4_3 => "4:3",
      Self::Aspect16_9 => "16:9",
      Self::Aspect2_21 => "2.21:1",
      Self::Unknown => "",
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TSChannelLayout {
  Unknown,
  Mono,
  Stereo,
  Multi,
  Combo,
}

impl TSChannelLayout {
  pub fn from_u8(v: u8) -> Self {
    match v {
      1 => Self::Mono,
      3 => Self::Stereo,
      6 => Self::Multi,
      12 => Self::Combo,
      _ => Self::Unknown,
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Mono => "1.0",
      Self::Stereo => "2.0",
      Self::Multi => "5.1",
      Self::Combo => "Combo",
      Self::Unknown => "",
    }
  }
}

pub fn convert_sample_rate(v: u8) -> u32 {
  match v {
    1 => 48000,
    4 | 14 => 96000,
    5 | 12 => 192000,
    _ => 0,
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TSAudioMode {
  Unknown,
  DualMono,
  Stereo,
  Surround,
  Extended,
  JointStereo,
  Mono,
}

impl TSAudioMode {
  pub fn label(self) -> &'static str {
    match self {
      Self::Unknown => "",
      Self::DualMono => "DualMono",
      Self::Stereo => "Stereo",
      Self::Surround => "Surround",
      Self::Extended => "Extended",
      Self::JointStereo => "JointStereo",
      Self::Mono => "Mono",
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn stream_type_from_u8_round_trips() {
    let all = [
      TSStreamType::MPEG1Video,
      TSStreamType::MPEG2Video,
      TSStreamType::AVCVideo,
      TSStreamType::MVCVideo,
      TSStreamType::HEVCVideo,
      TSStreamType::VC1Video,
      TSStreamType::MPEG1Audio,
      TSStreamType::MPEG2Audio,
      TSStreamType::MPEG2AacAudio,
      TSStreamType::MPEG4AacAudio,
      TSStreamType::LpcmAudio,
      TSStreamType::AC3Audio,
      TSStreamType::AC3PlusAudio,
      TSStreamType::AC3PlusSecondaryAudio,
      TSStreamType::AC3TrueHDAudio,
      TSStreamType::DTSAudio,
      TSStreamType::DTSHDAudio,
      TSStreamType::DTSHDSecondaryAudio,
      TSStreamType::DTSHDMasterAudio,
      TSStreamType::PresentationGraphics,
      TSStreamType::InteractiveGraphics,
      TSStreamType::Subtitle,
    ];
    for t in all {
      assert_eq!(TSStreamType::from_u8(t as u8), t);
    }
    assert_eq!(TSStreamType::from_u8(0x00), TSStreamType::Unknown);
    assert_eq!(TSStreamType::from_u8(0xEE), TSStreamType::Unknown);
  }

  #[test]
  fn codec_name_matches_bdinfo_base_cases() {
    use TSStreamType::*;
    assert_eq!(MPEG1Video.codec_name(), "MPEG-1 Video");
    assert_eq!(MPEG2Video.codec_name(), "MPEG-2 Video");
    assert_eq!(AVCVideo.codec_name(), "MPEG-4 AVC Video");
    assert_eq!(MVCVideo.codec_name(), "MPEG-4 MVC Video");
    assert_eq!(HEVCVideo.codec_name(), "MPEG-H HEVC Video");
    assert_eq!(VC1Video.codec_name(), "VC-1 Video");
    assert_eq!(LpcmAudio.codec_name(), "LPCM Audio");
    assert_eq!(AC3Audio.codec_name(), "Dolby Digital Audio");
    assert_eq!(AC3PlusAudio.codec_name(), "Dolby Digital Plus Audio");
    assert_eq!(AC3TrueHDAudio.codec_name(), "Dolby TrueHD Audio");
    assert_eq!(DTSAudio.codec_name(), "DTS Audio");
    assert_eq!(DTSHDAudio.codec_name(), "DTS-HD High-Res Audio");
    assert_eq!(DTSHDSecondaryAudio.codec_name(), "DTS Express");
    assert_eq!(DTSHDMasterAudio.codec_name(), "DTS-HD Master Audio");
    assert_eq!(PresentationGraphics.codec_name(), "Presentation Graphics");
    assert_eq!(InteractiveGraphics.codec_name(), "Interactive Graphics");
    assert_eq!(Subtitle.codec_name(), "Subtitle");
  }

  #[test]
  fn codec_short_name_matches_bdinfo_base_cases() {
    use TSStreamType::*;
    assert_eq!(AVCVideo.codec_short_name(), "AVC");
    assert_eq!(HEVCVideo.codec_short_name(), "HEVC");
    assert_eq!(AC3Audio.codec_short_name(), "AC3");
    assert_eq!(AC3PlusAudio.codec_short_name(), "AC3+");
    assert_eq!(AC3TrueHDAudio.codec_short_name(), "TrueHD");
    assert_eq!(DTSHDMasterAudio.codec_short_name(), "DTS-HD MA");
    assert_eq!(PresentationGraphics.codec_short_name(), "PGS");
    assert_eq!(Subtitle.codec_short_name(), "SUB");
  }

  #[test]
  fn dynamic_codec_names_match_bdinfo_extension_variants() {
    use TSStreamType::*;
    // has_extensions = true / extended_mode = true variants.
    assert_eq!(AC3Audio.codec_name_dynamic(false, true), "Dolby Digital EX Audio");
    assert_eq!(AC3Audio.codec_short_name_dynamic(false, true), "AC3-EX");
    assert_eq!(
      AC3PlusAudio.codec_name_dynamic(true, false),
      "Dolby Digital Plus/Atmos Audio"
    );
    assert_eq!(
      AC3TrueHDAudio.codec_name_dynamic(true, false),
      "Dolby TrueHD/Atmos Audio"
    );
    assert_eq!(AC3TrueHDAudio.codec_short_name_dynamic(true, false), "Atmos");
    assert_eq!(DTSAudio.codec_name_dynamic(false, true), "DTS-ES Audio");
    assert_eq!(DTSAudio.codec_short_name_dynamic(false, true), "DTS-ES");
    assert_eq!(DTSHDAudio.codec_name_dynamic(true, false), "DTS:X High-Res Audio");
    assert_eq!(DTSHDAudio.codec_short_name_dynamic(true, false), "DTS:X HR");
    assert_eq!(DTSHDMasterAudio.codec_name_dynamic(true, false), "DTS:X Master Audio");
    assert_eq!(DTSHDMasterAudio.codec_short_name_dynamic(true, false), "DTS:X MA");
  }

  #[test]
  fn dynamic_names_fall_back_to_base_without_extensions() {
    use TSStreamType::*;
    assert_eq!(AC3Audio.codec_name_dynamic(false, false), AC3Audio.codec_name());
    assert_eq!(AC3TrueHDAudio.codec_name_dynamic(false, false), "Dolby TrueHD Audio");
    assert_eq!(DTSHDMasterAudio.codec_name_dynamic(false, false), "DTS-HD Master Audio");
    // Video types ignore the flags entirely.
    assert_eq!(HEVCVideo.codec_name_dynamic(true, true), "MPEG-H HEVC Video");
  }

  #[test]
  fn type_text_categories() {
    assert_eq!(TSStreamType::AVCVideo.type_text(), "Video");
    assert_eq!(TSStreamType::AC3Audio.type_text(), "Audio");
    assert_eq!(TSStreamType::PresentationGraphics.type_text(), "Graphics");
    assert_eq!(TSStreamType::Subtitle.type_text(), "Subtitle");
    assert_eq!(TSStreamType::Unknown.type_text(), "Other");
  }

  #[test]
  fn video_format_height_and_interlace() {
    assert_eq!(TSVideoFormat::from_u8(4).height(), 1080);
    assert!(TSVideoFormat::Video1080i.is_interlaced());
    assert!(!TSVideoFormat::Video1080p.is_interlaced());
    assert_eq!(TSVideoFormat::Video2160p.height(), 2160);
    assert_eq!(TSVideoFormat::from_u8(99), TSVideoFormat::Unknown);
  }

  #[test]
  fn frame_rate_labels() {
    assert_eq!(TSFrameRate::from_u8(1).label(), "23.976");
    assert_eq!(TSFrameRate::from_u8(7).label(), "59.94");
    assert!(TSFrameRate::F25.is_50_hz());
    assert!(!TSFrameRate::F24.is_50_hz());
  }

  #[test]
  fn aspect_ratio_and_channel_layout_labels() {
    assert_eq!(TSAspectRatio::from_u8(3).label(), "16:9");
    assert_eq!(TSAspectRatio::from_u8(4).label(), "2.21:1");
    assert_eq!(TSChannelLayout::from_u8(6).label(), "5.1");
    assert_eq!(TSChannelLayout::from_u8(1).label(), "1.0");
  }

  #[test]
  fn sample_rate_conversion() {
    assert_eq!(convert_sample_rate(1), 48000);
    assert_eq!(convert_sample_rate(4), 96000);
    assert_eq!(convert_sample_rate(14), 96000);
    assert_eq!(convert_sample_rate(5), 192000);
    assert_eq!(convert_sample_rate(12), 192000);
    assert_eq!(convert_sample_rate(0), 0);
  }

  const ALL_STREAM_TYPES: [TSStreamType; 23] = [
    TSStreamType::Unknown,
    TSStreamType::MPEG1Video,
    TSStreamType::MPEG2Video,
    TSStreamType::AVCVideo,
    TSStreamType::MVCVideo,
    TSStreamType::HEVCVideo,
    TSStreamType::VC1Video,
    TSStreamType::MPEG1Audio,
    TSStreamType::MPEG2Audio,
    TSStreamType::MPEG2AacAudio,
    TSStreamType::MPEG4AacAudio,
    TSStreamType::LpcmAudio,
    TSStreamType::AC3Audio,
    TSStreamType::AC3PlusAudio,
    TSStreamType::AC3PlusSecondaryAudio,
    TSStreamType::AC3TrueHDAudio,
    TSStreamType::DTSAudio,
    TSStreamType::DTSHDAudio,
    TSStreamType::DTSHDSecondaryAudio,
    TSStreamType::DTSHDMasterAudio,
    TSStreamType::PresentationGraphics,
    TSStreamType::InteractiveGraphics,
    TSStreamType::Subtitle,
  ];

  #[test]
  fn every_name_accessor_is_total_over_all_stream_types() {
    // Exercise every match arm of all name accessors and the category
    // predicates for every stream type, with both extension flags.
    for t in ALL_STREAM_TYPES {
      for has_ext in [false, true] {
        for ext_mode in [false, true] {
          assert!(!t.codec_name_dynamic(has_ext, ext_mode).is_empty());
          assert!(!t.codec_short_name_dynamic(has_ext, ext_mode).is_empty());
        }
        assert!(!t.codec_alt_name(has_ext).is_empty());
      }
      assert!(!t.codec_name().is_empty());
      assert!(!t.codec_short_name().is_empty());
      assert!(!t.type_text().is_empty());
      // Exactly one category predicate is true for known media types.
      let cats = [t.is_video(), t.is_audio(), t.is_graphics(), t.is_text()];
      let count = cats.iter().filter(|b| **b).count();
      assert!(count <= 1, "{:?} matched multiple categories", t);
    }
  }

  #[test]
  fn codec_alt_name_matches_bdinfo() {
    use TSStreamType::*;
    assert_eq!(MPEG1Video.codec_alt_name(false), "MPEG-1");
    assert_eq!(MPEG2Video.codec_alt_name(false), "MPEG-2");
    assert_eq!(AVCVideo.codec_alt_name(false), "AVC");
    assert_eq!(MVCVideo.codec_alt_name(false), "MVC");
    assert_eq!(HEVCVideo.codec_alt_name(false), "HEVC");
    assert_eq!(VC1Video.codec_alt_name(false), "VC-1");
    assert_eq!(MPEG1Audio.codec_alt_name(false), "MP1");
    assert_eq!(MPEG2Audio.codec_alt_name(false), "MP2");
    assert_eq!(MPEG2AacAudio.codec_alt_name(false), "MPEG-2 AAC");
    assert_eq!(MPEG4AacAudio.codec_alt_name(false), "MPEG-4 AAC");
    assert_eq!(LpcmAudio.codec_alt_name(false), "LPCM");
    assert_eq!(AC3Audio.codec_alt_name(false), "DD AC3");
    assert_eq!(AC3PlusAudio.codec_alt_name(false), "DD AC3+");
    assert_eq!(AC3PlusSecondaryAudio.codec_alt_name(false), "DD AC3+");
    assert_eq!(AC3TrueHDAudio.codec_alt_name(false), "Dolby TrueHD");
    assert_eq!(AC3TrueHDAudio.codec_alt_name(true), "Dolby Atmos");
    assert_eq!(DTSAudio.codec_alt_name(false), "DTS");
    assert_eq!(DTSHDAudio.codec_alt_name(false), "DTS-HD Hi-Res");
    assert_eq!(DTSHDAudio.codec_alt_name(true), "DTS:X Hi-Res");
    assert_eq!(DTSHDSecondaryAudio.codec_alt_name(false), "DTS Express");
    assert_eq!(DTSHDMasterAudio.codec_alt_name(false), "DTS-HD Master");
    assert_eq!(DTSHDMasterAudio.codec_alt_name(true), "DTS:X Master");
    assert_eq!(PresentationGraphics.codec_alt_name(false), "PGS");
    assert_eq!(InteractiveGraphics.codec_alt_name(false), "IGS");
    assert_eq!(Subtitle.codec_alt_name(false), "SUB");
    assert_eq!(Unknown.codec_alt_name(false), "UNKNOWN");
  }

  #[test]
  fn all_enum_labels_and_conversions_are_total() {
    // TSVideoFormat: every code 0..=8 plus an out-of-range value.
    for v in 0u8..=9 {
      let f = TSVideoFormat::from_u8(v);
      let _ = f.height();
      let _ = f.is_interlaced();
    }
    assert_eq!(TSVideoFormat::Video480i.height(), 480);
    assert_eq!(TSVideoFormat::Video576p.height(), 576);
    assert_eq!(TSVideoFormat::Video720p.height(), 720);
    assert!(TSVideoFormat::Video576i.is_interlaced());
    assert!(!TSVideoFormat::Unknown.is_interlaced());

    // TSFrameRate: every code and its label.
    for v in 0u8..=8 {
      let _ = TSFrameRate::from_u8(v).label();
    }
    assert_eq!(TSFrameRate::F24.label(), "24");
    assert_eq!(TSFrameRate::F29_97.label(), "29.97");
    assert_eq!(TSFrameRate::F50.label(), "50");
    assert_eq!(TSFrameRate::Unknown.label(), "");
    assert!(TSFrameRate::F50.is_50_hz());

    // TSAspectRatio.
    for v in 0u8..=5 {
      let _ = TSAspectRatio::from_u8(v).label();
    }
    assert_eq!(TSAspectRatio::Aspect4_3.label(), "4:3");
    assert_eq!(TSAspectRatio::Unknown.label(), "");

    // TSChannelLayout including the Combo (code 12) variant.
    for v in 0u8..=13 {
      let _ = TSChannelLayout::from_u8(v).label();
    }
    assert_eq!(TSChannelLayout::from_u8(12), TSChannelLayout::Combo);
    assert_eq!(TSChannelLayout::Combo.label(), "Combo");
    assert_eq!(TSChannelLayout::Stereo.label(), "2.0");
    assert_eq!(TSChannelLayout::Unknown.label(), "");

    // TSAudioMode labels.
    for m in [
      TSAudioMode::Unknown,
      TSAudioMode::DualMono,
      TSAudioMode::Stereo,
      TSAudioMode::Surround,
      TSAudioMode::Extended,
      TSAudioMode::JointStereo,
      TSAudioMode::Mono,
    ] {
      let _ = m.label();
    }
    assert_eq!(TSAudioMode::JointStereo.label(), "JointStereo");
    assert_eq!(TSAudioMode::Mono.label(), "Mono");
    assert_eq!(TSAudioMode::Unknown.label(), "");
  }
}
