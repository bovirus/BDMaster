/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useMemo } from "react";
import {
  Box,
  Button,
  Card,
  CardActions,
  CardContent,
  Chip,
  CircularProgress,
  IconButton,
  Stack,
  Tooltip,
  Typography,
} from "@mui/material";
import FolderIcon from "@mui/icons-material/Folder";
import GitHubIcon from "@mui/icons-material/GitHub";
import DeleteIcon from "@mui/icons-material/Delete";
import OpenInNewIcon from "@mui/icons-material/OpenInNew";
import { useTranslation } from "react-i18next";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { useAppStore } from "../lib/store";
import { openDiscDirectoryDialog } from "../lib/dialog";
import { formatSize } from "../lib/format";

const GITHUB_URL = "https://github.com/caoccao/BDMaster";

export default function Cards() {
  const { t } = useTranslation();
  const discs = useAppStore((s) => s.discs);
  const removeDisc = useAppStore((s) => s.removeDisc);
  const setSelectedDiscPath = useAppStore((s) => s.setSelectedDiscPath);
  const scanningPaths = useAppStore((s) => s.scanningPaths);

  const isScanning = scanningPaths.size > 0;
  const scanningList = useMemo(() => Array.from(scanningPaths), [scanningPaths]);

  if (discs.length === 0 && !isScanning) {
    return (
      <Box sx={{ display: "flex", alignItems: "center", justifyContent: "center", flex: 1, p: 4 }}>
        <Stack spacing={3} sx={{ alignItems: "center", textAlign: "center", maxWidth: 600 }}>
          <Typography variant="h4" sx={{ fontWeight: 700 }}>
            {t("cards.welcomeTitle")}
          </Typography>
          <Typography variant="body1" color="text.secondary">
            {t("cards.welcomeSubtitle")}
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {t("cards.introBDMaster")}
          </Typography>
          <Stack direction="row" spacing={2}>
            <Button
              variant="contained"
              startIcon={<FolderIcon />}
              onClick={() => openDiscDirectoryDialog()}
            >
              {t("cards.addDisc")}
            </Button>
            <Button
              variant="outlined"
              startIcon={<GitHubIcon />}
              onClick={() => shellOpen(GITHUB_URL)}
            >
              {t("cards.viewOnGithub")}
            </Button>
          </Stack>
          <Typography variant="caption" color="text.secondary">
            {t("cards.emptyHint")}
          </Typography>
        </Stack>
      </Box>
    );
  }

  return (
    <Box sx={{ p: 2 }}>
      {scanningList.map((path) => (
        <Box
          key={path}
          sx={{
            display: "flex",
            alignItems: "center",
            gap: 2,
            p: 1.5,
            mb: 1,
            border: 1,
            borderColor: "divider",
            borderRadius: 1,
            bgcolor: "action.hover",
          }}
        >
          <CircularProgress size={18} />
          <Typography variant="body2">{t("cards.scanning", { path })}</Typography>
        </Box>
      ))}

      <Box
        sx={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
          gap: 2,
        }}
      >
        {discs.map((disc) => {
          const title = disc.discTitle || disc.metaTitle || disc.volumeLabel || disc.discName;
          const playlistCount = disc.playlists.length;
          const videoCount = disc.playlists.reduce((s, p) => s + p.videoStreams.length, 0);
          const audioCount = disc.playlists.reduce((s, p) => s + p.audioStreams.length, 0);
          const subCount = disc.playlists.reduce(
            (s, p) => s + p.graphicsStreams.length + p.textStreams.length,
            0
          );
          return (
            <Card
              key={disc.path}
              variant="outlined"
              sx={{
                transition: "transform 0.2s, box-shadow 0.2s",
                "&:hover": { transform: "translateY(-2px)", boxShadow: 3 },
              }}
            >
              <CardContent>
                <Typography variant="h6" noWrap title={title}>
                  {title}
                </Typography>
                <Typography variant="caption" color="text.secondary" noWrap title={disc.path}>
                  {disc.path}
                </Typography>
                <Stack direction="row" spacing={0.5} sx={{ mt: 1, flexWrap: "wrap", gap: 0.5 }}>
                  {disc.isUHD && <Chip size="small" label={t("disc.isUHD")} />}
                  {disc.is4K && <Chip size="small" label={t("disc.is4K")} />}
                  {disc.is3D && <Chip size="small" label={t("disc.is3D")} />}
                  {disc.is50Hz && <Chip size="small" label={t("disc.is50Hz")} />}
                  {disc.isBdJava && <Chip size="small" label={t("disc.hasBdJava")} />}
                  {disc.isBdPlus && <Chip size="small" label={t("disc.hasBdPlus")} />}
                  {disc.hasMVCExtension && <Chip size="small" label={t("disc.hasMVCExtension")} />}
                  {disc.hasHEVCStreams && <Chip size="small" label={t("disc.hasHEVCStreams")} />}
                </Stack>
                <Box sx={{ mt: 1.5, display: "grid", gridTemplateColumns: "1fr 1fr", gap: 0.5 }}>
                  <Typography variant="caption" color="text.secondary">{t("cards.size")}:</Typography>
                  <Typography variant="caption">{formatSize(disc.size)}</Typography>
                  <Typography variant="caption" color="text.secondary">{t("cards.playlists")}:</Typography>
                  <Typography variant="caption">{playlistCount}</Typography>
                  <Typography variant="caption" color="text.secondary">{t("cards.videoCount")}:</Typography>
                  <Typography variant="caption">{videoCount}</Typography>
                  <Typography variant="caption" color="text.secondary">{t("cards.audioCount")}:</Typography>
                  <Typography variant="caption">{audioCount}</Typography>
                  <Typography variant="caption" color="text.secondary">{t("cards.subtitleCount")}:</Typography>
                  <Typography variant="caption">{subCount}</Typography>
                </Box>
              </CardContent>
              <CardActions>
                <Button
                  size="small"
                  startIcon={<OpenInNewIcon />}
                  onClick={() => setSelectedDiscPath(disc.path)}
                >
                  {t("cards.open")}
                </Button>
                <Box sx={{ flex: 1 }} />
                <Tooltip title={t("cards.remove")}>
                  <IconButton size="small" onClick={() => removeDisc(disc.path)}>
                    <DeleteIcon fontSize="small" />
                  </IconButton>
                </Tooltip>
              </CardActions>
            </Card>
          );
        })}
      </Box>
    </Box>
  );
}
