/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use anyhow::Result;
#[cfg(target_os = "macos")]
use std::cmp::Ordering;
#[cfg(target_os = "macos")]
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;
use crate::protocol::MkvToolNixStatus;

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
    if cfg.mkv.mkv_toolnix_path == path {
        return Ok(());
    }
    cfg.mkv.mkv_toolnix_path = path;
    config::set_config(cfg)?;
    Ok(())
}

pub async fn is_mkvtoolnix_found(path: String, check_running: bool) -> Result<MkvToolNixStatus> {
    if check_running {
        if let Some(dir) = find_running_process_dir(mkvtoolnix_gui_process_name()) {
            if has_tool(&dir, "mkvtoolnix-gui") {
                let path_string = dir.to_string_lossy().to_string();
                let mut cfg = config::get_config();
                if cfg.mkv.mkv_toolnix_path != path_string {
                    cfg.mkv.mkv_toolnix_path = path_string.clone();
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

pub fn spawn_mkvtoolnix_gui(file: &str) -> Result<()> {
    let path = Path::new(file);
    if !path.exists() {
        return Err(anyhow::anyhow!("Path {} does not exist.", path.display()));
    }
    let cfg = config::get_config();
    let resolution = resolve_mkvtoolnix(&cfg.mkv.mkv_toolnix_path, &["mkvtoolnix-gui"]);
    if !resolution.found {
        return Err(anyhow::anyhow!(
            "MKVTOOLNIX_GUI_NOT_AVAILABLE:{}",
            resolution.path.display()
        ));
    }
    persist_mkvtoolnix_path_if_auto_detected(&resolution)?;
    let gui_path = get_tool_path(&resolution.path, "mkvtoolnix-gui");
    let mut cmd = std::process::Command::new(&gui_path);
    cmd.arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.spawn().map_err(|e| {
        anyhow::anyhow!("MKVTOOLNIX_GUI_NOT_AVAILABLE:{}: {}", gui_path.display(), e)
    })?;
    Ok(())
}
