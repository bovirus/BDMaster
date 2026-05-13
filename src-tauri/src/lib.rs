/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Manager;

static WINDOW_READY: AtomicBool = AtomicBool::new(false);

mod bdrom;
mod bettermediainfo;
mod config;
mod constants;
mod controller;
mod mkvtoolnix;
mod mpchc;
mod protocol;

use protocol::{FullScanState, ScanProgressInfo, UpdateCheckResult, UpdateCheckState};

fn convert_error(error: anyhow::Error) -> String {
    error.to_string()
}

#[tauri::command]
async fn get_about() -> Result<protocol::About, String> {
    controller::get_about().await.map_err(convert_error)
}

#[tauri::command]
async fn get_config() -> Result<config::Config, String> {
    controller::get_config().await.map_err(convert_error)
}

#[tauri::command]
async fn set_config(config: config::Config) -> Result<config::Config, String> {
    controller::set_config(config).await.map_err(convert_error)
}

#[tauri::command]
fn get_update_result(state: tauri::State<'_, UpdateCheckState>) -> Option<UpdateCheckResult> {
    state.result.lock().unwrap().clone()
}

#[tauri::command]
fn skip_version(version: String) -> Result<(), String> {
    let mut cfg = config::get_config();
    cfg.update.ignore_version = version;
    config::set_config(cfg).map_err(convert_error)
}

#[tauri::command]
fn get_launch_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

#[tauri::command]
async fn scan_disc(path: String) -> Result<protocol::DiscInfo, String> {
    controller::scan_disc(path).await.map_err(convert_error)
}

#[tauri::command]
fn start_full_scan(path: String, state: tauri::State<'_, Arc<FullScanState>>) {
    controller::start_full_scan(path, state.inner().clone());
}

#[tauri::command]
fn cancel_full_scan(state: tauri::State<'_, Arc<FullScanState>>) {
    controller::cancel_full_scan(state.inner());
}

#[tauri::command]
fn get_scan_progress(state: tauri::State<'_, Arc<FullScanState>>) -> ScanProgressInfo {
    controller::get_scan_progress(state.inner())
}

#[tauri::command]
async fn write_text_file(file: String, text: String) -> Result<(), String> {
    controller::write_text_file(file, text)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn write_binary_file(file: String, bytes: Vec<u8>) -> Result<(), String> {
    controller::write_binary_file(file, bytes)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn is_mkvtoolnix_found(
    path: String,
    check_running: bool,
) -> Result<protocol::MkvToolNixStatus, String> {
    mkvtoolnix::is_mkvtoolnix_found(path, check_running)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn open_playlist_in_mkvtoolnix_gui(
    disc_path: String,
    playlist_name: String,
) -> Result<(), String> {
    let resolved = bdrom::resolve_playlist_path(&disc_path, &playlist_name).map_err(convert_error)?;
    mkvtoolnix::spawn_mkvtoolnix_gui(&resolved.to_string_lossy()).map_err(convert_error)
}

#[tauri::command]
async fn open_stream_file_in_mkvtoolnix_gui(
    disc_path: String,
    stream_name: String,
) -> Result<(), String> {
    let resolved =
        bdrom::resolve_stream_file_path(&disc_path, &stream_name).map_err(convert_error)?;
    mkvtoolnix::spawn_mkvtoolnix_gui(&resolved.to_string_lossy()).map_err(convert_error)
}

#[tauri::command]
async fn is_bettermediainfo_found(
    path: String,
    check_running: bool,
) -> Result<protocol::BetterMediaInfoStatus, String> {
    bettermediainfo::is_bettermediainfo_found(path, check_running)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn open_playlist_in_bettermediainfo(
    disc_path: String,
    playlist_name: String,
) -> Result<(), String> {
    let resolved = bdrom::resolve_playlist_path(&disc_path, &playlist_name).map_err(convert_error)?;
    bettermediainfo::spawn_bettermediainfo(&resolved.to_string_lossy()).map_err(convert_error)
}

#[tauri::command]
async fn open_stream_file_in_bettermediainfo(
    disc_path: String,
    stream_name: String,
) -> Result<(), String> {
    let resolved =
        bdrom::resolve_stream_file_path(&disc_path, &stream_name).map_err(convert_error)?;
    bettermediainfo::spawn_bettermediainfo(&resolved.to_string_lossy()).map_err(convert_error)
}

#[tauri::command]
async fn is_mpchc_found(
    path: String,
    check_running: bool,
) -> Result<protocol::MpcHcStatus, String> {
    mpchc::is_mpchc_found(path, check_running)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn open_playlist_in_mpchc(
    disc_path: String,
    playlist_name: String,
) -> Result<(), String> {
    let resolved = bdrom::resolve_playlist_path(&disc_path, &playlist_name).map_err(convert_error)?;
    mpchc::spawn_mpchc(&resolved.to_string_lossy()).map_err(convert_error)
}

#[tauri::command]
async fn open_stream_file_in_mpchc(
    disc_path: String,
    stream_name: String,
) -> Result<(), String> {
    let resolved =
        bdrom::resolve_stream_file_path(&disc_path, &stream_name).map_err(convert_error)?;
    mpchc::spawn_mpchc(&resolved.to_string_lossy()).map_err(convert_error)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .manage(UpdateCheckState {
            result: Arc::new(Mutex::new(None)),
        })
        .manage(Arc::new(FullScanState::new()))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let _ = window.set_title(&format!(
                "{} v{}",
                constants::APP_NAME,
                controller::get_app_version()
            ));

            let cfg = config::get_config();
            let _ = window.set_size(tauri::LogicalSize::new(
                cfg.window.size.width,
                cfg.window.size.height,
            ));
            if cfg.window.position.x < 0 || cfg.window.position.y < 0 {
                let _ = window.center();
            } else {
                let _ = window.set_position(tauri::LogicalPosition::new(
                    cfg.window.position.x,
                    cfg.window.position.y,
                ));
            }
            let _ = window.show();
            let _ = window.set_focus();
            WINDOW_READY.store(true, Ordering::SeqCst);

            // Background update check, throttled by ConfigUpdate.check_interval.
            let update_state = app.state::<UpdateCheckState>();
            let result_arc = update_state.result.clone();
            let interval_seconds: i64 = match cfg.update.check_interval {
                config::UpdateCheckInterval::Daily => 86_400,
                config::UpdateCheckInterval::Weekly => 604_800,
                config::UpdateCheckInterval::Monthly => 2_592_000,
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if cfg.update.last_checked == 0 || now - cfg.update.last_checked > interval_seconds {
                std::thread::spawn(move || {
                    let check_result = std::panic::catch_unwind(|| controller::check_for_updates())
                        .unwrap_or_else(|_| {
                            log::error!("Update check panicked");
                            Err(anyhow::anyhow!("Update check panicked"))
                        });
                    match check_result {
                        Ok(result) => {
                            log::info!(
                                "Update check result: has_update={}, latest_version={:?}",
                                result.has_update,
                                result.latest_version
                            );
                            let mut updated_config = config::get_config();
                            updated_config.update.last_checked = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0);
                            if let Some(ref version) = result.latest_version {
                                updated_config.update.last_version = version.clone();
                            }
                            let _ = config::set_config(updated_config.clone());
                            // Suppress if this version is the user-ignored one.
                            let final_result = if result.has_update
                                && result.latest_version.as_deref()
                                    == Some(updated_config.update.ignore_version.as_str())
                                && !updated_config.update.ignore_version.is_empty()
                            {
                                UpdateCheckResult {
                                    has_update: false,
                                    latest_version: None,
                                }
                            } else {
                                result
                            };
                            *result_arc.lock().unwrap() = Some(final_result);
                        }
                        Err(e) => {
                            log::warn!("Update check failed: {}", e);
                            *result_arc.lock().unwrap() = Some(UpdateCheckResult {
                                has_update: false,
                                latest_version: None,
                            });
                        }
                    }
                });
            } else if !cfg.update.last_version.is_empty()
                && controller::is_newer_version(&cfg.update.last_version, controller::get_app_version())
                && cfg.update.last_version != cfg.update.ignore_version
            {
                *result_arc.lock().unwrap() = Some(UpdateCheckResult {
                    has_update: true,
                    latest_version: Some(cfg.update.last_version.clone()),
                });
            } else {
                *result_arc.lock().unwrap() = Some(UpdateCheckResult {
                    has_update: false,
                    latest_version: None,
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                    if !WINDOW_READY.load(Ordering::SeqCst) {
                        return;
                    }
                    let Ok(scale) = window.scale_factor() else { return; };
                    let Ok(pos) = window.outer_position() else { return; };
                    let Ok(size) = window.inner_size() else { return; };
                    let logical_pos: tauri::LogicalPosition<i32> = pos.to_logical(scale);
                    let logical_size: tauri::LogicalSize<u32> = size.to_logical(scale);
                    let mut cfg = config::get_config();
                    cfg.window.position.x = logical_pos.x;
                    cfg.window.position.y = logical_pos.y;
                    cfg.window.size.width = logical_size.width;
                    cfg.window.size.height = logical_size.height;
                    if let Err(err) = config::set_config(cfg) {
                        log::error!("Couldn't save window state: {}", err);
                    }
                }
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
                    // Cancel any running full scan so the worker thread
                    // exits before the process unwinds. Without this the
                    // disc reader can keep churning on shutdown and trip
                    // panics on dropped Tauri state.
                    if let Some(state) = window.try_state::<Arc<FullScanState>>() {
                        controller::cancel_full_scan(state.inner());
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_about,
            get_config,
            set_config,
            get_update_result,
            skip_version,
            get_launch_args,
            scan_disc,
            start_full_scan,
            cancel_full_scan,
            get_scan_progress,
            write_text_file,
            write_binary_file,
            is_mkvtoolnix_found,
            open_playlist_in_mkvtoolnix_gui,
            open_stream_file_in_mkvtoolnix_gui,
            is_bettermediainfo_found,
            open_playlist_in_bettermediainfo,
            open_stream_file_in_bettermediainfo,
            is_mpchc_found,
            open_playlist_in_mpchc,
            open_stream_file_in_mpchc,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
