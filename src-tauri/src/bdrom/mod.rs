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
pub mod lang;
pub mod m2ts;
pub mod mpls;
pub mod report;
pub mod types;

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::protocol::{
    ChartSample, DiscInfo, PlaylistInfo, PlaylistStreamClipInfo, StreamClipFileInfo,
    StreamFileInfo, TSStreamInfo,
};

use self::clpi::StreamClipFile;
use self::lang::language_name;
use self::mpls::{parse_mpls, PlaylistFile, PlaylistStream};
use self::types::*;

pub struct BDRom {
    pub path: PathBuf,
    pub directory_root: PathBuf,
    pub directory_bdmv: PathBuf,
    pub directory_playlist: Option<PathBuf>,
    pub directory_clipinf: Option<PathBuf>,
    pub directory_stream: Option<PathBuf>,
    pub directory_ssif: Option<PathBuf>,
    pub directory_bdjo: Option<PathBuf>,
    pub directory_meta: Option<PathBuf>,
    pub directory_snp: Option<PathBuf>,
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
    pub stream_files: HashMap<String, (PathBuf, u64)>,
    pub stream_clip_files: HashMap<String, StreamClipFile>,
}

pub fn scan(path_str: &str) -> Result<DiscInfo> {
    let path = Path::new(path_str);
    let bdrom = open_bdrom(path)?;
    Ok(to_disc_info(bdrom))
}

pub fn open_for_enrichment(path_str: &str) -> Result<BDRom> {
    open_bdrom(Path::new(path_str))
}

fn open_bdrom(path: &Path) -> Result<BDRom> {
    if !path.exists() {
        return Err(anyhow!("Path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(anyhow!("Disc image (.iso) is not yet supported in BDMaster."));
    }

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

    // Disc properties
    let volume_label = directory_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let size = directory_size(&directory_root);

    // Index version
    let mut is_uhd = false;
    let index_path = directory_bdmv.join("index.bdmv");
    if let Ok(bytes) = std::fs::read(&index_path) {
        if bytes.len() >= 8 {
            let header = String::from_utf8_lossy(&bytes[..8]);
            is_uhd = header == "INDX0300";
        }
    }

    // Detection flags
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

    let disc_title = directory_meta.as_ref().and_then(|m| read_disc_title(m));

    // Read MPLS playlists
    let mut playlists: HashMap<String, PlaylistFile> = HashMap::new();
    if let Some(plist_dir) = &directory_playlist {
        for entry in std::fs::read_dir(plist_dir)?.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("mpls") {
                    match parse_mpls(&p) {
                        Ok(pl) => {
                            playlists.insert(pl.name.clone(), pl);
                        }
                        Err(e) => log::warn!("Failed to parse {}: {}", p.display(), e),
                    }
                }
            }
        }
    }

    // CLPI
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

    // M2TS stream files
    let mut stream_files: HashMap<String, (PathBuf, u64)> = HashMap::new();
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
                    stream_files.insert(name, (p, size));
                }
            }
        }
    }

    // 50Hz check based on playlist video frame rates
    let is_50_hz = playlists.values().any(|pl| {
        pl.playlist_streams
            .iter()
            .any(|s| s.frame_rate.is_50_hz())
    });

    Ok(BDRom {
        path: path.to_path_buf(),
        directory_root,
        directory_bdmv,
        directory_playlist,
        directory_clipinf,
        directory_stream,
        directory_ssif,
        directory_bdjo,
        directory_meta,
        directory_snp,
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
    })
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
        .map(|it| {
            it.flatten().any(|e| {
                e.path().is_file()
            })
        })
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

fn read_disc_title(meta_dir: &Path) -> Option<String> {
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

fn to_disc_info(bd: BDRom) -> DiscInfo {
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

    // Sort playlists by name.
    let mut playlist_names: Vec<&String> = bd.playlists.keys().collect();
    playlist_names.sort();
    let playlists: Vec<PlaylistInfo> = playlist_names
        .iter()
        .map(|name| build_playlist_info(bd.playlists.get(*name).unwrap(), &bd))
        .collect();

    // Stream files (sorted)
    let mut stream_files: Vec<StreamFileInfo> = bd
        .stream_files
        .iter()
        .map(|(name, (_, size))| StreamFileInfo {
            name: name.clone(),
            size: *size,
            duration: 0,
            interleaved: false,
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
        || bd
            .playlists
            .values()
            .any(|pl| pl.playlist_streams.iter().any(|s| s.video_format == TSVideoFormat::Video2160p));

    DiscInfo {
        path: path_str,
        disc_name,
        disc_title: bd.disc_title.clone().unwrap_or_default(),
        volume_label: bd.volume_label,
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
        meta_title: bd.disc_title,
        meta_disc_number: None,
        file_set_identifier: None,
        playlists,
        stream_files,
        stream_clip_files,
    }
}

fn build_playlist_info(pl: &PlaylistFile, bd: &BDRom) -> PlaylistInfo {
    // Compute clip lengths and total length using only angle 0 clips.
    let mut total_length_45k: i64 = 0;
    let mut total_file_size: u64 = 0;
    let mut clips: Vec<PlaylistStreamClipInfo> = Vec::new();

    let mut relative_time_in: i64 = 0;
    for c in &pl.stream_clips {
        let length = (c.time_out - c.time_in).max(0);
        let file_size = bd
            .stream_files
            .get(&c.name)
            .map(|(_, s)| *s)
            .unwrap_or(0);
        total_file_size += file_size;
        let info = PlaylistStreamClipInfo {
            name: c.name.clone(),
            time_in: c.time_in as u64,
            time_out: c.time_out as u64,
            relative_time_in: relative_time_in.max(0) as u64,
            relative_time_out: (relative_time_in + length).max(0) as u64,
            length: length as u64,
            file_size,
            interleaved_file_size: 0,
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
        file_size: total_file_size,
        total_length: total_length_45k.max(0) as u64,
        has_hidden_tracks: false,
        has_loops: false,
        is_custom: false,
        chapters: pl.chapters.clone(),
        stream_clips: clips,
        video_streams,
        audio_streams,
        graphics_streams,
        text_streams,
        total_angles: pl.angle_count,
    }
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
        info.video_format = format!("{}{}", info.height, if info.is_interlaced { "i" } else { "p" });
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

pub fn build_chart_samples(path: &str, playlist_name: &str) -> Vec<ChartSample> {
    let bd = match open_bdrom(Path::new(path)) {
        Ok(bd) => bd,
        Err(err) => {
            log::warn!("chart: failed to open disc {}: {}", path, err);
            return Vec::new();
        }
    };
    let pl = match bd.playlists.get(&playlist_name.to_uppercase()) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let mut samples: Vec<ChartSample> = Vec::new();
    let mut offset_seconds: f64 = 0.0;
    for clip in &pl.stream_clips {
        if clip.angle_index != 0 {
            continue;
        }
        let entry = match bd.stream_files.get(&clip.name) {
            Some(e) => e,
            None => continue,
        };
        match m2ts::scan_m2ts(&entry.0) {
            Ok(res) => {
                // Restrict samples to the clip window [time_in, time_out).
                let clip_in_s = clip.time_in as f64 / 45000.0;
                let clip_out_s = clip.time_out as f64 / 45000.0;
                for (t, bps) in res.bitrate_samples {
                    if t < clip_in_s {
                        continue;
                    }
                    if t > clip_out_s {
                        break;
                    }
                    samples.push(ChartSample {
                        time: offset_seconds + (t - clip_in_s),
                        bit_rate: bps,
                    });
                }
                let length_s = clip_out_s - clip_in_s;
                if length_s.is_finite() && length_s > 0.0 {
                    offset_seconds += length_s;
                }
            }
            Err(err) => {
                log::warn!("chart: failed to scan {}: {}", clip.name, err);
            }
        }
    }
    samples
}

pub fn enrich_with_stream_stats(disc: &mut DiscInfo, bd: &BDRom) {
    enrich_inner(disc, bd, true);
}

pub fn enrich_inner(disc: &mut DiscInfo, bd: &BDRom, full_scan: bool) {
    use codec::CodecScanState;
    use std::collections::HashMap as HM;

    // Per-stream-file aggregated stats (bytes per PID, duration, bitrate samples).
    let mut per_file_duration: HashMap<String, f64> = HashMap::new();
    let mut per_file_bytes: HashMap<String, HM<u16, u64>> = HashMap::new();

    for pl in disc.playlists.iter_mut() {
        let mut total_seconds: f64 = 0.0;
        let mut per_pid_bytes: HM<u16, u64> = HM::new();

        for clip in &pl.stream_clips {
            if clip.angle_index != 0 {
                continue;
            }
            let entry = match bd.stream_files.get(&clip.name) {
                Some(e) => e,
                None => continue,
            };

            // Run the streaming scan, dispatching each PES through the codec
            // parsers for any tracked PID in this playlist.
            let mut pid_state: HM<u16, CodecScanState> = HM::new();

            // Map PID -> &mut TSStreamInfo. Build a flat table of pointers to
            // playlist streams so the closure can access them by PID. We use
            // a wrapper because the Rust borrow checker cannot prove the
            // disjoint-borrow safety of a heterogeneous chain here.
            let mut pid_streams: HM<u16, *mut TSStreamInfo> = HM::new();
            for s in pl.video_streams.iter_mut() {
                pid_streams.insert(s.pid, s as *mut TSStreamInfo);
            }
            for s in pl.audio_streams.iter_mut() {
                pid_streams.insert(s.pid, s as *mut TSStreamInfo);
            }
            for s in pl.graphics_streams.iter_mut() {
                pid_streams.insert(s.pid, s as *mut TSStreamInfo);
            }
            for s in pl.text_streams.iter_mut() {
                pid_streams.insert(s.pid, s as *mut TSStreamInfo);
            }

            // Initial bit-rate hint per stream (active_bit_rate from prior pass).
            let bitrate_hint: HM<u16, i64> = pid_streams
                .iter()
                .map(|(pid, p)| unsafe { (*pid, (**p).bit_rate as i64) })
                .collect();

            let res = m2ts::scan_m2ts_streaming(&entry.0, |pid, _stream_type, payload| {
                if let Some(stream_ptr) = pid_streams.get(&pid) {
                    let stream = unsafe { &mut **stream_ptr };
                    if !stream.is_initialized {
                        let state = pid_state.entry(pid).or_default();
                        let bitrate = bitrate_hint.get(&pid).copied().unwrap_or(0);
                        codec::scan_stream(
                            stream,
                            state,
                            payload,
                            bitrate,
                            true,
                            full_scan,
                        );
                    }
                }
                true
            });

            match res {
                Ok(r) => {
                    total_seconds += r.duration_seconds;
                    for (pid, stat) in &r.streams {
                        *per_pid_bytes.entry(*pid).or_insert(0) += stat.total_bytes;
                    }
                    per_file_duration.insert(clip.name.clone(), r.duration_seconds);
                    let mut bytes_map: HM<u16, u64> = HM::new();
                    for (pid, stat) in r.streams {
                        bytes_map.insert(pid, stat.total_bytes);
                    }
                    per_file_bytes.insert(clip.name.clone(), bytes_map);
                }
                Err(err) => {
                    log::warn!("scan {}: {}", clip.name, err);
                }
            }
        }

        // Compute active bit-rate for each playlist stream and finalize
        // descriptions.
        if total_seconds > 0.0 {
            for s in pl
                .video_streams
                .iter_mut()
                .chain(pl.audio_streams.iter_mut())
                .chain(pl.graphics_streams.iter_mut())
                .chain(pl.text_streams.iter_mut())
            {
                if let Some(b) = per_pid_bytes.get(&s.pid) {
                    s.active_bit_rate = (*b as f64 * 8.0 / total_seconds) as u64;
                    if s.bit_rate == 0 || s.is_vbr {
                        s.bit_rate = s.active_bit_rate;
                    }
                }
                codec::finalize_description(s);
            }
        } else {
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

    // Stream-file durations.
    for sf in disc.stream_files.iter_mut() {
        if let Some(d) = per_file_duration.get(&sf.name) {
            sf.duration = (d * 1_000_000.0) as u64;
        }
    }
}
