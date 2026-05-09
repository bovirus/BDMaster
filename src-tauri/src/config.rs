/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use anyhow::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::constants::APP_NAME;

static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(rename = "appendOnFileDrop")]
    pub append_on_file_drop: bool,
    #[serde(rename = "displayMode")]
    pub display_mode: DisplayMode,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub scan: ConfigScan,
    #[serde(default)]
    pub formatting: ConfigFormatting,
    #[serde(rename = "discInfoSplit", default = "default_disc_info_split")]
    pub disc_info_split: f32,
    #[serde(default)]
    pub update: ConfigUpdate,
    #[serde(default)]
    pub window: ConfigWindow,
}

fn default_disc_info_split() -> f32 {
    0.5
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigFormatting {
    #[serde(rename = "bitRate", default)]
    pub bit_rate: ConfigBitRate,
    #[serde(default)]
    pub size: ConfigSize,
}

impl Default for ConfigFormatting {
    fn default() -> Self {
        Self {
            bit_rate: Default::default(),
            size: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigBitRate {
    #[serde(default)]
    pub precision: FormatPrecision,
    #[serde(default)]
    pub unit: FormatUnit,
}

impl Default for ConfigBitRate {
    fn default() -> Self {
        Self {
            precision: Default::default(),
            unit: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigSize {
    #[serde(default)]
    pub precision: FormatPrecision,
    #[serde(default)]
    pub unit: FormatUnit,
}

impl Default for ConfigSize {
    fn default() -> Self {
        Self {
            precision: Default::default(),
            unit: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum FormatPrecision {
    Zero,
    One,
    Two,
}

impl Default for FormatPrecision {
    fn default() -> Self {
        Self::Two
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum FormatUnit {
    K,
    KM,
    KMG,
    KMGT,
    KMi,
    KMiGi,
    KMiGiTi,
}

impl Default for FormatUnit {
    fn default() -> Self {
        Self::KMGT
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            append_on_file_drop: true,
            display_mode: Default::default(),
            theme: Default::default(),
            language: Default::default(),
            scan: Default::default(),
            formatting: Default::default(),
            disc_info_split: 0.5,
            update: Default::default(),
            window: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigScan {
    #[serde(rename = "generateStreamDiagnostics", default = "default_true")]
    pub generate_stream_diagnostics: bool,
    #[serde(rename = "enableSsifSupport", default = "default_true")]
    pub enable_ssif_support: bool,
    #[serde(rename = "filterLoopingPlaylists", default = "default_true")]
    pub filter_looping_playlists: bool,
    #[serde(rename = "filterShortPlaylists", default = "default_true")]
    pub filter_short_playlists: bool,
    #[serde(rename = "filterShortPlaylistsValue", default = "default_filter_short_value")]
    pub filter_short_playlists_value: u32,
    #[serde(rename = "useImagePrefix", default)]
    pub use_image_prefix: bool,
    #[serde(rename = "useImagePrefixValue", default)]
    pub use_image_prefix_value: String,
    #[serde(rename = "keepStreamOrder", default = "default_true")]
    pub keep_stream_order: bool,
    #[serde(rename = "generateTextSummary", default = "default_true")]
    pub generate_text_summary: bool,
    #[serde(rename = "autosaveReport", default)]
    pub autosave_report: bool,
    #[serde(rename = "displayChapterCount", default = "default_true")]
    pub display_chapter_count: bool,
    #[serde(rename = "enableExtendedStreamDiagnostics", default)]
    pub enable_extended_stream_diagnostics: bool,
}

fn default_true() -> bool { true }
fn default_filter_short_value() -> u32 { 20 }

impl Default for ConfigScan {
    fn default() -> Self {
        Self {
            generate_stream_diagnostics: true,
            enable_ssif_support: true,
            filter_looping_playlists: true,
            filter_short_playlists: true,
            filter_short_playlists_value: 20,
            use_image_prefix: false,
            use_image_prefix_value: String::new(),
            keep_stream_order: true,
            generate_text_summary: true,
            autosave_report: false,
            display_chapter_count: true,
            enable_extended_stream_diagnostics: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigWindow {
    #[serde(default)]
    pub position: ConfigWindowPosition,
    #[serde(default)]
    pub size: ConfigWindowSize,
}

impl Default for ConfigWindow {
    fn default() -> Self {
        Self { position: Default::default(), size: Default::default() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigWindowPosition {
    pub x: i32,
    pub y: i32,
}

impl Default for ConfigWindowPosition {
    fn default() -> Self {
        Self { x: -1, y: -1 }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigWindowSize {
    pub width: u32,
    pub height: u32,
}

impl Default for ConfigWindowSize {
    fn default() -> Self {
        Self { width: 1200, height: 900 }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigUpdate {
    #[serde(rename = "checkInterval", default)]
    pub check_interval: UpdateCheckInterval,
    #[serde(rename = "lastChecked", default)]
    pub last_checked: i64,
    #[serde(rename = "lastVersion", default)]
    pub last_version: String,
    #[serde(rename = "ignoreVersion", default)]
    pub ignore_version: String,
}

impl Default for ConfigUpdate {
    fn default() -> Self {
        Self {
            check_interval: Default::default(),
            last_checked: 0,
            last_version: String::new(),
            ignore_version: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum UpdateCheckInterval {
    Daily,
    Weekly,
    Monthly,
}

impl Default for UpdateCheckInterval {
    fn default() -> Self { Self::Weekly }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Language {
    #[serde(rename = "de")]
    De,
    #[serde(rename = "en-US")]
    EnUS,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "zh-CN")]
    ZhCN,
    #[serde(rename = "zh-HK")]
    ZhHK,
    #[serde(rename = "zh-TW")]
    ZhTW,
}

impl Default for Language {
    fn default() -> Self { Self::EnUS }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum DisplayMode {
    Auto,
    Light,
    Dark,
}

impl Default for DisplayMode {
    fn default() -> Self { Self::Auto }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Theme {
    #[serde(alias = "Default")]
    Ocean,
    Aqua,
    Sky,
    Arctic,
    Glacier,
    Mist,
    Slate,
    Charcoal,
    Midnight,
    Indigo,
    Violet,
    Lavender,
    Rose,
    Blush,
    Coral,
    Sunset,
    Amber,
    Sand,
    Forest,
    Emerald,
}

impl Default for Theme {
    fn default() -> Self { Self::Ocean }
}

impl Config {
    fn new() -> Self {
        let path = Self::get_path_buf();
        if path.exists() {
            Self::load(path).unwrap_or_else(|err| {
                log::warn!("Couldn't load config: {}, using default", err);
                let cfg = Self::default();
                let _ = cfg.save(Self::get_path_buf());
                cfg
            })
        } else {
            let cfg = Self::default();
            if let Err(err) = cfg.save(path) {
                log::error!("Couldn't save default config: {}", err);
            }
            cfg
        }
    }

    fn get_path_buf() -> PathBuf {
        let dir = Self::get_config_dir();
        if !dir.exists() {
            if let Err(err) = std::fs::create_dir_all(&dir) {
                log::warn!("Couldn't create config dir: {}", err);
            }
        }
        dir.join(format!("{}.json", APP_NAME))
    }

    fn get_exe_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    #[cfg(target_os = "linux")]
    fn get_config_dir() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join(APP_NAME);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config").join(APP_NAME);
        }
        Self::get_exe_dir()
    }

    #[cfg(target_os = "macos")]
    fn get_config_dir() -> PathBuf {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_NAME);
        }
        Self::get_exe_dir()
    }

    #[cfg(target_os = "windows")]
    fn get_config_dir() -> PathBuf {
        let exe_dir = Self::get_exe_dir();
        let exe_path_lc = exe_dir.to_string_lossy().to_ascii_lowercase();
        let starts_with_env = |env_var: &str| -> bool {
            std::env::var(env_var)
                .ok()
                .map(|p| !p.is_empty() && exe_path_lc.starts_with(&p.to_ascii_lowercase()))
                .unwrap_or(false)
        };
        let is_installed = starts_with_env("LOCALAPPDATA")
            || starts_with_env("ProgramFiles")
            || starts_with_env("ProgramFiles(x86)");
        if is_installed {
            if let Ok(appdata) = std::env::var("APPDATA") {
                if !appdata.is_empty() {
                    return PathBuf::from(appdata).join(APP_NAME);
                }
            }
        }
        exe_dir
    }

    fn load(path: PathBuf) -> Result<Self> {
        let file = File::open(&path)?;
        let buf = BufReader::new(file);
        Ok(serde_json::from_reader(buf)?)
    }

    fn save(&self, path: PathBuf) -> Result<()> {
        let file = File::create(&path)?;
        let buf = BufWriter::new(file);
        serde_json::to_writer_pretty(buf, &self).map_err(Error::msg)
    }
}

pub fn get_config() -> Config {
    CONFIG
        .get_or_init(|| RwLock::new(Config::new()))
        .read()
        .unwrap()
        .clone()
}

pub fn set_config(config: Config) -> Result<()> {
    let path = Config::get_path_buf();
    let result = config.save(path);
    CONFIG
        .get_or_init(|| RwLock::new(Config::new()))
        .write()
        .unwrap()
        .clone_from(&config);
    result
}
