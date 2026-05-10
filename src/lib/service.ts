/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { invoke } from "@tauri-apps/api/core";
import * as Protocol from "./protocol";

export async function getAbout(): Promise<Protocol.About> {
  return await invoke<Protocol.About>("get_about");
}

export async function getConfig(): Promise<Protocol.Config> {
  return await invoke<Protocol.Config>("get_config");
}

export async function setConfig(config: Protocol.Config): Promise<Protocol.Config> {
  return await invoke<Protocol.Config>("set_config", { config });
}

export async function getUpdateResult(): Promise<Protocol.UpdateCheckResult | null> {
  return await invoke<Protocol.UpdateCheckResult | null>("get_update_result");
}

export async function skipVersion(version: string): Promise<void> {
  return await invoke<void>("skip_version", { version });
}

export async function getLaunchArgs(): Promise<string[]> {
  return await invoke<string[]>("get_launch_args");
}

export async function scanDisc(path: string): Promise<Protocol.DiscInfo> {
  return await invoke<Protocol.DiscInfo>("scan_disc", { path });
}

export async function startFullScan(path: string): Promise<void> {
  return await invoke<void>("start_full_scan", { path });
}

export async function cancelFullScan(): Promise<void> {
  return await invoke<void>("cancel_full_scan");
}

export async function getScanProgress(): Promise<Protocol.ScanProgress> {
  return await invoke<Protocol.ScanProgress>("get_scan_progress");
}

export async function generateReport(
  path: string,
  full: boolean,
  selectedPlaylists: string[] | null
): Promise<string> {
  return await invoke<string>("generate_report", { path, full, selectedPlaylists });
}

export async function getPlaylistChartData(
  path: string,
  playlistName: string
): Promise<{ time: number; bitRate: number }[]> {
  return await invoke<{ time: number; bitRate: number }[]>("get_playlist_chart_data", {
    path,
    playlistName,
  });
}

export async function writeTextFile(file: string, text: string): Promise<void> {
  return await invoke<void>("write_text_file", { file, text });
}

export async function isMkvtoolnixFound(
  path: string,
  checkRunning: boolean = false
): Promise<Protocol.MkvToolNixStatus> {
  return await invoke<Protocol.MkvToolNixStatus>("is_mkvtoolnix_found", {
    path,
    checkRunning,
  });
}

export async function openPlaylistInMkvToolNixGui(
  discPath: string,
  playlistName: string
): Promise<void> {
  return await invoke<void>("open_playlist_in_mkvtoolnix_gui", {
    discPath,
    playlistName,
  });
}

export async function isBetterMediaInfoFound(
  path: string,
  checkRunning: boolean = false
): Promise<Protocol.BetterMediaInfoStatus> {
  return await invoke<Protocol.BetterMediaInfoStatus>("is_bettermediainfo_found", {
    path,
    checkRunning,
  });
}

export async function openPlaylistInBetterMediaInfo(
  discPath: string,
  playlistName: string
): Promise<void> {
  return await invoke<void>("open_playlist_in_bettermediainfo", {
    discPath,
    playlistName,
  });
}
