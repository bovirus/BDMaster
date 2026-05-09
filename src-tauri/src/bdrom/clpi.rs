/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Minimal CLPI (Clip Information) reader. Just verifies the file exists and
 * extracts file size. Full TSStreamClipFile parser is a larger port.
 */

use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct StreamClipFile {
    pub name: String,
    pub size: u64,
}

pub fn parse_clpi(path: &Path) -> Result<StreamClipFile> {
    let meta = std::fs::metadata(path)?;
    Ok(StreamClipFile {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_uppercase())
            .unwrap_or_default(),
        size: meta.len(),
    })
}
