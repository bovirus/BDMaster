/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
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
        if l > c { return true; }
        if l < c { return false; }
    }
    false
}
