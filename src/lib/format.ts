/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import * as Protocol from "./protocol";

export function shrinkFileName(fileName: string, maxLength: number): string {
  if (fileName.length > maxLength) {
    return "..." + fileName.substring(fileName.length - maxLength + 3);
  }
  return fileName;
}

interface FormatTier {
  divisor: number;
  label: string;
}

function precisionToDecimalPlaces(p: Protocol.FormatPrecision): number {
  switch (p) {
    case Protocol.FormatPrecision.Zero: return 0;
    case Protocol.FormatPrecision.One: return 1;
    case Protocol.FormatPrecision.Two: return 2;
  }
}

function unitToTiers(u: Protocol.FormatUnit): FormatTier[] {
  switch (u) {
    case Protocol.FormatUnit.K:
      return [{ divisor: 1024, label: "K" }];
    case Protocol.FormatUnit.KM:
      return [
        { divisor: 1024, label: "K" },
        { divisor: 1048576, label: "M" },
      ];
    case Protocol.FormatUnit.KMG:
      return [
        { divisor: 1024, label: "K" },
        { divisor: 1048576, label: "M" },
        { divisor: 1073741824, label: "G" },
      ];
    case Protocol.FormatUnit.KMGT:
      return [
        { divisor: 1024, label: "K" },
        { divisor: 1048576, label: "M" },
        { divisor: 1073741824, label: "G" },
        { divisor: 1099511627776, label: "T" },
      ];
    case Protocol.FormatUnit.KMi:
      return [
        { divisor: 1e3, label: "Ki" },
        { divisor: 1e6, label: "Mi" },
      ];
    case Protocol.FormatUnit.KMiGi:
      return [
        { divisor: 1e3, label: "Ki" },
        { divisor: 1e6, label: "Mi" },
        { divisor: 1e9, label: "Gi" },
      ];
    case Protocol.FormatUnit.KMiGiTi:
      return [
        { divisor: 1e3, label: "Ki" },
        { divisor: 1e6, label: "Mi" },
        { divisor: 1e9, label: "Gi" },
        { divisor: 1e12, label: "Ti" },
      ];
  }
}

function trimFractionZeros(value: string): string {
  const dot = value.lastIndexOf(".");
  if (dot < 0) return value;
  let v = value;
  while (v.endsWith("0")) v = v.substring(0, v.length - 1);
  if (v.endsWith(".")) v = v.substring(0, v.length - 1);
  return v;
}

export function formatSize(
  bytes: number,
  precision: Protocol.FormatPrecision = Protocol.FormatPrecision.Two,
  unit: Protocol.FormatUnit = Protocol.FormatUnit.KMGT
): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0";
  const dp = precisionToDecimalPlaces(precision);
  const tiers = unitToTiers(unit);
  for (let i = tiers.length - 1; i >= 0; i--) {
    if (bytes >= tiers[i].divisor) {
      return `${trimFractionZeros((bytes / tiers[i].divisor).toFixed(dp))} ${tiers[i].label}B`;
    }
  }
  return `${bytes} B`;
}

export function formatBitRate(
  bps: number,
  precision: Protocol.FormatPrecision = Protocol.FormatPrecision.Two,
  unit: Protocol.FormatUnit = Protocol.FormatUnit.KMGT
): string {
  if (!Number.isFinite(bps) || bps <= 0) return "";
  const dp = precisionToDecimalPlaces(precision);
  const tiers = unitToTiers(unit);
  for (let i = tiers.length - 1; i >= 0; i--) {
    if (bps >= tiers[i].divisor) {
      return `${trimFractionZeros((bps / tiers[i].divisor).toFixed(dp))} ${tiers[i].label}bps`;
    }
  }
  return `${bps} bps`;
}

export function formatPid(pid: number): string {
  if (!Number.isFinite(pid)) return "";
  return Math.trunc(pid).toString(10);
}

// Length is in 45 kHz units (BD time base) per BDInfo convention.
export function formatLength45k(length45k: number): string {
  return formatLengthSeconds(length45k / 45000.0);
}

export function formatLengthSeconds(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds <= 0) return "00:00:00";
  const total = Math.floor(totalSeconds);
  const ms = Math.floor((totalSeconds - total) * 1000);
  const s = total % 60;
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  const pad = (n: number, w = 2) => n.toString().padStart(w, "0");
  if (ms > 0) {
    return `${pad(h)}:${pad(m)}:${pad(s)}.${pad(ms, 3)}`;
  }
  return `${pad(h)}:${pad(m)}:${pad(s)}`;
}
