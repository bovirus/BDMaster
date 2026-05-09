/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Button,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  IconButton,
  Paper,
  Stack,
  Tab,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TableSortLabel,
  Tabs,
  Typography,
} from "@mui/material";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import SaveIcon from "@mui/icons-material/Save";
import ShowChartIcon from "@mui/icons-material/ShowChart";
import DescriptionIcon from "@mui/icons-material/Description";
import CloseIcon from "@mui/icons-material/Close";
import { useTranslation } from "react-i18next";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import * as Protocol from "../lib/protocol";
import { useAppStore } from "../lib/store";
import { generateReport, getPlaylistChartData, setConfig as saveConfig, writeTextFile } from "../lib/service";
import { openSaveReportDialog } from "../lib/dialog";
import { formatBitRate, formatLength45k, formatLengthSeconds, formatSize } from "../lib/format";

type PlaylistSortKey = "name" | "groupIndex" | "totalLength" | "fileSize" | "measuredSize";
type SortDir = "asc" | "desc";

/**
 * Stable sort helper: pairs each item with its original index so equal keys
 * preserve their input order across asc/desc flips.
 */
function stableSort<T>(items: T[], comparator: (a: T, b: T) => number): T[] {
  const arr = items.map((item, idx) => [item, idx] as const);
  arr.sort((a, b) => {
    const r = comparator(a[0], b[0]);
    if (r !== 0) return r;
    return a[1] - b[1];
  });
  return arr.map((x) => x[0]);
}

function comparePlaylists(
  key: PlaylistSortKey,
  dir: SortDir
): (a: Protocol.PlaylistInfo, b: Protocol.PlaylistInfo) => number {
  return (a, b) => {
    let av: number | string;
    let bv: number | string;
    switch (key) {
      case "name":
        av = a.name;
        bv = b.name;
        break;
      case "groupIndex":
        av = a.groupIndex;
        bv = b.groupIndex;
        break;
      case "totalLength":
        av = a.totalLength;
        bv = b.totalLength;
        break;
      case "fileSize":
        av = a.fileSize;
        bv = b.fileSize;
        break;
      case "measuredSize":
        av = a.measuredSize;
        bv = b.measuredSize;
        break;
    }
    let cmp: number;
    if (typeof av === "number" && typeof bv === "number") {
      cmp = av - bv;
    } else {
      cmp = String(av).localeCompare(String(bv));
    }
    return dir === "asc" ? cmp : -cmp;
  };
}

/**
 * Header cell whose entire surface is clickable for sorting (rather than just
 * the label text inside MUI's TableSortLabel).
 */
function SortableHeaderCell({
  active,
  direction,
  onSort,
  align,
  children,
}: {
  active: boolean;
  direction: SortDir;
  onSort: () => void;
  align?: "right" | "left" | "center";
  children: React.ReactNode;
}) {
  return (
    <TableCell
      align={align}
      sortDirection={active ? direction : false}
      onClick={onSort}
      sx={{
        fontWeight: "bold",
        cursor: "pointer",
        userSelect: "none",
        "&:hover": { backgroundColor: "action.hover" },
      }}
    >
      <TableSortLabel
        active={active}
        direction={active ? direction : "asc"}
        // The TableCell itself handles the click; keep the label
        // non-interactive so the cell registers a single click event.
        hideSortIcon={false}
        sx={{ pointerEvents: "none" }}
      >
        {children}
      </TableSortLabel>
    </TableCell>
  );
}

export default function DiscDetail() {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);
  const config = useAppStore((s) => s.config);
  const setConfigState = useAppStore((s) => s.setConfig);
  const setNotification = useAppStore((s) => s.setDialogNotification);
  const [sortKey, setSortKey] = useState<PlaylistSortKey>("fileSize");
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  // Resizable splitter between the playlist table and the info panel.
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [splitFraction, setSplitFraction] = useState<number>(
    config?.discInfoSplit ?? 0.4
  );
  useEffect(() => {
    if (config) setSplitFraction(config.discInfoSplit ?? 0.4);
  }, [config?.discInfoSplit]);

  const draggingRef = useRef(false);
  const persistSplit = useCallback(
    (fraction: number) => {
      if (!config) return;
      const next = { ...config, discInfoSplit: fraction };
      saveConfig(next)
        .then((saved) => setConfigState(saved))
        .catch(() => {
          // Non-fatal — drag still works in-memory until the next reload.
        });
    },
    [config, setConfigState]
  );

  const handleSplitterMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      draggingRef.current = true;
      const onMove = (ev: MouseEvent) => {
        const rect = containerRef.current?.getBoundingClientRect();
        if (!rect || !draggingRef.current) return;
        const y = ev.clientY - rect.top;
        const fraction = Math.max(0.1, Math.min(0.9, y / rect.height));
        setSplitFraction(fraction);
      };
      const onUp = () => {
        draggingRef.current = false;
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        // Read the current value from state via a setState callback so the
        // closure doesn't capture a stale fraction.
        setSplitFraction((current) => {
          persistSplit(current);
          return current;
        });
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [persistSplit]
  );
  const sizePrecision = config?.formatting?.size?.precision ?? Protocol.FormatPrecision.Two;
  const sizeUnit = config?.formatting?.size?.unit ?? Protocol.FormatUnit.KMGT;
  const bitRatePrecision =
    config?.formatting?.bitRate?.precision ?? Protocol.FormatPrecision.Two;
  const bitRateUnit = config?.formatting?.bitRate?.unit ?? Protocol.FormatUnit.KMGT;
  const [selectedPlaylist, setSelectedPlaylist] = useState<string | null>(null);
  const [tabIndex, setTabIndex] = useState(0);
  const [reportText, setReportText] = useState<string | null>(null);
  const [reportTitle, setReportTitle] = useState<string>("");
  const [chartOpen, setChartOpen] = useState(false);
  const [chartData, setChartData] = useState<{ time: number; bitRate: number }[]>([]);

  useEffect(() => {
    if (!disc) return;
    if (disc.playlists.length > 0 && !selectedPlaylist) {
      setSelectedPlaylist(disc.playlists[0].name);
    }
  }, [disc, selectedPlaylist]);

  const playlist = useMemo(() => {
    if (!disc || !selectedPlaylist) return null;
    return disc.playlists.find((p) => p.name === selectedPlaylist) ?? null;
  }, [disc, selectedPlaylist]);

  const sortedPlaylists = useMemo(() => {
    if (!disc) return [];
    return stableSort(disc.playlists, comparePlaylists(sortKey, sortDir));
  }, [disc, sortKey, sortDir]);

  const handleSort = (key: PlaylistSortKey) => {
    if (sortKey === key) {
      setSortDir(sortDir === "asc" ? "desc" : "asc");
    } else {
      setSortKey(key);
      setSortDir(key === "name" ? "asc" : "desc");
    }
  };

  if (!disc) {
    return <Box sx={{ p: 2 }}>Loading…</Box>;
  }

  const handleGenerateReport = async (full: boolean) => {
    try {
      const text = await generateReport(
        disc.path,
        full,
        selectedPlaylist ? [selectedPlaylist] : null
      );
      setReportText(text);
      setReportTitle(full ? t("disc.generateFullReport") : t("disc.generateQuickSummary"));
    } catch (error) {
      setNotification({
        title: `${error}`,
        type: Protocol.DialogNotificationType.Error,
      });
    }
  };

  const handleCopyReport = async () => {
    if (!reportText) return;
    await writeText(reportText);
    setNotification({
      title: "Report copied to clipboard.",
      type: Protocol.DialogNotificationType.Info,
    });
  };

  const handleSaveReport = async () => {
    if (!reportText) return;
    const filePath = await openSaveReportDialog();
    if (filePath) {
      try {
        await writeTextFile(filePath as string, reportText);
        setNotification({
          title: `Saved to ${filePath}`,
          type: Protocol.DialogNotificationType.Info,
        });
      } catch (error) {
        setNotification({
          title: `${error}`,
          type: Protocol.DialogNotificationType.Error,
        });
      }
    }
  };

  const handleViewChart = async () => {
    if (!selectedPlaylist) return;
    try {
      const data = await getPlaylistChartData(disc.path, selectedPlaylist);
      setChartData(data);
      setChartOpen(true);
    } catch (error) {
      setNotification({
        title: `${error}`,
        type: Protocol.DialogNotificationType.Error,
      });
    }
  };

  return (
    <Box sx={{ display: "flex", flexDirection: "column", gap: 1, p: 1, height: "100%" }}>
      {/* Header */}
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction="row" spacing={2} sx={{ flexWrap: "wrap", alignItems: "flex-start" }}>
          <Box sx={{ minWidth: 0, flex: 1 }}>
            <Typography variant="h6" noWrap title={disc.discTitle || disc.discName}>
              {disc.discTitle || disc.discName}
            </Typography>
            <Stack direction="row" spacing={3} sx={{ mt: 0.5, flexWrap: "wrap" }}>
              <Typography variant="caption" title={disc.path} sx={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                <b>{t("disc.path")}:</b> {disc.path}
              </Typography>
              <Typography variant="caption">
                <b>{t("disc.volume")}:</b> {disc.volumeLabel || "-"}
              </Typography>
              <Typography variant="caption">
                <b>{t("disc.size")}:</b> {formatSize(disc.size, sizePrecision, sizeUnit)}
              </Typography>
              <Typography variant="caption">
                <b>{t("disc.playlists")}:</b> {disc.playlists.length}
              </Typography>
            </Stack>
          </Box>
          <Stack direction="row" spacing={0.5} sx={{ flexWrap: "wrap", gap: 0.5 }}>
            {disc.isUHD && <Chip size="small" label={t("disc.isUHD")} />}
            {disc.is4K && <Chip size="small" label={t("disc.is4K")} />}
            {disc.is3D && <Chip size="small" label={t("disc.is3D")} />}
            {disc.is50Hz && <Chip size="small" label={t("disc.is50Hz")} />}
            {disc.isBdJava && <Chip size="small" label={t("disc.hasBdJava")} />}
            {disc.isBdPlus && <Chip size="small" label={t("disc.hasBdPlus")} />}
            {disc.hasMVCExtension && <Chip size="small" label={t("disc.hasMVCExtension")} />}
            {disc.hasHEVCStreams && <Chip size="small" label={t("disc.hasHEVCStreams")} />}
          </Stack>
        </Stack>
      </Paper>

      {/* Body: playlists / splitter / info panel */}
      <Box
        ref={containerRef}
        sx={{
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Playlist list */}
        <Paper
          variant="outlined"
          sx={{
            overflow: "auto",
            minHeight: 0,
            flex: `0 0 ${(splitFraction * 100).toFixed(2)}%`,
          }}
        >
          <TableContainer>
            <Table size="small" stickyHeader>
              <TableHead>
                <TableRow>
                  <SortableHeaderCell
                    active={sortKey === "name"}
                    direction={sortDir}
                    onSort={() => handleSort("name")}
                  >
                    {t("disc.playlist")}
                  </SortableHeaderCell>
                  <SortableHeaderCell
                    active={sortKey === "groupIndex"}
                    direction={sortDir}
                    onSort={() => handleSort("groupIndex")}
                    align="right"
                  >
                    {t("disc.group")}
                  </SortableHeaderCell>
                  <SortableHeaderCell
                    active={sortKey === "totalLength"}
                    direction={sortDir}
                    onSort={() => handleSort("totalLength")}
                  >
                    {t("disc.length")}
                  </SortableHeaderCell>
                  <SortableHeaderCell
                    active={sortKey === "fileSize"}
                    direction={sortDir}
                    onSort={() => handleSort("fileSize")}
                    align="right"
                  >
                    {t("disc.estimatedSize")}
                  </SortableHeaderCell>
                  <SortableHeaderCell
                    active={sortKey === "measuredSize"}
                    direction={sortDir}
                    onSort={() => handleSort("measuredSize")}
                    align="right"
                  >
                    {t("disc.measuredSize")}
                  </SortableHeaderCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {sortedPlaylists.map((p) => (
                  <TableRow
                    key={p.name}
                    hover
                    selected={p.name === selectedPlaylist}
                    onClick={() => setSelectedPlaylist(p.name)}
                    sx={{ cursor: "pointer" }}
                  >
                    <TableCell>{p.name}</TableCell>
                    <TableCell align="right">{p.groupIndex || ""}</TableCell>
                    <TableCell>{formatLength45k(p.totalLength)}</TableCell>
                    <TableCell align="right">
                      {formatSize(p.fileSize, sizePrecision, sizeUnit)}
                    </TableCell>
                    <TableCell align="right">
                      {p.measuredSize > 0
                        ? formatSize(p.measuredSize, sizePrecision, sizeUnit)
                        : "—"}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        </Paper>

        {/* Draggable splitter */}
        <Box
          onMouseDown={handleSplitterMouseDown}
          sx={(theme) => ({
            height: 6,
            cursor: "row-resize",
            flexShrink: 0,
            backgroundColor: theme.palette.divider,
            transition: "background-color 120ms",
            "&:hover": {
              backgroundColor: theme.palette.primary.main,
            },
          })}
        />

        {/* Bottom panel: streams + clips for selected playlist */}
        <Paper variant="outlined" sx={{ overflow: "auto", minHeight: 0, p: 1, flex: 1 }}>
          {playlist ? (
            <>
              <Stack direction="row" spacing={1} sx={{ mb: 1, flexWrap: "wrap", gap: 1, alignItems: "center" }}>
                <Button
                  size="small"
                  variant="outlined"
                  startIcon={<DescriptionIcon />}
                  onClick={() => handleGenerateReport(false)}
                >
                  {t("disc.generateQuickSummary")}
                </Button>
                <Button
                  size="small"
                  variant="outlined"
                  startIcon={<DescriptionIcon />}
                  onClick={() => handleGenerateReport(true)}
                >
                  {t("disc.generateFullReport")}
                </Button>
                <Button
                  size="small"
                  variant="outlined"
                  startIcon={<ShowChartIcon />}
                  onClick={handleViewChart}
                >
                  {t("disc.viewChart")}
                </Button>
              </Stack>
              <Tabs value={tabIndex} onChange={(_, v) => setTabIndex(v)}>
                <Tab label={`${t("disc.videoStreams")} (${playlist.videoStreams.length})`} />
                <Tab label={`${t("disc.audioStreams")} (${playlist.audioStreams.length})`} />
                <Tab label={`${t("disc.graphicsStreams")} (${playlist.graphicsStreams.length})`} />
                <Tab label={`${t("disc.textStreams")} (${playlist.textStreams.length})`} />
                <Tab label={`Clips (${playlist.streamClips.length})`} />
                <Tab label={`${t("disc.chapters")} (${playlist.chapters.length})`} />
              </Tabs>
              <Box sx={{ mt: 1 }}>
                {tabIndex === 0 && (
                  <StreamTable
                    streams={playlist.videoStreams}
                    bitRatePrecision={bitRatePrecision}
                    bitRateUnit={bitRateUnit}
                  />
                )}
                {tabIndex === 1 && (
                  <StreamTable
                    streams={playlist.audioStreams}
                    bitRatePrecision={bitRatePrecision}
                    bitRateUnit={bitRateUnit}
                  />
                )}
                {tabIndex === 2 && (
                  <StreamTable
                    streams={playlist.graphicsStreams}
                    bitRatePrecision={bitRatePrecision}
                    bitRateUnit={bitRateUnit}
                  />
                )}
                {tabIndex === 3 && (
                  <StreamTable
                    streams={playlist.textStreams}
                    bitRatePrecision={bitRatePrecision}
                    bitRateUnit={bitRateUnit}
                  />
                )}
                {tabIndex === 4 && (
                  <ClipsTable
                    clips={playlist.streamClips}
                    sizePrecision={sizePrecision}
                    sizeUnit={sizeUnit}
                  />
                )}
                {tabIndex === 5 && <ChaptersTable chapters={playlist.chapters} />}
              </Box>
            </>
          ) : (
            <Typography variant="body2" color="text.secondary">
              {t("disc.noPlaylistSelected")}
            </Typography>
          )}
        </Paper>
      </Box>

      {/* Report dialog */}
      <Dialog open={reportText !== null} onClose={() => setReportText(null)} maxWidth="lg" fullWidth>
        <DialogTitle>
          {reportTitle}
          <IconButton
            onClick={() => setReportText(null)}
            sx={{ position: "absolute", right: 8, top: 8 }}
          >
            <CloseIcon />
          </IconButton>
        </DialogTitle>
        <DialogContent dividers>
          <Box
            component="pre"
            sx={{
              fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
              fontSize: "0.75rem",
              whiteSpace: "pre-wrap",
              m: 0,
              maxHeight: "70vh",
              overflow: "auto",
            }}
          >
            {reportText}
          </Box>
        </DialogContent>
        <DialogActions>
          <Button startIcon={<ContentCopyIcon />} onClick={handleCopyReport}>
            {t("disc.copyReport")}
          </Button>
          <Button startIcon={<SaveIcon />} onClick={handleSaveReport}>
            {t("disc.saveReport")}
          </Button>
        </DialogActions>
      </Dialog>

      {/* Chart dialog */}
      <Dialog open={chartOpen} onClose={() => setChartOpen(false)} maxWidth="lg" fullWidth>
        <DialogTitle>
          {t("disc.viewChart")}
          <IconButton
            onClick={() => setChartOpen(false)}
            sx={{ position: "absolute", right: 8, top: 8 }}
          >
            <CloseIcon />
          </IconButton>
        </DialogTitle>
        <DialogContent dividers>
          <BitrateChart data={chartData} />
        </DialogContent>
      </Dialog>
    </Box>
  );
}

function StreamTable({
  streams,
  bitRatePrecision,
  bitRateUnit,
}: {
  streams: Protocol.TSStreamInfo[];
  bitRatePrecision: Protocol.FormatPrecision;
  bitRateUnit: Protocol.FormatUnit;
}) {
  if (streams.length === 0) {
    return (
      <Typography variant="body2" color="text.secondary">
        —
      </Typography>
    );
  }
  return (
    <TableContainer>
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell sx={{ fontWeight: "bold" }}>PID</TableCell>
            <TableCell sx={{ fontWeight: "bold" }}>Codec</TableCell>
            <TableCell sx={{ fontWeight: "bold" }}>Description</TableCell>
            <TableCell sx={{ fontWeight: "bold" }}>Language</TableCell>
            <TableCell sx={{ fontWeight: "bold" }} align="right">Bit Rate</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {streams.map((s, i) => (
            <TableRow key={`${s.pid}-${i}`}>
              <TableCell>{`0x${s.pid.toString(16).toUpperCase().padStart(4, "0")}`}</TableCell>
              <TableCell>{s.codecShortName || s.codecName}</TableCell>
              <TableCell>{s.description}</TableCell>
              <TableCell>{s.languageName || s.languageCode}</TableCell>
              <TableCell align="right">
                {formatBitRate(s.bitRate || s.activeBitRate, bitRatePrecision, bitRateUnit)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}

function ChaptersTable({ chapters }: { chapters: number[] }) {
  if (chapters.length === 0) {
    return (
      <Typography variant="body2" color="text.secondary">
        —
      </Typography>
    );
  }
  return (
    <TableContainer>
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell sx={{ fontWeight: "bold" }} align="right">#</TableCell>
            <TableCell sx={{ fontWeight: "bold" }}>Time</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {chapters.map((sec, i) => (
            <TableRow key={i}>
              <TableCell align="right">{i + 1}</TableCell>
              <TableCell>{formatLengthSeconds(sec)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}

function ClipsTable({
  clips,
  sizePrecision,
  sizeUnit,
}: {
  clips: Protocol.PlaylistStreamClipInfo[];
  sizePrecision: Protocol.FormatPrecision;
  sizeUnit: Protocol.FormatUnit;
}) {
  if (clips.length === 0) {
    return (
      <Typography variant="body2" color="text.secondary">
        —
      </Typography>
    );
  }
  return (
    <TableContainer>
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell sx={{ fontWeight: "bold" }}>Clip</TableCell>
            <TableCell sx={{ fontWeight: "bold" }}>Length</TableCell>
            <TableCell sx={{ fontWeight: "bold" }} align="right">Size</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {clips.map((c, i) => (
            <TableRow key={`${c.name}-${i}`}>
              <TableCell>{c.name}</TableCell>
              <TableCell>{formatLength45k(c.length)}</TableCell>
              <TableCell align="right">
                {formatSize(c.fileSize, sizePrecision, sizeUnit)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}

function BitrateChart({ data }: { data: { time: number; bitRate: number }[] }) {
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
      const x = padX + ((d.time / maxTime) * (width - 2 * padX));
      const y = height - padY - ((d.bitRate / maxBitRate) * (height - 2 * padY));
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
