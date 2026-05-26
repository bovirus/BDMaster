/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

//! Parser-based string template engine for the MKV output file name.
//!
//! Mirrors the design of BatchMkvExtract's `renderTemplate`: a single forward
//! scan over the characters that supports
//! - `{{` -> literal `{` and `}}` -> literal `}` (brace escaping),
//! - `{name}` -> the placeholder's value when known, otherwise kept verbatim
//!   (so an unknown `{foo}` survives untouched), and
//! - an unterminated `{` (no closing `}`) is emitted as-is.
//!
//! The supported placeholders are derived from the input playlist or stream:
//! `{file_name}`, `{video_count}`, `{video_codec_1}`, `{audio_count}`,
//! `{audio_codec_1}`, `{text_count}`, `{text_codec_1}`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Result};

use crate::protocol::{PlaylistInfo, TSStreamInfo};

/// Raw TS stream type for Presentation Graphics (PGS) — the usual Blu-ray
/// subtitle format. Mirrors `bdrom::types::TSStreamType::PresentationGraphics`.
/// Interactive Graphics (0x91, menus) are deliberately not subtitles.
const PRESENTATION_GRAPHICS_STREAM_TYPE: u8 = 0x90;

/// Stream-derived placeholder values for a playlist or stream clip. `file_name`
/// is supplied separately at render time because it comes from the source file
/// path rather than the disc's stream tables.
#[derive(Debug, Clone, Default)]
pub struct StreamTemplateValues {
    pub video_count: usize,
    pub video_codec_1: String,
    pub audio_count: usize,
    pub audio_codec_1: String,
    pub text_count: usize,
    pub text_codec_1: String,
}

/// Placeholders whose values require parsing the disc's stream tables. Used to
/// skip that work when the template only needs `{file_name}` (the default).
const STREAM_PLACEHOLDERS: [&str; 6] = [
    "video_count",
    "video_codec_1",
    "audio_count",
    "audio_codec_1",
    "text_count",
    "text_codec_1",
];

/// Whether `template` references any placeholder that needs stream metadata.
pub fn template_needs_stream_values(template: &str) -> bool {
    STREAM_PLACEHOLDERS
        .iter()
        .any(|name| template.contains(&format!("{{{name}}}")))
}

/// Characters that are illegal in file names on the supported platforms.
fn sanitize_file_name_part(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect()
}

/// Render the output file name (without extension) from `template`, the source
/// `file_name` (a file stem), and the stream-derived `values`. Substituted
/// codec/type values are sanitized so the result is a valid file name; the
/// literal template text and `file_name` are left untouched.
pub fn render_output_file_name(
    template: &str,
    file_name: &str,
    values: &StreamTemplateValues,
) -> String {
    let mut map: HashMap<&str, String> = HashMap::new();
    map.insert("file_name", file_name.to_string());
    map.insert("video_count", values.video_count.to_string());
    map.insert(
        "video_codec_1",
        sanitize_file_name_part(&values.video_codec_1),
    );
    map.insert("audio_count", values.audio_count.to_string());
    map.insert(
        "audio_codec_1",
        sanitize_file_name_part(&values.audio_codec_1),
    );
    map.insert("text_count", values.text_count.to_string());
    map.insert(
        "text_codec_1",
        sanitize_file_name_part(&values.text_codec_1),
    );
    render_template(template, &map)
}

fn render_template(template: &str, values: &HashMap<&str, String>) -> String {
    let chars: Vec<char> = template.chars().collect();
    let len = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < len {
        let ch = chars[i];
        if ch == '{' {
            // `{{` escapes to a literal `{`.
            if i + 1 < len && chars[i + 1] == '{' {
                out.push('{');
                i += 2;
                continue;
            }
            // Scan to the closing `}`, stopping early at a stray `{`.
            let mut j = i + 1;
            while j < len && chars[j] != '}' && chars[j] != '{' {
                j += 1;
            }
            if j < len && chars[j] == '}' {
                let name: String = chars[i + 1..j].iter().collect();
                match values.get(name.as_str()) {
                    Some(value) => out.push_str(value),
                    // Unknown placeholder: keep `{name}` verbatim.
                    None => out.extend(chars[i..=j].iter()),
                }
                i = j + 1;
            } else {
                // Unterminated `{...`: emit what we scanned, sans closing brace.
                out.extend(chars[i..j].iter());
                i = j;
            }
            continue;
        }
        if ch == '}' {
            // `}}` escapes to a literal `}`.
            if i + 1 < len && chars[i + 1] == '}' {
                out.push('}');
                i += 2;
                continue;
            }
            out.push('}');
            i += 1;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// Build the stream-derived template values for a playlist, used to render the
/// MKV output file name. Parses the disc structure (playlists + clips) but
/// skips the heavy codec-init pass — `codec_short_name` / `stream_type_text`
/// are already known from the stream tables.
pub fn playlist_template_values(
    disc_path: &str,
    playlist_name: &str,
) -> Result<StreamTemplateValues> {
    let path = Path::new(disc_path);
    let use_ssif = crate::config::get_config().scan.enable_ssif_support;
    let bdrom = crate::bdrom::open_bdrom(path, use_ssif)?;
    let disc = crate::bdrom::to_disc_info(&bdrom);
    let playlist = disc
        .playlists
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(playlist_name))
        .ok_or_else(|| anyhow!("Playlist {} not found.", playlist_name))?;
    Ok(stream_values_from_playlist(playlist))
}

/// Build the stream-derived template values for a stream clip. A clip's
/// elementary streams aren't surfaced standalone, so they are taken from the
/// first playlist that references the clip; if none does, the counts are zero.
pub fn stream_template_values(
    disc_path: &str,
    stream_name: &str,
) -> Result<StreamTemplateValues> {
    let path = Path::new(disc_path);
    let use_ssif = crate::config::get_config().scan.enable_ssif_support;
    let bdrom = crate::bdrom::open_bdrom(path, use_ssif)?;
    let disc = crate::bdrom::to_disc_info(&bdrom);
    let stem = file_stem_lower(stream_name);
    let values = disc
        .playlists
        .iter()
        .find(|p| p.stream_clips.iter().any(|c| file_stem_lower(&c.name) == stem))
        .map(stream_values_from_playlist)
        .unwrap_or_default();
    Ok(values)
}

fn file_stem_lower(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn stream_values_from_playlist(pl: &PlaylistInfo) -> StreamTemplateValues {
    let (text_count, text_codec_1) =
        subtitle_count_and_first_codec(&pl.graphics_streams, &pl.text_streams);
    StreamTemplateValues {
        video_count: pl.video_streams.len(),
        video_codec_1: pl
            .video_streams
            .first()
            .map(|s| s.codec_short_name.clone())
            .unwrap_or_default(),
        audio_count: pl.audio_streams.len(),
        audio_codec_1: pl
            .audio_streams
            .first()
            .map(|s| s.codec_short_name.clone())
            .unwrap_or_default(),
        text_count,
        text_codec_1,
    }
}

/// Count subtitle streams and pick the first one's codec (e.g. PGS). Subtitles
/// are the textual subtitle streams plus Presentation Graphics (PGS);
/// Interactive Graphics (menus) are excluded. "First" is by PID, which
/// approximates the stream-table order across the separate graphics/text lists.
fn subtitle_count_and_first_codec(
    graphics_streams: &[TSStreamInfo],
    text_streams: &[TSStreamInfo],
) -> (usize, String) {
    let mut subtitles: Vec<&TSStreamInfo> = graphics_streams
        .iter()
        .filter(|s| s.stream_type == PRESENTATION_GRAPHICS_STREAM_TYPE)
        .chain(text_streams.iter())
        .collect();
    subtitles.sort_by_key(|s| s.pid);
    let text_codec_1 = subtitles
        .first()
        .map(|s| s.codec_short_name.clone())
        .unwrap_or_default();
    (subtitles.len(), text_codec_1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> StreamTemplateValues {
        StreamTemplateValues {
            video_count: 1,
            video_codec_1: "HEVC".to_owned(),
            audio_count: 2,
            audio_codec_1: "DTS-HD MA".to_owned(),
            text_count: 3,
            text_codec_1: "PGS".to_owned(),
        }
    }

    #[test]
    fn substitutes_all_known_placeholders() {
        let rendered = render_output_file_name(
            "{file_name}-{video_count}x{video_codec_1}-{audio_count}x{audio_codec_1}-{text_count}x{text_codec_1}",
            "00001",
            &values(),
        );
        assert_eq!(
            rendered,
            "00001-1xHEVC-2xDTS-HD MA-3xPGS"
        );
    }

    #[test]
    fn default_template_is_file_name() {
        let rendered = render_output_file_name("{file_name}", "00042", &values());
        assert_eq!(rendered, "00042");
    }

    #[test]
    fn unknown_placeholder_is_kept_verbatim() {
        let rendered = render_output_file_name("{file_name}-{unknown}", "x", &values());
        assert_eq!(rendered, "x-{unknown}");
    }

    #[test]
    fn escaped_braces_become_literals() {
        let rendered = render_output_file_name("{{{file_name}}}", "x", &values());
        assert_eq!(rendered, "{x}");
    }

    #[test]
    fn unterminated_brace_is_emitted() {
        // An unterminated `{...` is emitted verbatim (braces and all), matching
        // BatchMkvExtract's renderTemplate.
        let rendered = render_output_file_name("a{file_name", "x", &values());
        assert_eq!(rendered, "a{file_name");
    }

    #[test]
    fn substituted_values_are_sanitized() {
        let vals = StreamTemplateValues {
            video_codec_1: "a/b:c".to_owned(),
            ..StreamTemplateValues::default()
        };
        let rendered = render_output_file_name("{video_codec_1}", "x", &vals);
        assert_eq!(rendered, "a_b_c");
    }

    fn stream(pid: u16, stream_type: u8, codec: &str) -> TSStreamInfo {
        let mut s = TSStreamInfo::new(pid, stream_type);
        s.codec_short_name = codec.to_owned();
        s
    }

    #[test]
    fn subtitles_count_pgs_and_text_but_not_menus() {
        let graphics = vec![
            stream(0x1200, 0x90, "PGS"),
            stream(0x1400, 0x91, "IGS"), // menu, excluded
            stream(0x1201, 0x90, "PGS"),
        ];
        let text = vec![stream(0x1A00, 0x92, "SRT")];

        let (count, first_codec) = subtitle_count_and_first_codec(&graphics, &text);

        // 2 PGS + 1 text subtitle; the Interactive Graphics menu is excluded.
        assert_eq!(count, 3);
        // First by PID is the PGS stream at 0x1200.
        assert_eq!(first_codec, "PGS");
    }

    #[test]
    fn subtitles_first_codec_can_be_text_by_pid_order() {
        let graphics = vec![stream(0x1200, 0x90, "PGS")];
        let text = vec![stream(0x1100, 0x92, "SRT")];

        let (count, first_codec) = subtitle_count_and_first_codec(&graphics, &text);

        assert_eq!(count, 2);
        assert_eq!(first_codec, "SRT");
    }

    #[test]
    fn needs_stream_values_detects_placeholders() {
        assert!(!template_needs_stream_values("{file_name}"));
        assert!(template_needs_stream_values("{file_name}-{video_count}"));
        assert!(template_needs_stream_values("{text_codec_1}"));
    }
}
