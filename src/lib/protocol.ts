/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 *
 *   Licensed under the Apache License, Version 2.0 (the "License");
 *   you may not use this file except in compliance with the License.
 *   You may obtain a copy of the License at
 *
 *   http://www.apache.org/licenses/LICENSE-2.0
 */

export interface About {
  appVersion: string;
}

export enum Language {
  De = "de",
  EnUS = "en-US",
  Es = "es",
  Fr = "fr",
  Ja = "ja",
  ZhCN = "zh-CN",
  ZhHK = "zh-HK",
  ZhTW = "zh-TW",
}

export enum DisplayMode {
  Auto = "Auto",
  Light = "Light",
  Dark = "Dark",
}

export enum Theme {
  Ocean = "Ocean",
  Aqua = "Aqua",
  Sky = "Sky",
  Arctic = "Arctic",
  Glacier = "Glacier",
  Mist = "Mist",
  Slate = "Slate",
  Charcoal = "Charcoal",
  Midnight = "Midnight",
  Indigo = "Indigo",
  Violet = "Violet",
  Lavender = "Lavender",
  Rose = "Rose",
  Blush = "Blush",
  Coral = "Coral",
  Sunset = "Sunset",
  Amber = "Amber",
  Sand = "Sand",
  Forest = "Forest",
  Emerald = "Emerald",
}

export enum UpdateCheckInterval {
  Daily = "Daily",
  Weekly = "Weekly",
  Monthly = "Monthly",
}

export interface ConfigUpdate {
  checkInterval: UpdateCheckInterval;
  lastChecked: number;
  lastVersion: string;
  ignoreVersion: string;
}

export interface ConfigMkv {
  mkvToolNixPath: string;
}

export interface MkvToolNixStatus {
  found: boolean;
  mkvToolNixPath: string;
}

export interface ConfigBetterMediaInfo {
  path: string;
}

export interface BetterMediaInfoStatus {
  found: boolean;
  path: string;
}

export interface ConfigWindowPosition {
  x: number;
  y: number;
}

export interface ConfigWindowSize {
  width: number;
  height: number;
}

export interface ConfigWindow {
  position: ConfigWindowPosition;
  size: ConfigWindowSize;
}

export interface ConfigScan {
  generateStreamDiagnostics: boolean;
  enableSsifSupport: boolean;
  filterLoopingPlaylists: boolean;
  filterShortPlaylists: boolean;
  filterShortPlaylistsValue: number;
  useImagePrefix: boolean;
  useImagePrefixValue: string;
  keepStreamOrder: boolean;
  generateTextSummary: boolean;
  autosaveReport: boolean;
  displayChapterCount: boolean;
  enableExtendedStreamDiagnostics: boolean;
}

export interface Config {
  appendOnFileDrop: boolean;
  displayMode: DisplayMode;
  theme: Theme;
  language: Language;
  scan: ConfigScan;
  formatting: ConfigFormatting;
  discInfoSplit: number;
  infoPanelSplit: number;
  update: ConfigUpdate;
  mkv: ConfigMkv;
  betterMediaInfo: ConfigBetterMediaInfo;
  window: ConfigWindow;
}

export enum FormatPrecision {
  Zero = "Zero",
  One = "One",
  Two = "Two",
}

export enum FormatUnit {
  K = "K",
  KM = "KM",
  KMG = "KMG",
  KMGT = "KMGT",
  KMi = "KMi",
  KMiGi = "KMiGi",
  KMiGiTi = "KMiGiTi",
}

export interface ConfigBitRate {
  precision: FormatPrecision;
  unit: FormatUnit;
}

export interface ConfigSize {
  precision: FormatPrecision;
  unit: FormatUnit;
}

export interface ConfigFormatting {
  bitRate: ConfigBitRate;
  size: ConfigSize;
}

export function getFormatPrecisions(): FormatPrecision[] {
  return [FormatPrecision.Zero, FormatPrecision.One, FormatPrecision.Two];
}

export function getFormatUnits(): FormatUnit[] {
  return [
    FormatUnit.K,
    FormatUnit.KM,
    FormatUnit.KMG,
    FormatUnit.KMGT,
    FormatUnit.KMi,
    FormatUnit.KMiGi,
    FormatUnit.KMiGiTi,
  ];
}

export function getFormatPrecisionLabel(p: FormatPrecision): string {
  switch (p) {
    case FormatPrecision.Zero: return "#";
    case FormatPrecision.One: return "#.#";
    case FormatPrecision.Two: return "#.##";
  }
}

export function getFormatUnitLabel(u: FormatUnit): string {
  switch (u) {
    case FormatUnit.K: return "k";
    case FormatUnit.KM: return "k/M";
    case FormatUnit.KMG: return "k/M/G";
    case FormatUnit.KMGT: return "k/M/G/T";
    case FormatUnit.KMi: return "k/Mi";
    case FormatUnit.KMiGi: return "k/Mi/Gi";
    case FormatUnit.KMiGiTi: return "k/Mi/Gi/Ti";
  }
}

export enum ControlStatus {
  Hidden,
  Selected,
  Visible,
}

export interface DialogNotification {
  title: string;
  type: DialogNotificationType;
}

export enum DialogNotificationType {
  Info,
  Error,
}

export enum TabType {
  About,
  Config,
  DiscInfo,
  Chapters,
  QuickSummary,
  FullReport,
  BitRate,
}

export interface UpdateCheckResult {
  hasUpdate: boolean;
  latestVersion: string | null;
}

// ---------------- Disc data model ----------------

export interface DiscInfo {
  path: string;
  discName: string;
  discTitle: string;
  volumeLabel: string;
  size: number;
  isBdPlus: boolean;
  isBdJava: boolean;
  is3D: boolean;
  is4K: boolean;
  is50Hz: boolean;
  isDBOX: boolean;
  isPSP: boolean;
  isUHD: boolean;
  hasMVCExtension: boolean;
  hasHEVCStreams: boolean;
  hasUHDDiscMarker: boolean;
  metaTitle: string | null;
  metaDiscNumber: number | null;
  fileSetIdentifier: string | null;
  playlists: PlaylistInfo[];
  streamFiles: StreamFileInfo[];
  streamClipFiles: StreamClipFileInfo[];
}

export interface PlaylistInfo {
  name: string;
  groupIndex: number;
  fileSize: number;
  measuredSize: number;
  totalLength: number;
  hasHiddenTracks: boolean;
  hasLoops: boolean;
  isCustom: boolean;
  chapters: number[];
  chapterMetrics: ChapterMetricsInfo[];
  bitrateSamples: ChartSample[];
  streamClips: PlaylistStreamClipInfo[];
  videoStreams: TSStreamInfo[];
  audioStreams: TSStreamInfo[];
  graphicsStreams: TSStreamInfo[];
  textStreams: TSStreamInfo[];
  totalAngles: number;
}

export interface ChapterMetricsInfo {
  avgVideoRate: number;
  max1SecRate: number;
  max1SecTime: number;
  max5SecRate: number;
  max5SecTime: number;
  max10SecRate: number;
  max10SecTime: number;
  avgFrameSize: number;
  maxFrameSize: number;
  maxFrameTime: number;
}

export interface PlaylistStreamClipInfo {
  name: string;
  timeIn: number;
  timeOut: number;
  relativeTimeIn: number;
  relativeTimeOut: number;
  length: number;
  fileSize: number;
  measuredSize: number;
  interleavedFileSize: number;
  angleIndex: number;
}

export interface ChartSample {
  time: number;
  bitRate: number;
}

export interface StreamFileInfo {
  name: string;
  size: number;
  duration: number;
  interleaved: boolean;
}

export interface StreamClipFileInfo {
  name: string;
  size: number;
}

export interface TSStreamInfo {
  pid: number;
  streamType: number;
  streamTypeText: string;
  codecName: string;
  codecShortName: string;
  description: string;
  bitRate: number;
  activeBitRate: number;
  measuredSize: number;
  isVideoStream: boolean;
  isAudioStream: boolean;
  isGraphicsStream: boolean;
  isTextStream: boolean;
  isInitialized: boolean;
  isHidden: boolean;
  // Video
  width: number;
  height: number;
  framerate: string;
  aspectRatio: string;
  videoFormat: string;
  isInterlaced: boolean;
  // Audio
  channelCount: number;
  lfe: number;
  sampleRate: number;
  bitDepth: number;
  channelLayout: string;
  audioMode: string;
  // Subtitle / language
  languageCode: string;
  languageName: string;
}

export interface ScanProgress {
  path: string;
  totalBytes: number;
  finishedBytes: number;
  isRunning: boolean;
  isCompleted: boolean;
  isCancelled: boolean;
  error: string | null;
  currentFile: string | null;
  startedAtMs: number;
  disc: DiscInfo | null;
  version: number;
}

export function getLanguages(): Language[] {
  return [
    Language.De,
    Language.EnUS,
    Language.Es,
    Language.Fr,
    Language.Ja,
    Language.ZhCN,
    Language.ZhHK,
    Language.ZhTW,
  ];
}

export function getLanguageLabel(language: Language): string {
  switch (language) {
    case Language.De: return "Deutsch";
    case Language.EnUS: return "English (US)";
    case Language.Es: return "Español";
    case Language.Fr: return "Français";
    case Language.Ja: return "日本語";
    case Language.ZhCN: return "简体中文";
    case Language.ZhHK: return "繁體中文 (香港)";
    case Language.ZhTW: return "繁體中文 (臺灣)";
  }
}

export function getDisplayModes(): DisplayMode[] {
  return [DisplayMode.Auto, DisplayMode.Light, DisplayMode.Dark];
}

export function getThemes(): Theme[] {
  return [
    Theme.Ocean, Theme.Aqua, Theme.Sky, Theme.Arctic, Theme.Glacier,
    Theme.Mist, Theme.Slate, Theme.Charcoal, Theme.Midnight, Theme.Indigo,
    Theme.Violet, Theme.Lavender, Theme.Rose, Theme.Blush, Theme.Coral,
    Theme.Sunset, Theme.Amber, Theme.Sand, Theme.Forest, Theme.Emerald,
  ];
}
