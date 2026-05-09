/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useState, useEffect, useCallback, useRef } from "react";
import {
  Alert,
  Box,
  Checkbox,
  FormControlLabel,
  IconButton,
  Link,
  Tab,
  Tabs,
  Tooltip,
} from "@mui/material";
import CloseIcon from "@mui/icons-material/Close";
import { useTranslation } from "react-i18next";
import { getCurrentWindow, type DragDropEvent } from "@tauri-apps/api/window";
import type { Event, UnlistenFn } from "@tauri-apps/api/event";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import * as Protocol from "../lib/protocol";
import { useAppStore } from "../lib/store";
import { scanDiscPaths } from "../lib/fs";
import { getUpdateResult, skipVersion } from "../lib/service";
import DiscInfoTab from "./DiscInfoTab";
import Config from "./Config";
import About from "./About";
import ChaptersTab from "./ChaptersTab";

const RELEASES_URL = "https://github.com/caoccao/BDMaster/releases";

interface TabControl {
  type: Protocol.TabType;
  index: number;
}

export default function MainContent() {
  const { t } = useTranslation();
  const [tabIndex, setTabIndex] = useState(0);
  const [tabControls, setTabControls] = useState<TabControl[]>([
    { type: Protocol.TabType.DiscInfo, index: 0 },
  ]);

  const tabAboutStatus = useAppStore((state) => state.tabAboutStatus);
  const tabSettingsStatus = useAppStore((state) => state.tabSettingsStatus);
  const tabChaptersStatus = useAppStore((state) => state.tabChaptersStatus);
  const setTabAboutStatus = useAppStore((state) => state.setTabAboutStatus);
  const setTabSettingsStatus = useAppStore((state) => state.setTabSettingsStatus);
  const setTabChaptersStatus = useAppStore((state) => state.setTabChaptersStatus);

  const [newVersion, setNewVersion] = useState<string | null>(null);
  const [skipChecked, setSkipChecked] = useState(false);
  const updatePollRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined);

  // Poll the backend's update-check result until we get a definitive answer.
  useEffect(() => {
    updatePollRef.current = setInterval(async () => {
      try {
        const result = await getUpdateResult();
        if (result) {
          if (updatePollRef.current) {
            clearInterval(updatePollRef.current);
            updatePollRef.current = undefined;
          }
          if (result.hasUpdate && result.latestVersion) {
            setNewVersion(result.latestVersion);
          }
        }
      } catch {
        // Ignore errors; we'll retry on the next interval until success.
      }
    }, 1000);
    return () => {
      if (updatePollRef.current) {
        clearInterval(updatePollRef.current);
        updatePollRef.current = undefined;
      }
    };
  }, []);

  // Update tab controls when status changes.
  useEffect(() => {
    setTabControls((prev) => {
      const controls: TabControl[] = [{ type: Protocol.TabType.DiscInfo, index: 0 }];
      if (tabAboutStatus !== Protocol.ControlStatus.Hidden) {
        controls.push({ type: Protocol.TabType.About, index: 0 });
      }
      if (tabSettingsStatus !== Protocol.ControlStatus.Hidden) {
        controls.push({ type: Protocol.TabType.Config, index: 0 });
      }
      if (tabChaptersStatus !== Protocol.ControlStatus.Hidden) {
        controls.push({ type: Protocol.TabType.Chapters, index: 0 });
      }
      controls.forEach((c, i) => (c.index = i));

      const current = prev[tabIndex];
      if (current) {
        const newIdx = controls.findIndex((c) => c.type === current.type);
        if (newIdx >= 0 && newIdx !== tabIndex) {
          setTabIndex(newIdx);
        }
      }
      return controls;
    });
  }, [tabAboutStatus, tabSettingsStatus, tabChaptersStatus]);

  // Handle Selected status: jump to that tab.
  useEffect(() => {
    if (tabAboutStatus === Protocol.ControlStatus.Selected) {
      const t = tabControls.find((c) => c.type === Protocol.TabType.About);
      if (t) {
        setTabIndex(t.index);
        setTabAboutStatus(Protocol.ControlStatus.Visible);
      }
    }
  }, [tabAboutStatus, tabControls, setTabAboutStatus]);

  useEffect(() => {
    if (tabSettingsStatus === Protocol.ControlStatus.Selected) {
      const t = tabControls.find((c) => c.type === Protocol.TabType.Config);
      if (t) {
        setTabIndex(t.index);
        setTabSettingsStatus(Protocol.ControlStatus.Visible);
      }
    }
  }, [tabSettingsStatus, tabControls, setTabSettingsStatus]);

  useEffect(() => {
    if (tabChaptersStatus === Protocol.ControlStatus.Selected) {
      const t = tabControls.find((c) => c.type === Protocol.TabType.Chapters);
      if (t) {
        setTabIndex(t.index);
        setTabChaptersStatus(Protocol.ControlStatus.Visible);
      }
    }
  }, [tabChaptersStatus, tabControls, setTabChaptersStatus]);

  // Keep tabIndex within bounds.
  useEffect(() => {
    if (tabIndex >= tabControls.length && tabControls.length > 0) {
      setTabIndex(tabControls.length - 1);
    }
  }, [tabIndex, tabControls.length]);

  const closeTab = useCallback(
    (index: number) => {
      const tabControl = tabControls[index];
      if (!tabControl) return;
      switch (tabControl.type) {
        case Protocol.TabType.About:
          setTabAboutStatus(Protocol.ControlStatus.Hidden);
          break;
        case Protocol.TabType.Config:
          setTabSettingsStatus(Protocol.ControlStatus.Hidden);
          break;
        case Protocol.TabType.Chapters:
          setTabChaptersStatus(Protocol.ControlStatus.Hidden);
          break;
      }
    },
    [tabControls, setTabAboutStatus, setTabSettingsStatus, setTabChaptersStatus]
  );

  // Keyboard shortcuts.
  useEffect(() => {
    const handleKeyUp = (event: KeyboardEvent) => {
      if (event.ctrlKey && !event.altKey && !event.shiftKey) {
        if (event.key >= "1" && event.key <= "9") {
          const newTabIndex = parseInt(event.key) - 1;
          if (newTabIndex >= 0 && newTabIndex < tabControls.length) {
            event.stopPropagation();
            setTabIndex(newTabIndex);
          }
        } else if (event.key === "w") {
          event.stopPropagation();
          closeTab(tabIndex);
        } else if (event.key === "Tab") {
          event.stopPropagation();
          setTabIndex((prev) => (prev >= tabControls.length - 1 ? 0 : prev + 1));
        }
      } else if (event.ctrlKey && !event.altKey && event.shiftKey) {
        if (event.key === "Tab") {
          event.stopPropagation();
          setTabIndex((prev) => (prev > 0 ? prev - 1 : tabControls.length - 1));
        }
      } else if (!event.ctrlKey && event.altKey && !event.shiftKey) {
        if (event.key === "x") {
          event.stopPropagation();
          getCurrentWindow().close();
        }
      }
    };
    document.addEventListener("keyup", handleKeyUp);
    return () => document.removeEventListener("keyup", handleKeyUp);
  }, [tabIndex, tabControls.length, closeTab]);

  // Drag-and-drop.
  useEffect(() => {
    let cancelFileDrop: UnlistenFn | null = null;
    getCurrentWindow()
      .onDragDropEvent((event: Event<DragDropEvent>) => {
        if (event.payload.type === "drop") {
          scanDiscPaths(event.payload.paths);
        }
      })
      .then((value) => {
        cancelFileDrop = value;
      });
    return () => {
      if (cancelFileDrop) cancelFileDrop();
    };
  }, []);

  const getTabLabel = (control: TabControl) => {
    switch (control.type) {
      case Protocol.TabType.About: return t("tabs.about");
      case Protocol.TabType.Config: return t("tabs.settings");
      case Protocol.TabType.DiscInfo: return t("tabs.discInfo");
      case Protocol.TabType.Chapters: return t("tabs.chapters");
    }
  };

  return (
    <Box sx={{ width: "100%", height: "100%", overflow: "hidden", display: "flex", flexDirection: "column" }}>
      {newVersion && (
        <Alert
          severity="info"
          onClose={async () => {
            if (skipChecked) {
              await skipVersion(newVersion);
            }
            setNewVersion(null);
            setSkipChecked(false);
          }}
          sx={{ flexShrink: 0, "& .MuiAlert-message": { flex: 1 } }}
        >
          <Box sx={{ display: "flex", alignItems: "center", gap: 1 }}>
            <Link
              component="button"
              variant="body2"
              onClick={() => shellOpen(RELEASES_URL)}
              sx={{ cursor: "pointer" }}
            >
              {t("update.newVersionAvailable", { version: newVersion })}
            </Link>
            <Box sx={{ flex: 1 }} />
            <FormControlLabel
              control={
                <Checkbox
                  size="small"
                  sx={{ p: 0.5 }}
                  checked={skipChecked}
                  onChange={(e) => setSkipChecked(e.target.checked)}
                />
              }
              label={t("update.skipThisVersion")}
              slotProps={{ typography: { variant: "body2" } }}
              sx={{ mr: 0 }}
            />
          </Box>
        </Alert>
      )}
      <Box sx={{ borderBottom: 1, borderColor: "divider", flexShrink: 0 }}>
        <Tabs
          value={tabIndex}
          onChange={(_, v) => setTabIndex(v)}
          variant="scrollable"
          scrollButtons="auto"
          sx={{ mt: 0, minHeight: "24px", "& .MuiTab-root": { textTransform: "none" } }}
        >
          {tabControls.map((control) => (
            <Tab
              key={control.type}
              style={{ minHeight: "24px" }}
              label={
                <Box sx={{ display: "flex", alignItems: "center", gap: 0.5 }}>
                  <span>{getTabLabel(control)}</span>
                  {control.type !== Protocol.TabType.DiscInfo && (
                    <Tooltip title={t("tabs.close")}>
                      <IconButton
                        size="small"
                        sx={{ ml: 0.5, p: 0.25 }}
                        onClick={(e) => {
                          e.stopPropagation();
                          closeTab(control.index);
                        }}
                      >
                        <CloseIcon sx={{ fontSize: 14 }} />
                      </IconButton>
                    </Tooltip>
                  )}
                </Box>
              }
              sx={{ py: 0, my: 0 }}
            />
          ))}
        </Tabs>
      </Box>

      <Box
        sx={{
          p: 1,
          border: 1,
          borderColor: "divider",
          borderTop: 0,
          borderRadius: "0 0 4px 4px",
          width: "100%",
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        {tabControls.map((control) => {
          const isVisible = control.index === tabIndex;
          return (
            <Box
              key={`content-${control.type}`}
              sx={{
                display: isVisible ? "flex" : "none",
                flexDirection: "column",
                flex: 1,
                minHeight: 0,
                overflow: "auto",
              }}
            >
              {control.type === Protocol.TabType.About && <About />}
              {control.type === Protocol.TabType.Config && <Config />}
              {control.type === Protocol.TabType.DiscInfo && <DiscInfoTab />}
              {control.type === Protocol.TabType.Chapters && <ChaptersTab />}
            </Box>
          );
        })}
      </Box>
    </Box>
  );
}
