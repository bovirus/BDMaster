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
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TableSortLabel,
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
type StreamSortKey = "name" | "index" | "length" | "fileSize" | "measuredSize";
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

  // Horizontal splitter between the playlist table (top) and the bottom row.
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [splitFraction, setSplitFraction] = useState<number>(
    config?.discInfoSplit ?? 0.5
  );
  useEffect(() => {
    if (config) setSplitFraction(config.discInfoSplit ?? 0.5);
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
  const [reportText, setReportText] = useState<string | null>(null);
  const [reportTitle, setReportTitle] = useState<string>("");
  const [chartOpen, setChartOpen] = useState(false);
  const [chartData, setChartData] = useState<{ time: number; bitRate: number }[]>([]);

  // Sort state for the Stream (clip) table inside the info panel.
  const [streamSortKey, setStreamSortKey] = useState<StreamSortKey>("index");
  const [streamSortDir, setStreamSortDir] = useState<SortDir>("asc");
  const handleStreamSort = (key: StreamSortKey) => {
    if (streamSortKey === key) {
      setStreamSortDir(streamSortDir === "asc" ? "desc" : "asc");
    } else {
      setStreamSortKey(key);
      setStreamSortDir(key === "name" ? "asc" : "desc");
    }
  };

  // Vertical splitter in the bottom row: stream table on the left, track
  // table (with the button row beneath it) on the right.
  const infoPanelRef = useRef<HTMLDivElement | null>(null);
  const [infoSplitFraction, setInfoSplitFraction] = useState<number>(
    config?.infoPanelSplit ?? 0.5
  );
  useEffect(() => {
    if (config) setInfoSplitFraction(config.infoPanelSplit ?? 0.5);
  }, [config?.infoPanelSplit]);
  const infoDraggingRef = useRef(false);
  const persistInfoSplit = useCallback(
    (fraction: number) => {
      if (!config) return;
      const next = { ...config, infoPanelSplit: fraction };
      saveConfig(next)
        .then((saved) => setConfigState(saved))
        .catch(() => {});
    },
    [config, setConfigState]
  );
  const handleInfoSplitterMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      infoDraggingRef.current = true;
      const onMove = (ev: MouseEvent) => {
        const rect = infoPanelRef.current?.getBoundingClientRect();
        if (!rect || !infoDraggingRef.current) return;
        const x = ev.clientX - rect.left;
        const fraction = Math.max(0.1, Math.min(0.9, x / rect.width));
        setInfoSplitFraction(fraction);
      };
      const onUp = () => {
        infoDraggingRef.current = false;
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        setInfoSplitFraction((current) => {
          persistInfoSplit(current);
          return current;
        });
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [persistInfoSplit]
  );

  // Open the Chapters tab for the currently selected playlist.
  const setTabChaptersStatus = useAppStore((s) => s.setTabChaptersStatus);
  const setChapterPlaylist = useAppStore((s) => s.setChapterPlaylist);
  const handleViewChapters = () => {
    if (!selectedPlaylist) return;
    setChapterPlaylist(selectedPlaylist);
    setTabChaptersStatus(Protocol.ControlStatus.Selected);
  };

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

      {/* Body: playlist (top) — splitter — stream | splitter | track + buttons */}
      <Box
        ref={containerRef}
        sx={{
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Playlist list (full row) */}
        <Paper
          variant="outlined"
          sx={{
            overflow: "auto",
            minHeight: 0,
            minWidth: 0,
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

        {/* Horizontal splitter between playlist and bottom row */}
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

        {/* Bottom row: stream table | splitter | (track table + buttons) */}
        <Box
          ref={infoPanelRef}
          sx={{
            flex: 1,
            minHeight: 0,
            minWidth: 0,
            display: "flex",
            flexDirection: "row",
          }}
        >
          {playlist ? (
            <>
              {/* Stream (clip) table */}
              <Paper
                variant="outlined"
                sx={{
                  overflow: "auto",
                  minHeight: 0,
                  minWidth: 0,
                  flex: `0 0 ${(infoSplitFraction * 100).toFixed(2)}%`,
                }}
              >
                <StreamClipTable
                  clips={playlist.streamClips}
                  sortKey={streamSortKey}
                  sortDir={streamSortDir}
                  onSort={handleStreamSort}
                  sizePrecision={sizePrecision}
                  sizeUnit={sizeUnit}
                />
              </Paper>

              {/* Vertical splitter */}
              <Box
                onMouseDown={handleInfoSplitterMouseDown}
                sx={(theme) => ({
                  width: 6,
                  cursor: "col-resize",
                  flexShrink: 0,
                  backgroundColor: theme.palette.divider,
                  transition: "background-color 120ms",
                  "&:hover": { backgroundColor: theme.palette.primary.main },
                })}
              />

              {/* Right column: track table over button row */}
              <Box
                sx={{
                  flex: 1,
                  minHeight: 0,
                  minWidth: 0,
                  display: "flex",
                  flexDirection: "column",
                }}
              >
                <Paper variant="outlined" sx={{ overflow: "auto", minHeight: 0, flex: 1 }}>
                  <TrackTable
                    playlist={playlist}
                    bitRatePrecision={bitRatePrecision}
                    bitRateUnit={bitRateUnit}
                    sizePrecision={sizePrecision}
                    sizeUnit={sizeUnit}
                  />
                </Paper>

                <Stack direction="row" spacing={1} sx={{ mt: 1, flexWrap: "wrap", gap: 1, alignItems: "center" }}>
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
                    {t("disc.viewBitRateReport")}
                  </Button>
                  {playlist.chapters.length > 0 && (
                    <Button
                      size="small"
                      variant="outlined"
                      startIcon={<DescriptionIcon />}
                      onClick={handleViewChapters}
                    >
                      {t("disc.viewChapter")}
                    </Button>
                  )}
                </Stack>
              </Box>
            </>
          ) : (
            <Typography variant="body2" color="text.secondary" sx={{ p: 1 }}>
              {t("disc.noPlaylistSelected")}
            </Typography>
          )}
        </Box>
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
          {t("disc.viewBitRateReport")}
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

/** The Stream (clip) table: one row per M2TS clip in the selected playlist. */
function StreamClipTable({
  clips,
  sortKey,
  sortDir,
  onSort,
  sizePrecision,
  sizeUnit,
}: {
  clips: Protocol.PlaylistStreamClipInfo[];
  sortKey: StreamSortKey;
  sortDir: SortDir;
  onSort: (k: StreamSortKey) => void;
  sizePrecision: Protocol.FormatPrecision;
  sizeUnit: Protocol.FormatUnit;
}) {
  // Filter to angle 0 only (mirroring the playlist grouping in BDInfo).
  const angle0 = useMemo(
    () => clips.filter((c) => c.angleIndex === 0),
    [clips]
  );
  // Pair each clip with its 1-based original index before sorting.
  const sorted = useMemo(() => {
    const indexed = angle0.map((c, i) => ({ clip: c, index: i + 1 }));
    return stableSort(indexed, (a, b) => {
      let cmp: number;
      switch (sortKey) {
        case "name":
          cmp = a.clip.name.localeCompare(b.clip.name);
          break;
        case "index":
          cmp = a.index - b.index;
          break;
        case "length":
          cmp = a.clip.length - b.clip.length;
          break;
        case "fileSize":
          cmp = a.clip.fileSize - b.clip.fileSize;
          break;
        case "measuredSize":
          cmp = a.clip.measuredSize - b.clip.measuredSize;
          break;
      }
      return sortDir === "asc" ? cmp : -cmp;
    });
  }, [angle0, sortKey, sortDir]);

  if (sorted.length === 0) {
    return (
      <Typography variant="body2" color="text.secondary" sx={{ p: 1 }}>
        —
      </Typography>
    );
  }
  return (
    <TableContainer>
      <Table size="small" stickyHeader>
        <TableHead>
          <TableRow>
            <SortableHeaderCell
              active={sortKey === "name"}
              direction={sortDir}
              onSort={() => onSort("name")}
            >
              Stream
            </SortableHeaderCell>
            <SortableHeaderCell
              active={sortKey === "index"}
              direction={sortDir}
              onSort={() => onSort("index")}
              align="right"
            >
              Index
            </SortableHeaderCell>
            <SortableHeaderCell
              active={sortKey === "length"}
              direction={sortDir}
              onSort={() => onSort("length")}
            >
              Length
            </SortableHeaderCell>
            <SortableHeaderCell
              active={sortKey === "fileSize"}
              direction={sortDir}
              onSort={() => onSort("fileSize")}
              align="right"
            >
              Estimated Size
            </SortableHeaderCell>
            <SortableHeaderCell
              active={sortKey === "measuredSize"}
              direction={sortDir}
              onSort={() => onSort("measuredSize")}
              align="right"
            >
              Measured Size
            </SortableHeaderCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {sorted.map(({ clip, index }) => (
            <TableRow key={`${clip.name}-${index}`} hover>
              <TableCell>{clip.name}</TableCell>
              <TableCell align="right">{index}</TableCell>
              <TableCell>{formatLength45k(clip.length)}</TableCell>
              <TableCell align="right">
                {formatSize(clip.fileSize, sizePrecision, sizeUnit)}
              </TableCell>
              <TableCell align="right">
                {clip.measuredSize > 0
                  ? formatSize(clip.measuredSize, sizePrecision, sizeUnit)
                  : "—"}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}

/** The Track table: video → audio → graphics+text streams of the playlist. */
function TrackTable({
  playlist,
  bitRatePrecision,
  bitRateUnit,
  sizePrecision,
  sizeUnit,
}: {
  playlist: Protocol.PlaylistInfo;
  bitRatePrecision: Protocol.FormatPrecision;
  bitRateUnit: Protocol.FormatUnit;
  sizePrecision: Protocol.FormatPrecision;
  sizeUnit: Protocol.FormatUnit;
}) {
  const tracks = useMemo(
    () => [
      ...playlist.videoStreams,
      ...playlist.audioStreams,
      ...playlist.graphicsStreams,
      ...playlist.textStreams,
    ],
    [playlist]
  );
  if (tracks.length === 0) {
    return (
      <Typography variant="body2" color="text.secondary" sx={{ p: 1 }}>
        —
      </Typography>
    );
  }
  return (
    <TableContainer>
      <Table size="small" stickyHeader>
        <TableHead>
          <TableRow>
            <TableCell sx={{ fontWeight: "bold" }}>ID</TableCell>
            <TableCell sx={{ fontWeight: "bold" }}>Codec</TableCell>
            <TableCell sx={{ fontWeight: "bold" }}>Language</TableCell>
            <TableCell sx={{ fontWeight: "bold" }} align="right">Bit Rate</TableCell>
            <TableCell sx={{ fontWeight: "bold" }}>Description</TableCell>
            <TableCell sx={{ fontWeight: "bold" }} align="right">Measured Size</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {tracks.map((s, i) => (
            <TableRow key={`${s.pid}-${i}`}>
              <TableCell>{`0x${s.pid.toString(16).toUpperCase().padStart(4, "0")}`}</TableCell>
              <TableCell>{s.codecShortName || s.codecName}</TableCell>
              <TableCell>{s.languageName || s.languageCode}</TableCell>
              <TableCell align="right">
                {formatBitRate(s.bitRate || s.activeBitRate, bitRatePrecision, bitRateUnit)}
              </TableCell>
              <TableCell>{s.description}</TableCell>
              <TableCell align="right">
                {s.measuredSize > 0
                  ? formatSize(s.measuredSize, sizePrecision, sizeUnit)
                  : "—"}
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
