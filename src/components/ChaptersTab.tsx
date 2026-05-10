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
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography,
} from "@mui/material";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../lib/store";
import { formatLengthSeconds } from "../lib/format";

// Placeholder used for chapter columns that depend on per-frame stream
// diagnostics — those values aren't available without an explicit M2TS scan,
// matching BDInfo's "no scan" report behavior.
const NOT_AVAILABLE = "—";

export default function ChaptersTab({ playlistName }: { playlistName: string | null }) {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);

  const playlist = useMemo(() => {
    if (!disc || !playlistName) return null;
    return disc.playlists.find((p) => p.name === playlistName) ?? null;
  }, [disc, playlistName]);

  if (!playlist) {
    return (
      <Box sx={{ p: 2 }}>
        <Typography variant="body2" color="text.secondary">
          {t("disc.noPlaylistSelected")}
        </Typography>
      </Box>
    );
  }

  // playlist.totalLength is in 45 kHz BD time units; chapters[] is seconds.
  const totalLengthSeconds = playlist.totalLength / 45000;

  return (
    <Box sx={{ p: 1, display: "flex", flexDirection: "column", height: "100%" }}>
      <Card
        variant="outlined"
        sx={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}
      >
        <CardHeader
          title={`${t("disc.playlist")}: ${playlist.name}`}
          subheader={`${t("disc.chapters")}: ${playlist.chapters.length}`}
          titleTypographyProps={{ variant: "subtitle1" }}
          subheaderTypographyProps={{ variant: "caption" }}
          sx={{ py: 1 }}
        />
        <CardContent sx={{ flex: 1, minHeight: 0, overflow: "auto", pt: 0, "&:last-child": { pb: 1 } }}>
          <TableContainer>
            <Table size="small" stickyHeader>
              <TableHead>
                <TableRow>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">#</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }}>{t("disc.timeIn")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }}>{t("disc.length")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">{t("disc.avgVideoRate")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">{t("disc.max1SecRate")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }}>{t("disc.max1SecTime")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">{t("disc.max5SecRate")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }}>{t("disc.max5SecTime")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">{t("disc.max10SecRate")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }}>{t("disc.max10SecTime")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">{t("disc.avgFrameSize")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }} align="right">{t("disc.maxFrameSize")}</TableCell>
                  <TableCell sx={{ fontWeight: "bold" }}>{t("disc.maxFrameTime")}</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {playlist.chapters.map((startSec, i) => {
                  const endSec =
                    i + 1 < playlist.chapters.length
                      ? playlist.chapters[i + 1]
                      : totalLengthSeconds;
                  const length = Math.max(0, endSec - startSec);
                  return (
                    <TableRow key={i}>
                      <TableCell align="right">{i + 1}</TableCell>
                      <TableCell>{formatLengthSeconds(startSec)}</TableCell>
                      <TableCell>{formatLengthSeconds(length)}</TableCell>
                      <TableCell align="right">{NOT_AVAILABLE}</TableCell>
                      <TableCell align="right">{NOT_AVAILABLE}</TableCell>
                      <TableCell>{NOT_AVAILABLE}</TableCell>
                      <TableCell align="right">{NOT_AVAILABLE}</TableCell>
                      <TableCell>{NOT_AVAILABLE}</TableCell>
                      <TableCell align="right">{NOT_AVAILABLE}</TableCell>
                      <TableCell>{NOT_AVAILABLE}</TableCell>
                      <TableCell align="right">{NOT_AVAILABLE}</TableCell>
                      <TableCell align="right">{NOT_AVAILABLE}</TableCell>
                      <TableCell>{NOT_AVAILABLE}</TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </TableContainer>
        </CardContent>
      </Card>
    </Box>
  );
}
