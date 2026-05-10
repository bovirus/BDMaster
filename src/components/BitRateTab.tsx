/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useMemo } from "react";
import ReactECharts from "echarts-for-react";
import type { EChartsOption } from "echarts";
import {
  Box,
  Card,
  CardContent,
  CardHeader,
  Chip,
  Stack,
  Typography,
  useTheme,
} from "@mui/material";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../lib/store";
import { formatBitRate, formatLengthSeconds } from "../lib/format";
import type { ChartSample } from "../lib/protocol";

type ChartPoint = [number, number];

interface TooltipParam {
  marker?: unknown;
  seriesName?: string;
  value?: unknown;
}

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
          subheader={t("disc.viewChart")}
          titleTypographyProps={{ variant: "subtitle1" }}
          subheaderTypographyProps={{ variant: "caption" }}
          sx={{ py: 1, pb: 0.5 }}
        />
        <CardContent sx={{ flex: 1, minHeight: 0, overflow: "hidden", pt: 0, "&:last-child": { pb: 1 } }}>
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
  const theme = useTheme();

  const sortedData = useMemo(
    () =>
      data
        .filter((d) => Number.isFinite(d.time) && Number.isFinite(d.bitRate) && d.bitRate > 0)
        .slice()
        .sort((a, b) => a.time - b.time),
    [data]
  );

  const seriesData = useMemo(
    () => ({
      one: rollingAverageSeries(sortedData, 1),
      five: rollingAverageSeries(sortedData, 5),
      ten: rollingAverageSeries(sortedData, 10),
    }),
    [sortedData]
  );

  const stats = useMemo(() => {
    const peak = Math.max(...sortedData.map((d) => d.bitRate), 0);
    const average =
      sortedData.length > 0
        ? sortedData.reduce((sum, d) => sum + d.bitRate, 0) / sortedData.length
        : 0;
    const duration = Math.max(...sortedData.map((d) => d.time), 0);
    return { peak, average, duration };
  }, [sortedData]);

  const option = useMemo<EChartsOption>(() => {
    const textColor = theme.palette.text.primary;
    const mutedColor = theme.palette.text.secondary;
    const gridColor = theme.palette.divider;
    const axisFormatter = (value: number) => formatLengthSeconds(value * 60);
    const bitrateFormatter = (value: number) => `${trimChartNumber(value)} Mbps`;

    return {
      backgroundColor: "transparent",
      color: [
        theme.palette.primary.main,
        theme.palette.success.main,
        theme.palette.warning.main,
      ],
      animation: false,
      grid: {
        top: 36,
        right: 24,
        bottom: 74,
        left: 64,
        containLabel: true,
      },
      legend: {
        top: 0,
        textStyle: { color: textColor },
        selected: {
          "1 sec": true,
          "5 sec": false,
          "10 sec": false,
        },
      },
      toolbox: {
        right: 6,
        top: 0,
        feature: {
          restore: {},
          saveAsImage: { title: "Save" },
        },
        iconStyle: { borderColor: mutedColor },
      },
      tooltip: {
        trigger: "axis",
        axisPointer: { type: "cross" },
        backgroundColor: theme.palette.background.paper,
        borderColor: gridColor,
        textStyle: { color: textColor },
        formatter: (params: unknown) => {
          const items = normalizeTooltipParams(params);
          const firstValue = pointValue(items[0]?.value);
          const time = firstValue ? formatLengthSeconds(firstValue[0] * 60) : "";
          const rows = items
            .map((item) => {
              const value = pointValue(item.value);
              if (!value) return "";
              const marker = typeof item.marker === "string" ? item.marker : "";
              return `${marker}${item.seriesName ?? ""}: ${bitrateFormatter(value[1])}`;
            })
            .filter(Boolean)
            .join("<br/>");
          return `<strong>${time}</strong><br/>${rows}`;
        },
      },
      xAxis: {
        type: "value",
        name: "Time",
        nameLocation: "middle",
        nameGap: 34,
        min: 0,
        max: Math.max(stats.duration / 60, 1),
        axisLabel: {
          color: mutedColor,
          formatter: axisFormatter,
        },
        axisLine: { lineStyle: { color: gridColor } },
        splitLine: { lineStyle: { color: gridColor } },
      },
      yAxis: {
        type: "value",
        name: "Mbps",
        min: 0,
        axisLabel: {
          color: mutedColor,
          formatter: bitrateFormatter,
        },
        axisLine: { lineStyle: { color: gridColor } },
        splitLine: { lineStyle: { color: gridColor } },
      },
      dataZoom: [
        {
          type: "inside",
          filterMode: "none",
          minSpan: 1,
        },
        {
          type: "slider",
          filterMode: "none",
          height: 22,
          bottom: 24,
          textStyle: { color: mutedColor },
          borderColor: gridColor,
        },
      ],
      series: [
        chartSeries("1 sec", seriesData.one),
        chartSeries("5 sec", seriesData.five),
        chartSeries("10 sec", seriesData.ten),
      ],
    };
  }, [seriesData, stats.duration, theme]);

  if (data.length === 0) {
    return <Typography variant="body2">No bitrate data.</Typography>;
  }

  if (sortedData.length === 0) {
    return <Typography variant="body2">No bitrate data.</Typography>;
  }

  return (
    <Box sx={{ display: "flex", flexDirection: "column", gap: 1, height: "100%", minHeight: 360 }}>
      <Stack direction="row" spacing={1} sx={{ flexWrap: "wrap" }}>
        <Chip size="small" label={`Peak ${formatBitRate(stats.peak)}`} />
        <Chip size="small" label={`Average ${formatBitRate(stats.average)}`} />
        <Chip size="small" label={`Length ${formatLengthSeconds(stats.duration)}`} />
        <Chip size="small" label={`${sortedData.length.toLocaleString()} samples`} />
      </Stack>
      <Box sx={{ flex: 1, minHeight: 0 }}>
        <ReactECharts
          option={option}
          notMerge
          lazyUpdate
          style={{ width: "100%", height: "100%", minHeight: 320 }}
        />
      </Box>
    </Box>
  );
}

function rollingAverageSeries(samples: ChartSample[], windowSeconds: number): ChartPoint[] {
  const points: ChartPoint[] = [];
  let start = 0;
  let sum = 0;
  for (let end = 0; end < samples.length; end++) {
    const sample = samples[end];
    sum += sample.bitRate;
    while (start < end && sample.time - samples[start].time >= windowSeconds) {
      sum -= samples[start].bitRate;
      start++;
    }
    const count = end - start + 1;
    points.push([sample.time / 60, sum / count / 1_000_000]);
  }
  return points;
}

function chartSeries(name: string, data: ChartPoint[]) {
  return {
    name,
    type: "line" as const,
    data,
    showSymbol: false,
    smooth: true,
    sampling: "lttb" as const,
    lineStyle: { width: name === "1 sec" ? 1.5 : 2 },
    emphasis: { focus: "series" as const },
  };
}

function pointValue(value: unknown): ChartPoint | null {
  if (
    Array.isArray(value) &&
    value.length >= 2 &&
    typeof value[0] === "number" &&
    typeof value[1] === "number"
  ) {
    return [value[0], value[1]];
  }
  return null;
}

function normalizeTooltipParams(params: unknown): TooltipParam[] {
  const items = Array.isArray(params) ? params : [params];
  return items.filter((item): item is TooltipParam => {
    return typeof item === "object" && item !== null && "value" in item;
  });
}

function trimChartNumber(value: number): string {
  if (!Number.isFinite(value)) return "0";
  return value.toFixed(value >= 100 ? 0 : 2).replace(/\.?0+$/, "");
}
