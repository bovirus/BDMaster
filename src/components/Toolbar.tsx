/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useEffect, useCallback } from "react";
import { Box, ButtonGroup, Divider, IconButton, Tooltip } from "@mui/material";
import AlbumIcon from "@mui/icons-material/Album";
import FolderIcon from "@mui/icons-material/Folder";
import DeleteIcon from "@mui/icons-material/Delete";
import SettingsIcon from "@mui/icons-material/Settings";
import InfoIcon from "@mui/icons-material/Info";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../lib/store";
import * as Protocol from "../lib/protocol";
import { openDiscDirectoryDialog, openDiscFileDialog } from "../lib/dialog";
import { cancelFullScan } from "../lib/service";

export default function Toolbar() {
  const { t } = useTranslation();
  const disc = useAppStore((state) => state.disc);
  const openTab = useAppStore((state) => state.openTab);
  const aboutOpen = useAppStore((state) =>
    state.openTabs.some((tab) => tab.type === Protocol.TabType.About)
  );
  const settingsOpen = useAppStore((state) =>
    state.openTabs.some((tab) => tab.type === Protocol.TabType.Config)
  );
  const clearDisc = useAppStore((state) => state.clearDisc);

  const handleClear = useCallback(() => {
    // Cancel any in-flight full scan first so the worker thread releases
    // the M2TS reader before we wipe the disc state from the store.
    cancelFullScan().catch(() => {});
    clearDisc();
  }, [clearDisc]);

  const handleSelectTabSettings = useCallback(() => {
    openTab(Protocol.TabType.Config);
  }, [openTab]);

  const handleSelectTabAbout = useCallback(() => {
    openTab(Protocol.TabType.About);
  }, [openTab]);

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
    <Box sx={{ mx: 1, my: 0, display: "flex", gap: 1, alignItems: "center" }}>
      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.addDisk")}>
          <IconButton sx={buttonSx} onClick={() => openDiscFileDialog()}>
            <AlbumIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <Tooltip title={t("toolbar.addDiskFolder")}>
          <IconButton sx={buttonSx} onClick={() => openDiscDirectoryDialog()}>
            <FolderIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </ButtonGroup>

      <Divider orientation="vertical" flexItem />
      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.clear")}>
          <span>
            <IconButton sx={buttonSx} onClick={handleClear} disabled={disc === null}>
              <DeleteIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
      </ButtonGroup>
      <Divider orientation="vertical" flexItem />

      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.settings")}>
          <IconButton
            sx={settingsOpen ? activeButtonSx : buttonSx}
            onClick={handleSelectTabSettings}
          >
            <SettingsIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <Tooltip title={t("toolbar.about")}>
          <IconButton
            sx={aboutOpen ? activeButtonSx : buttonSx}
            onClick={handleSelectTabAbout}
          >
            <InfoIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </ButtonGroup>
    </Box>
  );
}
