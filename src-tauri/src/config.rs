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

use anyhow::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::constants::APP_NAME;

static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
  #[serde(rename = "appendOnFileDrop")]
  pub append_on_file_drop: bool,
  #[serde(rename = "displayMode")]
  pub display_mode: DisplayMode,
  #[serde(default)]
  pub theme: Theme,
  #[serde(default = "Language::detect_system")]
  pub language: Language,
  #[serde(default)]
  pub scan: ConfigScan,
  #[serde(default)]
  pub formatting: ConfigFormatting,
  #[serde(rename = "discInfoSplit", default = "default_disc_info_split")]
  pub disc_info_split: f32,
  #[serde(rename = "infoPanelSplit", default = "default_info_panel_split")]
  pub info_panel_split: f32,
  #[serde(default)]
  pub update: ConfigUpdate,
  #[serde(default)]
  pub integration: ConfigIntegration,
  #[serde(default)]
  pub window: ConfigWindow,
  // Legacy top-level external-tool keys, kept only to migrate configs written
  // before the tools were grouped under `integration`. They are read in but
  // never serialized; `migrate_legacy` folds them into `integration`.
  #[serde(rename = "mkv", default, skip_serializing)]
  legacy_mkv: Option<ConfigMkv>,
  #[serde(rename = "betterMediaInfo", default, skip_serializing)]
  legacy_better_media_info: Option<ConfigBetterMediaInfo>,
  #[serde(rename = "mpchc", default, skip_serializing)]
  legacy_mpchc: Option<ConfigMpcHc>,
}

fn default_disc_info_split() -> f32 {
  0.5
}

fn default_info_panel_split() -> f32 {
  0.4
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
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
#[serde(default)]
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
#[serde(default)]
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
      language: Language::detect_system(),
      scan: Default::default(),
      formatting: Default::default(),
      disc_info_split: 0.5,
      info_panel_split: 0.4,
      update: Default::default(),
      integration: Default::default(),
      window: Default::default(),
      legacy_mkv: None,
      legacy_better_media_info: None,
      legacy_mpchc: None,
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigIntegration {
  #[serde(default)]
  pub mkv: ConfigMkv,
  #[serde(rename = "betterMediaInfo", default)]
  pub better_media_info: ConfigBetterMediaInfo,
  #[serde(default)]
  pub mpchc: ConfigMpcHc,
}

impl Default for ConfigIntegration {
  fn default() -> Self {
    Self {
      mkv: Default::default(),
      better_media_info: Default::default(),
      mpchc: Default::default(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigMkv {
  #[serde(rename = "mkvToolNixPath", default = "ConfigMkv::default_mkv_toolnix_path")]
  pub mkv_toolnix_path: String,
  #[serde(rename = "outputFileTemplate", default = "ConfigMkv::default_output_file_template")]
  pub output_file_template: String,
}

impl ConfigMkv {
  fn default_mkv_toolnix_path() -> String {
    if cfg!(target_os = "windows") {
      r"C:\Program Files\MKVToolNix".to_owned()
    } else if cfg!(target_os = "macos") {
      "/Applications/MKVToolNix.app/Contents/MacOS".to_owned()
    } else {
      "/usr/bin".to_owned()
    }
  }

  fn default_output_file_template() -> String {
    "{file_name}".to_owned()
  }
}

impl Default for ConfigMkv {
  fn default() -> Self {
    Self {
      mkv_toolnix_path: Self::default_mkv_toolnix_path(),
      output_file_template: Self::default_output_file_template(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigBetterMediaInfo {
  #[serde(default = "ConfigBetterMediaInfo::default_path")]
  pub path: String,
}

impl ConfigBetterMediaInfo {
  fn default_path() -> String {
    if cfg!(target_os = "windows") {
      r"C:\Program Files\BetterMediaInfo".to_owned()
    } else if cfg!(target_os = "macos") {
      "/Applications/BetterMediaInfo.app/Contents/MacOS".to_owned()
    } else {
      "/usr/bin".to_owned()
    }
  }
}

impl Default for ConfigBetterMediaInfo {
  fn default() -> Self {
    Self {
      path: Self::default_path(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigMpcHc {
  #[serde(default = "ConfigMpcHc::default_path")]
  pub path: String,
}

impl ConfigMpcHc {
  fn default_path() -> String {
    if cfg!(target_os = "windows") {
      r"C:\Program Files (x86)\K-Lite Codec Pack\MPC-HC64\mpc-hc64.exe".to_owned()
    } else {
      String::new()
    }
  }
}

impl Default for ConfigMpcHc {
  fn default() -> Self {
    Self {
      path: Self::default_path(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigScan {
  #[serde(rename = "fastScanSeconds", default = "default_fast_scan_seconds")]
  pub fast_scan_seconds: u32,
  #[serde(rename = "enableSsifSupport", default = "default_true")]
  pub enable_ssif_support: bool,
  #[serde(rename = "filterLoopingPlaylists", default = "default_true")]
  pub filter_looping_playlists: bool,
  #[serde(rename = "filterShortPlaylists", default = "default_true")]
  pub filter_short_playlists: bool,
  #[serde(rename = "filterShortPlaylistsValue", default = "default_filter_short_value")]
  pub filter_short_playlists_value: u32,
}

fn default_true() -> bool {
  true
}
fn default_fast_scan_seconds() -> u32 {
  10
}
fn default_filter_short_value() -> u32 {
  20
}

impl Default for ConfigScan {
  fn default() -> Self {
    Self {
      fast_scan_seconds: default_fast_scan_seconds(),
      enable_ssif_support: true,
      filter_looping_playlists: true,
      filter_short_playlists: true,
      filter_short_playlists_value: 20,
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigWindow {
  #[serde(default)]
  pub position: ConfigWindowPosition,
  #[serde(default)]
  pub size: ConfigWindowSize,
}

impl Default for ConfigWindow {
  fn default() -> Self {
    Self {
      position: Default::default(),
      size: Default::default(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
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
#[serde(default)]
pub struct ConfigWindowSize {
  pub width: u32,
  pub height: u32,
}

impl Default for ConfigWindowSize {
  fn default() -> Self {
    Self {
      width: 1200,
      height: 900,
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
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
  fn default() -> Self {
    Self::Weekly
  }
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
  #[serde(rename = "it")]
  It,
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
  fn default() -> Self {
    Self::EnUS
  }
}

impl Language {
  fn detect_system() -> Self {
    let locales: Vec<String> = sys_locale::get_locales().collect();
    for locale in &locales {
      if let Some(language) = Self::from_locale_tag(locale) {
        log::debug!("Detected system language {:?} from locale {}.", language, locale);
        return language;
      }
    }
    if !locales.is_empty() {
      log::debug!("No supported app language found in system locales {:?}.", locales);
    }
    Self::default()
  }

  fn from_locale_tag(locale: &str) -> Option<Self> {
    let normalized = normalize_locale_tag(locale)?;
    let mut parts = normalized.split('-');
    let language = parts.next()?;
    let mut script: Option<&str> = None;
    let mut region: Option<&str> = None;
    for part in parts {
      if part.len() == 4 && script.is_none() {
        script = Some(part);
      } else if (part.len() == 2 || part.len() == 3) && region.is_none() {
        region = Some(part);
      }
    }

    match language {
      "de" => Some(Self::De),
      "en" => Some(Self::EnUS),
      "es" => Some(Self::Es),
      "fr" => Some(Self::Fr),
      "it" => Some(Self::It),
      "ja" => Some(Self::Ja),
      "zh" => Some(match (script, region) {
        (Some("hant"), Some("hk" | "mo")) => Self::ZhHK,
        (Some("hant"), _) => Self::ZhTW,
        (Some("hans"), _) => Self::ZhCN,
        (_, Some("tw")) => Self::ZhTW,
        (_, Some("hk" | "mo")) => Self::ZhHK,
        _ => Self::ZhCN,
      }),
      _ => None,
    }
  }
}

fn normalize_locale_tag(locale: &str) -> Option<String> {
  let locale = locale
    .split(':')
    .find(|part| !part.trim().is_empty())
    .unwrap_or(locale)
    .trim();
  if locale.eq_ignore_ascii_case("c") || locale.eq_ignore_ascii_case("posix") {
    return None;
  }
  let locale = locale
    .split('.')
    .next()
    .unwrap_or(locale)
    .split('@')
    .next()
    .unwrap_or(locale)
    .replace('_', "-")
    .to_ascii_lowercase();
  if locale.is_empty() || locale == "c" || locale == "posix" {
    None
  } else {
    Some(locale)
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum DisplayMode {
  Auto,
  Light,
  Dark,
}

impl Default for DisplayMode {
  fn default() -> Self {
    Self::Auto
  }
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
  fn default() -> Self {
    Self::Ocean
  }
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
    let is_installed =
      starts_with_env("LOCALAPPDATA") || starts_with_env("ProgramFiles") || starts_with_env("ProgramFiles(x86)");
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
    let value: serde_json::Value = serde_json::from_reader(buf)?;
    let should_save_language = value
      .as_object()
      .map(|object| !object.contains_key("language"))
      .unwrap_or(false);
    let mut config: Self = serde_json::from_value(value)?;
    config.migrate_legacy();
    if should_save_language {
      log::debug!("Saving detected language to config {}.", path.display());
      if let Err(err) = config.save(path) {
        log::error!("Couldn't save the detected language because {}", err);
      }
    }
    Ok(config)
  }

  /// Fold the pre-`integration` top-level tool keys into `integration` so a
  /// config written by an older build doesn't silently reset the user's
  /// configured tool paths. Once migrated, `save` drops the legacy keys.
  fn migrate_legacy(&mut self) {
    if let Some(mkv) = self.legacy_mkv.take() {
      self.integration.mkv = mkv;
    }
    if let Some(better_media_info) = self.legacy_better_media_info.take() {
      self.integration.better_media_info = better_media_info;
    }
    if let Some(mpchc) = self.legacy_mpchc.take() {
      self.integration.mpchc = mpchc;
    }
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn config_deserialization_uses_defaults_for_missing_nodes() {
    let config: Config = serde_json::from_str("{}").unwrap();

    assert!(config.append_on_file_drop);
    assert!(matches!(config.display_mode, DisplayMode::Auto));
    assert!(matches!(config.theme, Theme::Ocean));
    assert!(config.scan.enable_ssif_support);
    assert_eq!(config.scan.fast_scan_seconds, 10);
    assert!(config.scan.filter_looping_playlists);
    assert!(config.scan.filter_short_playlists);
    assert_eq!(config.scan.filter_short_playlists_value, 20);
    assert_eq!(config.disc_info_split, 0.5);
    assert_eq!(config.info_panel_split, 0.4);
    assert!(matches!(config.formatting.bit_rate.precision, FormatPrecision::Two));
    assert!(matches!(config.formatting.bit_rate.unit, FormatUnit::KMGT));
    assert_eq!(config.window.position.x, -1);
    assert_eq!(config.window.position.y, -1);
    assert_eq!(config.window.size.width, 1200);
    assert_eq!(config.window.size.height, 900);
    assert!(matches!(config.update.check_interval, UpdateCheckInterval::Weekly));
    assert_eq!(
      config.integration.mkv.mkv_toolnix_path,
      ConfigMkv::default().mkv_toolnix_path
    );
    assert_eq!(
      config.integration.better_media_info.path,
      ConfigBetterMediaInfo::default().path
    );
    assert_eq!(config.integration.mpchc.path, ConfigMpcHc::default().path);
  }

  #[test]
  fn config_deserialization_preserves_present_nodes_while_filling_missing_children() {
    let config: Config = serde_json::from_str(
      r#"{
                "appendOnFileDrop": false,
                "displayMode": "Dark",
                "language": "ja",
                "formatting": {
                    "bitRate": {
                        "unit": "KMi"
                    }
                },
                "scan": {
                    "fastScanSeconds": 7,
                    "filterShortPlaylistsValue": 30
                },
                "window": {
                    "position": {
                        "x": 10
                    }
                },
                "update": {
                    "lastVersion": "1.2.3"
                },
                "integration": {
                    "mkv": {},
                    "betterMediaInfo": {},
                    "mpchc": {}
                }
            }"#,
    )
    .unwrap();

    assert!(!config.append_on_file_drop);
    assert!(matches!(config.display_mode, DisplayMode::Dark));
    assert!(matches!(config.language, Language::Ja));
    assert!(matches!(config.formatting.bit_rate.unit, FormatUnit::KMi));
    assert!(matches!(config.formatting.bit_rate.precision, FormatPrecision::Two));
    assert!(matches!(config.formatting.size.unit, FormatUnit::KMGT));
    assert!(config.scan.enable_ssif_support);
    assert_eq!(config.scan.fast_scan_seconds, 7);
    assert!(config.scan.filter_looping_playlists);
    assert!(config.scan.filter_short_playlists);
    assert_eq!(config.scan.filter_short_playlists_value, 30);
    assert_eq!(config.window.position.x, 10);
    assert_eq!(config.window.position.y, -1);
    assert_eq!(config.window.size.width, 1200);
    assert_eq!(config.update.last_version, "1.2.3");
    assert!(matches!(config.update.check_interval, UpdateCheckInterval::Weekly));
    assert_eq!(
      config.integration.mkv.mkv_toolnix_path,
      ConfigMkv::default().mkv_toolnix_path
    );
    assert_eq!(
      config.integration.better_media_info.path,
      ConfigBetterMediaInfo::default().path
    );
    assert_eq!(config.integration.mpchc.path, ConfigMpcHc::default().path);
  }

  #[test]
  fn legacy_top_level_tool_keys_migrate_into_integration() {
    let mut config: Config = serde_json::from_str(
      r#"{
                "mkv": { "mkvToolNixPath": "/custom/mkv" },
                "betterMediaInfo": { "path": "/custom/bmi" },
                "mpchc": { "path": "/custom/mpc" }
            }"#,
    )
    .unwrap();
    config.migrate_legacy();

    assert_eq!(config.integration.mkv.mkv_toolnix_path, "/custom/mkv");
    assert_eq!(config.integration.better_media_info.path, "/custom/bmi");
    assert_eq!(config.integration.mpchc.path, "/custom/mpc");

    // Migrated configs must not re-emit the legacy top-level keys.
    let json = serde_json::to_value(&config).unwrap();
    assert!(json.get("mkv").is_none());
    assert!(json.get("betterMediaInfo").is_none());
    assert!(json.get("mpchc").is_none());
    assert!(json.get("integration").is_some());
  }

  #[test]
  fn mkv_output_file_template_defaults_and_deserializes() {
    let defaulted: ConfigMkv = serde_json::from_str("{}").unwrap();
    assert_eq!(defaulted.output_file_template, "{file_name}");

    let custom: ConfigMkv = serde_json::from_str(r#"{ "outputFileTemplate": "{file_name}-{video_codec_1}" }"#).unwrap();
    assert_eq!(custom.output_file_template, "{file_name}-{video_codec_1}");
    // Unrelated fields still fall back to their defaults.
    assert_eq!(custom.mkv_toolnix_path, ConfigMkv::default().mkv_toolnix_path);
  }

  #[test]
  fn language_from_locale_tag_maps_supported_locales() {
    assert!(matches!(Language::default(), Language::EnUS));
    assert!(matches!(Language::from_locale_tag("de-DE"), Some(Language::De)));
    assert!(matches!(Language::from_locale_tag("en_US.UTF-8"), Some(Language::EnUS)));
    assert!(matches!(Language::from_locale_tag("es-MX"), Some(Language::Es)));
    assert!(matches!(Language::from_locale_tag("fr-CA"), Some(Language::Fr)));
    assert!(matches!(Language::from_locale_tag("it-IT"), Some(Language::It)));
    assert!(matches!(Language::from_locale_tag("ja-JP"), Some(Language::Ja)));
    assert!(matches!(Language::from_locale_tag("zh-Hans-CN"), Some(Language::ZhCN)));
    assert!(matches!(Language::from_locale_tag("zh-Hant-TW"), Some(Language::ZhTW)));
    assert!(matches!(Language::from_locale_tag("zh-Hant-HK"), Some(Language::ZhHK)));
    assert!(matches!(Language::from_locale_tag("zh-MO"), Some(Language::ZhHK)));
    assert!(matches!(Language::from_locale_tag("C.UTF-8"), None));
  }
}
