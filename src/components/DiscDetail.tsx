/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Button,
  Chip,
  IconButton,
  LinearProgress,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TableSortLabel,
  Tooltip,
  Typography,
} from "@mui/material";
import ShowChartIcon from "@mui/icons-material/ShowChart";
import DescriptionIcon from "@mui/icons-material/Description";
import MovieIcon from "@mui/icons-material/Movie";
import AudiotrackIcon from "@mui/icons-material/Audiotrack";
import SubtitlesIcon from "@mui/icons-material/Subtitles";
import BookmarkIcon from "@mui/icons-material/Bookmark";
import SummarizeIcon from "@mui/icons-material/Summarize";
import StreamIcon from "@mui/icons-material/Stream";
import VisibilityOffIcon from "@mui/icons-material/VisibilityOff";
import { useTranslation } from "react-i18next";
import * as Protocol from "../lib/protocol";
import { useAppStore } from "../lib/store";
import {
  setConfig as saveConfig,
  startFullScan,
  cancelFullScan,
  getScanProgress,
} from "../lib/service";
import { formatLength45k, formatBitRate, formatSize } from "../lib/format";

type PlaylistSortKey =
  | "name"
  | "groupIndex"
  | "totalLength"
  | "streamCount"
  | "videoCount"
  | "audioCount"
  | "subtitleCount"
  | "chapterCount"
  | "fileSize"
  | "measuredSize";
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
      case "streamCount":
        av = a.streamClips.filter((c) => c.angleIndex === 0).length;
        bv = b.streamClips.filter((c) => c.angleIndex === 0).length;
        break;
      case "videoCount":
        av = a.videoStreams.length;
        bv = b.videoStreams.length;
        break;
      case "audioCount":
        av = a.audioStreams.length;
        bv = b.audioStreams.length;
        break;
      case "subtitleCount":
        av = a.graphicsStreams.length + a.textStreams.length;
        bv = b.graphicsStreams.length + b.textStreams.length;
        break;
      case "chapterCount":
        av = a.chapters.length;
        bv = b.chapters.length;
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
  const setDisc = useAppStore((s) => s.setDisc);
  const fullScanProgress = useAppStore((s) => s.fullScanProgress);
  const setFullScanProgress = useAppStore((s) => s.setFullScanProgress);
  const fullScanCompletedFor = useAppStore((s) => s.fullScanCompletedFor);
  const setFullScanCompletedFor = useAppStore((s) => s.setFullScanCompletedFor);
  const setDialogNotification = useAppStore((s) => s.setDialogNotification);
  const [sortKey, setSortKey] = useState<PlaylistSortKey>("fileSize");
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  const isScanning = !!fullScanProgress?.isRunning;
  const scanComplete = !!disc && fullScanCompletedFor === disc.path;

  // Periodically poll the backend for the scan snapshot. Started when the
  // user clicks Scan, cancelled when the worker reports done or errored.
  // Last successful snapshot is mirrored into the store so the disc tables
  // re-render with updated measured sizes / bit rates as the scan proceeds.
  //
  // `expectedPathRef` records the disc path the running scan was started
  // for. If the user swaps to a different disc mid-scan the backend keeps
  // working on the old one — without this guard we'd overwrite the freshly
  // loaded disc's tables with the stale scan's snapshot.
  const pollTimerRef = useRef<number | null>(null);
  const lastVersionRef = useRef<number>(0);
  const expectedPathRef = useRef<string | null>(null);
  const stopPolling = useCallback(() => {
    if (pollTimerRef.current !== null) {
      window.clearInterval(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }, []);
  useEffect(() => () => stopPolling(), [stopPolling]);

  const tick = useCallback(async () => {
    try {
      const progress = await getScanProgress();
      const expected = expectedPathRef.current;
      // Drop snapshots that no longer apply to the disc we started scanning
      // (the user has loaded a different disc since). The backend keeps
      // running but we don't propagate its updates to the new disc.
      if (expected && progress.path && progress.path !== expected) {
        stopPolling();
        return;
      }
      setFullScanProgress(progress);
      // Replace the live disc snapshot only when the worker actually wrote
      // new data (version bumps on every measured-size update). Avoids a
      // pointless re-render when the worker is mid-file and only the
      // finished_bytes counter has moved.
      if (progress.disc && progress.version !== lastVersionRef.current) {
        lastVersionRef.current = progress.version;
        setDisc(progress.disc);
      }
      if (!progress.isRunning) {
        stopPolling();
        if (progress.isCompleted && progress.disc) {
          setFullScanCompletedFor(progress.disc.path);
        } else if (progress.isCancelled) {
          // Cancelled scans leave the partial measurements in place and
          // simply revert the button back to "Scan". No notification.
        } else if (progress.error) {
          setDialogNotification({
            title: t("disc.scanFailed", { message: progress.error }),
            type: Protocol.DialogNotificationType.Error,
          });
        }
      }
    } catch (err) {
      console.error("Failed to fetch scan progress:", err);
      stopPolling();
    }
  }, [setFullScanProgress, setDisc, setFullScanCompletedFor, setDialogNotification, stopPolling, t]);

  const handleScan = useCallback(async () => {
    if (!disc) return;
    if (isScanning || scanComplete) return;
    lastVersionRef.current = 0;
    expectedPathRef.current = disc.path;
    try {
      await startFullScan(disc.path);
    } catch (err) {
      setDialogNotification({
        title: t("disc.scanFailed", { message: String(err) }),
        type: Protocol.DialogNotificationType.Error,
      });
      return;
    }
    // Kick the first poll immediately so the progress bar appears without
    // a one-second delay, then keep ticking every second per the spec.
    tick();
    stopPolling();
    pollTimerRef.current = window.setInterval(tick, 1000);
  }, [disc, isScanning, scanComplete, tick, stopPolling, setDialogNotification, t]);

  const handleCancelScan = useCallback(async () => {
    try {
      await cancelFullScan();
    } catch (err) {
      console.error("Failed to cancel scan:", err);
    }
    // Pull a fresh snapshot right away so the UI reverts to "Scan" without
    // waiting for the next polling tick.
    tick();
  }, [tick]);

  // Resume polling on mount (e.g. after a frontend reload) if the backend
  // is still in the middle of a scan for the currently displayed disc.
  useEffect(() => {
    if (!disc) return;
    let cancelled = false;
    (async () => {
      try {
        const p = await getScanProgress();
        if (cancelled || !p.isRunning) return;
        if (p.path && p.path !== disc.path) return;
        expectedPathRef.current = disc.path;
        lastVersionRef.current = 0;
        setFullScanProgress(p);
        if (p.disc) setDisc(p.disc);
        stopPolling();
        pollTimerRef.current = window.setInterval(tick, 1000);
      } catch {
        // Non-fatal: if the resume probe fails we just don't poll. The user
        // can re-trigger Scan if needed.
      }
    })();
    return () => {
      cancelled = true;
    };
    // Only re-evaluate when the active disc changes (path is the identity).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [disc?.path]);

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
    config?.infoPanelSplit ?? 0.4
  );
  useEffect(() => {
    if (config) setInfoSplitFraction(config.infoPanelSplit ?? 0.4);
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

  // Open or focus the per-(type, playlist) tab. Reuses an existing tab with
  // the same key, otherwise opens a new one.
  const openTab = useAppStore((s) => s.openTab);
  const handleViewChapters = (name: string) => openTab(Protocol.TabType.Chapters, name);
  const handleViewQuickSummary = (name: string) => openTab(Protocol.TabType.QuickSummary, name);
  const handleViewFullReport = (name: string) => openTab(Protocol.TabType.FullReport, name);
  const handleViewBitRate = (name: string) => openTab(Protocol.TabType.BitRate, name);

  const sortedPlaylists = useMemo(() => {
    if (!disc) return [];
    return stableSort(disc.playlists, comparePlaylists(sortKey, sortDir));
  }, [disc, sortKey, sortDir]);

  useEffect(() => {
    if (!disc) return;
    if (sortedPlaylists.length > 0 && !selectedPlaylist) {
      setSelectedPlaylist(sortedPlaylists[0].name);
    }
  }, [disc, sortedPlaylists, selectedPlaylist]);

  const playlist = useMemo(() => {
    if (!disc || !selectedPlaylist) return null;
    return disc.playlists.find((p) => p.name === selectedPlaylist) ?? null;
  }, [disc, selectedPlaylist]);

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
          {/* Scan control. While the scan is running this becomes the
              Cancel button; once a scan completes successfully it's
              replaced by the success badge. Cancelling reverts to Scan. */}
          <Box sx={{ alignSelf: "flex-start", flexShrink: 0 }}>
            {scanComplete ? (
              <Chip
                size="small"
                color="success"
                label={t("disc.scanCompleted")}
              />
            ) : isScanning ? (
              <Button
                variant="contained"
                size="small"
                color="error"
                onClick={handleCancelScan}
              >
                {t("disc.cancelScan")}
              </Button>
            ) : (
              <Button
                variant="contained"
                size="small"
                onClick={handleScan}
              >
                {t("disc.scan")}
              </Button>
            )}
          </Box>
        </Stack>
        {/* Progress bar lives in the card body (per the spec). 50px tall,
            primary colour, only visible while a scan is running. */}
        {isScanning && fullScanProgress && (
          <Box sx={{ mt: 1.5 }}>
            <LinearProgress
              variant={
                fullScanProgress.totalBytes > 0 ? "determinate" : "indeterminate"
              }
              value={
                fullScanProgress.totalBytes > 0
                  ? Math.min(
                      100,
                      Math.max(
                        0,
                        (fullScanProgress.finishedBytes /
                          fullScanProgress.totalBytes) *
                          100
                      )
                    )
                  : undefined
              }
              color="primary"
              sx={{ height: 50, borderRadius: 1 }}
            />
            {fullScanProgress.currentFile && (
              <Typography
                variant="caption"
                color="text.secondary"
                sx={{ mt: 0.5, display: "block" }}
              >
                {t("disc.scanning", { file: fullScanProgress.currentFile })}
              </Typography>
            )}
          </Box>
        )}
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
                    active={sortKey === "streamCount"}
                    direction={sortDir}
                    onSort={() => handleSort("streamCount")}
                    align="right"
                  >
                    <Tooltip title={t("disc.streamFiles")}>
                      <StreamIcon fontSize="small" sx={{ verticalAlign: "middle" }} />
                    </Tooltip>
                  </SortableHeaderCell>
                  <SortableHeaderCell
                    active={sortKey === "videoCount"}
                    direction={sortDir}
                    onSort={() => handleSort("videoCount")}
                    align="right"
                  >
                    <Tooltip title={t("disc.videoStreams")}>
                      <MovieIcon fontSize="small" sx={{ verticalAlign: "middle" }} />
                    </Tooltip>
                  </SortableHeaderCell>
                  <SortableHeaderCell
                    active={sortKey === "audioCount"}
                    direction={sortDir}
                    onSort={() => handleSort("audioCount")}
                    align="right"
                  >
                    <Tooltip title={t("disc.audioStreams")}>
                      <AudiotrackIcon fontSize="small" sx={{ verticalAlign: "middle" }} />
                    </Tooltip>
                  </SortableHeaderCell>
                  <SortableHeaderCell
                    active={sortKey === "subtitleCount"}
                    direction={sortDir}
                    onSort={() => handleSort("subtitleCount")}
                    align="right"
                  >
                    <Tooltip title={t("disc.subtitles")}>
                      <SubtitlesIcon fontSize="small" sx={{ verticalAlign: "middle" }} />
                    </Tooltip>
                  </SortableHeaderCell>
                  <SortableHeaderCell
                    active={sortKey === "chapterCount"}
                    direction={sortDir}
                    onSort={() => handleSort("chapterCount")}
                    align="right"
                  >
                    <Tooltip title={t("disc.chapters")}>
                      <BookmarkIcon fontSize="small" sx={{ verticalAlign: "middle" }} />
                    </Tooltip>
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
                  <TableCell align="center" sx={{ fontWeight: "bold" }}>
                    {t("disc.actions")}
                  </TableCell>
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
                      {(() => {
                        const streamCount = p.streamClips.filter(
                          (c) => c.angleIndex === 0
                        ).length;
                        return streamCount > 0 ? streamCount : "";
                      })()}
                    </TableCell>
                    <TableCell align="right">
                      {p.videoStreams.length > 0 ? p.videoStreams.length : ""}
                    </TableCell>
                    <TableCell align="right">
                      {p.audioStreams.length > 0 ? p.audioStreams.length : ""}
                    </TableCell>
                    <TableCell align="right">
                      {p.graphicsStreams.length + p.textStreams.length > 0
                        ? p.graphicsStreams.length + p.textStreams.length
                        : ""}
                    </TableCell>
                    <TableCell align="right">
                      {p.chapters.length > 0 ? p.chapters.length : ""}
                    </TableCell>
                    <TableCell align="right">
                      {formatSize(p.fileSize, sizePrecision, sizeUnit)}
                    </TableCell>
                    <TableCell align="right">
                      {p.measuredSize > 0
                        ? formatSize(p.measuredSize, sizePrecision, sizeUnit)
                        : "—"}
                    </TableCell>
                    <TableCell align="center" padding="none">
                      <Stack direction="row" spacing={0.5} sx={{ justifyContent: "center" }}>
                        {p.chapters.length > 0 && (
                          <Tooltip title={t("disc.viewChapters")}>
                            <IconButton
                              size="small"
                              sx={{ p: 0 }}
                              onClick={(e) => {
                                e.stopPropagation();
                                handleViewChapters(p.name);
                              }}
                            >
                              <BookmarkIcon fontSize="small" />
                            </IconButton>
                          </Tooltip>
                        )}
                        <Tooltip title={t("disc.generateQuickSummary")}>
                          <IconButton
                            size="small"
                            sx={{ p: 0 }}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleViewQuickSummary(p.name);
                            }}
                          >
                            <SummarizeIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                        <Tooltip title={t("disc.generateFullReport")}>
                          <IconButton
                            size="small"
                            sx={{ p: 0 }}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleViewFullReport(p.name);
                            }}
                          >
                            <DescriptionIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                        <Tooltip title={t("disc.viewBitRateReport")}>
                          <IconButton
                            size="small"
                            sx={{ p: 0 }}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleViewBitRate(p.name);
                            }}
                          >
                            <ShowChartIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                      </Stack>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TableContainer>
        </Paper>

        {/* Horizontal splitter between playlist and bottom row.
            Modern split-pane pattern: a wider transparent click area for
            forgiving hit-testing, a subtle always-visible centered "grip"
            pill that advertises the affordance, and a soft hover state
            that highlights the grip + tints the whole bar with the theme
            primary colour. */}
        <Box
          role="separator"
          aria-orientation="horizontal"
          onMouseDown={handleSplitterMouseDown}
          sx={(theme) => ({
            height: 10,
            my: "2px",
            cursor: "row-resize",
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            borderRadius: 1,
            transition: "background-color 150ms",
            "&::after": {
              content: '""',
              display: "block",
              width: 40,
              height: 3,
              borderRadius: 1.5,
              backgroundColor: theme.palette.action.disabled,
              transition: "background-color 150ms, width 150ms",
            },
            "&:hover": {
              backgroundColor: theme.palette.action.hover,
            },
            "&:hover::after": {
              backgroundColor: theme.palette.primary.main,
              width: 56,
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

              {/* Vertical splitter — same pattern as the horizontal one
                  rotated 90°: wider drag area, centered grip pill, soft
                  hover highlight. */}
              <Box
                role="separator"
                aria-orientation="vertical"
                onMouseDown={handleInfoSplitterMouseDown}
                sx={(theme) => ({
                  width: 10,
                  mx: "2px",
                  cursor: "col-resize",
                  flexShrink: 0,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  borderRadius: 1,
                  transition: "background-color 150ms",
                  "&::after": {
                    content: '""',
                    display: "block",
                    width: 3,
                    height: 40,
                    borderRadius: 1.5,
                    backgroundColor: theme.palette.action.disabled,
                    transition: "background-color 150ms, height 150ms",
                  },
                  "&:hover": {
                    backgroundColor: theme.palette.action.hover,
                  },
                  "&:hover::after": {
                    backgroundColor: theme.palette.primary.main,
                    height: 56,
                  },
                })}
              />

              {/* Track table */}
              <Paper variant="outlined" sx={{ overflow: "auto", minHeight: 0, flex: 1 }}>
                <TrackTable
                  playlist={playlist}
                  bitRatePrecision={bitRatePrecision}
                  bitRateUnit={bitRateUnit}
                  sizePrecision={sizePrecision}
                  sizeUnit={sizeUnit}
                />
              </Paper>
            </>
          ) : (
            <Typography variant="body2" color="text.secondary" sx={{ p: 1 }}>
              {t("disc.noPlaylistSelected")}
            </Typography>
          )}
        </Box>
      </Box>
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
  const { t } = useTranslation();
  const tracks = useMemo(
    () => [
      ...playlist.videoStreams,
      ...playlist.audioStreams,
      ...playlist.graphicsStreams,
      ...playlist.textStreams,
    ],
    [playlist]
  );
  // playlist.totalLength is in 45 kHz BD time units; convert to seconds for
  // the per-track estimated-size calculation.
  const lengthSeconds = playlist.totalLength / 45000;
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
            <TableCell sx={{ fontWeight: "bold" }} align="right">{t("disc.estimatedSize")}</TableCell>
            <TableCell sx={{ fontWeight: "bold" }} align="right">Measured Size</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {tracks.map((s, i) => {
            const bitRate = s.bitRate || s.activeBitRate;
            const estimatedBytes =
              bitRate > 0 && lengthSeconds > 0 ? (bitRate * lengthSeconds) / 8 : 0;
            return (
              <TableRow
                key={`${s.pid}-${i}`}
                // Hidden tracks render in a muted color so they're visually
                // distinct from declared (MPLS) streams.
                sx={s.isHidden ? { color: "text.secondary", "& .MuiTableCell-root": { color: "text.secondary" } } : undefined}
              >
                <TableCell>
                  {s.isHidden && (
                    <Tooltip title={t("disc.hiddenTrack")}>
                      <VisibilityOffIcon
                        // fontSize: "inherit" makes the icon scale to the
                        // text size, so it doesn't push the row taller.
                        sx={{ fontSize: "inherit", verticalAlign: "middle", mr: 0.5 }}
                      />
                    </Tooltip>
                  )}
                  {`0x${s.pid.toString(16).toUpperCase().padStart(4, "0")}`}
                </TableCell>
                <TableCell>{s.codecShortName || s.codecName}</TableCell>
                <TableCell>{s.languageName || s.languageCode}</TableCell>
                <TableCell align="right">
                  {formatBitRate(bitRate, bitRatePrecision, bitRateUnit)}
                </TableCell>
                <TableCell>{s.description}</TableCell>
                <TableCell align="right">
                  {estimatedBytes > 0
                    ? formatSize(estimatedBytes, sizePrecision, sizeUnit)
                    : ""}
                </TableCell>
                <TableCell align="right">
                  {s.measuredSize > 0
                    ? formatSize(s.measuredSize, sizePrecision, sizeUnit)
                    : "—"}
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
