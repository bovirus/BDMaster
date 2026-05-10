/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useEffect, useState } from "react";
import {
  Box,
  Button,
  Card,
  CardContent,
  CardHeader,
  Stack,
  Typography,
} from "@mui/material";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import SaveIcon from "@mui/icons-material/Save";
import { useTranslation } from "react-i18next";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import * as Protocol from "../lib/protocol";
import { useAppStore } from "../lib/store";
import { generateReport, writeTextFile } from "../lib/service";
import { openSaveReportDialog } from "../lib/dialog";

export default function QuickSummaryTab() {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);
  const playlistName = useAppStore((s) => s.quickSummaryPlaylist);
  const setNotification = useAppStore((s) => s.setDialogNotification);
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (!disc || !playlistName) {
      setText(null);
      return;
    }
    setText(null);
    setError(null);
    generateReport(disc.path, false, [playlistName])
      .then((result) => {
        if (!cancelled) setText(result);
      })
      .catch((e) => {
        if (!cancelled) setError(`${e}`);
      });
    return () => {
      cancelled = true;
    };
  }, [disc, playlistName]);

  const handleCopy = async () => {
    if (!text) return;
    await writeText(text);
    setNotification({
      title: "Report copied to clipboard.",
      type: Protocol.DialogNotificationType.Info,
    });
  };

  const handleSave = async () => {
    if (!text) return;
    const filePath = await openSaveReportDialog();
    if (filePath) {
      try {
        await writeTextFile(filePath as string, text);
        setNotification({
          title: `Saved to ${filePath}`,
          type: Protocol.DialogNotificationType.Info,
        });
      } catch (e) {
        setNotification({
          title: `${e}`,
          type: Protocol.DialogNotificationType.Error,
        });
      }
    }
  };

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
          action={
            <Stack direction="row" spacing={1}>
              <Button
                size="small"
                variant="outlined"
                startIcon={<ContentCopyIcon />}
                onClick={handleCopy}
                disabled={!text}
              >
                {t("disc.copy")}
              </Button>
              <Button
                size="small"
                variant="outlined"
                startIcon={<SaveIcon />}
                onClick={handleSave}
                disabled={!text}
              >
                {t("disc.save")}
              </Button>
            </Stack>
          }
          sx={{ py: 1, "& .MuiCardHeader-action": { alignSelf: "center", mt: 0, mr: 0 } }}
        />
        <CardContent sx={{ flex: 1, minHeight: 0, overflow: "auto", pt: 0, "&:last-child": { pb: 1 } }}>
          <Box
            component="pre"
            sx={{
              fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
              fontSize: "0.75rem",
              whiteSpace: "pre-wrap",
              m: 0,
            }}
          >
            {error ? error : text ?? "…"}
          </Box>
        </CardContent>
      </Card>
    </Box>
  );
}
