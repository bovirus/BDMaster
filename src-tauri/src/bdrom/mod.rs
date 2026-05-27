/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Top-level Blu-ray disc scanner. Locates BDMV/PLAYLIST/CLIPINF/STREAM
 * directories under a path and aggregates parsed playlists, clips and
 * streams into a DiscInfo.
 */

pub mod clpi;
pub mod codec;
pub mod full_scan;
pub mod lang;
pub mod m2ts;
pub mod mpls;
pub mod types;
pub mod udf;

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::protocol::{
    DiscInfo, PlaylistInfo, PlaylistStreamClipInfo, StreamClipFileInfo, StreamFileInfo,
    TSStreamInfo,
};

use self::clpi::{ClpiStream, StreamClipFile};
use self::lang::language_name;
use self::mpls::{PlaylistFile, PlaylistStream, parse_mpls_bytes};
use self::types::*;
use self::udf::{UdfFile, UdfFileReader, UdfImage};

pub(crate) const SSIF_MVC_PID: u16 = 0x1012;

#[derive(Clone)]
pub enum StreamSource {
    Native(PathBuf),
    Iso(UdfFile),
}

#[derive(Clone)]
pub enum DiscSource {
    Native,
    Iso(Arc<Mutex<UdfImage>>),
}

pub struct BDRom {
    pub path: PathBuf,
    pub source: DiscSource,
    pub volume_label: String,
    pub disc_title: Option<String>,
    pub size: u64,
    pub is_uhd: bool,
    pub is_bd_plus: bool,
    pub is_bd_java: bool,
    pub is_dbox: bool,
    pub is_psp: bool,
    pub is_3d: bool,
    pub is_50_hz: bool,
    pub playlists: HashMap<String, PlaylistFile>,
    pub stream_files: HashMap<String, (StreamSource, u64)>,
    pub stream_clip_files: HashMap<String, StreamClipFile>,
    /// SSIF (interleaved stereoscopic) counterparts keyed by the matching
    /// `.M2TS` clip name (uppercase). Populated from `BDMV/STREAM/SSIF/*.ssif`
    /// whenever the directory exists, regardless of the `use_ssif` flag —
    /// callers that don't want SSIF simply ignore the map.
    pub interleaved_files: HashMap<String, (StreamSource, u64)>,
    /// When true, `effective_stream_source` returns the SSIF reader / size for
    /// any clip with an interleaved counterpart, so codec init and the full
    /// scan see the AVC + MVC payload instead of the AVC-only `.m2ts`. Set
    /// from `config.scan.enable_ssif_support` at open time.
    pub use_ssif: bool,
}

pub fn scan(path_str: &str) -> Result<DiscInfo> {
    let path = Path::new(path_str);
    let use_ssif = crate::config::get_config().scan.enable_ssif_support;
    let bdrom = open_bdrom(path, use_ssif)?;
    let mut disc = to_disc_info(&bdrom);
    // Codec initialization pass — mirrors BDInfo's `streamFile.Scan(playlists,
    // isFullScan: false)`. For every unique M2TS clip we open the stream once
    // and feed its PES payloads to the codec parsers until every relevant PID
    // has reported `is_initialized`, at which point the scan early-stops. This
    // populates per-stream codec details (codec_name, height, frame rate,
    // encoding profile, channel layout, sample rate, bit depth, …) and the
    // codec-fixed bit_rate (LPCM, AC3, DTS, MPA, …). For VBR streams that
    // codec parsers can't pin down, we estimate bit_rate from the running
    // total of payload bytes / elapsed seconds collected during the scan.
    codec_init(&mut disc, &bdrom);
    refresh_ssif_derived_metadata(&mut disc, &bdrom);
    cache_estimated_stream_sizes(&mut disc);
    Ok(disc)
}

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

/// Pick the stream source (and size) for a given clip, honoring the
/// `use_ssif` flag on the BDRom. When SSIF is enabled and the clip has an
/// interleaved counterpart (`<stem>.SSIF` next to `<stem>.M2TS`), the SSIF
/// is returned — codec parsers and the full-scan worker then see the AVC +
/// MVC payload instead of the AVC-only base file. Falls back to the M2TS in
/// every other case.
pub(crate) fn effective_stream_source<'a>(
    bd: &'a BDRom,
    clip_name: &str,
) -> Option<&'a (StreamSource, u64)> {
    if bd.use_ssif {
        if let Some(ssif) = bd.interleaved_files.get(clip_name) {
            return Some(ssif);
        }
    }
    bd.stream_files.get(clip_name)
}

fn open_bdrom_native(path: &Path, use_ssif: bool) -> Result<BDRom> {
    let directory_bdmv = locate_bdmv(path)?;
    let directory_root = native_disc_root(&directory_bdmv)?;

    let directory_playlist = find_subdir(&directory_bdmv, "PLAYLIST");
    let directory_clipinf = find_subdir(&directory_bdmv, "CLIPINF");
    let directory_stream = find_subdir(&directory_bdmv, "STREAM");
    let directory_bdjo = find_subdir(&directory_bdmv, "BDJO");
    let directory_meta = find_subdir(&directory_bdmv, "META");
    let directory_ssif = directory_stream
        .as_ref()
        .and_then(|s| find_subdir(s, "SSIF"));
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

    let is_bd_java = directory_bdjo
        .as_ref()
        .map(|d| dir_has_files(d))
        .unwrap_or(false);

    let is_psp = directory_snp
        .as_ref()
        .map(|d| dir_has_extension(d, "MNV"))
        .unwrap_or(false);

    let is_3d = directory_ssif
        .as_ref()
        .map(|d| dir_has_files(d))
        .unwrap_or(false);

    let is_dbox = directory_root.join("FilmIndex.xml").exists();

    let disc_title = directory_meta
        .as_ref()
        .and_then(|m| read_disc_title_native(m));

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
        img.resolve("BDMV")
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
            return Err(anyhow!(
                "UDF: Unable to locate PLAYLIST or CLIPINF directory."
            ));
        }
    }

    // Volume label: prefer the UDF Logical Volume Identifier (what DiscUtils /
    // BDInfo report); fall back to the ISO file name when the LVD has none.
    let volume_label = {
        let img = image.lock().unwrap_or_else(|e| e.into_inner());
        let lvid = img.volume_label.trim().to_string();
        if lvid.is_empty() {
            path.file_stem()
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

    let is_bd_plus = img.try_resolve("BDSVM").is_some()
        || img.try_resolve("SLYVM").is_some()
        || img.try_resolve("ANYVM").is_some();

    let is_bd_java = img
        .try_resolve("BDMV/BDJO")
        .filter(|d| d.is_directory)
        .map(|d| {
            img.list_dir(&d)
                .map(|es| es.iter().any(|e| !e.is_parent && !e.is_directory))
                .unwrap_or(false)
        })
        .unwrap_or(false);

    let is_psp = img
        .try_resolve("SNP")
        .filter(|d| d.is_directory)
        .map(|d| {
            img.list_dir(&d)
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
            img.list_dir(&d)
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

fn read_disc_title_iso(img: &mut UdfImage) -> Option<String> {
    let meta_dir = img.try_resolve("BDMV/META")?;
    if !meta_dir.is_directory {
        return None;
    }
    fn walk_for_bdmt_eng(img: &mut UdfImage, dir: &UdfFile) -> Option<Vec<u8>> {
        let entries = img.list_dir(dir).ok()?;
        for e in entries {
            if e.is_parent || e.is_deleted {
                continue;
            }
            let child = crate::bdrom::udf::read_file_entry_at(img, &e.icb).ok()?;
            if child.is_directory {
                if let Some(bytes) = walk_for_bdmt_eng(img, &child) {
                    return Some(bytes);
                }
            } else if e.name.eq_ignore_ascii_case("bdmt_eng.xml") {
                return img.read_file(&child).ok();
            }
        }
        None
    }
    let bytes = walk_for_bdmt_eng(img, &meta_dir)?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    extract_title_from_xml(&text)
}

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
    let playlist_dir = find_subdir(&bdmv, "PLAYLIST")
        .ok_or_else(|| anyhow!("PLAYLIST directory not found under {}", bdmv.display()))?;
    // Match the playlist file case-insensitively to tolerate uppercase/lowercase
    // discrepancies between the MPLS name we hand back to the frontend
    // (uppercased in `to_disc_info`) and the file as it lives on disk.
    if let Ok(entries) = std::fs::read_dir(&playlist_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if p.file_name()
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

fn locate_bdmv(path: &Path) -> Result<PathBuf> {
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
    Err(anyhow!(
        "Unable to locate BDMV directory under {}.",
        path.display()
    ))
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

fn find_subdir(parent: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(parent).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if p.file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
                .unwrap_or(false)
            {
                return Some(p);
            }
        }
    }
    None
}

fn dir_has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|it| it.flatten().any(|e| e.path().is_file()))
        .unwrap_or(false)
}

fn dir_has_extension(dir: &Path, ext: &str) -> bool {
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

fn directory_size(dir: &Path) -> u64 {
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
                if p.extension()
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

fn read_disc_title_native(meta_dir: &Path) -> Option<String> {
    fn walk(dir: &Path, visited: &mut HashSet<PathBuf>, depth: usize, out: &mut Option<PathBuf>) {
        const MAX_DEPTH: usize = 100;
        if depth > MAX_DEPTH || out.is_some() {
            return;
        }

        let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if !visited.insert(key) {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if out.is_some() {
                    return;
                }
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() {
                    continue;
                }
                let p = entry.path();
                if file_type.is_dir() {
                    walk(&p, visited, depth + 1, out);
                } else if file_type.is_file()
                    && p.file_name()
                        .map(|n| n.to_string_lossy().eq_ignore_ascii_case("bdmt_eng.xml"))
                        .unwrap_or(false)
                {
                    *out = Some(p);
                }
            }
        }
    }
    let mut visited = HashSet::new();
    let mut found = None;
    walk(meta_dir, &mut visited, 0, &mut found);
    let path = found.as_ref()?;
    let text = std::fs::read_to_string(path).ok()?;
    extract_title_from_xml(&text)
}

fn extract_title_from_xml(xml: &str) -> Option<String> {
    let mut pos = 0usize;
    let mut stack: Vec<String> = Vec::new();

    while let Some(start_rel) = xml[pos..].find('<') {
        let start = pos + start_rel;
        let Some(end_rel) = xml[start..].find('>') else {
            return None;
        };
        let end = start + end_rel;
        let raw = xml[start + 1..end].trim();
        pos = end + 1;

        if raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }

        if let Some(close) = raw.strip_prefix('/') {
            let close_name = xml_local_name(close);
            while let Some(open_name) = stack.pop() {
                if open_name == close_name {
                    break;
                }
            }
            continue;
        }

        let self_closing = raw.ends_with('/');
        let tag_name = xml_local_name(raw.trim_end_matches('/').trim());
        let parent = stack.last().map(String::as_str);

        if tag_name == "name" && parent == Some("title") {
            let content_start = end + 1;
            let Some(close_start_rel) = xml[content_start..].find("</") else {
                return None;
            };
            let close_start = content_start + close_start_rel;
            let Some(close_end_rel) = xml[close_start..].find('>') else {
                return None;
            };
            let close_end = close_start + close_end_rel;
            if xml_local_name(xml[close_start + 2..close_end].trim()) == "name" {
                let title = xml_decode_text(xml[content_start..close_start].trim());
                if !title.is_empty() && !title.eq_ignore_ascii_case("blu-ray") {
                    return Some(title);
                }
            }
        }

        if !self_closing {
            stack.push(tag_name);
        }
    }
    None
}

fn xml_local_name(raw: &str) -> String {
    let name = raw
        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or_default();
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

fn xml_decode_text(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }

    let mut out = String::new();
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after_amp = &rest[amp + 1..];
        let Some(semi) = after_amp.find(';') else {
            out.push('&');
            rest = after_amp;
            continue;
        };
        let entity = &after_amp[..semi];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if entity.starts_with("#x") => {
                if let Ok(code) = u32::from_str_radix(&entity[2..], 16) {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
            }
            _ if entity.starts_with('#') => {
                if let Ok(code) = entity[1..].parse::<u32>() {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
            }
            _ => {
                out.push('&');
                out.push_str(entity);
                out.push(';');
            }
        }
        rest = &after_amp[semi + 1..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn to_disc_info(bd: &BDRom) -> DiscInfo {
    let scan_config = crate::config::get_config().scan;
    let path_str = bd.path.to_string_lossy().to_string();
    let disc_name = bd
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let has_hevc_streams = bd.playlists.values().any(|pl| {
        pl.playlist_streams
            .iter()
            .any(|s| s.stream_type == TSStreamType::HEVCVideo)
    });
    let has_mvc = bd.playlists.values().any(|pl| {
        pl.playlist_streams
            .iter()
            .any(|s| s.stream_type == TSStreamType::MVCVideo)
    });

    // Sort playlists by name and assign group indices. Two playlists belong
    // to the same group if they share at least one stream-clip name —
    // mirroring BDInfo's playlist grouping in FormMain.cs.
    let mut playlist_names: Vec<&String> = bd
        .playlists
        .iter()
        .filter_map(|(name, pl)| {
            if playlist_is_valid_for_scan(
                pl,
                scan_config.filter_looping_playlists,
                scan_config.filter_short_playlists,
                scan_config.filter_short_playlists_value,
            ) {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    playlist_names.sort();
    let mut groups: Vec<Vec<&String>> = Vec::new();
    let mut group_index_by_name: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    for name in &playlist_names {
        let pl = match bd.playlists.get(*name) {
            Some(p) => p,
            None => continue,
        };
        let mut matched: Option<usize> = None;
        'outer: for (gi, group) in groups.iter().enumerate() {
            for other_name in group {
                if let Some(other) = bd.playlists.get(*other_name) {
                    for c1 in &pl.stream_clips {
                        for c2 in &other.stream_clips {
                            if c1.name == c2.name {
                                matched = Some(gi);
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
        match matched {
            Some(gi) => groups[gi].push(*name),
            None => groups.push(vec![*name]),
        }
    }
    for (gi, group) in groups.iter().enumerate() {
        for name in group {
            group_index_by_name.insert((*name).clone(), (gi + 1) as u32);
        }
    }

    let playlists: Vec<PlaylistInfo> = playlist_names
        .iter()
        .map(|name| {
            let group = group_index_by_name.get(*name).copied().unwrap_or(0);
            build_playlist_info(bd.playlists.get(*name).unwrap(), &bd, group)
        })
        .collect();

    // Stream files (sorted). `interleaved=true` marks clips with an SSIF
    // counterpart so the UI can flag them regardless of whether SSIF mode is
    // currently active.
    let mut stream_files: Vec<StreamFileInfo> = bd
        .stream_files
        .iter()
        .map(|(name, (_, size))| {
            let interleaved_file_size =
                bd.interleaved_files.get(name).map(|(_, s)| *s).unwrap_or(0);
            StreamFileInfo {
                name: name.clone(),
                display_name: stream_display_name(bd, name),
                size: *size,
                interleaved_file_size,
                duration: 0,
                interleaved: interleaved_file_size > 0,
            }
        })
        .collect();
    stream_files.sort_by(|a, b| a.name.cmp(&b.name));

    let mut stream_clip_files: Vec<StreamClipFileInfo> = bd
        .stream_clip_files
        .values()
        .map(|c| StreamClipFileInfo {
            name: c.name.clone(),
            size: c.size,
        })
        .collect();
    stream_clip_files.sort_by(|a, b| a.name.cmp(&b.name));

    let is_4k = bd.is_uhd
        || bd.playlists.values().any(|pl| {
            pl.playlist_streams
                .iter()
                .any(|s| s.video_format == TSVideoFormat::Video2160p)
        });

    DiscInfo {
        path: path_str,
        disc_name,
        disc_title: bd.disc_title.clone().unwrap_or_default(),
        volume_label: bd.volume_label.clone(),
        size: bd.size,
        is_bd_plus: bd.is_bd_plus,
        is_bd_java: bd.is_bd_java,
        is_3d: bd.is_3d,
        is_4k,
        is_50hz: bd.is_50_hz,
        is_dbox: bd.is_dbox,
        is_psp: bd.is_psp,
        is_uhd: bd.is_uhd,
        has_mvc_extension: has_mvc,
        has_hevc_streams: has_hevc_streams,
        has_uhd_disc_marker: bd.is_uhd,
        meta_title: bd.disc_title.clone(),
        meta_disc_number: None,
        file_set_identifier: None,
        playlists,
        stream_files,
        stream_clip_files,
    }
}

fn build_playlist_info(pl: &PlaylistFile, bd: &BDRom, group_index: u32) -> PlaylistInfo {
    // Compute clip lengths and total length using only angle 0 clips.
    let mut total_length_45k: i64 = 0;
    let mut total_file_size: u64 = 0;
    let mut clips: Vec<PlaylistStreamClipInfo> = Vec::new();

    let mut relative_time_in: i64 = 0;
    for c in &pl.stream_clips {
        let length = (c.time_out - c.time_in).max(0);
        let m2ts_size = bd.stream_files.get(&c.name).map(|(_, s)| *s).unwrap_or(0);
        let interleaved_file_size = bd
            .interleaved_files
            .get(&c.name)
            .map(|(_, s)| *s)
            .unwrap_or(0);
        // When SSIF mode is on and the clip has an interleaved counterpart,
        // the "scanned size" is the SSIF — that's what `effective_stream_source`
        // hands back, that's what gets measured during the full scan, and
        // that's what the BDInfo "Size" column shows. Fall back to the M2TS
        // size in every other case.
        let file_size = if bd.use_ssif && interleaved_file_size > 0 {
            interleaved_file_size
        } else {
            m2ts_size
        };
        total_file_size += file_size;
        let info = PlaylistStreamClipInfo {
            name: c.name.clone(),
            display_name: stream_display_name(bd, &c.name),
            time_in: c.time_in as u64,
            time_out: c.time_out as u64,
            relative_time_in: relative_time_in.max(0) as u64,
            relative_time_out: (relative_time_in + length).max(0) as u64,
            length: length as u64,
            file_size,
            measured_size: 0,
            interleaved_file_size,
            angle_index: c.angle_index,
        };
        if c.angle_index == 0 {
            total_length_45k += length;
            relative_time_in += length;
        }
        clips.push(info);
    }

    let mut video_streams = Vec::new();
    let mut audio_streams = Vec::new();
    let mut graphics_streams = Vec::new();
    let mut text_streams = Vec::new();
    // Reference angle-0 clip, used to cross-check stream language codes and
    // hidden streams against CLPI when MPLS leaves metadata out. BDInfo
    // chooses the richest/longest clip rather than blindly using the first.
    let reference_clip = reference_clip_name_for_playlist(pl, bd);
    let mut has_hidden_tracks = false;

    for s in &pl.playlist_streams {
        let mut info = playlist_stream_to_info(s);
        if info.language_code.is_empty() {
            if let Some(clip_name) = &reference_clip {
                if let Some(code) = clpi_language_for(bd, clip_name, s.pid) {
                    info.language_name = language_name(&code);
                    info.language_code = code;
                }
            }
        }
        if s.stream_type.is_video() {
            video_streams.push(info);
        } else if s.stream_type.is_audio() {
            audio_streams.push(info);
        } else if s.stream_type.is_graphics() {
            graphics_streams.push(info);
        } else if s.stream_type.is_text() {
            text_streams.push(info);
        }
    }

    let declared_pids: HashSet<u16> = pl.playlist_streams.iter().map(|s| s.pid).collect();
    if let Some(clip_name) = &reference_clip {
        if let Some(clpi) = clpi_file_for_clip(bd, clip_name) {
            for stream in &clpi.streams {
                if declared_pids.contains(&stream.pid) {
                    continue;
                }
                let Some(mut info) = clpi_stream_to_info(stream) else {
                    continue;
                };
                info.is_hidden = true;
                has_hidden_tracks = true;
                if info.is_video_stream {
                    video_streams.push(info);
                } else if info.is_audio_stream {
                    audio_streams.push(info);
                } else if info.is_graphics_stream {
                    graphics_streams.push(info);
                } else if info.is_text_stream {
                    text_streams.push(info);
                }
            }
        }
    }

    PlaylistInfo {
        name: pl.name.clone(),
        group_index,
        file_size: total_file_size,
        measured_size: 0,
        total_length: total_length_45k.max(0) as u64,
        has_hidden_tracks,
        has_loops: playlist_has_loops(pl),
        is_custom: false,
        chapters: pl.chapters.clone(),
        chapter_metrics: Vec::new(),
        bitrate_samples: Vec::new(),
        stream_clips: clips,
        video_streams,
        audio_streams,
        graphics_streams,
        text_streams,
        total_angles: pl.angle_count,
    }
}

fn playlist_is_valid_for_scan(
    pl: &PlaylistFile,
    filter_looping_playlists: bool,
    filter_short_playlists: bool,
    filter_short_playlists_value: u32,
) -> bool {
    if filter_short_playlists {
        let total_seconds = playlist_total_length_45k(pl) as f64 / 45000.0;
        if total_seconds < filter_short_playlists_value as f64 {
            return false;
        }
    }

    if filter_looping_playlists && playlist_has_loops(pl) {
        return false;
    }

    true
}

fn playlist_total_length_45k(pl: &PlaylistFile) -> i64 {
    pl.stream_clips
        .iter()
        .filter(|c| c.angle_index == 0)
        .map(|c| (c.time_out - c.time_in).max(0))
        .sum()
}

fn playlist_has_loops(pl: &PlaylistFile) -> bool {
    let mut clip_times: HashSet<(String, i64)> = HashSet::new();
    for clip in pl.stream_clips.iter().filter(|c| c.angle_index == 0) {
        if !clip_times.insert((clip.name.clone(), clip.time_in)) {
            return true;
        }
    }
    false
}

/// Look up a stream's language code from the matching CLPI clip's program-info
/// table by PID. Used only as a fallback when MPLS supplies no language code.
fn clpi_language_for(bd: &BDRom, clip_name: &str, pid: u16) -> Option<String> {
    let scf = clpi_file_for_clip(bd, clip_name)?;
    scf.streams
        .iter()
        .find(|s| s.pid == pid && !s.language_code.is_empty())
        .map(|s| s.language_code.clone())
}

fn clpi_file_for_clip<'a>(bd: &'a BDRom, clip_name: &str) -> Option<&'a StreamClipFile> {
    let stem = clip_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(clip_name);
    let clpi_name = format!("{}.CLPI", stem.to_uppercase());
    let scf = bd.stream_clip_files.get(&clpi_name)?;
    scf.is_valid.then_some(scf)
}

fn reference_clip_name_for_playlist(pl: &PlaylistFile, bd: &BDRom) -> Option<String> {
    let mut best: Option<(String, usize, i64)> = None;
    for clip in pl.stream_clips.iter().filter(|c| c.angle_index == 0) {
        let stream_count = clpi_file_for_clip(bd, &clip.name)
            .map(|c| c.streams.len())
            .unwrap_or(0);
        let length = (clip.time_out - clip.time_in).max(0);
        match &best {
            Some((_, best_count, best_length))
                if stream_count < *best_count
                    || (stream_count == *best_count && length <= *best_length) => {}
            _ => best = Some((clip.name.clone(), stream_count, length)),
        }
    }
    best.map(|(name, _, _)| name)
}

fn clpi_stream_to_info(stream: &ClpiStream) -> Option<TSStreamInfo> {
    let stream_type = TSStreamType::from_u8(stream.stream_type);
    if stream_type == TSStreamType::Unknown {
        return None;
    }

    let playlist_stream = PlaylistStream {
        pid: stream.pid,
        stream_type,
        video_format: TSVideoFormat::from_u8(stream.video_format),
        frame_rate: TSFrameRate::from_u8(stream.frame_rate),
        aspect_ratio: TSAspectRatio::from_u8(stream.aspect_ratio),
        channel_layout: TSChannelLayout::from_u8(stream.channel_layout),
        sample_rate_hz: stream.sample_rate,
        language_code: stream.language_code.clone(),
    };
    Some(playlist_stream_to_info(&playlist_stream))
}

fn playlist_stream_to_info(s: &PlaylistStream) -> TSStreamInfo {
    let mut info = TSStreamInfo::new(s.pid, s.stream_type as u8);
    info.stream_type_text = s.stream_type.type_text().to_string();
    info.codec_name = s.stream_type.codec_name().to_string();
    info.codec_short_name = s.stream_type.codec_short_name().to_string();
    info.is_video_stream = s.stream_type.is_video();
    info.is_audio_stream = s.stream_type.is_audio();
    info.is_graphics_stream = s.stream_type.is_graphics();
    info.is_text_stream = s.stream_type.is_text();
    info.language_code = s.language_code.trim_end_matches('\0').to_string();
    info.language_name = language_name(&info.language_code);

    if s.stream_type.is_video() {
        info.height = s.video_format.height();
        info.is_interlaced = s.video_format.is_interlaced();
        info.framerate = s.frame_rate.label().to_string();
        info.aspect_ratio = s.aspect_ratio.label().to_string();
        info.video_format = format!(
            "{}{}",
            info.height,
            if info.is_interlaced { "i" } else { "p" }
        );
        // Approx widths from common heights:
        info.width = match info.height {
            480 => 720,
            576 => 720,
            720 => 1280,
            1080 => 1920,
            2160 => 3840,
            _ => 0,
        };

        let mut desc_parts: Vec<String> = Vec::new();
        if info.height > 0 {
            desc_parts.push(format!(
                "{}{}",
                info.height,
                if info.is_interlaced { "i" } else { "p" }
            ));
        }
        if !info.framerate.is_empty() {
            desc_parts.push(format!("{} fps", info.framerate));
        }
        if !info.aspect_ratio.is_empty() {
            desc_parts.push(info.aspect_ratio.clone());
        }
        info.description = desc_parts.join(" / ");
    }

    if s.stream_type.is_audio() {
        info.channel_layout = s.channel_layout.label().to_string();
        info.sample_rate = s.sample_rate_hz;

        let mut desc_parts: Vec<String> = Vec::new();
        if !info.channel_layout.is_empty() {
            desc_parts.push(info.channel_layout.clone());
        }
        if info.sample_rate > 0 {
            desc_parts.push(format!("{} kHz", info.sample_rate / 1000));
        }
        info.description = desc_parts.join(" / ");
    }

    info
}

fn stream_display_name(bd: &BDRom, clip_name: &str) -> String {
    if bd.use_ssif && bd.interleaved_files.contains_key(clip_name) {
        let stem = clip_name
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(clip_name);
        format!("{}.SSIF", stem)
    } else {
        clip_name.to_string()
    }
}

pub(crate) fn is_ssif_mvc_stream(
    bd: &BDRom,
    clip_name: &str,
    pid: u16,
    stream: &TSStreamInfo,
) -> bool {
    bd.use_ssif
        && pid == SSIF_MVC_PID
        && bd.interleaved_files.contains_key(clip_name)
        && TSStreamType::from_u8(stream.stream_type) == TSStreamType::MVCVideo
}

pub(crate) fn refresh_ssif_derived_metadata(disc: &mut DiscInfo, bd: &BDRom) {
    if bd.is_3d {
        for pl in disc.playlists.iter_mut() {
            let Some(src) = bd.playlists.get(&pl.name) else {
                continue;
            };
            if pl.video_streams.len() <= 1 {
                continue;
            }

            for stream in pl.video_streams.iter_mut() {
                match TSStreamType::from_u8(stream.stream_type) {
                    TSStreamType::AVCVideo => stream.base_view = Some(src.mvc_base_view_r),
                    TSStreamType::MVCVideo => stream.base_view = Some(!src.mvc_base_view_r),
                    _ => {}
                }
                codec::finalize_description(stream);
            }
        }
    }

    recompute_mvc_extension(disc);
}

pub(crate) fn recompute_mvc_extension(disc: &mut DiscInfo) {
    disc.has_mvc_extension = disc.playlists.iter().any(|pl| {
        pl.video_streams
            .iter()
            .any(|s| TSStreamType::from_u8(s.stream_type) == TSStreamType::MVCVideo)
    });
}

pub(crate) fn cache_estimated_stream_sizes(disc: &mut DiscInfo) {
    for pl in disc.playlists.iter_mut() {
        let total_seconds = pl.total_length as f64 / 45000.0;
        for stream in pl
            .video_streams
            .iter_mut()
            .chain(pl.audio_streams.iter_mut())
            .chain(pl.graphics_streams.iter_mut())
            .chain(pl.text_streams.iter_mut())
        {
            stream.estimated_size = estimate_stream_size(stream, total_seconds);
        }
    }
}

pub(crate) fn estimate_stream_size(stream: &TSStreamInfo, total_seconds: f64) -> u64 {
    let bit_rate = if stream.bit_rate > 0 {
        stream.bit_rate
    } else {
        stream.active_bit_rate
    };
    if bit_rate > 0 && total_seconds > 0.0 {
        (bit_rate as f64 * total_seconds / 8.0).round() as u64
    } else {
        0
    }
}

/// Open a streaming reader for an M2TS stream entry, regardless of whether
/// the disc source is a directory or an ISO image.
pub(crate) fn open_stream_reader(
    bd: &BDRom,
    src: &StreamSource,
) -> Result<Box<dyn std::io::Read + Send>> {
    match src {
        StreamSource::Native(p) => {
            let f = std::fs::File::open(p)?;
            Ok(Box::new(std::io::BufReader::with_capacity(1 << 20, f)))
        }
        StreamSource::Iso(fe) => {
            if let DiscSource::Iso(image) = &bd.source {
                // Wrap with BufReader: every UdfFileReader::read locks the
                // shared image mutex, seeks, and reads. Without buffering,
                // a 5 MB codec-init scan triggers tens of thousands of
                // mutex+seek+read cycles. A 1 MB buffer cuts that to a
                // handful of refills.
                Ok(Box::new(std::io::BufReader::with_capacity(
                    1 << 20,
                    UdfFileReader::new(image.clone(), fe)?,
                )))
            } else {
                Err(anyhow!("ISO stream source without ISO disc source"))
            }
        }
    }
}

/// Like `open_stream_reader` but returns the raw, *unbuffered* reader. The
/// full-scan worker uses this so it can interpose its own ProgressReader
/// below a single large BufReader — that way per-packet reads in the m2ts
/// loop hit the buffer (a memcpy) instead of the progress wrapper (atomic
/// load + addition + clock check), removing tens of seconds of overhead on
/// disc-sized inputs.
pub(crate) fn open_stream_reader_raw(
    bd: &BDRom,
    src: &StreamSource,
) -> Result<Box<dyn std::io::Read + Send>> {
    match src {
        StreamSource::Native(p) => {
            let f = std::fs::File::open(p)?;
            Ok(Box::new(f))
        }
        StreamSource::Iso(fe) => {
            if let DiscSource::Iso(image) = &bd.source {
                Ok(Box::new(UdfFileReader::new(image.clone(), fe)?))
            } else {
                Err(anyhow!("ISO stream source without ISO disc source"))
            }
        }
    }
}

/// Run a one-shot codec init pass over every unique angle-0 clip on the disc.
/// For each clip we open the M2TS reader, dispatch reassembled PES payloads
/// to the matching codec parser, and stop reading the moment every PMT-
/// listed PID has reported `is_initialized` (mirrors BDInfo's
/// `ScanStream` finish condition over `Streams.Values`). Codec-derived
/// fields populated during the scan are then snapshotted and copied to
/// every other playlist that references the same clip.
pub(crate) fn codec_init(disc: &mut DiscInfo, bd: &BDRom) {
    use codec::CodecScanState;

    /// Codec-init result captured per unique clip. `codec_metadata` is the
    /// snapshot of every PID's TSStreamInfo after the codec parsers ran
    /// (taken via the same raw pointers used during the scan, so it always
    /// reflects the mutated state). `per_pid_bytes` and `duration_seconds`
    /// are the partial-scan running totals used to estimate bit rate for
    /// VBR streams the codec parser can't pin down.
    struct ClipInitCache {
        codec_metadata: HashMap<u16, TSStreamInfo>,
        per_pid_bytes: HashMap<u16, u64>,
        duration_seconds: f64,
    }

    // Phase A.1: collect every playlist index that references each unique
    // angle-0 clip. We need the union (not just one "lead") because
    // playlists can subset streams differently — a PID present in this
    // clip's PMT might only appear in a non-lead playlist's MPLS.
    let mut clip_referencing_plis: HashMap<String, Vec<usize>> = HashMap::new();
    for (pli, pl) in disc.playlists.iter().enumerate() {
        for clip in &pl.stream_clips {
            if clip.angle_index != 0 {
                continue;
            }
            let entry = clip_referencing_plis.entry(clip.name.clone()).or_default();
            if !entry.contains(&pli) {
                entry.push(pli);
            }
        }
    }

    // Phase A.2: scan each unique clip until codecs are initialized.
    let mut clip_cache: HashMap<String, ClipInitCache> = HashMap::new();
    for (clip_name, plis) in &clip_referencing_plis {
        let entry = match effective_stream_source(bd, clip_name) {
            Some(e) => e,
            None => continue,
        };

        // Build a single PID -> *mut TSStreamInfo table merged across every
        // playlist that references this clip. First playlist with a given
        // PID wins; the codec parser will mutate that one stream and we'll
        // distribute its codec metadata to all other playlists in Phase B.
        let mut pid_state: HashMap<u16, CodecScanState> = HashMap::new();
        let mut pid_streams: HashMap<u16, *mut TSStreamInfo> = HashMap::new();
        for &pli in plis {
            let pl = &mut disc.playlists[pli];
            for s in pl.video_streams.iter_mut() {
                pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
            }
            for s in pl.audio_streams.iter_mut() {
                pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
            }
            for s in pl.graphics_streams.iter_mut() {
                pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
            }
            for s in pl.text_streams.iter_mut() {
                pid_streams.entry(s.pid).or_insert(s as *mut TSStreamInfo);
            }
        }
        if pid_streams.is_empty() {
            continue;
        }

        // BitRate hint passed to DTS / DTS-HD parsers (they accept a running
        // bitrate computed by the host). Seeded with the MPLS-derived value.
        let bitrate_hint: HashMap<u16, i64> = pid_streams
            .iter()
            .map(|(pid, p)| unsafe { (*pid, (**p).bit_rate as i64) })
            .collect();

        let reader = match open_stream_reader(bd, &entry.0) {
            Ok(r) => r,
            Err(err) => {
                log::warn!("codec scan {}: {}", clip_name, err);
                continue;
            }
        };

        // Safety cap on bytes read per clip. The PMT-driven early-stop
        // normally fires within the first ~1 MB on a well-formed Blu-ray,
        // but if anything goes wrong (multi-packet PMT we don't fully
        // reassemble, codec parser that never initializes a particular
        // PID, etc.) this guarantees the codec init pass stays fast.
        const CODEC_INIT_BYTE_BUDGET: u64 = 8 * 1024 * 1024;
        let reader = std::io::Read::take(reader, CODEC_INIT_BYTE_BUDGET);

        // PMT may declare PIDs that no playlist's MPLS references — those
        // are "hidden" tracks (BDInfo's TSPlaylistFile.cs sets IsHidden=true
        // for any clip stream not in PlaylistStreams). We allocate synthetic
        // TSStreamInfo entries for them on first PES so the codec parser
        // can populate their format fields the same way it does for the
        // real ones. Phase B then attaches a copy to every playlist that
        // doesn't declare the PID.
        let mut synthetic_holders: HashMap<u16, Box<TSStreamInfo>> = HashMap::new();

        let res =
            m2ts::scan_m2ts_streaming_from_reader(reader, |pid, _stream_type, payload, pmt| {
                let target_ptr: Option<*mut TSStreamInfo> =
                    if let Some(&ptr) = pid_streams.get(&pid) {
                        Some(ptr)
                    } else if let Some(&stream_type) = pmt.get(&pid) {
                        // PMT-declared but not in any MPLS — synthesize.
                        let mut stub = TSStreamInfo::new(pid, stream_type);
                        let st = TSStreamType::from_u8(stream_type);
                        stub.stream_type_text = st.type_text().to_string();
                        stub.codec_name = st.codec_name().to_string();
                        stub.codec_short_name = st.codec_short_name().to_string();
                        stub.is_video_stream = st.is_video();
                        stub.is_audio_stream = st.is_audio();
                        stub.is_graphics_stream = st.is_graphics();
                        stub.is_text_stream = st.is_text();
                        let mut boxed = Box::new(stub);
                        let ptr = &mut *boxed as *mut TSStreamInfo;
                        synthetic_holders.insert(pid, boxed);
                        pid_streams.insert(pid, ptr);
                        Some(ptr)
                    } else {
                        None
                    };

                if let Some(ptr) = target_ptr {
                    let stream = unsafe { &mut *ptr };
                    if !stream.is_initialized {
                        let state = pid_state.entry(pid).or_default();
                        let bitrate = bitrate_hint.get(&pid).copied().unwrap_or(0);
                        codec::scan_stream(stream, state, payload, bitrate, true, false);
                    }
                }

                // BDInfo-style early-stop: terminate the moment every PMT-
                // listed PID has reported initialized — including hidden
                // ones we synthesized above (so their codec details get
                // captured before we exit). PIDs in PMT that haven't yet
                // delivered a PES are still pending; keep scanning.
                if pmt.is_empty() {
                    return m2ts::PesAction::Continue;
                }
                let any_uninit = pmt.keys().any(|p| {
                    pid_streams
                        .get(p)
                        .map(|ptr| unsafe { !(**ptr).is_initialized })
                        .unwrap_or(true)
                });
                if any_uninit {
                    m2ts::PesAction::Continue
                } else {
                    m2ts::PesAction::Stop
                }
            });

        match res {
            Ok(r) => {
                let mut by_pid: HashMap<u16, u64> = HashMap::new();
                for (pid, stat) in &r.streams {
                    by_pid.insert(*pid, stat.total_bytes);
                }
                let duration = r.duration_seconds;

                // Estimate bit_rate for VBR streams from running totals.
                // We mutate the very streams pid_streams pointed at, so the
                // snapshot taken below reflects these updates too.
                if duration > 0.0 {
                    for (pid, ptr) in &pid_streams {
                        if let Some(b) = by_pid.get(pid) {
                            let active = (*b as f64 * 8.0 / duration).round() as u64;
                            unsafe {
                                let s = &mut **ptr;
                                s.active_bit_rate = active;
                                if s.is_vbr || s.bit_rate == 0 {
                                    s.bit_rate = active;
                                }
                            }
                        }
                    }
                }

                // Snapshot codec metadata via the same raw pointers so we
                // capture whichever playlist owned the mutated stream.
                let mut codec_metadata: HashMap<u16, TSStreamInfo> = HashMap::new();
                for (pid, ptr) in &pid_streams {
                    unsafe {
                        codec_metadata.insert(*pid, (**ptr).clone());
                    }
                }

                clip_cache.insert(
                    clip_name.clone(),
                    ClipInitCache {
                        codec_metadata,
                        per_pid_bytes: by_pid,
                        duration_seconds: duration,
                    },
                );
            }
            Err(err) => {
                log::warn!("codec scan {}: {}", clip_name, err);
            }
        }
    }

    // Phase B: distribute codec metadata. For PIDs the playlist already
    // declares in MPLS, copy codec details into the existing stream. For
    // PIDs that appeared in the clip's PMT but not in this playlist's MPLS
    // (BDInfo's "hidden" tracks), append a new is_hidden=true stream.
    for pl in disc.playlists.iter_mut() {
        // PIDs the playlist already has from MPLS (used to detect hidden).
        let mut declared_pids: HashSet<u16> = pl
            .video_streams
            .iter()
            .chain(pl.audio_streams.iter())
            .chain(pl.graphics_streams.iter())
            .chain(pl.text_streams.iter())
            .map(|s| s.pid)
            .collect();

        for clip in &pl.stream_clips {
            if clip.angle_index != 0 {
                continue;
            }
            let cached = match clip_cache.get(&clip.name) {
                Some(c) => c,
                None => continue,
            };

            // Update existing streams with codec details.
            for s in pl
                .video_streams
                .iter_mut()
                .chain(pl.audio_streams.iter_mut())
                .chain(pl.graphics_streams.iter_mut())
                .chain(pl.text_streams.iter_mut())
            {
                if s.is_initialized {
                    continue;
                }
                if let Some(meta) = cached.codec_metadata.get(&s.pid) {
                    if meta.is_initialized {
                        copy_codec_metadata(s, meta);
                    }
                }
            }

            // Add hidden streams for PMT PIDs not in this playlist's MPLS.
            for (pid, meta) in &cached.codec_metadata {
                if declared_pids.contains(pid) {
                    continue;
                }
                if is_ssif_mvc_stream(bd, &clip.name, *pid, meta) {
                    let mut mvc = meta.clone();
                    mvc.is_hidden = false;
                    if mvc.is_video_stream {
                        pl.video_streams.push(mvc);
                        declared_pids.insert(*pid);
                    }
                    continue;
                }
                let mut hidden = meta.clone();
                hidden.is_hidden = true;
                pl.has_hidden_tracks = true;
                if hidden.is_video_stream {
                    pl.video_streams.push(hidden);
                } else if hidden.is_audio_stream {
                    pl.audio_streams.push(hidden);
                } else if hidden.is_graphics_stream {
                    pl.graphics_streams.push(hidden);
                } else if hidden.is_text_stream {
                    pl.text_streams.push(hidden);
                } else {
                    // Unknown stream type — drop.
                    continue;
                }
                // Don't add the same hidden PID twice if multiple clips of
                // the playlist contain it.
                declared_pids.insert(*pid);
            }
        }
    }

    // For VBR streams that didn't get a fixed bit rate from the codec
    // parser, accumulate per-PID bytes across all clips of the playlist and
    // divide by total scanned seconds — gives a more representative running
    // average than any single clip's first few seconds.
    for pl in disc.playlists.iter_mut() {
        let mut per_pid_total_bytes: HashMap<u16, u64> = HashMap::new();
        let mut total_seconds: f64 = 0.0;
        for clip in &pl.stream_clips {
            if clip.angle_index != 0 {
                continue;
            }
            if let Some(cached) = clip_cache.get(&clip.name) {
                total_seconds += cached.duration_seconds;
                for (pid, bytes) in &cached.per_pid_bytes {
                    *per_pid_total_bytes.entry(*pid).or_insert(0) += *bytes;
                }
            }
        }
        if total_seconds > 0.0 {
            for s in pl
                .video_streams
                .iter_mut()
                .chain(pl.audio_streams.iter_mut())
                .chain(pl.graphics_streams.iter_mut())
                .chain(pl.text_streams.iter_mut())
            {
                if let Some(b) = per_pid_total_bytes.get(&s.pid) {
                    let active = (*b as f64 * 8.0 / total_seconds).round() as u64;
                    s.active_bit_rate = active;
                    if s.is_vbr || s.bit_rate == 0 {
                        s.bit_rate = active;
                    }
                }
            }
        }

        // Refine VBR video bit_rate using the playlist's total bandwidth.
        // The codec-init partial scan only reads ~8 MB per clip, so its
        // running average for VBR streams is biased toward whatever happens
        // in the first few seconds. Total bandwidth (angle-0 clip bytes ×
        // 8 / total length) is exact, and audio bit rates are mostly
        // codec-fixed and accurate — so the residual is a much better
        // estimate of the dominant VBR video stream's actual average.
        let total_length_s = pl.total_length as f64 / 45000.0;
        if total_length_s > 0.0 && !pl.video_streams.is_empty() {
            let mut angle0_bytes: u64 = 0;
            for c in &pl.stream_clips {
                if c.angle_index == 0 {
                    angle0_bytes += c.file_size;
                }
            }
            if angle0_bytes > 0 {
                let total_bps = angle0_bytes as f64 * 8.0 / total_length_s;
                let non_video_bps: f64 = pl
                    .audio_streams
                    .iter()
                    .chain(pl.graphics_streams.iter())
                    .chain(pl.text_streams.iter())
                    .map(|s| s.bit_rate as f64)
                    .sum();
                let video_residual = total_bps - non_video_bps;
                if video_residual > 0.0 {
                    let total_video_partial: f64 =
                        pl.video_streams.iter().map(|s| s.bit_rate as f64).sum();
                    if total_video_partial > 0.0 {
                        // Multiple video streams (e.g. MVC + AVC for 3D):
                        // split the residual proportionally to their
                        // partial-scan averages.
                        for s in pl.video_streams.iter_mut() {
                            let proportion = s.bit_rate as f64 / total_video_partial;
                            s.bit_rate = (video_residual * proportion).max(0.0) as u64;
                        }
                    } else {
                        // Single uninitialized video stream — give it the
                        // entire residual (still better than 0).
                        let per_video = video_residual / pl.video_streams.len() as f64;
                        for s in pl.video_streams.iter_mut() {
                            s.bit_rate = per_video.max(0.0) as u64;
                        }
                    }
                }
            }
        }

        // Description is recomputed once all underlying fields are populated
        // so it reflects codec init + audio CoreStream linkage.
        for s in pl
            .video_streams
            .iter_mut()
            .chain(pl.audio_streams.iter_mut())
            .chain(pl.graphics_streams.iter_mut())
            .chain(pl.text_streams.iter_mut())
        {
            codec::finalize_description(s);
        }
    }
}

/// Copy codec-derived fields from the lead playlist's snapshot into a
/// sibling stream on a different playlist that shares the same underlying
/// clip + PID. Leaves measurement and language fields alone.
fn copy_codec_metadata(dst: &mut TSStreamInfo, src: &TSStreamInfo) {
    if !src.is_initialized {
        return;
    }
    dst.is_initialized = true;
    dst.is_vbr = src.is_vbr;
    dst.codec_name = src.codec_name.clone();
    dst.codec_short_name = src.codec_short_name.clone();
    dst.stream_type_text = src.stream_type_text.clone();
    dst.description = src.description.clone();
    dst.width = src.width;
    dst.height = src.height;
    dst.framerate = src.framerate.clone();
    dst.frame_rate_enumerator = src.frame_rate_enumerator;
    dst.frame_rate_denominator = src.frame_rate_denominator;
    dst.aspect_ratio = src.aspect_ratio.clone();
    dst.aspect_ratio_code = src.aspect_ratio_code;
    dst.video_format = src.video_format.clone();
    dst.is_interlaced = src.is_interlaced;
    dst.encoding_profile = src.encoding_profile.clone();
    dst.extended_format_info = src.extended_format_info.clone();
    dst.base_view = src.base_view;
    dst.channel_count = src.channel_count;
    dst.lfe = src.lfe;
    dst.sample_rate = src.sample_rate;
    dst.bit_depth = src.bit_depth;
    dst.channel_layout = src.channel_layout.clone();
    dst.audio_mode = src.audio_mode.clone();
    dst.dial_norm = src.dial_norm;
    dst.has_extensions = src.has_extensions;
    dst.core = src.core.clone();
    dst.captions = src.captions;
    dst.forced_captions = src.forced_captions;
    if dst.bit_rate == 0 && src.bit_rate > 0 {
        dst.bit_rate = src.bit_rate;
    }
    if dst.active_bit_rate == 0 && src.active_bit_rate > 0 {
        dst.active_bit_rate = src.active_bit_rate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    // ---- Temp-dir scaffolding (no external crates). -----------------------

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A unique temp directory that removes itself (recursively) on drop, so
    /// the BDMV tree we build is cleaned up even if a test panics.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                "bdmaster_modtest_{}_{}_{}_{}",
                tag,
                std::process::id(),
                n,
                nanos
            );
            let path = std::env::temp_dir().join(name);
            std::fs::create_dir_all(&path).expect("create temp dir");
            TempDir { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        /// Create a file (creating parent dirs) under the temp root.
        fn write(&self, rel: &str, bytes: &[u8]) {
            let p = self.path.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).expect("create parent dir");
            }
            std::fs::write(&p, bytes).expect("write file");
        }

        fn mkdir(&self, rel: &str) {
            std::fs::create_dir_all(self.path.join(rel)).expect("create dir");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    // ---- MPLS builder (mirrors mpls.rs `build_mpls`, parameterized). ------

    /// One stream entry for the STN table: (header_type byte stays 1) PID +
    /// coding-info bytes already shaped for the requested stream type.
    struct StreamSpec {
        pid: u16,
        coding: Vec<u8>,
    }

    fn avc_video(pid: u16) -> StreamSpec {
        StreamSpec {
            pid,
            // length 3: 0x1b AVC, video format 1080p / 23.976, aspect 16:9
            coding: vec![3, 0x1b, (6 << 4) | 1, 3 << 4],
        }
    }

    fn ac3_audio(pid: u16, lang: &[u8; 3]) -> StreamSpec {
        StreamSpec {
            pid,
            coding: vec![5, 0x81, (6 << 4) | 1, lang[0], lang[1], lang[2]],
        }
    }

    /// Build an MPLS with a single play item (clip 00001.M2TS) carrying the
    /// supplied video + audio streams, an out_time, and one chapter.
    fn build_mpls_custom(
        out_time_45k: u32,
        videos: &[StreamSpec],
        audios: &[StreamSpec],
        mvc_base_view_r: bool,
    ) -> Vec<u8> {
        let mut d: Vec<u8> = Vec::new();
        d.extend_from_slice(b"MPLS0200");
        d.extend_from_slice(&[0u8; 4]); // playlist_offset @8
        d.extend_from_slice(&[0u8; 4]); // chapters_offset @12
        d.extend_from_slice(&[0u8; 4]); // extensions_offset @16
        while d.len() < 0x38 {
            d.push(0);
        }
        d.push(if mvc_base_view_r { 0x10 } else { 0x00 }); // misc flags @0x38

        let playlist_offset = d.len() as u32;
        d[8..12].copy_from_slice(&playlist_offset.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes()); // playlist_length
        d.extend_from_slice(&0u16.to_be_bytes()); // reserved
        d.extend_from_slice(&1u16.to_be_bytes()); // item_count
        d.extend_from_slice(&0u16.to_be_bytes()); // subitem_count

        let item_start = d.len();
        d.extend_from_slice(&0u16.to_be_bytes()); // item_length placeholder
        d.extend_from_slice(b"00001"); // item name
        d.extend_from_slice(b"M2TS"); // item type
        d.push(0x00);
        d.push(0x00); // no multiangle
        d.push(0x00);
        d.extend_from_slice(&0u32.to_be_bytes()); // in_time
        d.extend_from_slice(&out_time_45k.to_be_bytes()); // out_time
        d.extend_from_slice(&[0u8; 12]); // reserved skip

        d.extend_from_slice(&0u16.to_be_bytes()); // stn_length
        d.extend_from_slice(&0u16.to_be_bytes()); // reserved
        d.push(videos.len() as u8); // video count
        d.push(audios.len() as u8); // audio count
        d.push(0); // pg
        d.push(0); // ig
        d.push(0); // secondary audio
        d.push(0); // secondary video
        d.push(0); // pip
        d.extend_from_slice(&[0u8; 5]); // reserved skip

        for v in videos {
            d.push(3); // stream-entry header length
            d.push(1); // header type 1 -> PID follows
            d.extend_from_slice(&v.pid.to_be_bytes());
            d.extend_from_slice(&v.coding);
        }
        for a in audios {
            d.push(3);
            d.push(1);
            d.extend_from_slice(&a.pid.to_be_bytes());
            d.extend_from_slice(&a.coding);
        }

        let item_len = (d.len() - item_start - 2) as u16;
        d[item_start..item_start + 2].copy_from_slice(&item_len.to_be_bytes());

        let chapters_offset = d.len() as u32;
        d[12..16].copy_from_slice(&chapters_offset.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes()); // length (skipped)
        d.extend_from_slice(&1u16.to_be_bytes()); // chapter count
        let mut chapter = vec![0u8; 14];
        chapter[1] = 1; // chapter type 1
        chapter[4..8].copy_from_slice(&(45000u32 * 10).to_be_bytes()); // 10 s
        d.extend_from_slice(&chapter);

        d
    }

    fn build_mpls_default() -> Vec<u8> {
        build_mpls_custom(
            4_500_000,
            &[avc_video(0x1011)],
            &[ac3_audio(0x1100, b"eng")],
            true,
        )
    }

    // ---- CLPI builder (mirrors clpi.rs test helpers). ---------------------

    fn clpi_stream_entry(pid: u16, coding_type: u8, attrs: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&pid.to_be_bytes());
        v.push((1 + attrs.len()) as u8);
        v.push(coding_type);
        v.extend_from_slice(attrs);
        v
    }

    fn build_clpi(file_type: &str, streams: &[Vec<u8>]) -> Vec<u8> {
        let mut clip = vec![0u8; 10];
        clip[8] = streams.len() as u8;
        for s in streams {
            clip.extend_from_slice(s);
        }
        let clip_len = clip.len() as u32;

        let mut data = Vec::new();
        data.extend_from_slice(file_type.as_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]);
        let clip_index = 16u32;
        data.extend_from_slice(&clip_index.to_be_bytes());
        data.extend_from_slice(&clip_len.to_be_bytes());
        data.extend_from_slice(&clip);
        data
    }

    /// A CLPI with an AVC video (no MPLS-side language), an AC3 audio carrying
    /// a language code, and a PGS graphics entry — so `clpi_language_for` has
    /// something to fall back to.
    fn build_clpi_default() -> Vec<u8> {
        let video = clpi_stream_entry(0x1011, 0x1b, &[(6 << 4) | 1, 3 << 4]);
        let audio = clpi_stream_entry(0x1100, 0x81, &[(6 << 4) | 1, b'e', b'n', b'g']);
        let pgs = clpi_stream_entry(0x1200, 0x90, &[b'j', b'p', b'n']);
        build_clpi("HDMV0200", &[video, audio, pgs])
    }

    // ---- M2TS builder (mirrors m2ts.rs test helpers). ---------------------

    const TS_PACKET_SIZE: usize = 188;
    const SYNC_BYTE: u8 = 0x47;

    fn ts_packet(pusi: bool, pid: u16, payload: &[u8]) -> Vec<u8> {
        let mut ts = vec![0xFFu8; TS_PACKET_SIZE];
        ts[0] = SYNC_BYTE;
        ts[1] = ((pid >> 8) as u8 & 0x1F) | if pusi { 0x40 } else { 0 };
        ts[2] = (pid & 0xFF) as u8;
        ts[3] = 0x10; // payload only
        let n = payload.len().min(TS_PACKET_SIZE - 4);
        ts[4..4 + n].copy_from_slice(&payload[..n]);
        ts
    }

    fn m2ts_frame(ts: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 4];
        p.extend_from_slice(ts);
        p
    }

    fn pat_payload(program: u16, pmt_pid: u16) -> Vec<u8> {
        vec![
            0x00,
            0x00,
            0xB0,
            0x0D,
            0x00,
            0x01,
            0x01,
            0x00,
            0x00,
            (program >> 8) as u8,
            (program & 0xFF) as u8,
            0xE0 | (pmt_pid >> 8) as u8,
            (pmt_pid & 0xFF) as u8,
            0x00,
            0x00,
            0x00,
            0x00,
        ]
    }

    /// Build a PMT payload listing arbitrary (stream_type, pid) elementary
    /// streams. The section_length is computed so the scanner accepts it.
    fn pmt_payload_multi(streams: &[(u8, u16)]) -> Vec<u8> {
        // Body after section_length field: program_number(2) version/section/last(3)
        // PCR(2) program_info_length(2) = 9 bytes, plus 5 per ES, plus 4 CRC.
        let es_bytes = streams.len() * 5;
        let section_length = 9 + es_bytes + 4;
        let mut v = vec![0x00u8, 0x02]; // pointer, table_id (PMT)
        v.push(0xB0 | ((section_length >> 8) as u8 & 0x0F));
        v.push((section_length & 0xFF) as u8);
        v.extend_from_slice(&[0x00, 0x01]); // program_number
        v.extend_from_slice(&[0x01, 0x00, 0x00]); // version / section / last
        v.extend_from_slice(&[0xE0, 0x00]); // PCR PID
        v.extend_from_slice(&[0xF0, 0x00]); // program_info_length = 0
        for (st, pid) in streams {
            v.push(*st);
            v.push(0xE0 | ((pid >> 8) as u8 & 0x1F));
            v.push((pid & 0xFF) as u8);
            v.push(0xF0);
            v.push(0x00);
        }
        v.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // CRC (not validated)
        v
    }

    fn pes_payload(stream_id: u8, es: &[u8]) -> Vec<u8> {
        let mut v = vec![0x00, 0x00, 0x01, stream_id, 0x00, 0x00, 0x80, 0x00, 0x00];
        v.extend_from_slice(es);
        v
    }

    /// Build an M2TS image: PAT, a multi-stream PMT (AVC/AC3/MVC/PGS), plus a
    /// few PES for each so codec_init dispatches them. `extra_pids` are PIDs in
    /// the PMT that the MPLS doesn't declare (hidden tracks / SSIF MVC).
    fn build_m2ts() -> Vec<u8> {
        let streams: &[(u8, u16)] = &[
            (0x1b, 0x1011),       // AVC (declared)
            (0x81, 0x1100),       // AC3 (declared)
            (0x20, SSIF_MVC_PID), // MVC (hidden / SSIF)
            (0x90, 0x1200),       // PGS (hidden)
            (0x92, 0x1300),       // Subtitle (hidden)
        ];
        let mut data = Vec::new();
        data.extend_from_slice(&m2ts_frame(&ts_packet(
            true,
            0x0000,
            &pat_payload(1, 0x0100),
        )));
        data.extend_from_slice(&m2ts_frame(&ts_packet(
            true,
            0x0100,
            &pmt_payload_multi(streams),
        )));
        // A handful of PES per ES PID so the codec parsers get fed and the
        // byte accounting accumulates.
        for _ in 0..4 {
            for (st, pid) in streams {
                let sid = if TSStreamType::from_u8(*st).is_video() {
                    0xE0
                } else {
                    0xC0
                };
                data.extend_from_slice(&m2ts_frame(&ts_packet(
                    true,
                    *pid,
                    &pes_payload(sid, &[0x00, 0x00, 0x01, 0x09, 0x10, 0xAA, 0xBB, 0xCC]),
                )));
            }
        }
        data
    }

    // ---- Disc-tree assembly. ----------------------------------------------

    struct DiscOpts {
        uhd: bool,
        with_ssif: bool,
        with_bdjo: bool,
        with_snp: bool,
        with_filmindex: bool,
        with_bdsvm: bool,
        with_meta: bool,
        meta_title: &'static str,
    }

    impl Default for DiscOpts {
        fn default() -> Self {
            DiscOpts {
                uhd: false,
                with_ssif: true,
                with_bdjo: true,
                with_snp: true,
                with_filmindex: true,
                with_bdsvm: true,
                with_meta: true,
                meta_title: "My Movie Title",
            }
        }
    }

    /// Build a complete native disc tree under a fresh temp dir.
    fn make_disc(opts: &DiscOpts) -> TempDir {
        let dir = TempDir::new("disc");

        dir.write("BDMV/PLAYLIST/00800.mpls", &build_mpls_default());
        dir.write("BDMV/CLIPINF/00001.clpi", &build_clpi_default());
        dir.write("BDMV/STREAM/00001.m2ts", &build_m2ts());

        let index_header: &[u8] = if opts.uhd {
            b"INDX0300extra"
        } else {
            b"INDX0200extra"
        };
        dir.write("BDMV/index.bdmv", index_header);

        if opts.with_meta {
            let xml = format!(
                "<?xml version=\"1.0\"?><disclib><di:title><di:name>{}</di:name></di:title></disclib>",
                opts.meta_title
            );
            dir.write("BDMV/META/DL/bdmt_eng.xml", xml.as_bytes());
        }
        if opts.with_bdjo {
            dir.write("BDMV/BDJO/00000.bdjo", b"BDJO");
        } else {
            dir.mkdir("BDMV/BDJO");
        }
        if opts.with_ssif {
            dir.write("BDMV/STREAM/SSIF/00001.ssif", &build_m2ts());
        }
        if opts.with_snp {
            dir.write("SNP/clip.mnv", b"mnv");
        } else {
            dir.mkdir("SNP");
        }
        if opts.with_filmindex {
            dir.write("FilmIndex.xml", b"<FilmIndex/>");
        }
        if opts.with_bdsvm {
            dir.mkdir("BDSVM");
        }

        dir
    }

    fn find_pl<'a>(disc: &'a DiscInfo, name: &str) -> &'a PlaylistInfo {
        disc.playlists
            .iter()
            .find(|p| p.name == name)
            .expect("playlist present")
    }

    // ====================================================================
    // 1. Full native-disc integration test (SSIF on, via default config).
    // ====================================================================

    #[test]
    fn scan_full_native_disc_ssif_on() {
        // Default config has enable_ssif_support = true.
        assert!(crate::config::get_config().scan.enable_ssif_support);

        let dir = make_disc(&DiscOpts {
            uhd: true,
            ..Default::default()
        });
        let root = dir.path().to_string_lossy().to_string();

        let disc = scan(&root).expect("scan succeeds");

        // Disc-level flags from the tree.
        assert!(disc.is_uhd, "INDX0300 header => UHD");
        assert!(disc.is_4k, "UHD implies 4k");
        assert!(disc.has_uhd_disc_marker);
        assert!(disc.is_bd_java, "BDJO with a file => BD-Java");
        assert!(disc.is_psp, "SNP/*.mnv => PSP");
        assert!(disc.is_dbox, "FilmIndex.xml => D-BOX");
        assert!(disc.is_bd_plus, "BDSVM => BD+");
        assert!(disc.is_3d, "SSIF dir with file => 3D");
        assert_eq!(disc.disc_title, "My Movie Title");
        assert_eq!(disc.meta_title.as_deref(), Some("My Movie Title"));
        assert!(!disc.volume_label.is_empty());
        assert_eq!(disc.volume_label, disc.disc_name);
        assert!(disc.size > 0, "directory_size summed something");

        // One playlist, group index assigned.
        assert_eq!(disc.playlists.len(), 1);
        let pl = &disc.playlists[0];
        assert_eq!(pl.name, "00800.MPLS");
        assert_eq!(pl.group_index, 1);
        assert_eq!(pl.total_angles, 0);
        assert_eq!(pl.total_length, 4_500_000);
        assert_eq!(pl.stream_clips.len(), 1);
        assert_eq!(pl.stream_clips[0].name, "00001.M2TS");
        // SSIF on => the clip display name uses the SSIF extension.
        assert_eq!(pl.stream_clips[0].display_name, "00001.SSIF");
        assert!(pl.stream_clips[0].interleaved_file_size > 0);

        // Declared streams from MPLS.
        assert_eq!(
            pl.video_streams.iter().filter(|s| !s.is_hidden).count() >= 1,
            true
        );
        let avc = pl
            .video_streams
            .iter()
            .find(|s| s.pid == 0x1011)
            .expect("AVC present");
        assert_eq!(avc.stream_type, TSStreamType::AVCVideo as u8);
        let ac3 = pl
            .audio_streams
            .iter()
            .find(|s| s.pid == 0x1100)
            .expect("AC3 present");
        assert_eq!(ac3.stream_type, TSStreamType::AC3Audio as u8);
        assert_eq!(ac3.language_code, "eng");

        // Hidden streams synthesized from PMT (PGS + Subtitle were not in MPLS).
        let pgs = pl.graphics_streams.iter().find(|s| s.pid == 0x1200);
        assert!(pgs.is_some(), "PGS hidden track added");
        assert!(pgs.unwrap().is_hidden);
        assert_eq!(pgs.unwrap().language_code, "jpn");
        let sub = pl.text_streams.iter().find(|s| s.pid == 0x1300);
        assert!(sub.is_some(), "subtitle hidden track added");
        assert!(pl.has_hidden_tracks);

        // MVC (PID 0x1012) under SSIF mode is promoted to a non-hidden video
        // stream, not a hidden track.
        let mvc = pl.video_streams.iter().find(|s| s.pid == SSIF_MVC_PID);
        assert!(mvc.is_some(), "MVC stream present under SSIF");
        assert!(!mvc.unwrap().is_hidden, "MVC promoted, not hidden");
        assert_eq!(mvc.unwrap().stream_type, TSStreamType::MVCVideo as u8);
        assert!(disc.has_mvc_extension, "MVC present => mvc extension flag");

        // Stream files / clip files in the DiscInfo.
        assert_eq!(disc.stream_files.len(), 1);
        assert_eq!(disc.stream_files[0].name, "00001.M2TS");
        assert!(disc.stream_files[0].interleaved);
        assert!(disc.stream_files[0].interleaved_file_size > 0);
        assert_eq!(disc.stream_files[0].display_name, "00001.SSIF");
        assert_eq!(disc.stream_clip_files.len(), 1);
        assert_eq!(disc.stream_clip_files[0].name, "00001.CLPI");

        // estimated_size cached for streams with a bit rate.
        let any_estimate = pl
            .video_streams
            .iter()
            .chain(pl.audio_streams.iter())
            .any(|s| s.estimated_size > 0);
        assert!(any_estimate, "at least one stream got an estimated size");
    }

    // ====================================================================
    // Non-UHD variant: INDX0200, no SSIF/BDJO/SNP/FilmIndex/BDSVM/META.
    // ====================================================================

    #[test]
    fn scan_minimal_non_uhd_disc() {
        let dir = make_disc(&DiscOpts {
            uhd: false,
            with_ssif: false,
            with_bdjo: false,
            with_snp: false,
            with_filmindex: false,
            with_bdsvm: false,
            with_meta: false,
            meta_title: "",
        });
        let root = dir.path().to_string_lossy().to_string();
        let disc = scan(&root).expect("scan succeeds");

        assert!(!disc.is_uhd);
        assert!(!disc.is_3d);
        assert!(!disc.is_bd_java, "empty BDJO dir => not BD-Java");
        assert!(!disc.is_psp, "empty SNP dir => not PSP");
        assert!(!disc.is_dbox);
        assert!(!disc.is_bd_plus);
        assert!(disc.disc_title.is_empty());
        assert!(disc.meta_title.is_none());

        // SSIF off => no interleaved counterpart, display name == clip name.
        let pl = &disc.playlists[0];
        assert_eq!(pl.stream_clips[0].display_name, "00001.M2TS");
        assert!(!disc.stream_files[0].interleaved);
        assert_eq!(disc.stream_files[0].interleaved_file_size, 0);

        // The CLPI language fallback fills the AC3 language (MPLS also had it,
        // but assert it survives).
        let ac3 = pl.audio_streams.iter().find(|s| s.pid == 0x1100).unwrap();
        assert_eq!(ac3.language_code, "eng");
    }

    // ====================================================================
    // 2a. Dragged-as-file: pass STREAM/00001.m2ts; open_bdrom walks up.
    // ====================================================================

    #[test]
    fn scan_file_inside_disc_walks_up() {
        let dir = make_disc(&DiscOpts::default());
        let m2ts_path = dir.path().join("BDMV/STREAM/00001.m2ts");
        let disc = scan(&m2ts_path.to_string_lossy()).expect("scan from inner file");
        assert_eq!(disc.playlists.len(), 1);
        assert_eq!(disc.playlists[0].name, "00800.MPLS");
    }

    // ====================================================================
    // 2b. Non-existent path => Err.
    // ====================================================================

    #[test]
    fn scan_nonexistent_path_errors() {
        let missing = std::env::temp_dir().join("bdmaster_modtest_does_not_exist_xyz");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(scan(&missing.to_string_lossy()).is_err());
        assert!(open_bdrom(&missing, false).is_err());
    }

    // ====================================================================
    // 2c. open_bdrom on a file inside a disc, with SSIF off, directly.
    //     Exercises open_bdrom_native with use_ssif=false branch in
    //     effective_stream_source / build_playlist_info / stream_display_name.
    // ====================================================================

    #[test]
    fn open_bdrom_native_ssif_off_branches() {
        let dir = make_disc(&DiscOpts::default());
        let bd = open_bdrom(dir.path(), false).expect("open native");
        assert!(!bd.use_ssif);
        assert!(bd.is_3d, "SSIF files still present => is_3d true");
        assert!(
            !bd.interleaved_files.is_empty(),
            "interleaved map populated"
        );

        // effective_stream_source with SSIF off returns the M2TS, not the SSIF.
        let src = effective_stream_source(&bd, "00001.M2TS").expect("source");
        match &src.0 {
            StreamSource::Native(p) => {
                assert!(p.to_string_lossy().to_uppercase().ends_with("00001.M2TS"))
            }
            _ => panic!("expected native source"),
        }

        // stream_display_name keeps the M2TS name when SSIF is off.
        assert_eq!(stream_display_name(&bd, "00001.M2TS"), "00001.M2TS");

        // to_disc_info on a SSIF-off BDRom: clips report the M2TS size, not SSIF.
        let disc = to_disc_info(&bd);
        let pl = &disc.playlists[0];
        assert_eq!(pl.stream_clips[0].display_name, "00001.M2TS");
        // file_size is the m2ts size.
        let m2ts_size = bd.stream_files.get("00001.M2TS").unwrap().1;
        assert_eq!(pl.stream_clips[0].file_size, m2ts_size);

        // is_ssif_mvc_stream is false when use_ssif is off.
        let mvc = TSStreamInfo::new(SSIF_MVC_PID, TSStreamType::MVCVideo as u8);
        assert!(!is_ssif_mvc_stream(&bd, "00001.M2TS", SSIF_MVC_PID, &mvc));
    }

    #[test]
    fn effective_stream_source_ssif_on_prefers_ssif() {
        let dir = make_disc(&DiscOpts::default());
        let bd = open_bdrom(dir.path(), true).expect("open native");
        assert!(bd.use_ssif);
        let src = effective_stream_source(&bd, "00001.M2TS").expect("source");
        match &src.0 {
            StreamSource::Native(p) => {
                assert!(p.to_string_lossy().to_uppercase().ends_with("00001.SSIF"))
            }
            _ => panic!("expected SSIF source"),
        }
        // display name swaps to SSIF.
        assert_eq!(stream_display_name(&bd, "00001.M2TS"), "00001.SSIF");

        // is_ssif_mvc_stream true path.
        let mvc = TSStreamInfo::new(SSIF_MVC_PID, TSStreamType::MVCVideo as u8);
        assert!(is_ssif_mvc_stream(&bd, "00001.M2TS", SSIF_MVC_PID, &mvc));
        // Wrong PID / wrong type / unknown clip are false.
        assert!(!is_ssif_mvc_stream(&bd, "00001.M2TS", 0x1011, &mvc));
        let avc = TSStreamInfo::new(SSIF_MVC_PID, TSStreamType::AVCVideo as u8);
        assert!(!is_ssif_mvc_stream(&bd, "00001.M2TS", SSIF_MVC_PID, &avc));
        assert!(!is_ssif_mvc_stream(&bd, "NOPE.M2TS", SSIF_MVC_PID, &mvc));
    }

    // ====================================================================
    // 4. Error/edge branches: missing PLAYLIST / CLIPINF, empty disc.
    // ====================================================================

    #[test]
    fn open_bdrom_missing_playlist_dir_errors() {
        let dir = TempDir::new("noplaylist");
        dir.write("BDMV/index.bdmv", b"INDX0200");
        dir.write("BDMV/CLIPINF/00001.clpi", &build_clpi_default());
        let err = open_bdrom(dir.path(), false).err().expect("expected error");
        assert!(err.to_string().contains("PLAYLIST or CLIPINF"));
    }

    #[test]
    fn open_bdrom_missing_clipinf_dir_errors() {
        let dir = TempDir::new("noclipinf");
        dir.write("BDMV/index.bdmv", b"INDX0200");
        dir.write("BDMV/PLAYLIST/00800.mpls", &build_mpls_default());
        assert!(open_bdrom(dir.path(), false).is_err());
    }

    #[test]
    fn locate_bdmv_via_index_at_root() {
        // A folder that IS the BDMV (index.bdmv at root, no BDMV ancestor).
        let dir = TempDir::new("rootbdmv");
        dir.write("index.bdmv", b"INDX0200");
        dir.write("PLAYLIST/00800.mpls", &build_mpls_default());
        dir.write("CLIPINF/00001.clpi", &build_clpi_default());
        dir.write("STREAM/00001.m2ts", &build_m2ts());
        let bd = open_bdrom(dir.path(), false).expect("open via index.bdmv root");
        assert_eq!(bd.playlists.len(), 1);
    }

    #[test]
    fn locate_bdmv_fails_when_absent() {
        let dir = TempDir::new("nobdmv");
        dir.write("random.txt", b"hi");
        assert!(open_bdrom(dir.path(), false).is_err());
    }

    // ====================================================================
    // 3. Pure-helper unit tests.
    // ====================================================================

    #[test]
    fn extract_title_from_xml_variants() {
        assert_eq!(
            extract_title_from_xml("<di:title><di:name>Hello World</di:name></di:title>")
                .as_deref(),
            Some("Hello World")
        );
        assert_eq!(
            extract_title_from_xml("<x:title><y:name>Rock &amp; Roll</y:name></x:title>")
                .as_deref(),
            Some("Rock & Roll")
        );
        assert_eq!(
            extract_title_from_xml("<di:title><di:name>blu-ray</di:name></di:title>"),
            None
        );
        assert_eq!(
            extract_title_from_xml("<di:title><di:name></di:name></di:title>"),
            None
        );
        // A name tag outside the title element is ignored, which avoids
        // unrelated metadata fields being treated as the disc title.
        assert_eq!(
            extract_title_from_xml("<di:other><di:name>Wrong</di:name></di:other>"),
            None
        );
        // No name tag => None.
        assert_eq!(extract_title_from_xml("<other>x</other>"), None);
        // Unterminated tag => None (no closing </).
        assert_eq!(
            extract_title_from_xml("<di:title><di:name>oops</di:title>"),
            None
        );
    }

    #[test]
    fn estimate_stream_size_paths() {
        let mut s = TSStreamInfo::new(0x1011, 0x1b);
        // No bit rate => 0.
        assert_eq!(estimate_stream_size(&s, 100.0), 0);
        // bit_rate used when > 0.
        s.bit_rate = 8_000_000;
        assert_eq!(estimate_stream_size(&s, 10.0), 10_000_000);
        // total_seconds 0 => 0.
        assert_eq!(estimate_stream_size(&s, 0.0), 0);
        // falls back to active_bit_rate when bit_rate == 0.
        s.bit_rate = 0;
        s.active_bit_rate = 8_000;
        assert_eq!(estimate_stream_size(&s, 1.0), 1_000);
    }

    fn mk_clip(name: &str, time_in: i64, time_out: i64, angle: u32) -> mpls::PlaylistStreamClip {
        mpls::PlaylistStreamClip {
            name: name.to_string(),
            time_in,
            time_out,
            angle_index: angle,
        }
    }

    fn mk_playlist(name: &str, clips: Vec<mpls::PlaylistStreamClip>) -> PlaylistFile {
        PlaylistFile {
            name: name.to_string(),
            file_type: "MPLS0200".to_string(),
            mvc_base_view_r: false,
            stream_clips: clips,
            chapters: Vec::new(),
            angle_count: 0,
            playlist_streams: Vec::new(),
        }
    }

    #[test]
    fn playlist_length_and_loops_and_validity() {
        // Two angle-0 clips of 45000 (1s) each = 90000.
        let pl = mk_playlist(
            "P.MPLS",
            vec![
                mk_clip("00001.M2TS", 0, 45000, 0),
                mk_clip("00002.M2TS", 0, 45000, 0),
                // angle 1 clip is ignored by length / loop computation.
                mk_clip("00003.M2TS", 0, 90000, 1),
            ],
        );
        assert_eq!(playlist_total_length_45k(&pl), 90000);
        assert!(!playlist_has_loops(&pl));

        // Looping playlist: same (name, time_in) appears twice on angle 0.
        let looped = mk_playlist(
            "L.MPLS",
            vec![
                mk_clip("00001.M2TS", 0, 45000, 0),
                mk_clip("00001.M2TS", 0, 45000, 0),
            ],
        );
        assert!(playlist_has_loops(&looped));

        // Validity: short playlist filtered out.
        // total seconds = 90000/45000 = 2.0; threshold 20 => invalid.
        assert!(!playlist_is_valid_for_scan(&pl, true, true, 20));
        // threshold 1 => valid (2.0 >= 1).
        assert!(playlist_is_valid_for_scan(&pl, true, true, 1));
        // looping filter on => looped playlist invalid.
        assert!(!playlist_is_valid_for_scan(&looped, true, false, 0));
        // looping filter off => looped playlist valid.
        assert!(playlist_is_valid_for_scan(&looped, false, false, 0));
        // both filters off => always valid.
        assert!(playlist_is_valid_for_scan(&pl, false, false, 0));
    }

    #[test]
    fn playlist_stream_to_info_video_and_audio() {
        // Video stream.
        let vs = mpls::PlaylistStream {
            pid: 0x1011,
            stream_type: TSStreamType::AVCVideo,
            video_format: TSVideoFormat::Video1080p,
            frame_rate: TSFrameRate::F23_976,
            aspect_ratio: TSAspectRatio::Aspect16_9,
            channel_layout: TSChannelLayout::Unknown,
            sample_rate_hz: 0,
            language_code: String::new(),
        };
        let vi = playlist_stream_to_info(&vs);
        assert!(vi.is_video_stream);
        assert_eq!(vi.height, 1080);
        assert_eq!(vi.width, 1920);
        assert!(!vi.is_interlaced);
        assert_eq!(vi.framerate, "23.976");
        assert_eq!(vi.aspect_ratio, "16:9");
        assert_eq!(vi.video_format, "1080p");
        assert!(vi.description.contains("1080p"));

        // Interlaced video to hit the "i" branch and a different width.
        let vs2 = mpls::PlaylistStream {
            pid: 0x1011,
            stream_type: TSStreamType::MPEG2Video,
            video_format: TSVideoFormat::Video480i,
            frame_rate: TSFrameRate::Unknown,
            aspect_ratio: TSAspectRatio::Unknown,
            channel_layout: TSChannelLayout::Unknown,
            sample_rate_hz: 0,
            language_code: "fra\0".to_string(),
        };
        let vi2 = playlist_stream_to_info(&vs2);
        assert!(vi2.is_interlaced);
        assert_eq!(vi2.width, 720);
        assert_eq!(vi2.video_format, "480i");
        // language code trailing NUL trimmed.
        assert_eq!(vi2.language_code, "fra");

        // Audio stream.
        let as_ = mpls::PlaylistStream {
            pid: 0x1100,
            stream_type: TSStreamType::AC3Audio,
            video_format: TSVideoFormat::Unknown,
            frame_rate: TSFrameRate::Unknown,
            aspect_ratio: TSAspectRatio::Unknown,
            channel_layout: TSChannelLayout::Multi,
            sample_rate_hz: 48000,
            language_code: "eng".to_string(),
        };
        let ai = playlist_stream_to_info(&as_);
        assert!(ai.is_audio_stream);
        assert_eq!(ai.channel_layout, "5.1");
        assert_eq!(ai.sample_rate, 48000);
        assert!(ai.description.contains("48 kHz"));
    }

    #[test]
    fn recompute_mvc_extension_toggles() {
        let mut disc = DiscInfo {
            path: String::new(),
            disc_name: String::new(),
            disc_title: String::new(),
            volume_label: String::new(),
            size: 0,
            is_bd_plus: false,
            is_bd_java: false,
            is_3d: false,
            is_4k: false,
            is_50hz: false,
            is_dbox: false,
            is_psp: false,
            is_uhd: false,
            has_mvc_extension: false,
            has_hevc_streams: false,
            has_uhd_disc_marker: false,
            meta_title: None,
            meta_disc_number: None,
            file_set_identifier: None,
            playlists: Vec::new(),
            stream_files: Vec::new(),
            stream_clip_files: Vec::new(),
        };
        recompute_mvc_extension(&mut disc);
        assert!(!disc.has_mvc_extension);

        let mut pl = PlaylistInfo {
            name: "P".into(),
            group_index: 1,
            file_size: 0,
            measured_size: 0,
            total_length: 0,
            has_hidden_tracks: false,
            has_loops: false,
            is_custom: false,
            chapters: Vec::new(),
            chapter_metrics: Vec::new(),
            bitrate_samples: Vec::new(),
            stream_clips: Vec::new(),
            video_streams: Vec::new(),
            audio_streams: Vec::new(),
            graphics_streams: Vec::new(),
            text_streams: Vec::new(),
            total_angles: 0,
        };
        pl.video_streams.push(TSStreamInfo::new(
            SSIF_MVC_PID,
            TSStreamType::MVCVideo as u8,
        ));
        disc.playlists.push(pl);
        recompute_mvc_extension(&mut disc);
        assert!(disc.has_mvc_extension);
    }

    #[test]
    fn refresh_ssif_derived_metadata_sets_base_view() {
        // Build a 3D BDRom and a DiscInfo with two video streams (AVC + MVC)
        // so refresh_ssif_derived_metadata assigns base_view per the source's
        // mvc_base_view_r flag.
        let dir = make_disc(&DiscOpts::default());
        let bd = open_bdrom(dir.path(), true).expect("open");
        assert!(bd.is_3d);

        let mut disc = to_disc_info(&bd);
        // to_disc_info yields only the MPLS-declared AVC video stream; the SSIF
        // MVC counterpart is normally appended by codec_init. Inject it here so
        // refresh_ssif_derived_metadata has both AVC + MVC to assign base_view.
        {
            let pl = disc
                .playlists
                .iter_mut()
                .find(|p| p.name == "00800.MPLS")
                .expect("playlist present");
            pl.video_streams.push(TSStreamInfo::new(
                SSIF_MVC_PID,
                TSStreamType::MVCVideo as u8,
            ));
        }

        refresh_ssif_derived_metadata(&mut disc, &bd);

        let pl = find_pl(&disc, "00800.MPLS");
        let avc = pl.video_streams.iter().find(|s| s.pid == 0x1011).unwrap();
        let mvc = pl
            .video_streams
            .iter()
            .find(|s| s.pid == SSIF_MVC_PID)
            .unwrap();
        // mvc_base_view_r is true in our MPLS => AVC base_view = true, MVC = false.
        let src = bd.playlists.get("00800.MPLS").unwrap();
        assert_eq!(avc.base_view, Some(src.mvc_base_view_r));
        assert_eq!(mvc.base_view, Some(!src.mvc_base_view_r));
        assert!(disc.has_mvc_extension);
    }

    #[test]
    fn refresh_ssif_derived_metadata_no_op_when_not_3d() {
        // A disc without SSIF: is_3d false => the base_view loop is skipped,
        // recompute_mvc_extension still runs.
        let dir = make_disc(&DiscOpts {
            with_ssif: false,
            ..Default::default()
        });
        let bd = open_bdrom(dir.path(), true).expect("open");
        assert!(!bd.is_3d);
        let mut disc = to_disc_info(&bd);
        refresh_ssif_derived_metadata(&mut disc, &bd);
        // No MVC promotion happened (no SSIF), so no mvc extension.
        // (Codec init wasn't run here, so video stream list is just the AVC.)
        assert!(!disc.has_mvc_extension);
    }

    #[test]
    fn copy_codec_metadata_copies_fields() {
        let mut src = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
        src.is_initialized = true;
        src.codec_name = "AVC".into();
        src.width = 1920;
        src.height = 1080;
        src.bit_rate = 20_000_000;
        src.active_bit_rate = 21_000_000;
        src.is_vbr = true;

        let mut dst = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
        copy_codec_metadata(&mut dst, &src);
        assert!(dst.is_initialized);
        assert_eq!(dst.width, 1920);
        assert_eq!(dst.height, 1080);
        assert_eq!(dst.bit_rate, 20_000_000);
        assert_eq!(dst.active_bit_rate, 21_000_000);
        assert!(dst.is_vbr);

        // Not-initialized source is a no-op.
        let uninit = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
        let mut dst2 = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
        copy_codec_metadata(&mut dst2, &uninit);
        assert!(!dst2.is_initialized);
    }

    #[test]
    fn clpi_language_for_lookup() {
        let dir = make_disc(&DiscOpts::default());
        let bd = open_bdrom(dir.path(), false).expect("open");
        // AC3 at PID 0x1100 has language "eng" in CLPI.
        assert_eq!(
            clpi_language_for(&bd, "00001.M2TS", 0x1100).as_deref(),
            Some("eng")
        );
        // PGS at 0x1200 has "jpn".
        assert_eq!(
            clpi_language_for(&bd, "00001.M2TS", 0x1200).as_deref(),
            Some("jpn")
        );
        // Unknown PID => None.
        assert!(clpi_language_for(&bd, "00001.M2TS", 0x9999).is_none());
        // Unknown clip => None.
        assert!(clpi_language_for(&bd, "99999.M2TS", 0x1100).is_none());
    }

    #[test]
    fn cache_estimated_stream_sizes_fills_estimates() {
        let mut disc = DiscInfo {
            path: String::new(),
            disc_name: String::new(),
            disc_title: String::new(),
            volume_label: String::new(),
            size: 0,
            is_bd_plus: false,
            is_bd_java: false,
            is_3d: false,
            is_4k: false,
            is_50hz: false,
            is_dbox: false,
            is_psp: false,
            is_uhd: false,
            has_mvc_extension: false,
            has_hevc_streams: false,
            has_uhd_disc_marker: false,
            meta_title: None,
            meta_disc_number: None,
            file_set_identifier: None,
            playlists: Vec::new(),
            stream_files: Vec::new(),
            stream_clip_files: Vec::new(),
        };
        let mut pl = PlaylistInfo {
            name: "P".into(),
            group_index: 1,
            file_size: 0,
            measured_size: 0,
            total_length: 45000 * 100, // 100 s
            has_hidden_tracks: false,
            has_loops: false,
            is_custom: false,
            chapters: Vec::new(),
            chapter_metrics: Vec::new(),
            bitrate_samples: Vec::new(),
            stream_clips: Vec::new(),
            video_streams: Vec::new(),
            audio_streams: Vec::new(),
            graphics_streams: Vec::new(),
            text_streams: Vec::new(),
            total_angles: 0,
        };
        let mut v = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
        v.bit_rate = 8_000_000; // 8 Mbps => 100 MB over 100 s
        pl.video_streams.push(v);
        disc.playlists.push(pl);

        cache_estimated_stream_sizes(&mut disc);
        assert_eq!(
            disc.playlists[0].video_streams[0].estimated_size,
            100_000_000
        );
    }

    // ====================================================================
    // resolve_playlist_path / resolve_stream_file_path.
    // ====================================================================

    #[test]
    fn resolve_playlist_path_success_and_errors() {
        let dir = make_disc(&DiscOpts::default());
        let root = dir.path().to_string_lossy().to_string();

        // Success (case-insensitive name match: stored uppercase, on-disk lowercase).
        let p = resolve_playlist_path(&root, "00800.MPLS").expect("resolve");
        assert!(p.to_string_lossy().to_lowercase().ends_with("00800.mpls"));

        // Not found.
        assert!(resolve_playlist_path(&root, "99999.MPLS").is_err());

        // Non-existent disc path.
        let missing = dir.path().join("nope");
        assert!(resolve_playlist_path(&missing.to_string_lossy(), "00800.MPLS").is_err());

        // is_file path (point at a real file) => Err (ISO-style rejection).
        let m2ts = dir.path().join("BDMV/STREAM/00001.m2ts");
        let err = resolve_playlist_path(&m2ts.to_string_lossy(), "00800.MPLS").unwrap_err();
        assert!(err.to_string().contains(".iso"));
    }

    #[test]
    fn resolve_playlist_path_no_playlist_dir_errors() {
        // BDMV present (index.bdmv) but no PLAYLIST subdir.
        let dir = TempDir::new("noplaylistdir");
        dir.write("index.bdmv", b"INDX0200");
        let root = dir.path().to_string_lossy().to_string();
        let err = resolve_playlist_path(&root, "00800.MPLS").unwrap_err();
        assert!(err.to_string().contains("PLAYLIST"));
    }

    #[test]
    fn resolve_stream_file_path_success_and_errors() {
        let dir = make_disc(&DiscOpts::default());
        let root = dir.path().to_string_lossy().to_string();

        // Default config has SSIF on, so the SSIF file is returned for the clip.
        let p = resolve_stream_file_path(&root, "00001.M2TS").expect("resolve stream");
        let upper = p.to_string_lossy().to_uppercase();
        assert!(upper.ends_with("00001.SSIF") || upper.ends_with("00001.M2TS"));

        // Unknown stream => Err.
        assert!(resolve_stream_file_path(&root, "99999.M2TS").is_err());

        // Non-existent disc path => Err.
        let missing = dir.path().join("nope");
        assert!(resolve_stream_file_path(&missing.to_string_lossy(), "00001.M2TS").is_err());

        // is_file path => Err (ISO-style rejection).
        let m2ts = dir.path().join("BDMV/STREAM/00001.m2ts");
        let err = resolve_stream_file_path(&m2ts.to_string_lossy(), "00001.M2TS").unwrap_err();
        assert!(err.to_string().contains(".iso"));
    }

    // ====================================================================
    // directory_size / dir_has_files / dir_has_extension / find_subdir.
    // ====================================================================

    #[test]
    fn directory_helpers() {
        let dir = TempDir::new("helpers");
        dir.write("a/file1.bin", &[0u8; 100]);
        dir.write("a/sub/file2.bin", &[0u8; 200]);
        dir.write("a/skip.ssif", &[0u8; 9999]); // excluded from size
        dir.mkdir("a/EmptyDir");

        let a = dir.path().join("a");
        // directory_size excludes .ssif files.
        assert_eq!(directory_size(&a), 300);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&a, a.join("sub/loop")).expect("create symlink loop");
            assert_eq!(directory_size(&a), 300);
        }
        // dir_has_files: 'a' has files; EmptyDir does not.
        assert!(dir_has_files(&a));
        assert!(!dir_has_files(&a.join("EmptyDir")));
        // dir_has_extension (case-insensitive).
        assert!(dir_has_extension(&a, "SSIF"));
        assert!(dir_has_extension(&a, "bin"));
        assert!(!dir_has_extension(&a, "mnv"));
        // find_subdir case-insensitive, returns None for missing.
        assert!(find_subdir(&a, "sub").is_some());
        assert!(find_subdir(&a, "SUB").is_some());
        assert!(find_subdir(&a, "missing").is_none());
        // directory_size on a non-existent dir is 0.
        assert_eq!(directory_size(&dir.path().join("nope")), 0);
        assert!(!dir_has_files(&dir.path().join("nope")));
        assert!(!dir_has_extension(&dir.path().join("nope"), "bin"));
    }

    #[test]
    fn read_disc_title_native_walks_meta() {
        let dir = TempDir::new("meta");
        dir.write(
            "META/DL/bdmt_eng.xml",
            b"<x><di:title><di:name>Nested Title</di:name></di:title></x>",
        );
        let title = read_disc_title_native(&dir.path().join("META"));
        assert_eq!(title.as_deref(), Some("Nested Title"));

        // No bdmt_eng.xml => None.
        let empty = TempDir::new("metaempty");
        empty.mkdir("META");
        assert!(read_disc_title_native(&empty.path().join("META")).is_none());
    }

    #[test]
    fn open_stream_reader_native_round_trip() {
        let dir = make_disc(&DiscOpts::default());
        let bd = open_bdrom(dir.path(), false).expect("open");
        let entry = bd.stream_files.get("00001.M2TS").expect("m2ts entry");
        // Buffered reader.
        let mut r = open_stream_reader(&bd, &entry.0).expect("reader");
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut r, &mut buf).expect("read");
        assert!(!buf.is_empty());
        // Raw reader.
        let mut r2 = open_stream_reader_raw(&bd, &entry.0).expect("raw reader");
        let mut buf2 = Vec::new();
        std::io::Read::read_to_end(&mut r2, &mut buf2).expect("read raw");
        assert_eq!(buf.len(), buf2.len());
    }
}
