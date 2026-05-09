/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useMemo } from "react";
import {
  Box,
  Paper,
  Stack,
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

export default function ChaptersTab() {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);
  const playlistName = useAppStore((s) => s.chapterPlaylist);

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

  return (
    <Box sx={{ p: 1, display: "flex", flexDirection: "column", gap: 1, height: "100%" }}>
      <Stack direction="row" spacing={3} sx={{ flexWrap: "wrap" }}>
        <Typography variant="caption">
          <b>{t("disc.playlist")}:</b> {playlist.name}
        </Typography>
        <Typography variant="caption">
          <b>{t("disc.chapters")}:</b> {playlist.chapters.length}
        </Typography>
      </Stack>
      <Paper variant="outlined" sx={{ flex: 1, minHeight: 0, overflow: "auto" }}>
        <TableContainer>
          <Table size="small" stickyHeader>
            <TableHead>
              <TableRow>
                <TableCell sx={{ fontWeight: "bold" }} align="right">#</TableCell>
                <TableCell sx={{ fontWeight: "bold" }}>{t("disc.length")}</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {playlist.chapters.map((sec, i) => (
                <TableRow key={i}>
                  <TableCell align="right">{i + 1}</TableCell>
                  <TableCell>{formatLengthSeconds(sec)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      </Paper>
    </Box>
  );
}
