/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useMemo } from "react";
import {
  Box,
  Card,
  CardContent,
  CardHeader,
  Typography,
} from "@mui/material";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../lib/store";
import { formatBitRate, formatLengthSeconds } from "../lib/format";
import type { ChartSample } from "../lib/protocol";

export default function BitRateTab({ playlistName }: { playlistName: string | null }) {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);

  const playlist = useMemo(() => {
    if (!disc || !playlistName) return null;
    return disc.playlists.find((p) => p.name === playlistName) ?? null;
  }, [disc, playlistName]);

  if (!playlistName) {
    return (
      <Box sx={{ p: 2 }}>
        <Typography variant="body2" color="text.secondary">
          {t("disc.noPlaylistSelected")}
        </Typography>
      </Box>
    );
  }

  return (
    <Box sx={{ p: 1, display: "flex", flexDirection: "column", height: "100%" }}>
      <Card
        variant="outlined"
        sx={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}
      >
        <CardHeader
          title={`${t("disc.playlist")}: ${playlistName}`}
          titleTypographyProps={{ variant: "subtitle1" }}
          sx={{ py: 1 }}
        />
        <CardContent sx={{ flex: 1, minHeight: 0, overflow: "auto", pt: 0, "&:last-child": { pb: 1 } }}>
          {playlist ? (
            <BitRateChart data={playlist.bitrateSamples ?? []} />
          ) : (
            <Typography variant="body2" color="text.secondary">
              {t("disc.noPlaylistSelected")}
            </Typography>
          )}
        </CardContent>
      </Card>
    </Box>
  );
}

function BitRateChart({ data }: { data: ChartSample[] }) {
  if (data.length === 0) {
    return <Typography variant="body2">No bitrate data.</Typography>;
  }
  const width = 800;
  const height = 300;
  const padX = 40;
  const padY = 20;
  const maxBitRate = Math.max(...data.map((d) => d.bitRate), 1);
  const maxTime = Math.max(...data.map((d) => d.time), 1);
  const points = data
    .map((d) => {
      const x = padX + (d.time / maxTime) * (width - 2 * padX);
      const y = height - padY - (d.bitRate / maxBitRate) * (height - 2 * padY);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <Box>
      <svg width={width} height={height} style={{ background: "rgba(0,0,0,0.04)" }}>
        <polyline points={points} fill="none" stroke="#0288d1" strokeWidth="1.5" />
        <line x1={padX} y1={height - padY} x2={width - padX} y2={height - padY} stroke="#999" />
        <line x1={padX} y1={padY} x2={padX} y2={height - padY} stroke="#999" />
        <text x={padX} y={padY - 4} fontSize="10" fill="#666">
          {formatBitRate(maxBitRate)}
        </text>
        <text x={width - padX} y={height - padY + 12} fontSize="10" fill="#666" textAnchor="end">
          {formatLengthSeconds(maxTime)}
        </text>
      </svg>
    </Box>
  );
}
