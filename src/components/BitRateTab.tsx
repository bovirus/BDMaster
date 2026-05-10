/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useEffect, useState } from "react";
import {
  Box,
  Card,
  CardContent,
  CardHeader,
  Typography,
} from "@mui/material";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../lib/store";
import { getPlaylistChartData } from "../lib/service";
import { formatBitRate, formatLengthSeconds } from "../lib/format";

interface ChartPoint {
  time: number;
  bitRate: number;
}

export default function BitRateTab() {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);
  const playlistName = useAppStore((s) => s.bitRatePlaylist);
  const [data, setData] = useState<ChartPoint[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (!disc || !playlistName) {
      setData(null);
      return;
    }
    setData(null);
    setError(null);
    getPlaylistChartData(disc.path, playlistName)
      .then((result) => {
        if (!cancelled) setData(result);
      })
      .catch((e) => {
        if (!cancelled) setError(`${e}`);
      });
    return () => {
      cancelled = true;
    };
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
          {error ? (
            <Typography variant="body2" color="error">
              {error}
            </Typography>
          ) : data === null ? (
            <Typography variant="body2" color="text.secondary">
              …
            </Typography>
          ) : (
            <BitRateChart data={data} />
          )}
        </CardContent>
      </Card>
    </Box>
  );
}

function BitRateChart({ data }: { data: ChartPoint[] }) {
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
