/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::config;
use crate::protocol::MpcHcStatus;

const PROCESS_NAME: &str = "mpc-hc64.exe";

fn find_running_process_exe() -> Option<PathBuf> {
    let sys = sysinfo::System::new_all();
    for process in sys.processes().values() {
        let name = process.name().to_string_lossy();
        if !name.eq_ignore_ascii_case(PROCESS_NAME) {
            continue;
        }
        if let Some(exe) = process.exe() {
            return Some(exe.to_path_buf());
        }
    }
    None
}

fn is_valid_exe(path: &Path) -> bool {
    path.exists() && path.is_file()
}

fn persist_path(path: &Path) -> Result<()> {
    let new_path = path.to_string_lossy().to_string();
    let mut cfg = config::get_config();
    if cfg.mpchc.path == new_path {
        return Ok(());
    }
    cfg.mpchc.path = new_path;
    config::set_config(cfg)?;
    Ok(())
}

pub async fn is_mpchc_found(path: String, check_running: bool) -> Result<MpcHcStatus> {
    if check_running {
        if let Some(exe) = find_running_process_exe() {
            if is_valid_exe(&exe) {
                persist_path(&exe)?;
                return Ok(MpcHcStatus {
                    found: true,
                    path: exe.to_string_lossy().to_string(),
                });
            }
        }
    }
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(MpcHcStatus {
            found: false,
            path: String::new(),
        });
    }
    let candidate = PathBuf::from(trimmed);
    let found = is_valid_exe(&candidate);
    if found {
        persist_path(&candidate)?;
    }
    Ok(MpcHcStatus {
        found,
        path: candidate.to_string_lossy().to_string(),
    })
}

pub fn spawn_mpchc(file: &str) -> Result<()> {
    let file_path = Path::new(file);
    if !file_path.exists() {
        return Err(anyhow::anyhow!("File {} does not exist.", file));
    }
    let cfg = config::get_config();
    let exe = PathBuf::from(&cfg.mpchc.path);
    if !is_valid_exe(&exe) {
        return Err(anyhow::anyhow!("MPCHC_NOT_AVAILABLE:{}", exe.display()));
    }
    let mut cmd = std::process::Command::new(&exe);
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
        .map_err(|e| anyhow::anyhow!("Failed to launch MPC-HC: {}", e))?;
    Ok(())
}
