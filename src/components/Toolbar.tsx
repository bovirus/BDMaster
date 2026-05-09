/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useEffect, useCallback } from "react";
import { Box, ButtonGroup, IconButton, Tooltip } from "@mui/material";
import FolderIcon from "@mui/icons-material/Folder";
import DeleteIcon from "@mui/icons-material/Delete";
import SettingsIcon from "@mui/icons-material/Settings";
import InfoIcon from "@mui/icons-material/Info";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../lib/store";
import * as Protocol from "../lib/protocol";
import { openDiscDirectoryDialog } from "../lib/dialog";

export default function Toolbar() {
  const { t } = useTranslation();
  const disc = useAppStore((state) => state.disc);
  const tabAboutStatus = useAppStore((state) => state.tabAboutStatus);
  const tabSettingsStatus = useAppStore((state) => state.tabSettingsStatus);
  const setTabAboutStatus = useAppStore((state) => state.setTabAboutStatus);
  const setTabSettingsStatus = useAppStore((state) => state.setTabSettingsStatus);
  const clearDisc = useAppStore((state) => state.clearDisc);

  const handleClear = useCallback(() => {
    clearDisc();
  }, [clearDisc]);

  const handleSelectTabSettings = useCallback(() => {
    setTabSettingsStatus(Protocol.ControlStatus.Selected);
  }, [setTabSettingsStatus]);

  const handleSelectTabAbout = useCallback(() => {
    setTabAboutStatus(Protocol.ControlStatus.Selected);
  }, [setTabAboutStatus]);

  useEffect(() => {
    const handleKeyUp = (event: KeyboardEvent) => {
      if (!event.altKey && !event.ctrlKey && !event.shiftKey) {
        if (event.key === "F10") {
          event.stopPropagation();
          handleSelectTabSettings();
        }
      } else if (event.ctrlKey && !event.altKey && !event.shiftKey) {
        if (event.key === "q") {
          event.stopPropagation();
          handleClear();
        }
      }
    };
    document.addEventListener("keyup", handleKeyUp);
    return () => document.removeEventListener("keyup", handleKeyUp);
  }, [handleClear, handleSelectTabSettings]);

  const buttonSx = { width: 28, height: 28, margin: "2px", borderRadius: 1 };
  const activeButtonSx = { ...buttonSx, color: "primary.main" };

  return (
    <Box sx={{ mx: 1, my: 0, display: "flex", gap: 1 }}>
      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.addDisc")}>
          <IconButton sx={buttonSx} onClick={() => openDiscDirectoryDialog()}>
            <FolderIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </ButtonGroup>

      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.clear")}>
          <span>
            <IconButton sx={buttonSx} onClick={handleClear} disabled={disc === null}>
              <DeleteIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
      </ButtonGroup>

      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.settings")}>
          <IconButton
            sx={tabSettingsStatus !== Protocol.ControlStatus.Hidden ? activeButtonSx : buttonSx}
            onClick={handleSelectTabSettings}
          >
            <SettingsIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <Tooltip title={t("toolbar.about")}>
          <IconButton
            sx={tabAboutStatus !== Protocol.ControlStatus.Hidden ? activeButtonSx : buttonSx}
            onClick={handleSelectTabAbout}
          >
            <InfoIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </ButtonGroup>
    </Box>
  );
}
