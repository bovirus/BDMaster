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

use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::protocol::{
    DiscInfo, PlaylistInfo, PlaylistStreamClipInfo, StreamClipFileInfo, StreamFileInfo, TSStreamInfo,
};

use self::clpi::StreamClipFile;
use self::lang::language_name;
use self::mpls::{parse_mpls_bytes, PlaylistFile, PlaylistStream};
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
    let directory_root = directory_bdmv
        .parent()
        .ok_or_else(|| anyhow!("BDMV has no parent directory"))?
        .to_path_buf();

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
        let mut img = image.lock().unwrap();
        img.resolve("BDMV")
            .map_err(|e| anyhow!("UDF: BDMV not found in image: {}", e))?
    };
    if !bdmv.is_directory {
        return Err(anyhow!("UDF: BDMV is not a directory"));
    }

    // Volume label: derive from the ISO file name (without extension).
    let volume_label = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Total disc size: sum of all files in the root directory tree, skipping
    // .ssif files (mirroring BDInfo's behavior).
    let size = {
        let mut img = image.lock().unwrap();
        let root = img.root.clone();
        img.directory_size(&root).unwrap_or(0)
    };

    // index.bdmv → UHD detection.
    let mut is_uhd = false;
    {
        let mut img = image.lock().unwrap();
        if let Ok(index_fe) = img.resolve("BDMV/index.bdmv") {
            if let Ok(bytes) = img.read_file(&index_fe) {
                if bytes.len() >= 8 {
                    let header = String::from_utf8_lossy(&bytes[..8]);
                    is_uhd = header == "INDX0300";
                }
            }
        }
    }

    let mut img = image.lock().unwrap();

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
                    stream_clip_files.insert(
                        name.clone(),
                        StreamClipFile {
                            name,
                            size: fe.size,
                        },
                    );
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
    let mut size: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                size += directory_size(&p);
            } else if p.is_file() {
                if p.extension()
                    .map(|x| x.to_string_lossy().eq_ignore_ascii_case("ssif"))
                    .unwrap_or(false)
                {
                    continue;
                }
                size += std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    size
}

fn read_disc_title_native(meta_dir: &Path) -> Option<String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p
                    .file_name()
                    .map(|n| n.to_string_lossy().eq_ignore_ascii_case("bdmt_eng.xml"))
                    .unwrap_or(false)
                {
                    out.push(p);
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(meta_dir, &mut found);
    let path = found.first()?;
    let text = std::fs::read_to_string(path).ok()?;
    extract_title_from_xml(&text)
}

fn extract_title_from_xml(xml: &str) -> Option<String> {
    // Look for <di:name>...</di:name>, accepting any di prefix.
    let lower = xml.to_ascii_lowercase();
    let mut search_from = 0usize;
    while let Some(start) = lower[search_from..].find(":name>") {
        let abs = search_from + start;
        let after = abs + ":name>".len();
        if let Some(end_rel) = lower[after..].find("</") {
            let title = xml[after..after + end_rel].trim().to_string();
            if !title.is_empty() && title.to_lowercase() != "blu-ray" {
                return Some(title);
            }
        }
        search_from = after;
    }
    None
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
    for s in &pl.playlist_streams {
        let info = playlist_stream_to_info(s);
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

    PlaylistInfo {
        name: pl.name.clone(),
        group_index,
        file_size: total_file_size,
        measured_size: 0,
        total_length: total_length_45k.max(0) as u64,
        has_hidden_tracks: false,
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
    info.language_name = language_name(&info.language_code).to_string();

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
