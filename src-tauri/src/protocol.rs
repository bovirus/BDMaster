/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct About {
    #[serde(rename = "appVersion")]
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MkvToolNixStatus {
    pub found: bool,
    #[serde(rename = "mkvToolNixPath")]
    pub mkv_toolnix_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetterMediaInfoStatus {
    pub found: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    #[serde(rename = "hasUpdate")]
    pub has_update: bool,
    #[serde(rename = "latestVersion")]
    pub latest_version: Option<String>,
}

pub struct UpdateCheckState {
    pub result: Arc<Mutex<Option<UpdateCheckResult>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscInfo {
    pub path: String,
    #[serde(rename = "discName")]
    pub disc_name: String,
    #[serde(rename = "discTitle")]
    pub disc_title: String,
    #[serde(rename = "volumeLabel")]
    pub volume_label: String,
    pub size: u64,
    #[serde(rename = "isBdPlus")]
    pub is_bd_plus: bool,
    #[serde(rename = "isBdJava")]
    pub is_bd_java: bool,
    #[serde(rename = "is3D")]
    pub is_3d: bool,
    #[serde(rename = "is4K")]
    pub is_4k: bool,
    #[serde(rename = "is50Hz")]
    pub is_50hz: bool,
    #[serde(rename = "isDBOX")]
    pub is_dbox: bool,
    #[serde(rename = "isPSP")]
    pub is_psp: bool,
    #[serde(rename = "isUHD")]
    pub is_uhd: bool,
    #[serde(rename = "hasMVCExtension")]
    pub has_mvc_extension: bool,
    #[serde(rename = "hasHEVCStreams")]
    pub has_hevc_streams: bool,
    #[serde(rename = "hasUHDDiscMarker")]
    pub has_uhd_disc_marker: bool,
    #[serde(rename = "metaTitle")]
    pub meta_title: Option<String>,
    #[serde(rename = "metaDiscNumber")]
    pub meta_disc_number: Option<u32>,
    #[serde(rename = "fileSetIdentifier")]
    pub file_set_identifier: Option<String>,
    pub playlists: Vec<PlaylistInfo>,
    #[serde(rename = "streamFiles")]
    pub stream_files: Vec<StreamFileInfo>,
    #[serde(rename = "streamClipFiles")]
    pub stream_clip_files: Vec<StreamClipFileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistInfo {
    pub name: String,
    #[serde(rename = "groupIndex")]
    pub group_index: u32,
    #[serde(rename = "fileSize")]
    pub file_size: u64,
    #[serde(rename = "measuredSize")]
    pub measured_size: u64,
    #[serde(rename = "totalLength")]
    pub total_length: u64,
    #[serde(rename = "hasHiddenTracks")]
    pub has_hidden_tracks: bool,
    #[serde(rename = "hasLoops")]
    pub has_loops: bool,
    #[serde(rename = "isCustom")]
    pub is_custom: bool,
    pub chapters: Vec<f64>,
    #[serde(rename = "chapterMetrics")]
    pub chapter_metrics: Vec<ChapterMetricsInfo>,
    #[serde(rename = "bitrateSamples")]
    pub bitrate_samples: Vec<ChartSample>,
    #[serde(rename = "streamClips")]
    pub stream_clips: Vec<PlaylistStreamClipInfo>,
    #[serde(rename = "videoStreams")]
    pub video_streams: Vec<TSStreamInfo>,
    #[serde(rename = "audioStreams")]
    pub audio_streams: Vec<TSStreamInfo>,
    #[serde(rename = "graphicsStreams")]
    pub graphics_streams: Vec<TSStreamInfo>,
    #[serde(rename = "textStreams")]
    pub text_streams: Vec<TSStreamInfo>,
    #[serde(rename = "totalAngles")]
    pub total_angles: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChapterMetricsInfo {
    #[serde(rename = "avgVideoRate")]
    pub avg_video_rate: u64,
    #[serde(rename = "max1SecRate")]
    pub max_1_sec_rate: u64,
    #[serde(rename = "max1SecTime")]
    pub max_1_sec_time: f64,
    #[serde(rename = "max5SecRate")]
    pub max_5_sec_rate: u64,
    #[serde(rename = "max5SecTime")]
    pub max_5_sec_time: f64,
    #[serde(rename = "max10SecRate")]
    pub max_10_sec_rate: u64,
    #[serde(rename = "max10SecTime")]
    pub max_10_sec_time: f64,
    #[serde(rename = "avgFrameSize")]
    pub avg_frame_size: u64,
    #[serde(rename = "maxFrameSize")]
    pub max_frame_size: u64,
    #[serde(rename = "maxFrameTime")]
    pub max_frame_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistStreamClipInfo {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "timeIn")]
    pub time_in: u64,
    #[serde(rename = "timeOut")]
    pub time_out: u64,
    #[serde(rename = "relativeTimeIn")]
    pub relative_time_in: u64,
    #[serde(rename = "relativeTimeOut")]
    pub relative_time_out: u64,
    pub length: u64,
    #[serde(rename = "fileSize")]
    pub file_size: u64,
    #[serde(rename = "measuredSize")]
    pub measured_size: u64,
    #[serde(rename = "interleavedFileSize")]
    pub interleaved_file_size: u64,
    #[serde(rename = "angleIndex")]
    pub angle_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamFileInfo {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub size: u64,
    #[serde(rename = "interleavedFileSize")]
    pub interleaved_file_size: u64,
    pub duration: u64,
    pub interleaved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamClipFileInfo {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TSStreamInfo {
    pub pid: u16,
    #[serde(rename = "streamType")]
    pub stream_type: u8,
    #[serde(rename = "streamTypeText")]
    pub stream_type_text: String,
    #[serde(rename = "codecName")]
    pub codec_name: String,
    #[serde(rename = "codecShortName")]
    pub codec_short_name: String,
    pub description: String,
    #[serde(rename = "bitRate")]
    pub bit_rate: u64,
    #[serde(rename = "activeBitRate")]
    pub active_bit_rate: u64,
    #[serde(rename = "measuredSize")]
    pub measured_size: u64,
    #[serde(rename = "isVideoStream")]
    pub is_video_stream: bool,
    #[serde(rename = "isAudioStream")]
    pub is_audio_stream: bool,
    #[serde(rename = "isGraphicsStream")]
    pub is_graphics_stream: bool,
    #[serde(rename = "isTextStream")]
    pub is_text_stream: bool,
    #[serde(rename = "isInitialized")]
    pub is_initialized: bool,
    #[serde(rename = "isHidden")]
    pub is_hidden: bool,
    #[serde(rename = "isVbr")]
    pub is_vbr: bool,
    pub width: u32,
    pub height: u32,
    pub framerate: String,
    #[serde(rename = "frameRateEnumerator")]
    pub frame_rate_enumerator: u32,
    #[serde(rename = "frameRateDenominator")]
    pub frame_rate_denominator: u32,
    #[serde(rename = "aspectRatio")]
    pub aspect_ratio: String,
    #[serde(rename = "aspectRatioCode")]
    pub aspect_ratio_code: u32,
    #[serde(rename = "videoFormat")]
    pub video_format: String,
    #[serde(rename = "isInterlaced")]
    pub is_interlaced: bool,
    #[serde(rename = "encodingProfile")]
    pub encoding_profile: String,
    #[serde(rename = "extendedFormatInfo")]
    pub extended_format_info: Vec<String>,
    #[serde(rename = "baseView")]
    pub base_view: Option<bool>,
    #[serde(rename = "channelCount")]
    pub channel_count: u32,
    pub lfe: u32,
    #[serde(rename = "sampleRate")]
    pub sample_rate: u32,
    #[serde(rename = "bitDepth")]
    pub bit_depth: u32,
    #[serde(rename = "channelLayout")]
    pub channel_layout: String,
    #[serde(rename = "audioMode")]
    pub audio_mode: String,
    #[serde(rename = "dialNorm")]
    pub dial_norm: i32,
    #[serde(rename = "hasExtensions")]
    pub has_extensions: bool,
    #[serde(rename = "core")]
    pub core: Option<Box<TSStreamInfo>>,
    #[serde(rename = "captions")]
    pub captions: u32,
    #[serde(rename = "forcedCaptions")]
    pub forced_captions: u32,
    #[serde(rename = "languageCode")]
    pub language_code: String,
    #[serde(rename = "languageName")]
    pub language_name: String,
}

impl TSStreamInfo {
    pub fn new(pid: u16, stream_type: u8) -> Self {
        Self {
            pid,
            stream_type,
            stream_type_text: String::new(),
            codec_name: String::new(),
            codec_short_name: String::new(),
            description: String::new(),
            bit_rate: 0,
            active_bit_rate: 0,
            measured_size: 0,
            is_video_stream: false,
            is_audio_stream: false,
            is_graphics_stream: false,
            is_text_stream: false,
            is_initialized: false,
            is_hidden: false,
            is_vbr: false,
            width: 0,
            height: 0,
            framerate: String::new(),
            frame_rate_enumerator: 0,
            frame_rate_denominator: 0,
            aspect_ratio: String::new(),
            aspect_ratio_code: 0,
            video_format: String::new(),
            is_interlaced: false,
            encoding_profile: String::new(),
            extended_format_info: Vec::new(),
            base_view: None,
            channel_count: 0,
            lfe: 0,
            sample_rate: 0,
            bit_depth: 0,
            channel_layout: String::new(),
            audio_mode: String::new(),
            dial_norm: 0,
            has_extensions: false,
            core: None,
            captions: 0,
            forced_captions: 0,
            language_code: String::new(),
            language_name: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSample {
    pub time: f64,
    #[serde(rename = "bitRate")]
    pub bit_rate: u64,
}

/// Snapshot of a running (or just-finished) full scan. The frontend polls for
/// this once a second. `disc` carries the latest measured-size / bit-rate
/// values so the tables update progressively. `version` increments whenever
/// the worker writes a new snapshot, letting the client re-render only on
/// real changes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanProgressInfo {
    pub path: String,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    #[serde(rename = "finishedBytes")]
    pub finished_bytes: u64,
    #[serde(rename = "isRunning")]
    pub is_running: bool,
    #[serde(rename = "isCompleted")]
    pub is_completed: bool,
    #[serde(rename = "isCancelled")]
    pub is_cancelled: bool,
    pub error: Option<String>,
    #[serde(rename = "currentFile")]
    pub current_file: Option<String>,
    /// Unix epoch milliseconds at which the worker started. The frontend
    /// derives Elapsed and Remaining time from this so the values remain
    /// correct even across a frontend reload while the scan is running.
    #[serde(rename = "startedAtMs")]
    pub started_at_ms: u64,
    pub disc: Option<DiscInfo>,
    pub version: u64,
}

pub struct FullScanState {
    pub running: std::sync::atomic::AtomicBool,
    pub cancel: std::sync::atomic::AtomicBool,
    pub progress: Arc<Mutex<ScanProgressInfo>>,
}

impl FullScanState {
    pub fn new() -> Self {
        Self {
            running: std::sync::atomic::AtomicBool::new(false),
            cancel: std::sync::atomic::AtomicBool::new(false),
            progress: Arc::new(Mutex::new(ScanProgressInfo::default())),
        }
    }
}
