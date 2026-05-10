/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import * as Protocol from "./protocol";
import { formatSize } from "./format";

const DEFAULT_APP_VERSION = "0.1.0";

function line(out: string[], value = "") {
  out.push(value);
}

function padRight(value: unknown, width: number): string {
  const s = String(value ?? "");
  return s.length >= width ? s : s + " ".repeat(width - s.length);
}

function padLeft(value: unknown, width: number): string {
  const s = String(value ?? "");
  return s.length >= width ? s : " ".repeat(width - s.length) + s;
}

function formatThousands(value: number): string {
  const n = Math.max(0, Math.round(Number.isFinite(value) ? value : 0));
  return n.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function trimFractionZeros(value: string): string {
  return value.includes(".") ? value.replace(/\.?0+$/, "") : value;
}

function formatSecondsShort(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00:00";
  const total = Math.floor(seconds);
  const s = total % 60;
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}

function formatSecondsFull(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00:00.000";
  const total = Math.floor(seconds);
  const ms = Math.round((seconds - total) * 1000);
  const s = total % 60;
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  return `${h}:${m.toString().padStart(2, "0")}:${s
    .toString()
    .padStart(2, "0")}.${ms.toString().padStart(3, "0")}`;
}

function formatSeconds(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "00:00:00";
  const total = Math.floor(seconds);
  const ms = Math.floor((seconds - total) * 1000);
  const s = total % 60;
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  const base = `${h.toString().padStart(2, "0")}:${m
    .toString()
    .padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return ms > 0 ? `${base}.${ms.toString().padStart(3, "0")}` : base;
}

function format45k(length45k: number): string {
  return formatSeconds(length45k / 45000.0);
}

function effectiveBitrate(stream: Protocol.TSStreamInfo): number {
  return stream.bitRate > 0 ? stream.bitRate : stream.activeBitRate;
}

function playlistSize(playlist: Protocol.PlaylistInfo): number {
  return playlist.measuredSize > 0 ? playlist.measuredSize : playlist.fileSize;
}

function clipSize(clip: Protocol.PlaylistStreamClipInfo): number {
  return clip.measuredSize > 0 ? clip.measuredSize : clip.fileSize;
}

function streamCodecLongName(stream: Protocol.TSStreamInfo): string {
  return stream.codecName || stream.codecShortName;
}

function streamCodecShortName(stream: Protocol.TSStreamInfo): string {
  return stream.codecShortName || stream.codecName;
}

function discProtection(disc: Protocol.DiscInfo): string {
  if (disc.isBdPlus) return "BD+";
  if (disc.isUHD) return "AACS2";
  return "AACS";
}

function discExtras(disc: Protocol.DiscInfo): string[] {
  const extras: string[] = [];
  if (disc.isUHD) extras.push("Ultra HD");
  if (disc.isBdJava) extras.push("BD-Java");
  if (disc.is50Hz) extras.push("50Hz Content");
  if (disc.is3D) extras.push("Blu-ray 3D");
  if (disc.isDBOX) extras.push("D-BOX Motion Code");
  if (disc.isPSP) extras.push("PSP Digital Copy");
  return extras;
}

function writeDiscInfoBlock(
  out: string[],
  disc: Protocol.DiscInfo,
  protection: string,
  extras: string[],
  appVersion: string,
  labels: ReportLabels
) {
  if (disc.discTitle) line(out, `${padRight(`${labels.discTitle}:`, 16)}${disc.discTitle}`);
  line(out, `${padRight(`${labels.discLabel}:`, 16)}${disc.volumeLabel}`);
  line(out, `${padRight(`${labels.discSize}:`, 16)}${formatThousands(disc.size)} ${labels.bytes}`);
  line(out, `${padRight(`${labels.protection}:`, 16)}${protection}`);
  if (extras.length > 0) line(out, `${padRight(`${labels.extras}:`, 16)}${extras.join(", ")}`);
  line(out, `${padRight(`${labels.application}:`, 16)}BDMaster v${appVersion}`);
}

function printStreams(out: string[], streams: Protocol.TSStreamInfo[]) {
  for (const stream of streams) {
    const lang = stream.languageName
      ? ` [${stream.languageName}]`
      : stream.languageCode
        ? ` [${stream.languageCode}]`
        : "";
    const pid = `0x${stream.pid.toString(16).toUpperCase().padStart(4, "0")}`;
    line(
      out,
      `    ${pid}  ${padRight(streamCodecShortName(stream), 14)} ${stream.description}${lang}`
    );
  }
}

function formatKbps(rate: number, labels: ReportLabels = DEFAULT_REPORT_LABELS): string {
  return `${padLeft(formatThousands(Math.round(rate / 1000.0)), 6)} ${labels.kbps}`;
}

function formatAudioSummary(
  stream: Protocol.TSStreamInfo | undefined,
  labels: ReportLabels = DEFAULT_REPORT_LABELS
): [string, string] {
  if (!stream) return ["", ""];
  let out = `${streamCodecLongName(stream)} ${stream.channelLayout}`.trim();
  const bitrate = effectiveBitrate(stream);
  if (bitrate > 0) out += ` ${Math.round(bitrate / 1000.0)} ${labels.kbps}`;
  if (stream.sampleRate > 0 && stream.bitDepth > 0) {
    out += ` (${Math.round(stream.sampleRate / 1000.0)}kHz/${stream.bitDepth}-bit)`;
  }
  return [out, stream.languageCode];
}

function formatSecondaryAudio(
  streams: Protocol.TSStreamInfo[],
  primaryLanguage: string,
  labels: ReportLabels = DEFAULT_REPORT_LABELS
): string {
  for (const stream of streams.slice(1)) {
    if (stream.languageCode !== primaryLanguage) continue;
    const isSecondary = stream.streamType === 0xa1 || stream.streamType === 0xa2;
    const isStereoAc3 = stream.streamType === 0x81 && stream.channelCount === 2;
    if (isSecondary || isStereoAc3) continue;
    return formatAudioSummary(stream, labels)[0];
  }
  return "";
}

function writeChaptersTable(out: string[], playlist: Protocol.PlaylistInfo, labels: ReportLabels) {
  const totalLengthSeconds = playlist.totalLength / 45000.0;
  for (let i = 0; i < playlist.chapters.length; i += 1) {
    const chapterStart = playlist.chapters[i];
    const chapterEnd =
      i + 1 < playlist.chapters.length ? playlist.chapters[i + 1] : totalLengthSeconds;
    const chapterLength = Math.max(0, chapterEnd - chapterStart);
    const metrics = playlist.chapterMetrics?.[i];
    line(
      out,
      padRight(i + 1, 8) +
        padRight(formatSecondsFull(chapterStart), 16) +
        padRight(formatSecondsFull(chapterLength), 16) +
        padRight(formatKbps(metrics?.avgVideoRate ?? 0, labels), 16) +
        padRight(formatKbps(metrics?.max1SecRate ?? 0, labels), 16) +
        padRight(formatSecondsFull(metrics?.max1SecTime ?? 0), 16) +
        padRight(formatKbps(metrics?.max5SecRate ?? 0, labels), 16) +
        padRight(formatSecondsFull(metrics?.max5SecTime ?? 0), 16) +
        padRight(formatKbps(metrics?.max10SecRate ?? 0, labels), 16) +
        padRight(formatSecondsFull(metrics?.max10SecTime ?? 0), 16) +
        padRight(`      0 ${labels.bytes}`, 16) +
        padRight(`      0 ${labels.bytes}`, 16) +
        "0:00:00.000"
    );
  }
}

function selectedPlaylists(disc: Protocol.DiscInfo, playlistNames?: string[]): Protocol.PlaylistInfo[] {
  if (!playlistNames) return disc.playlists;
  const selected = new Set(playlistNames);
  return disc.playlists.filter((playlist) => selected.has(playlist.name));
}

export function generateQuickSummaryReport(
  disc: Protocol.DiscInfo,
  playlistNames: string[] | undefined,
  formatting: Protocol.ConfigFormatting | undefined,
  labelOverrides?: Partial<ReportLabels>
): string {
  const labels = createReportLabels(labelOverrides);
  const sizePrecision = formatting?.size.precision ?? Protocol.FormatPrecision.Two;
  const sizeUnit = formatting?.size.unit ?? Protocol.FormatUnit.KMGT;
  const out: string[] = [];
  const protection =
    `${disc.isUHD ? "UHD " : ""}${disc.is4K ? "4K " : ""}${disc.is3D ? "3D " : ""}` +
    `${disc.is50Hz ? "50Hz " : ""}${disc.isBdJava ? "BD-Java " : ""}` +
    `${disc.isBdPlus ? "BD+ " : ""}${disc.hasMVCExtension ? "MVC " : ""}`;

  line(out, `${labels.discInfo.toUpperCase()}:`);
  line(out, "----------");
  line(out, `  ${labels.discTitle}: ${disc.discTitle || labels.none}`);
  line(out, `  ${labels.discVolume}: ${disc.volumeLabel || labels.none}`);
  line(out, `  ${labels.discPath}: ${disc.path}`);
  line(out, `  ${labels.discSize}: ${disc.size} (${formatSize(disc.size, sizePrecision, sizeUnit)})`);
  line(out, `  ${labels.protection}: ${protection}`);
  line(out);

  line(out, `${labels.playlists.toUpperCase()} (${disc.playlists.length}):`);
  line(out, "--------------");
  for (const playlist of disc.playlists) {
    line(
      out,
      `  ${playlist.name}: ${playlist.streamClips.length} ${labels.clips}, ` +
        `${playlist.chapters.length} ${labels.chapters}, ` +
        `${labels.length}=${format45k(playlist.totalLength)}`
    );
  }
  line(out);

  for (const playlist of selectedPlaylists(disc, playlistNames)) {
    line(out, `${labels.playlist.toUpperCase()}: ${playlist.name}`);
    line(out, "----------");
    line(out, `  ${labels.length}: ${format45k(playlist.totalLength)}`);
    const size = playlistSize(playlist);
    line(out, `  ${labels.size}:   ${size} ${labels.bytes} (${formatSize(size, sizePrecision, sizeUnit)})`);
    line(out, `  ${labels.chapters}: ${playlist.chapters.length}`);
    if (playlist.videoStreams.length > 0) {
      line(out);
      line(out, `  ${labels.video.toUpperCase()}:`);
      printStreams(out, playlist.videoStreams);
    }
    if (playlist.audioStreams.length > 0) {
      line(out);
      line(out, `  ${labels.audio.toUpperCase()}:`);
      printStreams(out, playlist.audioStreams);
    }
    if (playlist.graphicsStreams.length > 0) {
      line(out);
      line(out, `  ${labels.subtitles.toUpperCase()}:`);
      printStreams(out, playlist.graphicsStreams);
    }
    if (playlist.textStreams.length > 0) {
      line(out);
      line(out, `  ${labels.text.toUpperCase()}:`);
      printStreams(out, playlist.textStreams);
    }
    line(out);
  }

  return out.join("\n");
}

export function generateFullReport(
  disc: Protocol.DiscInfo,
  playlistNames: string[] | undefined,
  appVersion = DEFAULT_APP_VERSION,
  labelOverrides?: Partial<ReportLabels>
): string {
  const labels = createReportLabels(labelOverrides);
  const out: string[] = [];
  const protection = discProtection(disc);
  const extras = discExtras(disc);

  writeDiscInfoBlock(out, disc, protection, extras, appVersion, labels);
  line(out);

  for (const playlist of selectedPlaylists(disc, playlistNames)) {
    writePlaylistFull(out, disc, playlist, protection, extras, appVersion, labels);
  }

  return out.join("\n");
}

function writePlaylistFull(
  out: string[],
  disc: Protocol.DiscInfo,
  playlist: Protocol.PlaylistInfo,
  protection: string,
  extras: string[],
  appVersion: string,
  labels: ReportLabels
) {
  const totalLengthSeconds = playlist.totalLength / 45000.0;
  const totalSize = playlistSize(playlist);
  const totalBitrateMbps =
    totalLengthSeconds > 0 ? (totalSize * 8.0) / totalLengthSeconds / 1_000_000.0 : 0.0;
  const videoCodec = playlist.videoStreams[0]?.codecShortName ?? "";
  const videoBitrateMbps = playlist.videoStreams[0]
    ? effectiveBitrate(playlist.videoStreams[0]) / 1_000_000.0
    : 0.0;
  const [audio1, languageCode1] = formatAudioSummary(playlist.audioStreams[0], labels);
  const audio2 = formatSecondaryAudio(playlist.audioStreams, languageCode1, labels);

  line(out);
  line(out, "********************");
  line(out, `${labels.playlist.toUpperCase()}: ${playlist.name}`);
  line(out, "********************");
  line(out);
  line(
    out,
    padRight("", 64) +
      padRight("", 8) +
      padRight("", 8) +
      padRight("", 16) +
      padRight("", 18) +
      padRight(labels.total, 13) +
      padRight(labels.video, 13) +
      padRight("", 42)
  );
  line(
    out,
    padRight(labels.title, 64) +
      padRight(labels.codec, 8) +
      padRight(labels.length, 8) +
      padRight(labels.movieSize, 16) +
      padRight(labels.discSize, 18) +
      padRight(labels.bitrate, 13) +
      padRight(labels.bitrate, 13) +
      padRight(labels.mainAudioTrack, 42) +
      labels.secondaryAudioTrack
  );
  line(
    out,
    padRight("-----", 64) +
      padRight("------", 8) +
      padRight("-------", 8) +
      padRight("--------------", 16) +
      padRight("----------------", 18) +
      padRight("-----------", 13) +
      padRight("-----------", 13) +
      padRight("------------------", 42) +
      "---------------------"
  );
  line(
    out,
    padRight(playlist.name, 64) +
      padRight(videoCodec, 8) +
      padRight(formatSecondsShort(totalLengthSeconds), 8) +
      padRight(formatThousands(totalSize), 16) +
      padRight(formatThousands(disc.size), 18) +
      padRight(`${totalBitrateMbps.toFixed(2)} ${labels.mbps}`, 13) +
      padRight(`${videoBitrateMbps.toFixed(2)} ${labels.mbps}`, 13) +
      padRight(audio1, 42) +
      audio2
  );
  line(out);
  line(out, `${labels.discInfo.toUpperCase()}:`);
  line(out);
  writeDiscInfoBlock(out, disc, protection, extras, appVersion, labels);

  line(out);
  line(out, `${labels.playlistReport.toUpperCase()}:`);
  line(out);
  line(out, `${padRight(`${labels.name}:`, 24)}${playlist.name}`);
  line(out, `${padRight(`${labels.length}:`, 24)}${formatSecondsFull(totalLengthSeconds)} (${labels.hmsMs})`);
  line(out, `${padRight(`${labels.size}:`, 24)}${formatThousands(totalSize)} ${labels.bytes}`);
  line(out, `${padRight(`${labels.totalBitrate}:`, 24)}${totalBitrateMbps.toFixed(2)} ${labels.mbps}`);

  writeStreamTable(out, `${labels.video.toUpperCase()}:`, playlist.videoStreams, "video", labels);
  writeStreamTable(out, `${labels.audio.toUpperCase()}:`, playlist.audioStreams, "audio", labels);
  writeStreamTable(out, `${labels.subtitles.toUpperCase()}:`, playlist.graphicsStreams, "subtitle", labels);
  writeStreamTable(out, `${labels.text.toUpperCase()}:`, playlist.textStreams, "subtitle", labels);

  line(out);
  line(out, `${labels.files.toUpperCase()}:`);
  line(out);
  line(
    out,
    padRight(labels.name, 16) +
      padRight(labels.timeIn, 16) +
      padRight(labels.length, 16) +
      padRight(labels.size, 16) +
      labels.totalBitrate
  );
  line(
    out,
    padRight("---------------", 16) +
      padRight("-------------", 16) +
      padRight("-------------", 16) +
      padRight("-------------", 16) +
      "-------------"
  );
  for (const clip of playlist.streamClips) {
    if (clip.angleIndex > 1) continue;
    const lengthSeconds = clip.length / 45000.0;
    const timeInSeconds = clip.relativeTimeIn / 45000.0;
    const size = clipSize(clip);
    const bitrateKbps =
      lengthSeconds > 0 ? Math.round((clip.fileSize * 8.0) / lengthSeconds / 1000.0) : 0;
    const displayName =
      clip.angleIndex > 0 ? `${clip.displayName} (${clip.angleIndex})` : clip.displayName;
    line(
      out,
      padRight(displayName, 16) +
        padRight(formatSecondsFull(timeInSeconds), 16) +
        padRight(formatSecondsFull(lengthSeconds), 16) +
        padRight(formatThousands(size), 16) +
        `${padLeft(formatThousands(bitrateKbps), 6)} ${labels.kbps}`
    );
  }

  if (playlist.chapters.length > 0) {
    line(out);
    line(out, `${labels.chapters.toUpperCase()}:`);
    line(out);
    line(
      out,
      padRight("#", 8) +
        padRight(labels.timeIn, 16) +
        padRight(labels.length, 16) +
        padRight(labels.avgVideoRate, 16) +
        padRight(labels.max1SecRate, 16) +
        padRight(labels.max1SecTime, 16) +
        padRight(labels.max5SecRate, 16) +
        padRight(labels.max5SecTime, 16) +
        padRight(labels.max10SecRate, 16) +
        padRight(labels.max10SecTime, 16) +
        padRight(labels.avgFrameSize, 16) +
        padRight(labels.maxFrameSize, 16) +
        labels.maxFrameTime
    );
    line(
      out,
      padRight("------", 8) +
        padRight("-------------", 16) +
        padRight("-------------", 16) +
        padRight("--------------", 16) +
        padRight("--------------", 16) +
        padRight("--------------", 16) +
        padRight("--------------", 16) +
        padRight("--------------", 16) +
        padRight("--------------", 16) +
        padRight("--------------", 16) +
        padRight("--------------", 16) +
        padRight("--------------", 16) +
        "--------------"
    );
    writeChaptersTable(out, playlist, labels);
  }
  line(out);
}

function writeStreamTable(
  out: string[],
  title: string,
  streams: Protocol.TSStreamInfo[],
  type: "video" | "audio" | "subtitle",
  labels: ReportLabels
) {
  if (streams.length === 0) return;
  line(out);
  line(out, title);
  line(out);
  if (type === "video") {
    line(out, `${padRight(labels.codec, 24)}${padRight(labels.bitrate, 20)}${labels.description}`);
    line(out, `${padRight("---------------", 24)}${padRight("-------------", 20)}-----------`);
    for (const stream of streams) {
      const bitrate = `${formatThousands(Math.round(effectiveBitrate(stream) / 1000.0))} ${labels.kbps}`;
      line(out, `${padRight(streamCodecLongName(stream), 24)}${padRight(bitrate, 20)}${stream.description}`);
    }
    return;
  }

  line(out, `${padRight(labels.codec, 32)}${padRight(labels.language, 16)}${padRight(labels.bitrate, 16)}${labels.description}`);
  line(
    out,
    `${padRight("---------------", 32)}${padRight("-------------", 16)}${padRight("-------------", 16)}-----------`
  );
  for (const stream of streams) {
    const bitrate =
      type === "audio"
        ? `${padLeft(Math.round(effectiveBitrate(stream) / 1000.0), 5)} ${labels.kbps}`
        : `${padLeft(trimFractionZeros((effectiveBitrate(stream) / 1000.0).toFixed(2)), 5)} ${labels.kbps}`;
    line(
      out,
      padRight(streamCodecLongName(stream), 32) +
        padRight(stream.languageName || stream.languageCode, 16) +
        padRight(bitrate, 16) +
        stream.description
    );
  }
}

export type ReportCellAlign = "left" | "right";

export interface ReportCell {
  value: string;
  align?: ReportCellAlign;
}

export interface ReportTable {
  title?: string;
  headers: string[];
  rows: ReportCell[][];
}

export interface ReportSection {
  title: string;
  tables: ReportTable[];
}

export interface ReportDocument {
  title: string;
  sections: ReportSection[];
}

export interface ReportLabels {
  name: string;
  value: string;
  quickSummary: string;
  fullReport: string;
  discInfo: string;
  discTitle: string;
  discVolume: string;
  discPath: string;
  discSize: string;
  discLabel: string;
  protection: string;
  extras: string;
  application: string;
  playlists: string;
  playlist: string;
  playlistReport: string;
  overview: string;
  streams: string;
  files: string;
  chapters: string;
  clips: string;
  length: string;
  size: string;
  video: string;
  audio: string;
  subtitles: string;
  text: string;
  pid: string;
  codec: string;
  description: string;
  language: string;
  bitrate: string;
  title: string;
  movieSize: string;
  totalBitrate: string;
  videoBitrate: string;
  mainAudioTrack: string;
  secondaryAudioTrack: string;
  timeIn: string;
  avgVideoRate: string;
  max1SecRate: string;
  max1SecTime: string;
  max5SecRate: string;
  max5SecTime: string;
  max10SecRate: string;
  max10SecTime: string;
  avgFrameSize: string;
  maxFrameSize: string;
  maxFrameTime: string;
  total: string;
  bytes: string;
  kbps: string;
  mbps: string;
  none: string;
  hmsMs: string;
}

const DEFAULT_REPORT_LABELS: ReportLabels = {
  name: "Name",
  value: "Value",
  quickSummary: "Quick Summary",
  fullReport: "Full Report",
  discInfo: "Disc Info",
  discTitle: "Disc Title",
  discVolume: "Disc Volume",
  discPath: "Disc Path",
  discSize: "Disc Size",
  discLabel: "Disc Label",
  protection: "Protection",
  extras: "Extras",
  application: "Application",
  playlists: "Playlists",
  playlist: "Playlist",
  playlistReport: "Playlist Report",
  overview: "Overview",
  streams: "Streams",
  files: "Files",
  chapters: "Chapters",
  clips: "Clips",
  length: "Length",
  size: "Size",
  video: "Video",
  audio: "Audio",
  subtitles: "Subtitles",
  text: "Text",
  pid: "PID",
  codec: "Codec",
  description: "Description",
  language: "Language",
  bitrate: "Bitrate",
  title: "Title",
  movieSize: "Movie Size",
  totalBitrate: "Total Bitrate",
  videoBitrate: "Video Bitrate",
  mainAudioTrack: "Main Audio Track",
  secondaryAudioTrack: "Secondary Audio Track",
  timeIn: "Time In",
  avgVideoRate: "Avg Video Rate",
  max1SecRate: "Max 1-Sec Rate",
  max1SecTime: "Max 1-Sec Time",
  max5SecRate: "Max 5-Sec Rate",
  max5SecTime: "Max 5-Sec Time",
  max10SecRate: "Max 10-Sec Rate",
  max10SecTime: "Max 10-Sec Time",
  avgFrameSize: "Avg Frame Size",
  maxFrameSize: "Max Frame Size",
  maxFrameTime: "Max Frame Time",
  total: "Total",
  bytes: "bytes",
  kbps: "kbps",
  mbps: "Mbps",
  none: "(none)",
  hmsMs: "h:m:s.ms",
};

export function createReportLabels(labels?: Partial<ReportLabels>): ReportLabels {
  return { ...DEFAULT_REPORT_LABELS, ...labels };
}

function reportCell(value: unknown, align: ReportCellAlign = "left"): ReportCell {
  return { value: String(value ?? ""), align };
}

function keyValueTable(rows: Array<[string, unknown]>, labels: ReportLabels): ReportTable {
  return {
    headers: [labels.name, labels.value],
    rows: rows.map(([name, value]) => [reportCell(name), reportCell(value)]),
  };
}

function streamLanguage(stream: Protocol.TSStreamInfo): string {
  return stream.languageName || stream.languageCode || "";
}

function streamReportTable(
  title: string,
  streams: Protocol.TSStreamInfo[],
  type: "summary" | "video" | "audio" | "subtitle",
  labels: ReportLabels
): ReportTable | null {
  if (streams.length === 0) return null;
  if (type === "summary") {
    return {
      title,
      headers: [labels.pid, labels.codec, labels.description, labels.language],
      rows: streams.map((stream) => [
        reportCell(`0x${stream.pid.toString(16).toUpperCase().padStart(4, "0")}`),
        reportCell(streamCodecShortName(stream)),
        reportCell(stream.description),
        reportCell(streamLanguage(stream)),
      ]),
    };
  }
  if (type === "video") {
    return {
      title,
      headers: [labels.codec, labels.bitrate, labels.description],
      rows: streams.map((stream) => [
        reportCell(streamCodecLongName(stream)),
        reportCell(`${formatThousands(Math.round(effectiveBitrate(stream) / 1000.0))} ${labels.kbps}`, "right"),
        reportCell(stream.description),
      ]),
    };
  }
  return {
    title,
    headers: [labels.codec, labels.language, labels.bitrate, labels.description],
    rows: streams.map((stream) => {
      const bitrate =
        type === "audio"
          ? `${Math.round(effectiveBitrate(stream) / 1000.0)} ${labels.kbps}`
          : `${trimFractionZeros((effectiveBitrate(stream) / 1000.0).toFixed(2))} ${labels.kbps}`;
      return [
        reportCell(streamCodecLongName(stream)),
        reportCell(streamLanguage(stream)),
        reportCell(bitrate, "right"),
        reportCell(stream.description),
      ];
    }),
  };
}

function addTableIfPresent(tables: ReportTable[], table: ReportTable | null) {
  if (table) tables.push(table);
}

function playlistLengthSeconds(playlist: Protocol.PlaylistInfo): number {
  return playlist.totalLength / 45000.0;
}

function playlistTotalBitrateMbps(playlist: Protocol.PlaylistInfo): number {
  const lengthSeconds = playlistLengthSeconds(playlist);
  return lengthSeconds > 0
    ? (playlist.fileSize * 8.0) / lengthSeconds / 1_000_000.0
    : 0.0;
}

function chapterReportTable(playlist: Protocol.PlaylistInfo, labels: ReportLabels): ReportTable | null {
  if (playlist.chapters.length === 0) return null;
  const totalLengthSeconds = playlistLengthSeconds(playlist);
  return {
    headers: [
      "#",
      labels.timeIn,
      labels.length,
      labels.avgVideoRate,
      labels.max1SecRate,
      labels.max1SecTime,
      labels.max5SecRate,
      labels.max5SecTime,
      labels.max10SecRate,
      labels.max10SecTime,
      labels.avgFrameSize,
      labels.maxFrameSize,
      labels.maxFrameTime,
    ],
    rows: playlist.chapters.map((chapterStart, i) => {
      const chapterEnd = i + 1 < playlist.chapters.length ? playlist.chapters[i + 1] : totalLengthSeconds;
      const chapterLength = Math.max(0, chapterEnd - chapterStart);
      const metrics = playlist.chapterMetrics?.[i];
      const hasMetrics = !!metrics && metrics.avgVideoRate > 0;
      return [
        reportCell(i + 1, "right"),
        reportCell(formatSecondsFull(chapterStart)),
        reportCell(formatSecondsFull(chapterLength)),
        reportCell(hasMetrics ? formatKbps(metrics.avgVideoRate, labels) : "", "right"),
        reportCell(hasMetrics ? formatKbps(metrics.max1SecRate, labels) : "", "right"),
        reportCell(hasMetrics ? formatSecondsFull(metrics.max1SecTime) : ""),
        reportCell(hasMetrics ? formatKbps(metrics.max5SecRate, labels) : "", "right"),
        reportCell(hasMetrics ? formatSecondsFull(metrics.max5SecTime) : ""),
        reportCell(hasMetrics ? formatKbps(metrics.max10SecRate, labels) : "", "right"),
        reportCell(hasMetrics ? formatSecondsFull(metrics.max10SecTime) : ""),
        reportCell(hasMetrics ? `${formatThousands(metrics.avgFrameSize)} ${labels.bytes}` : "", "right"),
        reportCell(hasMetrics ? `${formatThousands(metrics.maxFrameSize)} ${labels.bytes}` : "", "right"),
        reportCell(hasMetrics ? formatSecondsFull(metrics.maxFrameTime) : ""),
      ];
    }),
  };
}

function filesReportTable(playlist: Protocol.PlaylistInfo, labels: ReportLabels): ReportTable {
  return {
    headers: [labels.name, labels.timeIn, labels.length, labels.size, labels.totalBitrate],
    rows: playlist.streamClips
      .filter((clip) => clip.angleIndex <= 1)
      .map((clip) => {
        const lengthSeconds = clip.length / 45000.0;
        const size = clipSize(clip);
        const bitrateKbps =
          lengthSeconds > 0 ? Math.round((clip.fileSize * 8.0) / lengthSeconds / 1000.0) : 0;
        const displayName = clip.angleIndex > 0 ? `${clip.displayName} (${clip.angleIndex})` : clip.displayName;
        return [
          reportCell(displayName),
          reportCell(formatSecondsFull(clip.relativeTimeIn / 45000.0)),
          reportCell(formatSecondsFull(lengthSeconds)),
          reportCell(`${formatThousands(size)} ${labels.bytes}`, "right"),
          reportCell(`${formatThousands(bitrateKbps)} ${labels.kbps}`, "right"),
        ];
      }),
  };
}

export function generateQuickSummaryReportDocument(
  disc: Protocol.DiscInfo,
  playlistNames: string[] | undefined,
  formatting: Protocol.ConfigFormatting | undefined,
  labelOverrides?: Partial<ReportLabels>
): ReportDocument {
  const labels = createReportLabels(labelOverrides);
  const sizePrecision = formatting?.size.precision ?? Protocol.FormatPrecision.Two;
  const sizeUnit = formatting?.size.unit ?? Protocol.FormatUnit.KMGT;
  const protection =
    `${disc.isUHD ? "UHD " : ""}${disc.is4K ? "4K " : ""}${disc.is3D ? "3D " : ""}` +
    `${disc.is50Hz ? "50Hz " : ""}${disc.isBdJava ? "BD-Java " : ""}` +
    `${disc.isBdPlus ? "BD+ " : ""}${disc.hasMVCExtension ? "MVC " : ""}`;
  const sections: ReportSection[] = [
    {
      title: labels.discInfo,
      tables: [
        keyValueTable([
          [labels.discTitle, disc.discTitle || labels.none],
          [labels.discVolume, disc.volumeLabel || labels.none],
          [labels.discPath, disc.path],
          [labels.discSize, `${disc.size} (${formatSize(disc.size, sizePrecision, sizeUnit)})`],
          [labels.protection, protection],
        ], labels),
      ],
    },
    {
      title: `${labels.playlists} (${disc.playlists.length})`,
      tables: [
        {
          headers: [labels.playlist, labels.clips, labels.chapters, labels.length],
          rows: disc.playlists.map((playlist) => [
            reportCell(playlist.name),
            reportCell(playlist.streamClips.length, "right"),
            reportCell(playlist.chapters.length, "right"),
            reportCell(format45k(playlist.totalLength)),
          ]),
        },
      ],
    },
  ];

  for (const playlist of selectedPlaylists(disc, playlistNames)) {
    const tables: ReportTable[] = [
      keyValueTable([
        [labels.length, format45k(playlist.totalLength)],
        [labels.size, `${playlistSize(playlist)} ${labels.bytes} (${formatSize(playlistSize(playlist), sizePrecision, sizeUnit)})`],
        [labels.chapters, playlist.chapters.length],
      ], labels),
    ];
    addTableIfPresent(tables, streamReportTable(labels.video, playlist.videoStreams, "summary", labels));
    addTableIfPresent(tables, streamReportTable(labels.audio, playlist.audioStreams, "summary", labels));
    addTableIfPresent(tables, streamReportTable(labels.subtitles, playlist.graphicsStreams, "summary", labels));
    addTableIfPresent(tables, streamReportTable(labels.text, playlist.textStreams, "summary", labels));
    sections.push({ title: `${labels.playlist}: ${playlist.name}`, tables });
  }

  return { title: labels.quickSummary, sections };
}

export function generateFullReportDocument(
  disc: Protocol.DiscInfo,
  playlistNames: string[] | undefined,
  appVersion = DEFAULT_APP_VERSION,
  labelOverrides?: Partial<ReportLabels>
): ReportDocument {
  const labels = createReportLabels(labelOverrides);
  const protection = discProtection(disc);
  const extras = discExtras(disc);
  const sections: ReportSection[] = [
    {
      title: labels.discInfo,
      tables: [
        keyValueTable([
          ...(disc.discTitle ? [[labels.discTitle, disc.discTitle] as [string, unknown]] : []),
          [labels.discLabel, disc.volumeLabel],
          [labels.discSize, `${formatThousands(disc.size)} ${labels.bytes}`],
          [labels.protection, protection],
          ...(extras.length > 0 ? [[labels.extras, extras.join(", ")] as [string, unknown]] : []),
          [labels.application, `BDMaster v${appVersion}`],
        ], labels),
      ],
    },
  ];

  for (const playlist of selectedPlaylists(disc, playlistNames)) {
    const totalLengthSeconds = playlistLengthSeconds(playlist);
    const totalSize = playlistSize(playlist);
    const totalBitrateMbps = playlistTotalBitrateMbps(playlist);
    const videoBitrateMbps = playlist.videoStreams[0]
      ? effectiveBitrate(playlist.videoStreams[0]) / 1_000_000.0
      : 0.0;
    const [audio1, languageCode1] = formatAudioSummary(playlist.audioStreams[0], labels);
    const audio2 = formatSecondaryAudio(playlist.audioStreams, languageCode1, labels);

    sections.push({
      title: `${labels.overview}: ${playlist.name}`,
      tables: [
        {
          headers: [
            labels.title,
            labels.codec,
            labels.length,
            labels.movieSize,
            labels.discSize,
            labels.totalBitrate,
            labels.videoBitrate,
            labels.mainAudioTrack,
            labels.secondaryAudioTrack,
          ],
          rows: [
            [
              reportCell(playlist.name),
              reportCell(playlist.videoStreams[0]?.codecShortName ?? ""),
              reportCell(formatSecondsShort(totalLengthSeconds)),
              reportCell(formatThousands(totalSize), "right"),
              reportCell(formatThousands(disc.size), "right"),
              reportCell(`${totalBitrateMbps.toFixed(2)} ${labels.mbps}`, "right"),
              reportCell(`${videoBitrateMbps.toFixed(2)} ${labels.mbps}`, "right"),
              reportCell(audio1),
              reportCell(audio2),
            ],
          ],
        },
      ],
    });

    sections.push({
      title: `${labels.playlistReport}: ${playlist.name}`,
      tables: [
        keyValueTable([
          [labels.name, playlist.name],
          [labels.length, `${formatSecondsFull(totalLengthSeconds)} (${labels.hmsMs})`],
          [labels.size, `${formatThousands(totalSize)} ${labels.bytes}`],
          [labels.totalBitrate, `${totalBitrateMbps.toFixed(2)} ${labels.mbps}`],
        ], labels),
      ],
    });

    const streamTables: ReportTable[] = [];
    addTableIfPresent(streamTables, streamReportTable(labels.video, playlist.videoStreams, "video", labels));
    addTableIfPresent(streamTables, streamReportTable(labels.audio, playlist.audioStreams, "audio", labels));
    addTableIfPresent(streamTables, streamReportTable(labels.subtitles, playlist.graphicsStreams, "subtitle", labels));
    addTableIfPresent(streamTables, streamReportTable(labels.text, playlist.textStreams, "subtitle", labels));
    if (streamTables.length > 0) sections.push({ title: `${labels.streams}: ${playlist.name}`, tables: streamTables });

    sections.push({ title: `${labels.files}: ${playlist.name}`, tables: [filesReportTable(playlist, labels)] });
    const chapters = chapterReportTable(playlist, labels);
    if (chapters) sections.push({ title: `${labels.chapters}: ${playlist.name}`, tables: [chapters] });
  }

  return { title: labels.fullReport, sections };
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function generateReportDocumentHtml(document: ReportDocument): string {
  const sections = document.sections
    .map((section, sectionIndex) => {
      const tables = section.tables
        .map((table) => {
          const title = table.title ? `<h3>${escapeHtml(table.title)}</h3>` : "";
          const headers = table.headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("");
          const rows = table.rows
            .map((row) => {
              const cells = row
                .map((cell) => {
                  const align = cell.align === "right" ? ' class="num"' : "";
                  return `<td${align}>${escapeHtml(cell.value)}</td>`;
                })
                .join("");
              return `<tr>${cells}</tr>`;
            })
            .join("");
          return `${title}<table><thead><tr>${headers}</tr></thead><tbody>${rows}</tbody></table>`;
        })
        .join("\n");
      return `<details${sectionIndex === 0 ? " open" : ""}><summary>${escapeHtml(section.title)}</summary>${tables}</details>`;
    })
    .join("\n");

  return `<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>${escapeHtml(document.title)}</title>
<style>
body{font-family:Arial,sans-serif;margin:24px;color:#1f2937;background:#fff;}
h1{font-size:24px;margin:0 0 16px;}
details{border:1px solid #d6dbe1;border-radius:8px;margin:10px 0;background:#fff;overflow:hidden;}
summary{cursor:pointer;font-weight:700;padding:10px 12px;background:#f5f7fa;}
h3{font-size:15px;margin:14px 12px 8px;}
table{border-collapse:collapse;width:calc(100% - 24px);margin:8px 12px 14px;font-size:13px;}
th,td{border:1px solid #d6dbe1;padding:6px 8px;text-align:left;vertical-align:top;}
th{background:#eef2f6;}
td.num{text-align:right;white-space:nowrap;}
</style>
</head>
<body>
<h1>${escapeHtml(document.title)}</h1>
${sections}
</body>
</html>`;
}
