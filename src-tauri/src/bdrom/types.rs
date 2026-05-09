/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
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
            Self::MPEG1Video | Self::MPEG2Video | Self::AVCVideo | Self::MVCVideo
                | Self::VC1Video | Self::HEVCVideo
        )
    }

    pub fn is_audio(self) -> bool {
        matches!(
            self,
            Self::MPEG1Audio | Self::MPEG2Audio | Self::MPEG2AacAudio | Self::MPEG4AacAudio
                | Self::LpcmAudio | Self::AC3Audio | Self::AC3PlusAudio
                | Self::AC3PlusSecondaryAudio | Self::AC3TrueHDAudio | Self::DTSAudio
                | Self::DTSHDAudio | Self::DTSHDSecondaryAudio | Self::DTSHDMasterAudio
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
