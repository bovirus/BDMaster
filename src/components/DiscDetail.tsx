/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useEffect, useMemo, useState } from "react";
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
import { generateReport, getPlaylistChartData, writeTextFile } from "../lib/service";
import { openSaveReportDialog } from "../lib/dialog";
import { formatBitRate, formatLength45k, formatLengthSeconds, formatSize } from "../lib/format";

export default function DiscDetail() {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);
  const config = useAppStore((s) => s.config);
  const setNotification = useAppStore((s) => s.setDialogNotification);
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
            <Typography variant="caption" color="text.secondary" component="div" noWrap title={disc.path}>
              {disc.path}
            </Typography>
            <Stack direction="row" spacing={3} sx={{ mt: 0.5, flexWrap: "wrap" }}>
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

      {/* Body: playlists | streams */}
      <Box
        sx={{
          flex: 1,
          minHeight: 0,
          display: "grid",
          gridTemplateColumns: "minmax(360px, 36%) 1fr",
          gap: 1,
        }}
      >
        {/* Playlist list */}
        <Paper variant="outlined" sx={{ overflow: "auto", minHeight: 0 }}>
          <TableContainer>
            <Table size="small" stickyHeader>
              <TableHead>
                <TableRow>
                  <TableCell sx={{ fontWeight: "bold" }}>{t("disc.playlist")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">
                    {t("disc.group")}
                  </TableCell>
                  <TableCell sx={{ fontWeight: "bold" }}>{t("disc.length")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">
                    {t("disc.estimatedSize")}
                  </TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">
                    {t("disc.measuredSize")}
                  </TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {disc.playlists.map((p) => (
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

        {/* Right side: streams + clips for selected playlist */}
        <Paper variant="outlined" sx={{ overflow: "auto", minHeight: 0, p: 1 }}>
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
