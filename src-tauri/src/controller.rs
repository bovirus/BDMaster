/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::bdrom;
use crate::config;
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

pub async fn generate_report(
    path: String,
    full: bool,
    selected_playlists: Option<Vec<String>>,
) -> Result<String> {
    let mut info = bdrom::scan(&path)?;
    if full {
        let bd = bdrom::open_for_enrichment(&path)?;
        bdrom::enrich_with_stream_stats(&mut info, &bd);
    }
    Ok(bdrom::report::generate(
        &info,
        full,
        selected_playlists.as_deref(),
    ))
}

pub async fn get_playlist_chart_data(
    path: String,
    playlist_name: String,
) -> Result<Vec<ChartSample>> {
    Ok(bdrom::build_chart_samples(&path, &playlist_name))
}

pub async fn write_text_file(file: String, text: String) -> Result<()> {
    let path = Path::new(file.as_str());
    let mut f = File::create(path)?;
    f.write_all(text.as_bytes())?;
    Ok(())
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
