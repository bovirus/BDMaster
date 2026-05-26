/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use anyhow::Result;
#[cfg(target_os = "macos")]
use std::cmp::Ordering;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config;
use crate::protocol::MkvToolNixStatus;
use crate::template::{render_output_file_name, StreamTemplateValues};

fn mkvtoolnix_gui_process_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "mkvtoolnix-gui.exe"
    } else {
        "mkvtoolnix-gui"
    }
}

fn find_running_process_dir(exe_name: &str) -> Option<PathBuf> {
    let sys = sysinfo::System::new_all();
    for process in sys.processes().values() {
        let name = process.name().to_string_lossy();
        if !name.eq_ignore_ascii_case(exe_name) {
            continue;
        }
        if let Some(exe) = process.exe() {
            if let Some(parent) = exe.parent() {
                return Some(parent.to_path_buf());
            }
        }
    }
    None
}

struct MkvToolNixResolution {
    path: PathBuf,
    auto_detected: bool,
    found: bool,
}

#[cfg(target_os = "macos")]
fn compare_version_parts(left: &[u32], right: &[u32]) -> Ordering {
    let len = left.len().max(right.len());
    for i in 0..len {
        let l = left.get(i).copied().unwrap_or(0);
        let r = right.get(i).copied().unwrap_or(0);
        match l.cmp(&r) {
            Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    Ordering::Equal
}

#[cfg(target_os = "macos")]
fn parse_version_parts(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|part| {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse::<u32>().ok()
            }
        })
        .collect()
}

fn get_tool_path(path: &Path, tool: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let exe_path = path.join(format!("{}.exe", tool));
        if exe_path.exists() && exe_path.is_file() {
            return exe_path;
        }
    }
    path.join(tool)
}

fn has_tool(path: &Path, tool: &str) -> bool {
    let tool_path = path.join(tool);
    if tool_path.exists() && tool_path.is_file() {
        return true;
    }
    #[cfg(target_os = "windows")]
    {
        let tool_exe_path = path.join(format!("{}.exe", tool));
        if tool_exe_path.exists() && tool_exe_path.is_file() {
            return true;
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn is_default_macos_mkvtoolnix_path(path: &str) -> bool {
    path.trim().trim_end_matches('/') == "/Applications/MKVToolNix.app/Contents/MacOS"
}

#[cfg(target_os = "macos")]
fn find_latest_versioned_macos_mkvtoolnix_path(tools: &[&str]) -> Option<PathBuf> {
    let entries = fs::read_dir("/Applications").ok()?;
    let mut latest: Option<(Vec<u32>, PathBuf)> = None;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let app_name = match file_name.to_str() {
            Some(value) => value,
            None => continue,
        };
        if !app_name.starts_with("MKVToolNix-") || !app_name.ends_with(".app") {
            continue;
        }
        let version = &app_name["MKVToolNix-".len()..app_name.len() - ".app".len()];
        let version_parts = parse_version_parts(version);
        if version_parts.is_empty() {
            continue;
        }
        let mkvtoolnix_path = entry.path().join("Contents").join("MacOS");
        if !tools.iter().all(|t| has_tool(&mkvtoolnix_path, t)) {
            continue;
        }
        match &latest {
            None => latest = Some((version_parts, mkvtoolnix_path)),
            Some((latest_version, _)) => {
                if compare_version_parts(&version_parts, latest_version) == Ordering::Greater {
                    latest = Some((version_parts, mkvtoolnix_path));
                }
            }
        }
    }
    latest.map(|(_, path)| path)
}

fn resolve_mkvtoolnix(path: &str, tools: &[&str]) -> MkvToolNixResolution {
    let trimmed_path = path.trim();
    let configured_path = PathBuf::from(trimmed_path);
    if tools.iter().all(|t| has_tool(&configured_path, t)) {
        return MkvToolNixResolution {
            path: configured_path,
            auto_detected: false,
            found: true,
        };
    }
    #[cfg(target_os = "macos")]
    {
        if is_default_macos_mkvtoolnix_path(trimmed_path) {
            if let Some(latest_path) = find_latest_versioned_macos_mkvtoolnix_path(tools) {
                return MkvToolNixResolution {
                    path: latest_path,
                    auto_detected: true,
                    found: true,
                };
            }
        }
    }
    MkvToolNixResolution {
        path: configured_path,
        auto_detected: false,
        found: false,
    }
}

fn persist_mkvtoolnix_path_if_auto_detected(resolution: &MkvToolNixResolution) -> Result<()> {
    if !resolution.auto_detected {
        return Ok(());
    }
    let path = resolution.path.to_string_lossy().to_string();
    let mut cfg = config::get_config();
    if cfg.integration.mkv.mkv_toolnix_path == path {
        return Ok(());
    }
    cfg.integration.mkv.mkv_toolnix_path = path;
    config::set_config(cfg)?;
    Ok(())
}

fn output_path(
    source_file: &Path,
    to_path: &str,
    template: &str,
    values: &StreamTemplateValues,
) -> Result<PathBuf> {
    let file_stem = source_file
        .file_stem()
        .ok_or_else(|| anyhow::anyhow!("Source file has no file name: {}", source_file.display()))?
        .to_string_lossy();
    // The rendered name is a base file name without extension; mkvmerge always
    // produces Matroska, so the output extension is always `.mkv`. Fall back to
    // the source stem when the template renders to an empty name so we never
    // produce a bare ".mkv".
    let rendered = render_output_file_name(template, &file_stem, values);
    let base_name = if rendered.trim().is_empty() {
        file_stem.into_owned()
    } else {
        rendered
    };
    Ok(Path::new(to_path).join(format!("{base_name}.mkv")))
}

/// Resolve the muxed output path. The output directory defaults to the source
/// (disc) path when the caller hasn't chosen a separate one, so the templated
/// output file name is always applied — it is never skipped just because the
/// chosen directory matches the disc.
fn resolve_output_path(
    source_file: &Path,
    from_path: &str,
    to_path: Option<&str>,
    template: &str,
    values: &StreamTemplateValues,
) -> Result<PathBuf> {
    let to_path = to_path
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(from_path);
    output_path(source_file, to_path, template, values)
}

/// The transient MKVToolNix GUI hand-off config is written into the output
/// directory as `.bdm{n}.mtxcfg`, where `{n}` is a process-wide atomic counter.
const CONFIG_FILE_PREFIX: &str = ".bdm";
const CONFIG_FILE_SUFFIX: &str = ".mtxcfg";
/// A hand-off config is safe to delete once it's older than this — MKVToolNix
/// reads it at startup, well within the window.
const CONFIG_STALE_SECS: u64 = 10;

static CONFIG_FILE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Delete hand-off configs (`.bdm*.mtxcfg`) in `dir` older than
/// `CONFIG_STALE_SECS` by creation time. Returns how many matching configs are
/// still present afterwards (young ones, or any that couldn't be removed yet).
fn remove_stale_configs(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut remaining = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with(CONFIG_FILE_PREFIX) && name.ends_with(CONFIG_FILE_SUFFIX)) {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|m| m.created().or_else(|_| m.modified()))
            .ok()
            .and_then(|created| now.duration_since(created).ok())
            .map(|age| age.as_secs() >= CONFIG_STALE_SECS)
            .unwrap_or(false);
        if old_enough && fs::remove_file(entry.path()).is_ok() {
            continue;
        }
        remaining += 1;
    }
    remaining
}

/// Background cleanup: poll the output directory and remove our `.bdm*.mtxcfg`
/// hand-off configs once they age past `CONFIG_STALE_SECS`, until none remain.
/// Bounded so the thread can't run forever if a file stays locked.
fn schedule_config_cleanup(dir: PathBuf) {
    thread::spawn(move || {
        for _ in 0..12 {
            thread::sleep(Duration::from_secs(5));
            if remove_stale_configs(&dir) == 0 {
                break;
            }
        }
    });
}

fn write_gui_output_config(output: &Path) -> Result<PathBuf> {
    let output_dir = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("Output path has no parent directory: {}", output.display())
        })?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let destination = output.to_string_lossy().to_string();
    let destination_auto = output
        .with_file_name(format!(
            ".bdmaster-auto-destination-{}-{timestamp}.mkv",
            std::process::id()
        ))
        .to_string_lossy()
        .to_string();
    let config = serde_json::json!({
        "MKVToolNix GUI Settings": {
            "version": 3,
            "type": "MuxConfig"
        },
        "input": {
            "files": {
                "numberOfEntries": 0
            },
            "attachments": {
                "numberOfEntries": 0
            },
            "trackOrder": [],
            "firstInputFileName": ""
        },
        "global": {
            "destination": destination,
            "destinationAuto": destination_auto,
            "destinationUniquenessSuffix": "",
            "title": "",
            "globalTags": "",
            "segmentInfo": "",
            "splitOptions": "",
            "segmentUIDs": "",
            "previousSegmentUID": "",
            "nextSegmentUID": "",
            "chapters": "",
            "chapterTitleNumber": 1,
            "chapterLanguage": "und",
            "chapterCharacterSet": "",
            "chapterDelay": "",
            "chapterStretchBy": "",
            "chapterCueNameFormat": "",
            "additionalOptions": "",
            "splitMode": 0,
            "splitMaxFiles": 1,
            "linkFiles": false,
            "webmMode": false,
            "stopAfterVideoEnds": false,
            "chapterGenerationMode": 0,
            "chapterGenerationNameTemplate": "Chapter <NUM:2>",
            "chapterGenerationInterval": ""
        }
    });
    // The hand-off config lives in the output directory as `.bdm{n}.mtxcfg`.
    // Creating it here doubles as the writability check: a failure (e.g. a
    // read-only mounted disc) surfaces a translatable error to the UI.
    let number = CONFIG_FILE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let config_path = output_dir.join(format!("{CONFIG_FILE_PREFIX}{number}{CONFIG_FILE_SUFFIX}"));
    let file = File::create(&config_path)
        .map_err(|_| anyhow::anyhow!("OUTPUT_DIR_NOT_WRITABLE:{}", output_dir.display()))?;
    serde_json::to_writer_pretty(file, &config)?;
    Ok(config_path)
}

pub async fn is_mkvtoolnix_found(path: String, check_running: bool) -> Result<MkvToolNixStatus> {
    if check_running {
        if let Some(dir) = find_running_process_dir(mkvtoolnix_gui_process_name()) {
            if has_tool(&dir, "mkvtoolnix-gui") {
                let path_string = dir.to_string_lossy().to_string();
                let mut cfg = config::get_config();
                if cfg.integration.mkv.mkv_toolnix_path != path_string {
                    cfg.integration.mkv.mkv_toolnix_path = path_string.clone();
                    config::set_config(cfg)?;
                }
                return Ok(MkvToolNixStatus {
                    found: true,
                    mkv_toolnix_path: path_string,
                });
            }
        }
    }
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return Ok(MkvToolNixStatus {
            found: false,
            mkv_toolnix_path: String::new(),
        });
    }
    let resolution = resolve_mkvtoolnix(trimmed_path, &["mkvtoolnix-gui"]);
    if resolution.found {
        persist_mkvtoolnix_path_if_auto_detected(&resolution)?;
    }
    Ok(MkvToolNixStatus {
        found: resolution.found,
        mkv_toolnix_path: resolution.path.to_string_lossy().to_string(),
    })
}

pub fn spawn_mkvtoolnix_gui(
    source_file: &Path,
    from_path: &str,
    to_path: Option<&str>,
    values: &StreamTemplateValues,
) -> Result<()> {
    if !source_file.exists() {
        return Err(anyhow::anyhow!(
            "Path {} does not exist.",
            source_file.display()
        ));
    }
    let cfg = config::get_config();
    let resolution = resolve_mkvtoolnix(&cfg.integration.mkv.mkv_toolnix_path, &["mkvtoolnix-gui"]);
    if !resolution.found {
        return Err(anyhow::anyhow!(
            "MKVTOOLNIX_GUI_NOT_AVAILABLE:{}",
            resolution.path.display()
        ));
    }
    persist_mkvtoolnix_path_if_auto_detected(&resolution)?;
    let gui_path = get_tool_path(&resolution.path, "mkvtoolnix-gui");
    let output = resolve_output_path(
        source_file,
        from_path,
        to_path,
        &cfg.integration.mkv.output_file_template,
        values,
    )?;
    // Writing the hand-off config into the output directory also acts as the
    // writability check: a failure surfaces a translatable error to the UI.
    let config_path = write_gui_output_config(&output)?;
    let mut cmd = std::process::Command::new(&gui_path);
    cmd.arg(&config_path)
        .arg(source_file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let spawn_result = cmd.spawn().map(|_| ()).map_err(|e| {
        anyhow::anyhow!("MKVTOOLNIX_GUI_NOT_AVAILABLE:{}: {}", gui_path.display(), e)
    });
    match spawn_result {
        Ok(()) => {
            // Clean up the hand-off config in the background once it ages out.
            if let Some(dir) = config_path.parent() {
                schedule_config_cleanup(dir.to_path_buf());
            }
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&config_path);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_output_path_defaults_to_source_path_when_to_path_matches() {
        let source_file = Path::new("BDMV").join("PLAYLIST").join("00001.mpls");

        // No separate output directory chosen (to_path == from_path): the
        // template is still applied and the file lands in the disc path.
        let output = resolve_output_path(
            &source_file,
            "disc",
            Some("disc"),
            "{file_name}",
            &StreamTemplateValues::default(),
        )
        .unwrap();

        assert_eq!(output, Path::new("disc").join("00001.mkv"));
    }

    #[test]
    fn resolve_output_path_defaults_to_source_path_when_to_path_missing() {
        let source_file = Path::new("BDMV").join("PLAYLIST").join("00001.mpls");

        let output = resolve_output_path(
            &source_file,
            "disc",
            None,
            "{file_name}",
            &StreamTemplateValues::default(),
        )
        .unwrap();

        assert_eq!(output, Path::new("disc").join("00001.mkv"));
    }

    #[test]
    fn resolve_output_path_uses_to_path_and_source_file_stem() {
        let source_file = Path::new("BDMV").join("STREAM").join("00002.m2ts");

        let output = resolve_output_path(
            &source_file,
            "disc",
            Some("output"),
            "{file_name}",
            &StreamTemplateValues::default(),
        )
        .unwrap();

        assert_eq!(output, Path::new("output").join("00002.mkv"));
    }

    #[test]
    fn resolve_output_path_renders_stream_placeholders() {
        let source_file = Path::new("BDMV").join("PLAYLIST").join("00001.mpls");
        let values = StreamTemplateValues {
            video_count: 1,
            video_codec_1: "HEVC".to_owned(),
            audio_count: 2,
            ..StreamTemplateValues::default()
        };

        let output = resolve_output_path(
            &source_file,
            "disc",
            Some("output"),
            "{file_name}-{video_codec_1}-{audio_count}ch",
            &values,
        )
        .unwrap();

        assert_eq!(output, Path::new("output").join("00001-HEVC-2ch.mkv"));
    }

    #[test]
    fn output_path_falls_back_to_stem_when_template_renders_empty() {
        let source_file = Path::new("BDMV").join("STREAM").join("00002.m2ts");

        let output =
            output_path(&source_file, "output", "", &StreamTemplateValues::default()).unwrap();

        assert_eq!(output, Path::new("output").join("00002.mkv"));
    }

    #[test]
    fn output_path_appends_mkv_to_rendered_base_name() {
        let source_file = Path::new("BDMV").join("PLAYLIST").join("00001.mpls");
        let values = StreamTemplateValues::default();

        // `{file_name}` is the source stem (no extension); `.mkv` is appended.
        assert_eq!(
            output_path(&source_file, "out", "{file_name}", &values).unwrap(),
            Path::new("out").join("00001.mkv")
        );
        // Whatever literal text the template adds is kept verbatim before `.mkv`.
        assert_eq!(
            output_path(&source_file, "out", "{file_name}.abc", &values).unwrap(),
            Path::new("out").join("00001.abc.mkv")
        );
    }

    #[test]
    fn gui_output_config_is_created_in_output_dir() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_dir = std::env::temp_dir().join(format!("bdmaster-mkvtoolnix-test-{timestamp}"));
        fs::create_dir_all(&output_dir).unwrap();
        let output = output_dir.join("00002.mkv");

        let config_path = write_gui_output_config(&output).unwrap();

        // The hand-off config lives in the output directory, named `.bdm{n}.mtxcfg`.
        assert_eq!(config_path.parent(), Some(output_dir.as_path()));
        let name = config_path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(".bdm") && name.ends_with(".mtxcfg"), "name was {name}");
        assert!(config_path.exists());
        let config = fs::read_to_string(&config_path).unwrap();
        // Destination still points at the real (absolute) output file.
        assert!(config.contains("00002.mkv"));
        assert!(config.contains("\"chapterLanguage\": \"und\""));
        assert!(config.contains("\"chapterGenerationNameTemplate\": \"Chapter <NUM:2>\""));
        fs::remove_file(config_path).unwrap();
        fs::remove_dir(output_dir).unwrap();
    }

    #[test]
    fn write_gui_output_config_reports_unwritable_output_dir() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // A directory that doesn't exist can't be written to.
        let missing = std::env::temp_dir().join(format!("bdmaster-missing-{timestamp}"));
        let output = missing.join("00002.mkv");

        let error = write_gui_output_config(&output).unwrap_err();

        assert!(error.to_string().contains("OUTPUT_DIR_NOT_WRITABLE"));
    }

    #[test]
    fn remove_stale_configs_keeps_fresh_and_ignores_other_files() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bdmaster-cleanup-test-{timestamp}"));
        fs::create_dir_all(&dir).unwrap();
        // A freshly written config is younger than the stale window, so it's kept.
        let fresh = dir.join(".bdm0.mtxcfg");
        fs::write(&fresh, b"{}").unwrap();
        // A non-matching file must never be touched.
        let other = dir.join("keep.txt");
        fs::write(&other, b"x").unwrap();

        let remaining = remove_stale_configs(&dir);

        assert_eq!(remaining, 1);
        assert!(fresh.exists());
        assert!(other.exists());

        fs::remove_file(fresh).unwrap();
        fs::remove_file(other).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
