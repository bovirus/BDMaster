/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Text report generator. Mirrors the format produced by BDInfo's FormReport.cs.
 *
 * Both quick-summary and full report render directly from `DiscInfo` (the
 * data parsed by the basic disc scan: MPLS, CLPI, file sizes). No M2TS
 * streaming is performed — that matches BDInfo's behavior when the user
 * generates a report without first running an explicit per-frame scan.
 * Fields that would otherwise be produced by streaming (per-chapter peak
 * bitrates, per-frame max sizes) are emitted as 0 to keep the column layout
 * consistent.
 */

use std::fmt::Write as _;

use crate::config::{self, FormatPrecision, FormatUnit};
use crate::protocol::{DiscInfo, PlaylistInfo, TSStreamInfo};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Quick-summary report (current behavior preserved). The full report path
/// goes through `generate_full`.
pub fn generate(disc: &DiscInfo, full: bool, selected_playlists: Option<&[String]>) -> String {
    let cfg = config::get_config();
    let size_precision = cfg.formatting.size.precision;
    let size_unit = cfg.formatting.size.unit;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "DISC INFO:\n----------\n  Disc Title: {}\n  Disc Volume: {}\n  Disc Path: {}\n  Disc Size: {} ({})\n  Protection: {}{}{}{}{}{}{}",
        if !disc.disc_title.is_empty() { &disc.disc_title } else { "(none)" },
        if !disc.volume_label.is_empty() { &disc.volume_label } else { "(none)" },
        disc.path,
        disc.size,
        format_size(disc.size, &size_precision, &size_unit),
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
            "  Size:   {} bytes ({})",
            pl.file_size,
            format_size(pl.file_size, &size_precision, &size_unit)
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

/// BDInfo-style full report rendered from the basic disc scan only. No M2TS
/// streaming required — runs in well under a second even for large discs.
pub fn generate_full(disc: &DiscInfo, selected_playlists: Option<&[String]>) -> String {
    let selected_set: Option<std::collections::HashSet<String>> =
        selected_playlists.map(|s| s.iter().cloned().collect());

    let mut out = String::new();
    let protection = if disc.is_bd_plus {
        "BD+"
    } else if disc.is_uhd {
        "AACS2"
    } else {
        "AACS"
    };
    let mut extras: Vec<&str> = Vec::new();
    if disc.is_uhd {
        extras.push("Ultra HD");
    }
    if disc.is_bd_java {
        extras.push("BD-Java");
    }
    if disc.is_50hz {
        extras.push("50Hz Content");
    }
    if disc.is_3d {
        extras.push("Blu-ray 3D");
    }
    if disc.is_dbox {
        extras.push("D-BOX Motion Code");
    }
    if disc.is_psp {
        extras.push("PSP Digital Copy");
    }

    write_disc_info_block(&mut out, disc, protection, &extras);
    let _ = writeln!(out);

    let target: Vec<&PlaylistInfo> = match &selected_set {
        Some(set) => disc.playlists.iter().filter(|p| set.contains(&p.name)).collect(),
        None => disc.playlists.iter().collect(),
    };

    for pl in target {
        write_playlist_full(&mut out, disc, pl, protection, &extras);
    }

    out
}

fn write_disc_info_block(out: &mut String, disc: &DiscInfo, protection: &str, extras: &[&str]) {
    if !disc.disc_title.is_empty() {
        let _ = writeln!(out, "{:<16}{}", "Disc Title:", disc.disc_title);
    }
    let _ = writeln!(out, "{:<16}{}", "Disc Label:", disc.volume_label);
    let _ = writeln!(out, "{:<16}{} bytes", "Disc Size:", format_thousands(disc.size));
    let _ = writeln!(out, "{:<16}{}", "Protection:", protection);
    if !extras.is_empty() {
        let _ = writeln!(out, "{:<16}{}", "Extras:", extras.join(", "));
    }
    let _ = writeln!(out, "{:<16}BDMaster v{}", "BDInfo:", APP_VERSION);
}

fn write_playlist_full(
    out: &mut String,
    disc: &DiscInfo,
    pl: &PlaylistInfo,
    protection: &str,
    extras: &[&str],
) {
    let total_length_seconds = pl.total_length as f64 / 45000.0;
    let total_size = pl.file_size;
    let total_bitrate_mbps = if total_length_seconds > 0.0 {
        total_size as f64 * 8.0 / total_length_seconds / 1_000_000.0
    } else {
        0.0
    };

    let video_codec = pl
        .video_streams
        .first()
        .map(|s| s.codec_short_name.clone())
        .unwrap_or_default();
    let video_bitrate_mbps = pl
        .video_streams
        .first()
        .map(|s| effective_bitrate(s) as f64 / 1_000_000.0)
        .unwrap_or(0.0);

    let (audio1, language_code1) = format_audio_summary(pl.audio_streams.first());
    let audio2 = format_secondary_audio(&pl.audio_streams, &language_code1);

    let _ = writeln!(out);
    let _ = writeln!(out, "********************");
    let _ = writeln!(out, "PLAYLIST: {}", pl.name);
    let _ = writeln!(out, "********************");
    let _ = writeln!(out);

    // Forum paste header.
    let _ = writeln!(
        out,
        "{:<64}{:<8}{:<8}{:<16}{:<18}{:<13}{:<13}{:<42}{}",
        "", "", "", "", "", "Total", "Video", "", ""
    );
    let _ = writeln!(
        out,
        "{:<64}{:<8}{:<8}{:<16}{:<18}{:<13}{:<13}{:<42}{}",
        "Title",
        "Codec",
        "Length",
        "Movie Size",
        "Disc Size",
        "Bitrate",
        "Bitrate",
        "Main Audio Track",
        "Secondary Audio Track"
    );
    let _ = writeln!(
        out,
        "{:<64}{:<8}{:<8}{:<16}{:<18}{:<13}{:<13}{:<42}{}",
        "-----",
        "------",
        "-------",
        "--------------",
        "----------------",
        "-----------",
        "-----------",
        "------------------",
        "---------------------"
    );
    let _ = writeln!(
        out,
        "{:<64}{:<8}{:<8}{:<16}{:<18}{:<13}{:<13}{:<42}{}",
        pl.name,
        video_codec,
        format_seconds_short(total_length_seconds),
        format_thousands(total_size),
        format_thousands(disc.size),
        format!("{:.2} Mbps", total_bitrate_mbps),
        format!("{:.2} Mbps", video_bitrate_mbps),
        audio1,
        audio2
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "DISC INFO:");
    let _ = writeln!(out);
    write_disc_info_block(out, disc, protection, extras);

    let _ = writeln!(out);
    let _ = writeln!(out, "PLAYLIST REPORT:");
    let _ = writeln!(out);
    let _ = writeln!(out, "{:<24}{}", "Name:", pl.name);
    let _ = writeln!(
        out,
        "{:<24}{} (h:m:s.ms)",
        "Length:",
        format_seconds_full(total_length_seconds)
    );
    let _ = writeln!(out, "{:<24}{} bytes", "Size:", format_thousands(total_size));
    let _ = writeln!(
        out,
        "{:<24}{:.2} Mbps",
        "Total Bitrate:", total_bitrate_mbps
    );

    if !pl.video_streams.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "VIDEO:");
        let _ = writeln!(out);
        let _ = writeln!(out, "{:<24}{:<20}{}", "Codec", "Bitrate", "Description");
        let _ = writeln!(
            out,
            "{:<24}{:<20}{}",
            "---------------", "-------------", "-----------"
        );
        for s in &pl.video_streams {
            let bitrate_kbps = effective_bitrate(s) as f64 / 1000.0;
            let bitrate_str = format!("{} kbps", format_thousands(bitrate_kbps.round() as u64));
            let codec = stream_codec_long_name(s);
            let _ = writeln!(out, "{:<24}{:<20}{}", codec, bitrate_str, s.description);
        }
    }

    if !pl.audio_streams.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "AUDIO:");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<32}{:<16}{:<16}{}",
            "Codec", "Language", "Bitrate", "Description"
        );
        let _ = writeln!(
            out,
            "{:<32}{:<16}{:<16}{}",
            "---------------", "-------------", "-------------", "-----------"
        );
        for s in &pl.audio_streams {
            let bitrate_kbps = (effective_bitrate(s) as f64 / 1000.0).round() as i64;
            let bitrate_str = format!("{:>5} kbps", bitrate_kbps);
            let codec = stream_codec_long_name(s);
            let _ = writeln!(
                out,
                "{:<32}{:<16}{:<16}{}",
                codec,
                if s.language_name.is_empty() {
                    &s.language_code
                } else {
                    &s.language_name
                },
                bitrate_str,
                s.description
            );
        }
    }

    if !pl.graphics_streams.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "SUBTITLES:");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<32}{:<16}{:<16}{}",
            "Codec", "Language", "Bitrate", "Description"
        );
        let _ = writeln!(
            out,
            "{:<32}{:<16}{:<16}{}",
            "---------------", "-------------", "-------------", "-----------"
        );
        for s in &pl.graphics_streams {
            let bitrate_str = format!("{:>5.2} kbps", effective_bitrate(s) as f64 / 1000.0);
            let codec = stream_codec_long_name(s);
            let _ = writeln!(
                out,
                "{:<32}{:<16}{:<16}{}",
                codec,
                if s.language_name.is_empty() {
                    &s.language_code
                } else {
                    &s.language_name
                },
                bitrate_str,
                s.description
            );
        }
    }

    if !pl.text_streams.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "TEXT:");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<32}{:<16}{:<16}{}",
            "Codec", "Language", "Bitrate", "Description"
        );
        let _ = writeln!(
            out,
            "{:<32}{:<16}{:<16}{}",
            "---------------", "-------------", "-------------", "-----------"
        );
        for s in &pl.text_streams {
            let bitrate_str = format!("{:>5.2} kbps", effective_bitrate(s) as f64 / 1000.0);
            let codec = stream_codec_long_name(s);
            let _ = writeln!(
                out,
                "{:<32}{:<16}{:<16}{}",
                codec,
                if s.language_name.is_empty() {
                    &s.language_code
                } else {
                    &s.language_name
                },
                bitrate_str,
                s.description
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "FILES:");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<16}{:<16}{:<16}{:<16}{}",
        "Name", "Time In", "Length", "Size", "Total Bitrate"
    );
    let _ = writeln!(
        out,
        "{:<16}{:<16}{:<16}{:<16}{}",
        "---------------",
        "-------------",
        "-------------",
        "-------------",
        "-------------"
    );
    for clip in &pl.stream_clips {
        if clip.angle_index > 1 {
            continue;
        }
        let length_s = clip.length as f64 / 45000.0;
        let time_in_s = clip.relative_time_in as f64 / 45000.0;
        let size = clip.file_size;
        let bitrate_kbps = if length_s > 0.0 {
            (size as f64 * 8.0 / length_s / 1000.0).round() as u64
        } else {
            0
        };
        let display_name = if clip.angle_index > 0 {
            format!("{} ({})", clip.name, clip.angle_index)
        } else {
            clip.name.clone()
        };
        let _ = writeln!(
            out,
            "{:<16}{:<16}{:<16}{:<16}{:>6} kbps",
            display_name,
            format_seconds_full(time_in_s),
            format_seconds_full(length_s),
            format_thousands(size),
            format_thousands(bitrate_kbps)
        );
    }

    if !pl.chapters.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "CHAPTERS:");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<8}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{}",
            "Number",
            "Time In",
            "Length",
            "Avg Video Rate",
            "Max 1-Sec Rate",
            "Max 1-Sec Time",
            "Max 5-Sec Rate",
            "Max 5-Sec Time",
            "Max 10Sec Rate",
            "Max 10Sec Time",
            "Avg Frame Size",
            "Max Frame Size",
            "Max Frame Time"
        );
        let _ = writeln!(
            out,
            "{:<8}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{}",
            "------",
            "-------------",
            "-------------",
            "--------------",
            "--------------",
            "--------------",
            "--------------",
            "--------------",
            "--------------",
            "--------------",
            "--------------",
            "--------------",
            "--------------"
        );
        write_chapters_table(out, pl);
    }

    let _ = writeln!(out);
}

/// Per-chapter table without per-frame bitrate diagnostics. Emits the same
/// 13-column shape BDInfo uses but with zeros for fields that depend on a
/// per-PES stream scan (peak rates, frame sizes), matching BDInfo's behavior
/// when no scan was run before generating the report.
fn write_chapters_table(out: &mut String, pl: &PlaylistInfo) {
    let total_length_s = pl.total_length as f64 / 45000.0;
    for ci in 0..pl.chapters.len() {
        let chapter_start = pl.chapters[ci];
        let chapter_end = if ci + 1 < pl.chapters.len() {
            pl.chapters[ci + 1]
        } else {
            total_length_s
        };
        let chapter_length = (chapter_end - chapter_start).max(0.0);
        let _ = writeln!(
            out,
            "{:<8}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{:<16}{}",
            ci + 1,
            format_seconds_full(chapter_start),
            format_seconds_full(chapter_length),
            "     0 kbps",
            "     0 kbps",
            "0:00:00.000",
            "     0 kbps",
            "0:00:00.000",
            "     0 kbps",
            "0:00:00.000",
            "      0 bytes",
            "      0 bytes",
            "0:00:00.000",
        );
    }
}

fn effective_bitrate(s: &TSStreamInfo) -> u64 {
    if s.bit_rate > 0 {
        s.bit_rate
    } else {
        s.active_bit_rate
    }
}

fn stream_codec_long_name(s: &TSStreamInfo) -> &str {
    if !s.codec_name.is_empty() {
        &s.codec_name
    } else {
        &s.codec_short_name
    }
}

fn format_audio_summary(s: Option<&TSStreamInfo>) -> (String, String) {
    let Some(s) = s else {
        return (String::new(), String::new());
    };
    let codec = stream_codec_long_name(s);
    let mut out = format!("{} {}", codec, s.channel_layout);
    let bitrate = effective_bitrate(s);
    if bitrate > 0 {
        let _ = write!(out, " {} kbps", (bitrate as f64 / 1000.0).round() as i64);
    }
    if s.sample_rate > 0 && s.bit_depth > 0 {
        let _ = write!(
            out,
            " ({}kHz/{}-bit)",
            (s.sample_rate as f64 / 1000.0).round() as i64,
            s.bit_depth
        );
    }
    (out, s.language_code.clone())
}

fn format_secondary_audio(audio_streams: &[TSStreamInfo], primary_lang: &str) -> String {
    if audio_streams.len() <= 1 {
        return String::new();
    }
    for s in &audio_streams[1..] {
        if s.language_code != primary_lang {
            continue;
        }
        // Skip secondary audio types and stereo AC3 (matching BDInfo).
        let st = s.stream_type;
        let is_secondary = st == 0xA1 || st == 0xA2;
        let is_stereo_ac3 = st == 0x81 && s.channel_count == 2;
        if is_secondary || is_stereo_ac3 {
            continue;
        }
        let codec = stream_codec_long_name(s);
        let mut out = format!("{} {}", codec, s.channel_layout);
        let bitrate = effective_bitrate(s);
        if bitrate > 0 {
            let _ = write!(out, " {} kbps", (bitrate as f64 / 1000.0).round() as i64);
        }
        if s.sample_rate > 0 && s.bit_depth > 0 {
            let _ = write!(
                out,
                " ({}kHz/{}-bit)",
                (s.sample_rate as f64 / 1000.0).round() as i64,
                s.bit_depth
            );
        }
        return out;
    }
    String::new()
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

fn format_seconds_short(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "0:00:00".to_string();
    }
    let total = secs as u64;
    let s = total % 60;
    let m = (total / 60) % 60;
    let h = total / 3600;
    format!("{}:{:02}:{:02}", h, m, s)
}

fn format_seconds_full(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "0:00:00.000".to_string();
    }
    let total = secs as u64;
    let ms = ((secs - total as f64) * 1000.0).round() as u64;
    let s = total % 60;
    let m = (total / 60) % 60;
    let h = total / 3600;
    format!("{}:{:02}:{:02}.{:03}", h, m, s, ms)
}

/// Format a byte count using the user-selected precision and unit. Mirrors
/// `formatSize` in `src/lib/format.ts` so the report matches the UI.
fn format_size(bytes: u64, precision: &FormatPrecision, unit: &FormatUnit) -> String {
    if bytes == 0 {
        return "0".to_string();
    }
    let dp: usize = match precision {
        FormatPrecision::Zero => 0,
        FormatPrecision::One => 1,
        FormatPrecision::Two => 2,
    };
    // (divisor, label) tiers, smallest first. Note: per the frontend's
    // convention, K/KM/KMG/KMGT use binary (1024-based) and KMi/KMiGi/KMiGiTi
    // use decimal (1000-based). The report mirrors that.
    let tiers: &[(f64, &str)] = match unit {
        FormatUnit::K => &[(1024.0, "K")],
        FormatUnit::KM => &[(1024.0, "K"), (1_048_576.0, "M")],
        FormatUnit::KMG => &[
            (1024.0, "K"),
            (1_048_576.0, "M"),
            (1_073_741_824.0, "G"),
        ],
        FormatUnit::KMGT => &[
            (1024.0, "K"),
            (1_048_576.0, "M"),
            (1_073_741_824.0, "G"),
            (1_099_511_627_776.0, "T"),
        ],
        FormatUnit::KMi => &[(1e3, "Ki"), (1e6, "Mi")],
        FormatUnit::KMiGi => &[(1e3, "Ki"), (1e6, "Mi"), (1e9, "Gi")],
        FormatUnit::KMiGiTi => &[(1e3, "Ki"), (1e6, "Mi"), (1e9, "Gi"), (1e12, "Ti")],
    };
    let bytes_f = bytes as f64;
    for (divisor, label) in tiers.iter().rev() {
        if bytes_f >= *divisor {
            let formatted = format!("{:.*}", dp, bytes_f / divisor);
            return format!("{} {}B", trim_fraction_zeros(&formatted), label);
        }
    }
    format!("{} B", bytes)
}

fn trim_fraction_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let mut v = s.trim_end_matches('0').to_string();
    if v.ends_with('.') {
        v.pop();
    }
    v
}

fn format_thousands<T: Into<u128>>(n: T) -> String {
    let n: u128 = n.into();
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}
