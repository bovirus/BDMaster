/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useMemo, useState } from "react";
import {
  Box,
  Button,
  Card,
  CardContent,
  CardHeader,
  Checkbox,
  FormControlLabel,
  Stack,
  Typography,
} from "@mui/material";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import SaveIcon from "@mui/icons-material/Save";
import { useTranslation } from "react-i18next";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import * as Protocol from "../lib/protocol";
import { useAppStore } from "../lib/store";
import { writeTextFile } from "../lib/service";
import { openSaveReportDialog } from "../lib/dialog";
import {
  generateQuickSummaryReport,
  generateQuickSummaryReportDocument,
} from "../lib/report";
import { createTranslatedReportLabels } from "../lib/reportI18n";
import ReportDocumentView from "./ReportDocumentView";

export default function QuickSummaryTab({ playlistName }: { playlistName: string | null }) {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);
  const config = useAppStore((s) => s.config);
  const setNotification = useAppStore((s) => s.setDialogNotification);
  const [showText, setShowText] = useState(false);
  const reportLabels = useMemo(() => createTranslatedReportLabels(t), [t]);

  const text = useMemo(() => {
    if (!disc || !playlistName) return null;
    return generateQuickSummaryReport(disc, [playlistName], config?.formatting, reportLabels);
  }, [disc, playlistName, config?.formatting, reportLabels]);

  const document = useMemo(() => {
    if (!disc || !playlistName) return null;
    return generateQuickSummaryReportDocument(disc, [playlistName], config?.formatting, reportLabels);
  }, [disc, playlistName, config?.formatting, reportLabels]);

  const handleCopy = async () => {
    if (!text) return;
    await writeText(text);
    setNotification({
      title: t("disc.reportCopied"),
      type: Protocol.DialogNotificationType.Info,
    });
  };

  const handleSave = async () => {
    if (!text) return;
    const filePath = await openSaveReportDialog("text");
    if (filePath) {
      try {
        await writeTextFile(filePath as string, text);
        setNotification({
          title: t("disc.savedTo", { path: filePath }),
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
            <Stack direction="row" spacing={1} sx={{ alignItems: "center" }}>
              <FormControlLabel
                label={t("fileFilter.text")}
                control={
                  <Checkbox
                    size="small"
                    checked={showText}
                    onChange={(event) => setShowText(event.target.checked)}
                  />
                }
                sx={{ mr: 0.5 }}
              />
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
        <CardContent
          sx={{ flex: 1, minHeight: 0, overflow: "auto", pt: 0, "&:last-child": { pb: 1 } }}
        >
          {showText ? (
            <Box
              component="pre"
              sx={{
                fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
                fontSize: "0.75rem",
                whiteSpace: "pre-wrap",
                m: 0,
              }}
            >
              {text ?? "-"}
            </Box>
          ) : document ? (
            <ReportDocumentView document={document} />
          ) : (
            <Typography variant="body2" color="text.secondary">-</Typography>
          )}
        </CardContent>
      </Card>
    </Box>
  );
}
