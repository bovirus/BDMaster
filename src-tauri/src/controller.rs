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

use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use crate::bdrom;
use crate::config;
use crate::constants::APP_NAME;
use crate::protocol::*;

pub fn get_app_version() -> &'static str {
  env!("CARGO_PKG_VERSION")
}

pub async fn get_about() -> Result<About> {
  Ok(About {
    app_version: get_app_version().to_owned(),
  })
}

pub async fn get_config() -> Result<config::Config> {
  Ok(config::get_config())
}

pub async fn set_config(c: config::Config) -> Result<config::Config> {
  config::set_config(c)?;
  Ok(config::get_config())
}

pub async fn scan_disc(path: String) -> Result<DiscInfo> {
  bdrom::scan(&path)
}

/// Dispose the per-disc codec cache when a disc is closed in the app.
pub fn close_disc(path: String) {
  bdrom::close_codec_cache(&path);
}

pub fn start_full_scan(path: String, state: Arc<FullScanState>) {
  bdrom::full_scan::start(path, state);
}

pub fn cancel_full_scan(state: &FullScanState) {
  bdrom::full_scan::cancel(state);
}

pub fn get_scan_progress(state: &FullScanState) -> ScanProgressInfo {
  bdrom::full_scan::snapshot(state)
}

pub async fn write_text_file(file: String, text: String) -> Result<()> {
  let path = Path::new(file.as_str());
  let mut f = File::create(path)?;
  f.write_all(text.as_bytes())?;
  Ok(())
}

pub async fn write_binary_file(file: String, bytes: Vec<u8>) -> Result<()> {
  let path = Path::new(file.as_str());
  let mut f = File::create(path)?;
  f.write_all(&bytes)?;
  Ok(())
}

/// Whether `path` (or, if it doesn't yet exist, its nearest existing ancestor)
/// is a writable directory. Used by the output-path dialog to block a path the
/// external tool would later fail to write into.
pub async fn check_output_path_writable(path: String) -> Result<bool> {
  let mut current = std::path::PathBuf::from(&path);
  loop {
    if current.exists() {
      break;
    }
    let Some(parent) = current.parent() else {
      return Ok(false);
    };
    current = parent.to_path_buf();
  }
  if !current.is_dir() {
    return Ok(false);
  }
  let test_name = format!(".bdmaster_writecheck_{}", std::process::id());
  let test_path = current.join(&test_name);
  match File::create(&test_path) {
    Ok(_) => {
      let _ = std::fs::remove_file(&test_path);
      Ok(true)
    }
    Err(_) => Ok(false),
  }
}

/// Whether the exact directory `path` already exists. Used by the output-path
/// dialog to warn (non-blocking) that a non-existent path will be created when
/// the external tool runs.
pub async fn output_path_exists(path: String) -> Result<bool> {
  Ok(Path::new(&path).is_dir())
}

pub fn check_for_updates() -> Result<UpdateCheckResult> {
  let app_version = get_app_version();
  log::info!("Checking for updates. Current version: {}", app_version);
  let resp = ureq::get("https://api.github.com/repos/caoccao/BDMaster/releases")
    .set("User-Agent", APP_NAME)
    .call()
    .map_err(|e| anyhow::anyhow!("Failed to fetch releases: {}", e))?;
  let json: serde_json::Value = resp
    .into_json()
    .map_err(|e| anyhow::anyhow!("Failed to parse releases: {}", e))?;
  if let Some(first) = json.as_array().and_then(|arr| arr.first()) {
    let tag = first["tag_name"].as_str().unwrap_or("");
    log::info!("Latest release tag: {}", tag);
    if is_newer_version(tag, app_version) {
      let version = tag.trim_start_matches('v').to_owned();
      return Ok(UpdateCheckResult {
        has_update: true,
        latest_version: Some(version),
      });
    }
  }
  Ok(UpdateCheckResult {
    has_update: false,
    latest_version: None,
  })
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
  let latest = latest.trim_start_matches('v');
  let current = current.trim_start_matches('v');
  let latest_parts: Vec<u32> = latest.split('.').filter_map(|s| s.parse().ok()).collect();
  let current_parts: Vec<u32> = current.split('.').filter_map(|s| s.parse().ok()).collect();
  let len = latest_parts.len().max(current_parts.len());
  for i in 0..len {
    let l = latest_parts.get(i).copied().unwrap_or(0);
    let c = current_parts.get(i).copied().unwrap_or(0);
    if l > c {
      return true;
    }
    if l < c {
      return false;
    }
  }
  false
}
