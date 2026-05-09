/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 */

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Manager;

static WINDOW_READY: AtomicBool = AtomicBool::new(false);

mod bdrom;
mod config;
mod constants;
mod controller;
mod protocol;

use protocol::{UpdateCheckResult, UpdateCheckState};

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
async fn generate_report(
    path: String,
    full: bool,
    selected_playlists: Option<Vec<String>>,
) -> Result<String, String> {
    controller::generate_report(path, full, selected_playlists)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn get_playlist_chart_data(
    path: String,
    playlist_name: String,
) -> Result<Vec<protocol::ChartSample>, String> {
    controller::get_playlist_chart_data(path, playlist_name)
        .await
        .map_err(convert_error)
}

#[tauri::command]
async fn write_text_file(file: String, text: String) -> Result<(), String> {
    controller::write_text_file(file, text)
        .await
        .map_err(convert_error)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .manage(UpdateCheckState {
            result: Arc::new(Mutex::new(None)),
        })
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
            generate_report,
            get_playlist_chart_data,
            write_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
