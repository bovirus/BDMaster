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
        </CardContent>
      </Card>
    </Box>
  );
}
