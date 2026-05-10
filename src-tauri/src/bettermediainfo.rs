/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::config;
use crate::protocol::BetterMediaInfoStatus;

const TOOL_STEM: &str = "BetterMediaInfo";

fn process_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "BetterMediaInfo.exe"
    } else {
        TOOL_STEM
    }
}

fn find_running_process_dir() -> Option<PathBuf> {
    let exe_name = process_name();
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

fn binary_path(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join(TOOL_STEM);
    if direct.exists() && direct.is_file() {
        return Some(direct);
    }
    #[cfg(target_os = "windows")]
    {
        let exe = dir.join(format!("{}.exe", TOOL_STEM));
        if exe.exists() && exe.is_file() {
            return Some(exe);
        }
    }
    None
}

fn has_executable(path: &Path) -> bool {
    binary_path(path).is_some()
}

fn resolve(path: &str) -> (PathBuf, bool) {
    let trimmed = path.trim();
    let configured = PathBuf::from(trimmed);
    if has_executable(&configured) {
        return (configured, true);
    }
    (configured, false)
}

fn persist_path(path: &Path) -> Result<()> {
    let new_path = path.to_string_lossy().to_string();
    let mut cfg = config::get_config();
    if cfg.better_media_info.path == new_path {
        return Ok(());
    }
    cfg.better_media_info.path = new_path;
    config::set_config(cfg)?;
    Ok(())
}

pub async fn is_bettermediainfo_found(
    path: String,
    check_running: bool,
) -> Result<BetterMediaInfoStatus> {
    if check_running {
        if let Some(dir) = find_running_process_dir() {
            // Directory holding the running binary. On macOS that's
            // `<App>.app/Contents/MacOS`, which is what we want to store. On
            // Windows/Linux it's the install directory.
            if has_executable(&dir) {
                persist_path(&dir)?;
                return Ok(BetterMediaInfoStatus {
                    found: true,
                    path: dir.to_string_lossy().to_string(),
                });
            }
        }
    }
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(BetterMediaInfoStatus {
            found: false,
            path: String::new(),
        });
    }
    let (resolved, found) = resolve(trimmed);
    if found {
        persist_path(&resolved)?;
    }
    Ok(BetterMediaInfoStatus {
        found,
        path: resolved.to_string_lossy().to_string(),
    })
}

#[cfg(target_os = "macos")]
fn find_macos_app_bundle(bin: &Path) -> Option<PathBuf> {
    // The Mach-O binary lives at `<bundle>.app/Contents/MacOS/BetterMediaInfo`,
    // so the `.app` ancestor is at most three levels up.
    let mut current = bin.parent()?;
    for _ in 0..4 {
        if current.extension().and_then(|s| s.to_str()) == Some("app") {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
    None
}

pub fn spawn_bettermediainfo(file: &str) -> Result<()> {
    let file_path = Path::new(file);
    if !file_path.exists() {
        return Err(anyhow::anyhow!("File {} does not exist.", file));
    }
    let cfg = config::get_config();
    let dir = PathBuf::from(&cfg.better_media_info.path);
    let bin = binary_path(&dir).ok_or_else(|| {
        anyhow::anyhow!(
            "BETTERMEDIAINFO_NOT_AVAILABLE:{}",
            dir.display()
        )
    })?;

    // On macOS, going through `open` lets Launch Services activate the bundle
    // cleanly — spawning the bundle's Mach-O binary directly causes a brief
    // window flash because the child never goes through proper app activation.
    #[cfg(target_os = "macos")]
    {
        if let Some(app_bundle) = find_macos_app_bundle(&bin) {
            let mut cmd = std::process::Command::new("/usr/bin/open");
            cmd.arg("-a")
                .arg(&app_bundle)
                .arg("--args")
                .arg(file)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            cmd.spawn()
                .map_err(|e| anyhow::anyhow!("Failed to launch BetterMediaInfo via open: {}", e))?;
            return Ok(());
        }
    }

    let mut cmd = std::process::Command::new(&bin);
    cmd.arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.spawn()
        .map_err(|e| anyhow::anyhow!("Failed to launch BetterMediaInfo: {}", e))?;
    Ok(())
}
