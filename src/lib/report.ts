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
  appVersion: string
) {
  if (disc.discTitle) line(out, `${padRight("Disc Title:", 16)}${disc.discTitle}`);
  line(out, `${padRight("Disc Label:", 16)}${disc.volumeLabel}`);
  line(out, `${padRight("Disc Size:", 16)}${formatThousands(disc.size)} bytes`);
  line(out, `${padRight("Protection:", 16)}${protection}`);
  if (extras.length > 0) line(out, `${padRight("Extras:", 16)}${extras.join(", ")}`);
  line(out, `${padRight("BDInfo:", 16)}BDMaster v${appVersion}`);
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

function formatKbps(rate: number): string {
  return `${padLeft(formatThousands(Math.round(rate / 1000.0)), 6)} kbps`;
}

function formatAudioSummary(stream: Protocol.TSStreamInfo | undefined): [string, string] {
  if (!stream) return ["", ""];
  let out = `${streamCodecLongName(stream)} ${stream.channelLayout}`.trim();
  const bitrate = effectiveBitrate(stream);
  if (bitrate > 0) out += ` ${Math.round(bitrate / 1000.0)} kbps`;
  if (stream.sampleRate > 0 && stream.bitDepth > 0) {
    out += ` (${Math.round(stream.sampleRate / 1000.0)}kHz/${stream.bitDepth}-bit)`;
  }
  return [out, stream.languageCode];
}

function formatSecondaryAudio(streams: Protocol.TSStreamInfo[], primaryLanguage: string): string {
  for (const stream of streams.slice(1)) {
    if (stream.languageCode !== primaryLanguage) continue;
    const isSecondary = stream.streamType === 0xa1 || stream.streamType === 0xa2;
    const isStereoAc3 = stream.streamType === 0x81 && stream.channelCount === 2;
    if (isSecondary || isStereoAc3) continue;
    return formatAudioSummary(stream)[0];
  }
  return "";
}

function writeChaptersTable(out: string[], playlist: Protocol.PlaylistInfo) {
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
        padRight(formatKbps(metrics?.avgVideoRate ?? 0), 16) +
        padRight(formatKbps(metrics?.max1SecRate ?? 0), 16) +
        padRight(formatSecondsFull(metrics?.max1SecTime ?? 0), 16) +
        padRight(formatKbps(metrics?.max5SecRate ?? 0), 16) +
        padRight(formatSecondsFull(metrics?.max5SecTime ?? 0), 16) +
        padRight(formatKbps(metrics?.max10SecRate ?? 0), 16) +
        padRight(formatSecondsFull(metrics?.max10SecTime ?? 0), 16) +
        padRight("      0 bytes", 16) +
        padRight("      0 bytes", 16) +
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
  formatting: Protocol.ConfigFormatting | undefined
): string {
  const sizePrecision = formatting?.size.precision ?? Protocol.FormatPrecision.Two;
  const sizeUnit = formatting?.size.unit ?? Protocol.FormatUnit.KMGT;
  const out: string[] = [];
  const protection =
    `${disc.isUHD ? "UHD " : ""}${disc.is4K ? "4K " : ""}${disc.is3D ? "3D " : ""}` +
    `${disc.is50Hz ? "50Hz " : ""}${disc.isBdJava ? "BD-Java " : ""}` +
    `${disc.isBdPlus ? "BD+ " : ""}${disc.hasMVCExtension ? "MVC " : ""}`;

  line(out, "DISC INFO:");
  line(out, "----------");
  line(out, `  Disc Title: ${disc.discTitle || "(none)"}`);
  line(out, `  Disc Volume: ${disc.volumeLabel || "(none)"}`);
  line(out, `  Disc Path: ${disc.path}`);
  line(out, `  Disc Size: ${disc.size} (${formatSize(disc.size, sizePrecision, sizeUnit)})`);
  line(out, `  Protection: ${protection}`);
  line(out);

  line(out, `PLAYLISTS (${disc.playlists.length}):`);
  line(out, "--------------");
  for (const playlist of disc.playlists) {
    line(
      out,
      `  ${playlist.name}: ${playlist.streamClips.length} clips, ${playlist.chapters.length} chapters, length=${format45k(playlist.totalLength)}`
    );
  }
  line(out);

  for (const playlist of selectedPlaylists(disc, playlistNames)) {
    line(out, `PLAYLIST: ${playlist.name}`);
    line(out, "----------");
    line(out, `  Length: ${format45k(playlist.totalLength)}`);
    const size = playlistSize(playlist);
    line(out, `  Size:   ${size} bytes (${formatSize(size, sizePrecision, sizeUnit)})`);
    line(out, `  Chapters: ${playlist.chapters.length}`);
    if (playlist.videoStreams.length > 0) {
      line(out);
      line(out, "  VIDEO:");
      printStreams(out, playlist.videoStreams);
    }
    if (playlist.audioStreams.length > 0) {
      line(out);
      line(out, "  AUDIO:");
      printStreams(out, playlist.audioStreams);
    }
    if (playlist.graphicsStreams.length > 0) {
      line(out);
      line(out, "  SUBTITLES:");
      printStreams(out, playlist.graphicsStreams);
    }
    if (playlist.textStreams.length > 0) {
      line(out);
      line(out, "  TEXT:");
      printStreams(out, playlist.textStreams);
    }
    line(out);
  }

  return out.join("\n");
}

export function generateFullReport(
  disc: Protocol.DiscInfo,
  playlistNames: string[] | undefined,
  appVersion = DEFAULT_APP_VERSION
): string {
  const out: string[] = [];
  const protection = discProtection(disc);
  const extras = discExtras(disc);

  writeDiscInfoBlock(out, disc, protection, extras, appVersion);
  line(out);

  for (const playlist of selectedPlaylists(disc, playlistNames)) {
    writePlaylistFull(out, disc, playlist, protection, extras, appVersion);
  }

  return out.join("\n");
}

function writePlaylistFull(
  out: string[],
  disc: Protocol.DiscInfo,
  playlist: Protocol.PlaylistInfo,
  protection: string,
  extras: string[],
  appVersion: string
) {
  const totalLengthSeconds = playlist.totalLength / 45000.0;
  const totalSize = playlistSize(playlist);
  const totalBitrateMbps =
    totalLengthSeconds > 0 ? (totalSize * 8.0) / totalLengthSeconds / 1_000_000.0 : 0.0;
  const videoCodec = playlist.videoStreams[0]?.codecShortName ?? "";
  const videoBitrateMbps = playlist.videoStreams[0]
    ? effectiveBitrate(playlist.videoStreams[0]) / 1_000_000.0
    : 0.0;
  const [audio1, languageCode1] = formatAudioSummary(playlist.audioStreams[0]);
  const audio2 = formatSecondaryAudio(playlist.audioStreams, languageCode1);

  line(out);
  line(out, "********************");
  line(out, `PLAYLIST: ${playlist.name}`);
  line(out, "********************");
  line(out);
  line(
    out,
    padRight("", 64) +
      padRight("", 8) +
      padRight("", 8) +
      padRight("", 16) +
      padRight("", 18) +
      padRight("Total", 13) +
      padRight("Video", 13) +
      padRight("", 42)
  );
  line(
    out,
    padRight("Title", 64) +
      padRight("Codec", 8) +
      padRight("Length", 8) +
      padRight("Movie Size", 16) +
      padRight("Disc Size", 18) +
      padRight("Bitrate", 13) +
      padRight("Bitrate", 13) +
      padRight("Main Audio Track", 42) +
      "Secondary Audio Track"
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
      padRight(`${totalBitrateMbps.toFixed(2)} Mbps`, 13) +
      padRight(`${videoBitrateMbps.toFixed(2)} Mbps`, 13) +
      padRight(audio1, 42) +
      audio2
  );
  line(out);
  line(out, "DISC INFO:");
  line(out);
  writeDiscInfoBlock(out, disc, protection, extras, appVersion);

  line(out);
  line(out, "PLAYLIST REPORT:");
  line(out);
  line(out, `${padRight("Name:", 24)}${playlist.name}`);
  line(out, `${padRight("Length:", 24)}${formatSecondsFull(totalLengthSeconds)} (h:m:s.ms)`);
  line(out, `${padRight("Size:", 24)}${formatThousands(totalSize)} bytes`);
  line(out, `${padRight("Total Bitrate:", 24)}${totalBitrateMbps.toFixed(2)} Mbps`);

  writeStreamTable(out, "VIDEO:", playlist.videoStreams, "video");
  writeStreamTable(out, "AUDIO:", playlist.audioStreams, "audio");
  writeStreamTable(out, "SUBTITLES:", playlist.graphicsStreams, "subtitle");
  writeStreamTable(out, "TEXT:", playlist.textStreams, "subtitle");

  line(out);
  line(out, "FILES:");
  line(out);
  line(
    out,
    padRight("Name", 16) +
      padRight("Time In", 16) +
      padRight("Length", 16) +
      padRight("Size", 16) +
      "Total Bitrate"
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
    const bitrateKbps = lengthSeconds > 0 ? Math.round((size * 8.0) / lengthSeconds / 1000.0) : 0;
    const displayName =
      clip.angleIndex > 0 ? `${clip.displayName} (${clip.angleIndex})` : clip.displayName;
    line(
      out,
      padRight(displayName, 16) +
        padRight(formatSecondsFull(timeInSeconds), 16) +
        padRight(formatSecondsFull(lengthSeconds), 16) +
        padRight(formatThousands(size), 16) +
        `${padLeft(formatThousands(bitrateKbps), 6)} kbps`
    );
  }

  if (playlist.chapters.length > 0) {
    line(out);
    line(out, "CHAPTERS:");
    line(out);
    line(
      out,
      padRight("Number", 8) +
        padRight("Time In", 16) +
        padRight("Length", 16) +
        padRight("Avg Video Rate", 16) +
        padRight("Max 1-Sec Rate", 16) +
        padRight("Max 1-Sec Time", 16) +
        padRight("Max 5-Sec Rate", 16) +
        padRight("Max 5-Sec Time", 16) +
        padRight("Max 10Sec Rate", 16) +
        padRight("Max 10Sec Time", 16) +
        padRight("Avg Frame Size", 16) +
        padRight("Max Frame Size", 16) +
        "Max Frame Time"
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
    writeChaptersTable(out, playlist);
  }
  line(out);
}

function writeStreamTable(
  out: string[],
  title: string,
  streams: Protocol.TSStreamInfo[],
  type: "video" | "audio" | "subtitle"
) {
  if (streams.length === 0) return;
  line(out);
  line(out, title);
  line(out);
  if (type === "video") {
    line(out, `${padRight("Codec", 24)}${padRight("Bitrate", 20)}Description`);
    line(out, `${padRight("---------------", 24)}${padRight("-------------", 20)}-----------`);
    for (const stream of streams) {
      const bitrate = `${formatThousands(Math.round(effectiveBitrate(stream) / 1000.0))} kbps`;
      line(out, `${padRight(streamCodecLongName(stream), 24)}${padRight(bitrate, 20)}${stream.description}`);
    }
    return;
  }

  line(out, `${padRight("Codec", 32)}${padRight("Language", 16)}${padRight("Bitrate", 16)}Description`);
  line(
    out,
    `${padRight("---------------", 32)}${padRight("-------------", 16)}${padRight("-------------", 16)}-----------`
  );
  for (const stream of streams) {
    const bitrate =
      type === "audio"
        ? `${padLeft(Math.round(effectiveBitrate(stream) / 1000.0), 5)} kbps`
        : `${padLeft(trimFractionZeros((effectiveBitrate(stream) / 1000.0).toFixed(2)), 5)} kbps`;
    line(
      out,
      padRight(streamCodecLongName(stream), 32) +
        padRight(stream.languageName || stream.languageCode, 16) +
        padRight(bitrate, 16) +
        stream.description
    );
  }
}
