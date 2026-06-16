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

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::bdrom::model::{StreamSource, effective_stream_source};
use crate::bdrom::open::{find_subdir, locate_bdmv, open_bdrom};

/// Resolve the on-disk path of a playlist (.mpls) file given a disc path
/// (which may point at the disc root, BDMV, or any subdirectory). Only
/// supported for native disc folders — ISO disc images don't expose the
/// playlist as a real file path.
pub fn resolve_playlist_path(disc_path: &str, playlist_name: &str) -> Result<PathBuf> {
  let path = Path::new(disc_path);
  if !path.exists() {
    return Err(anyhow!("Path does not exist: {}", path.display()));
  }
  if path.is_file() {
    return Err(anyhow!(
      "Disc images (.iso) don't expose playlists as files: {}",
      path.display()
    ));
  }
  let bdmv = locate_bdmv(path)?;
  let playlist_dir =
    find_subdir(&bdmv, "PLAYLIST").ok_or_else(|| anyhow!("PLAYLIST directory not found under {}", bdmv.display()))?;
  // Match the playlist file case-insensitively to tolerate uppercase/lowercase
  // discrepancies between the MPLS name we hand back to the frontend
  // (uppercased in `to_disc_info`) and the file as it lives on disk.
  if let Ok(entries) = std::fs::read_dir(&playlist_dir) {
    for entry in entries.flatten() {
      let p = entry.path();
      if p.is_file() {
        if p
          .file_name()
          .map(|n| n.to_string_lossy().eq_ignore_ascii_case(playlist_name))
          .unwrap_or(false)
        {
          return Ok(p);
        }
      }
    }
  }
  Err(anyhow!(
    "Playlist {} not found under {}",
    playlist_name,
    playlist_dir.display()
  ))
}

/// Resolve the on-disk path of a stream clip file for native disc folders.
/// ISO disc images don't expose stream clips as standalone filesystem paths.
pub fn resolve_stream_file_path(disc_path: &str, stream_name: &str) -> Result<PathBuf> {
  let path = Path::new(disc_path);
  if !path.exists() {
    return Err(anyhow!("Path does not exist: {}", path.display()));
  }
  if path.is_file() {
    return Err(anyhow!(
      "Disc images (.iso) don't expose stream clips as files: {}",
      path.display()
    ));
  }

  let use_ssif = crate::config::get_config().scan.enable_ssif_support;
  let bdrom = open_bdrom(path, use_ssif)?;
  let Some((source, _)) = effective_stream_source(&bdrom, stream_name) else {
    return Err(anyhow!("Stream file {} not found.", stream_name));
  };
  match source {
    StreamSource::Native(p) => Ok(p.clone()),
    StreamSource::Iso(_) => Err(anyhow!(
      "Disc images (.iso) don't expose stream clips as files: {}",
      path.display()
    )),
  }
}
