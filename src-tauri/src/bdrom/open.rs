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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};

use crate::bdrom::clpi::{self, StreamClipFile};
use crate::bdrom::disc_title::{read_disc_title_iso, read_disc_title_native};
use crate::bdrom::mpls::{PlaylistFile, parse_mpls_bytes};
use crate::bdrom::model::{BDRom, DiscSource, StreamSource};
use crate::bdrom::udf::UdfImage;

pub(crate) fn open_bdrom(path: &Path, use_ssif: bool) -> Result<BDRom> {
  if !path.exists() {
    return Err(anyhow!("Path does not exist: {}", path.display()));
  }
  if path.is_file() {
    let ext = path
      .extension()
      .map(|e| e.to_string_lossy().to_ascii_lowercase())
      .unwrap_or_default();
    if ext == "iso" {
      return open_bdrom_iso(path, use_ssif);
    }
    // Non-ISO file: inspect the disc rooted at the file's parent folder
    // so dragging a file from inside a Blu-ray (e.g. BDMV/STREAM/00001.m2ts)
    // — or passing one on the CLI — is treated the same as dropping the
    // surrounding folder. `locate_bdmv` walks up from there to find BDMV.
    let parent = path
      .parent()
      .ok_or_else(|| anyhow!("File has no parent folder: {}", path.display()))?;
    return open_bdrom_native(parent, use_ssif);
  }
  open_bdrom_native(path, use_ssif)
}

fn open_bdrom_native(path: &Path, use_ssif: bool) -> Result<BDRom> {
  let directory_bdmv = locate_bdmv(path)?;
  let directory_root = native_disc_root(&directory_bdmv)?;

  let directory_playlist = find_subdir(&directory_bdmv, "PLAYLIST");
  let directory_clipinf = find_subdir(&directory_bdmv, "CLIPINF");
  let directory_stream = find_subdir(&directory_bdmv, "STREAM");
  let directory_bdjo = find_subdir(&directory_bdmv, "BDJO");
  let directory_meta = find_subdir(&directory_bdmv, "META");
  let directory_ssif = directory_stream.as_ref().and_then(|s| find_subdir(s, "SSIF"));
  let directory_snp = find_subdir(&directory_root, "SNP");

  if directory_playlist.is_none() || directory_clipinf.is_none() {
    return Err(anyhow!("Unable to locate PLAYLIST or CLIPINF directory."));
  }

  let volume_label = directory_root
    .file_name()
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_default();

  let size = directory_size(&directory_root);

  let mut is_uhd = false;
  let index_path = directory_bdmv.join("index.bdmv");
  if let Ok(bytes) = std::fs::read(&index_path) {
    if bytes.len() >= 8 {
      let header = String::from_utf8_lossy(&bytes[..8]);
      is_uhd = header == "INDX0300";
    }
  }

  let is_bd_plus = find_subdir(&directory_root, "BDSVM").is_some()
    || find_subdir(&directory_root, "SLYVM").is_some()
    || find_subdir(&directory_root, "ANYVM").is_some();

  let is_bd_java = directory_bdjo.as_ref().map(|d| dir_has_files(d)).unwrap_or(false);

  let is_psp = directory_snp
    .as_ref()
    .map(|d| dir_has_extension(d, "MNV"))
    .unwrap_or(false);

  let is_3d = directory_ssif.as_ref().map(|d| dir_has_files(d)).unwrap_or(false);

  let is_dbox = directory_root.join("FilmIndex.xml").exists();

  let disc_title = directory_meta.as_ref().and_then(|m| read_disc_title_native(m));

  let mut playlists: HashMap<String, PlaylistFile> = HashMap::new();
  if let Some(plist_dir) = &directory_playlist {
    for entry in std::fs::read_dir(plist_dir)?.flatten() {
      let p = entry.path();
      if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("mpls") {
          if let Ok(bytes) = std::fs::read(&p) {
            let name = p
              .file_name()
              .map(|n| n.to_string_lossy().to_uppercase())
              .unwrap_or_default();
            match parse_mpls_bytes(name, &bytes) {
              Ok(pl) => {
                playlists.insert(pl.name.clone(), pl);
              }
              Err(e) => log::warn!("Failed to parse {}: {}", p.display(), e),
            }
          }
        }
      }
    }
  }

  let mut stream_clip_files: HashMap<String, StreamClipFile> = HashMap::new();
  if let Some(clip_dir) = &directory_clipinf {
    for entry in std::fs::read_dir(clip_dir)?.flatten() {
      let p = entry.path();
      if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("clpi") {
          if let Ok(sc) = clpi::parse_clpi(&p) {
            stream_clip_files.insert(sc.name.clone(), sc);
          }
        }
      }
    }
  }

  let mut stream_files: HashMap<String, (StreamSource, u64)> = HashMap::new();
  if let Some(stream_dir) = &directory_stream {
    for entry in std::fs::read_dir(stream_dir)?.flatten() {
      let p = entry.path();
      if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("m2ts") {
          let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_uppercase())
            .unwrap_or_default();
          let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
          stream_files.insert(name, (StreamSource::Native(p), size));
        }
      }
    }
  }

  // SSIF interleaved counterparts (Blu-ray 3D). Pair each `<stem>.SSIF`
  // with the matching `<stem>.M2TS` clip name so codec / scan paths can
  // look up the SSIF reader by clip name when SSIF mode is on.
  let mut interleaved_files: HashMap<String, (StreamSource, u64)> = HashMap::new();
  if let Some(ssif_dir) = &directory_ssif {
    if let Ok(entries) = std::fs::read_dir(ssif_dir) {
      for entry in entries.flatten() {
        let p = entry.path();
        let Some(ext) = p.extension().and_then(|s| s.to_str()) else {
          continue;
        };
        if !ext.eq_ignore_ascii_case("ssif") {
          continue;
        }
        let Some(stem) = p.file_stem().map(|n| n.to_string_lossy().to_uppercase()) else {
          continue;
        };
        let m2ts_name = format!("{}.M2TS", stem);
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        interleaved_files.insert(m2ts_name, (StreamSource::Native(p), size));
      }
    }
  }

  let is_50_hz = playlists
    .values()
    .any(|pl| pl.playlist_streams.iter().any(|s| s.frame_rate.is_50_hz()));

  Ok(BDRom {
    path: path.to_path_buf(),
    source: DiscSource::Native,
    volume_label,
    disc_title,
    size,
    is_uhd,
    is_bd_plus,
    is_bd_java,
    is_dbox,
    is_psp,
    is_3d,
    is_50_hz,
    playlists,
    stream_files,
    stream_clip_files,
    interleaved_files,
    use_ssif,
  })
}

fn open_bdrom_iso(path: &Path, use_ssif: bool) -> Result<BDRom> {
  let image = Arc::new(Mutex::new(UdfImage::open(path)?));

  // Resolve the BDMV directory (case-insensitive).
  let bdmv = {
    let mut img = image.lock().unwrap_or_else(|e| e.into_inner());
    img
      .resolve("BDMV")
      .map_err(|e| anyhow!("UDF: BDMV not found in image: {}", e))?
  };
  if !bdmv.is_directory {
    return Err(anyhow!("UDF: BDMV is not a directory"));
  }

  {
    let mut img = image.lock().unwrap_or_else(|e| e.into_inner());
    let playlist = img.try_resolve("BDMV/PLAYLIST");
    let clipinf = img.try_resolve("BDMV/CLIPINF");
    if !playlist.as_ref().map(|f| f.is_directory).unwrap_or(false)
      || !clipinf.as_ref().map(|f| f.is_directory).unwrap_or(false)
    {
      return Err(anyhow!("UDF: Unable to locate PLAYLIST or CLIPINF directory."));
    }
  }

  // Volume label: prefer the UDF Logical Volume Identifier (what DiscUtils /
  // BDInfo report); fall back to the ISO file name when the LVD has none.
  let volume_label = {
    let img = image.lock().unwrap_or_else(|e| e.into_inner());
    let lvid = img.volume_label.trim().to_string();
    if lvid.is_empty() {
      path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
    } else {
      lvid
    }
  };

  // Total disc size: sum of all files in the root directory tree, skipping
  // .ssif files (mirroring BDInfo's behavior).
  let size = {
    let mut img = image.lock().unwrap_or_else(|e| e.into_inner());
    let root = img.root.clone();
    img.directory_size(&root).unwrap_or(0)
  };

  // index.bdmv → UHD detection.
  let mut is_uhd = false;
  {
    let mut img = image.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(index_fe) = img.resolve("BDMV/index.bdmv") {
      if let Ok(bytes) = img.read_file(&index_fe) {
        if bytes.len() >= 8 {
          let header = String::from_utf8_lossy(&bytes[..8]);
          is_uhd = header == "INDX0300";
        }
      }
    }
  }

  let mut img = image.lock().unwrap_or_else(|e| e.into_inner());

  let is_bd_plus =
    img.try_resolve("BDSVM").is_some() || img.try_resolve("SLYVM").is_some() || img.try_resolve("ANYVM").is_some();

  let is_bd_java = img
    .try_resolve("BDMV/BDJO")
    .filter(|d| d.is_directory)
    .map(|d| {
      img
        .list_dir(&d)
        .map(|es| es.iter().any(|e| !e.is_parent && !e.is_directory))
        .unwrap_or(false)
    })
    .unwrap_or(false);

  let is_psp = img
    .try_resolve("SNP")
    .filter(|d| d.is_directory)
    .map(|d| {
      img
        .list_dir(&d)
        .map(|es| {
          es.iter()
            .any(|e| !e.is_parent && e.name.to_ascii_lowercase().ends_with(".mnv"))
        })
        .unwrap_or(false)
    })
    .unwrap_or(false);

  let is_3d = img
    .try_resolve("BDMV/STREAM/SSIF")
    .filter(|d| d.is_directory)
    .map(|d| {
      img
        .list_dir(&d)
        .map(|es| es.iter().any(|e| !e.is_parent && !e.is_directory))
        .unwrap_or(false)
    })
    .unwrap_or(false);

  let is_dbox = img.try_resolve("FilmIndex.xml").is_some();

  let disc_title = read_disc_title_iso(&mut img);

  // Read MPLS playlists from BDMV/PLAYLIST.
  let mut playlists: HashMap<String, PlaylistFile> = HashMap::new();
  if let Ok(playlist_dir) = img.resolve("BDMV/PLAYLIST") {
    if let Ok(entries) = img.list_dir(&playlist_dir) {
      for entry in entries {
        if entry.is_parent || entry.is_deleted || entry.is_directory {
          continue;
        }
        if !entry.name.to_ascii_lowercase().ends_with(".mpls") {
          continue;
        }
        if let Ok(fe) = crate::bdrom::udf::read_file_entry_at(&mut img, &entry.icb) {
          if let Ok(bytes) = img.read_file(&fe) {
            let name = entry.name.to_uppercase();
            match parse_mpls_bytes(name.clone(), &bytes) {
              Ok(pl) => {
                playlists.insert(pl.name.clone(), pl);
              }
              Err(e) => log::warn!("Failed to parse {}: {}", name, e),
            }
          }
        }
      }
    }
  }

  // CLPI.
  let mut stream_clip_files: HashMap<String, StreamClipFile> = HashMap::new();
  if let Ok(clip_dir) = img.resolve("BDMV/CLIPINF") {
    if let Ok(entries) = img.list_dir(&clip_dir) {
      for entry in entries {
        if entry.is_parent || entry.is_deleted || entry.is_directory {
          continue;
        }
        if !entry.name.to_ascii_lowercase().ends_with(".clpi") {
          continue;
        }
        if let Ok(fe) = crate::bdrom::udf::read_file_entry_at(&mut img, &entry.icb) {
          let name = entry.name.to_uppercase();
          // CLPI files are small; read and parse the content so stream
          // metadata is available (same as the native-folder path).
          let scf = match img.read_file(&fe) {
            Ok(bytes) => clpi::parse_clpi_bytes(name.clone(), fe.size, &bytes),
            Err(_) => StreamClipFile {
              name: name.clone(),
              size: fe.size,
              ..Default::default()
            },
          };
          stream_clip_files.insert(name, scf);
        }
      }
    }
  }

  // M2TS.
  let mut stream_files: HashMap<String, (StreamSource, u64)> = HashMap::new();
  if let Ok(stream_dir) = img.resolve("BDMV/STREAM") {
    if let Ok(entries) = img.list_dir(&stream_dir) {
      for entry in entries {
        if entry.is_parent || entry.is_deleted || entry.is_directory {
          continue;
        }
        if !entry.name.to_ascii_lowercase().ends_with(".m2ts") {
          continue;
        }
        if let Ok(fe) = crate::bdrom::udf::read_file_entry_at(&mut img, &entry.icb) {
          let name = entry.name.to_uppercase();
          let size = fe.size;
          stream_files.insert(name, (StreamSource::Iso(fe), size));
        }
      }
    }
  }

  // SSIF interleaved counterparts. Same pairing as native: clip name
  // `<stem>.M2TS` → file `<stem>.SSIF` under `BDMV/STREAM/SSIF/`.
  let mut interleaved_files: HashMap<String, (StreamSource, u64)> = HashMap::new();
  if let Ok(ssif_dir) = img.resolve("BDMV/STREAM/SSIF") {
    if let Ok(entries) = img.list_dir(&ssif_dir) {
      for entry in entries {
        if entry.is_parent || entry.is_deleted || entry.is_directory {
          continue;
        }
        let name_lc = entry.name.to_ascii_lowercase();
        if !name_lc.ends_with(".ssif") {
          continue;
        }
        if let Ok(fe) = crate::bdrom::udf::read_file_entry_at(&mut img, &entry.icb) {
          let upper = entry.name.to_uppercase();
          let stem = &upper[..upper.len() - ".SSIF".len()];
          let m2ts_name = format!("{}.M2TS", stem);
          let size = fe.size;
          interleaved_files.insert(m2ts_name, (StreamSource::Iso(fe), size));
        }
      }
    }
  }

  drop(img);

  let is_50_hz = playlists
    .values()
    .any(|pl| pl.playlist_streams.iter().any(|s| s.frame_rate.is_50_hz()));

  Ok(BDRom {
    path: path.to_path_buf(),
    source: DiscSource::Iso(image),
    volume_label,
    disc_title,
    size,
    is_uhd,
    is_bd_plus,
    is_bd_java,
    is_dbox,
    is_psp,
    is_3d,
    is_50_hz,
    playlists,
    stream_files,
    stream_clip_files,
    interleaved_files,
    use_ssif,
  })
}

pub(crate) fn locate_bdmv(path: &Path) -> Result<PathBuf> {
  // Walk up the path looking for a BDMV ancestor.
  let mut p: Option<&Path> = Some(path);
  while let Some(cur) = p {
    if cur.file_name().map(|n| n == "BDMV").unwrap_or(false) {
      return Ok(cur.to_path_buf());
    }
    p = cur.parent();
  }
  // Search inside path for a BDMV child.
  if let Some(child) = find_subdir(path, "BDMV") {
    return Ok(child);
  }
  // If path is a folder with index.bdmv at root, treat path itself as BDMV.
  if path.join("index.bdmv").exists() {
    return Ok(path.to_path_buf());
  }
  Err(anyhow!("Unable to locate BDMV directory under {}.", path.display()))
}

fn native_disc_root(directory_bdmv: &Path) -> Result<PathBuf> {
  if directory_bdmv
    .file_name()
    .map(|n| n.to_string_lossy().eq_ignore_ascii_case("BDMV"))
    .unwrap_or(false)
  {
    directory_bdmv
      .parent()
      .ok_or_else(|| anyhow!("BDMV has no parent directory"))
      .map(Path::to_path_buf)
  } else {
    Ok(directory_bdmv.to_path_buf())
  }
}

pub(crate) fn find_subdir(parent: &Path, name: &str) -> Option<PathBuf> {
  let entries = std::fs::read_dir(parent).ok()?;
  for entry in entries.flatten() {
    let p = entry.path();
    if p.is_dir() {
      if p
        .file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        .unwrap_or(false)
      {
        return Some(p);
      }
    }
  }
  None
}

pub(crate) fn dir_has_files(dir: &Path) -> bool {
  std::fs::read_dir(dir)
    .map(|it| it.flatten().any(|e| e.path().is_file()))
    .unwrap_or(false)
}

pub(crate) fn dir_has_extension(dir: &Path, ext: &str) -> bool {
  std::fs::read_dir(dir)
    .map(|it| {
      it.flatten().any(|e| {
        e.path()
          .extension()
          .map(|x| x.to_string_lossy().eq_ignore_ascii_case(ext))
          .unwrap_or(false)
      })
    })
    .unwrap_or(false)
}

pub(crate) fn directory_size(dir: &Path) -> u64 {
  fn inner(dir: &Path, visited: &mut HashSet<PathBuf>, depth: usize) -> u64 {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
      return 0;
    }

    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
      return 0;
    }

    let mut size: u64 = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
      return 0;
    };

    for entry in entries.flatten() {
      let Ok(file_type) = entry.file_type() else {
        continue;
      };
      if file_type.is_symlink() {
        continue;
      }

      let p = entry.path();
      if file_type.is_dir() {
        size += inner(&p, visited, depth + 1);
      } else if file_type.is_file() {
        if p
          .extension()
          .map(|x| x.to_string_lossy().eq_ignore_ascii_case("ssif"))
          .unwrap_or(false)
        {
          continue;
        }
        size += std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
      }
    }
    size
  }

  let mut visited = HashSet::new();
  inner(dir, &mut visited, 0)
}
