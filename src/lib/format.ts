/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

export function shrinkFileName(fileName: string, maxLength: number): string {
  if (fileName.length > maxLength) {
    return "..." + fileName.substring(fileName.length - maxLength + 3);
  }
  return fileName;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const tiers = [
    { divisor: 1024, label: "KB" },
    { divisor: 1024 ** 2, label: "MB" },
    { divisor: 1024 ** 3, label: "GB" },
    { divisor: 1024 ** 4, label: "TB" },
  ];
  for (let i = tiers.length - 1; i >= 0; i--) {
    if (bytes >= tiers[i].divisor) {
      return `${(bytes / tiers[i].divisor).toFixed(2)} ${tiers[i].label}`;
    }
  }
  return `${bytes} B`;
}

export function formatBitRate(bps: number): string {
  if (bps <= 0) return "";
  if (bps < 1000) return `${bps} bps`;
  const tiers = [
    { divisor: 1e3, label: "kbps" },
    { divisor: 1e6, label: "Mbps" },
    { divisor: 1e9, label: "Gbps" },
  ];
  for (let i = tiers.length - 1; i >= 0; i--) {
    if (bps >= tiers[i].divisor) {
      return `${(bps / tiers[i].divisor).toFixed(2)} ${tiers[i].label}`;
    }
  }
  return `${bps} bps`;
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
