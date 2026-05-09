/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Text report generator. Mirrors the format produced by BDInfo's FormReport.cs.
 */

use std::fmt::Write as _;

use crate::protocol::{DiscInfo, PlaylistInfo, TSStreamInfo};

pub fn generate(disc: &DiscInfo, full: bool, selected_playlists: Option<&[String]>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "DISC INFO:\n----------\n  Disc Title: {}\n  Disc Volume: {}\n  Disc Path: {}\n  Disc Size: {} ({:.2} GB)\n  Protection: {}{}{}{}{}{}{}",
        if !disc.disc_title.is_empty() { &disc.disc_title } else { "(none)" },
        if !disc.volume_label.is_empty() { &disc.volume_label } else { "(none)" },
        disc.path,
        disc.size,
        disc.size as f64 / (1024.0_f64.powi(3)),
        if disc.is_uhd { "UHD " } else { "" },
        if disc.is_4k { "4K " } else { "" },
        if disc.is_3d { "3D " } else { "" },
        if disc.is_50hz { "50Hz " } else { "" },
        if disc.is_bd_java { "BD-Java " } else { "" },
        if disc.is_bd_plus { "BD+ " } else { "" },
        if disc.has_mvc_extension { "MVC " } else { "" },
    );
    let _ = writeln!(out);

    let selected_set: Option<std::collections::HashSet<String>> =
        selected_playlists.map(|s| s.iter().cloned().collect());

    let _ = writeln!(out, "PLAYLISTS ({}):\n--------------", disc.playlists.len());
    for pl in &disc.playlists {
        let _ = writeln!(
            out,
            "  {}: {} clips, {} chapters, length={}",
            pl.name,
            pl.stream_clips.len(),
            pl.chapters.len(),
            format_45k_length(pl.total_length),
        );
    }
    let _ = writeln!(out);

    let target: Vec<&PlaylistInfo> = match selected_set {
        Some(set) => disc.playlists.iter().filter(|p| set.contains(&p.name)).collect(),
        None => disc.playlists.iter().collect(),
    };

    for pl in target {
        let _ = writeln!(out, "PLAYLIST: {}", pl.name);
        let _ = writeln!(out, "----------");
        let _ = writeln!(out, "  Length: {}", format_45k_length(pl.total_length));
        let _ = writeln!(
            out,
            "  Size:   {} bytes ({:.2} MB)",
            pl.file_size,
            pl.file_size as f64 / 1024.0_f64.powi(2)
        );
        let _ = writeln!(out, "  Chapters: {}", pl.chapters.len());

        if !pl.video_streams.is_empty() {
            let _ = writeln!(out, "\n  VIDEO:");
            print_streams(&mut out, &pl.video_streams);
        }
        if !pl.audio_streams.is_empty() {
            let _ = writeln!(out, "\n  AUDIO:");
            print_streams(&mut out, &pl.audio_streams);
        }
        if !pl.graphics_streams.is_empty() {
            let _ = writeln!(out, "\n  SUBTITLES:");
            print_streams(&mut out, &pl.graphics_streams);
        }
        if !pl.text_streams.is_empty() {
            let _ = writeln!(out, "\n  TEXT:");
            print_streams(&mut out, &pl.text_streams);
        }

        if full {
            let _ = writeln!(out, "\n  CLIPS ({}):", pl.stream_clips.len());
            for c in &pl.stream_clips {
                let _ = writeln!(
                    out,
                    "    {} - length={} size={} bytes",
                    c.name,
                    format_45k_length(c.length),
                    c.file_size
                );
            }
            if !pl.chapters.is_empty() {
                let _ = writeln!(out, "\n  CHAPTERS:");
                for (i, sec) in pl.chapters.iter().enumerate() {
                    let _ = writeln!(out, "    {:>3}: {}", i + 1, format_seconds(*sec));
                }
            }
        }
        let _ = writeln!(out);
    }

    out
}

fn print_streams(out: &mut String, streams: &[TSStreamInfo]) {
    for s in streams {
        let lang = if !s.language_name.is_empty() {
            format!(" [{}]", s.language_name)
        } else if !s.language_code.is_empty() {
            format!(" [{}]", s.language_code)
        } else {
            String::new()
        };
        let pid = format!("0x{:04X}", s.pid);
        let codec = if !s.codec_short_name.is_empty() {
            &s.codec_short_name
        } else {
            &s.codec_name
        };
        let _ = writeln!(
            out,
            "    {pid}  {codec:<14} {desc}{lang}",
            pid = pid,
            codec = codec,
            desc = s.description,
            lang = lang,
        );
    }
}

fn format_45k_length(length_45k: u64) -> String {
    let total_secs = length_45k as f64 / 45000.0;
    format_seconds(total_secs)
}

fn format_seconds(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "00:00:00".to_string();
    }
    let total = secs as u64;
    let ms = ((secs - total as f64) * 1000.0) as u64;
    let s = total % 60;
    let m = (total / 60) % 60;
    let h = total / 3600;
    if ms > 0 {
        format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
    } else {
        format!("{:02}:{:02}:{:02}", h, m, s)
    }
}
