/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useState, useEffect, useRef } from "react";
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
import SummarizeIcon from "@mui/icons-material/Summarize";
import DescriptionIcon from "@mui/icons-material/Description";
import ShowChartIcon from "@mui/icons-material/ShowChart";
import { useTranslation } from "react-i18next";
import { getCurrentWindow, type DragDropEvent } from "@tauri-apps/api/window";
import type { Event, UnlistenFn } from "@tauri-apps/api/event";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import * as Protocol from "../lib/protocol";
import { useAppStore, type OpenTab } from "../lib/store";
import { scanDiscPaths } from "../lib/fs";
import { getLaunchArgs, getUpdateResult, skipVersion } from "../lib/service";
import DiscInfoTab from "./DiscInfoTab";
import Config from "./Config";
import About from "./About";
import QuickSummaryTab from "./QuickSummaryTab";
import FullReportTab from "./FullReportTab";
import BitRateTab from "./BitRateTab";

const RELEASES_URL = "https://github.com/caoccao/BDMaster/releases";

type PlaylistDetailView = "quickSummary" | "fullReport" | "bitRate";

function PlaylistDetailTab({ playlistName }: { playlistName: string | null }) {
  const { t } = useTranslation();
  const disc = useAppStore((state) => state.disc);
  const fullScanCompletedFor = useAppStore((state) => state.fullScanCompletedFor);
  const [activeView, setActiveView] = useState<PlaylistDetailView>("quickSummary");
  const isBitRateAvailable = !!disc && fullScanCompletedFor === disc.path;

  useEffect(() => {
    if (!isBitRateAvailable && activeView === "bitRate") {
      setActiveView("quickSummary");
    }
  }, [activeView, isBitRateAvailable]);

  return (
    <Box sx={{ display: "flex", flex: 1, minHeight: 0, overflow: "hidden" }}>
      <Tabs
        orientation="vertical"
        value={activeView}
        onChange={(_, value) => setActiveView(value)}
        sx={{
          borderRight: 1,
          borderColor: "divider",
          flexShrink: 0,
          minWidth: 180,
          "& .MuiTab-root": {
            alignItems: "center",
            justifyContent: "flex-start",
            minHeight: 40,
            px: 1.5,
            textAlign: "left",
            textTransform: "none",
          },
          "& .MuiTab-iconWrapper": {
            minWidth: 24,
            mr: 1,
          },
        }}
      >
        <Tab
          value="quickSummary"
          icon={<SummarizeIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t("tabs.quickSummary")}
        />
        <Tab
          value="fullReport"
          icon={<DescriptionIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t("tabs.fullReport")}
        />
        <Tab
          value="bitRate"
          disabled={!isBitRateAvailable}
          icon={<ShowChartIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t("tabs.bitRate")}
          title={!isBitRateAvailable ? t("disc.scan") : undefined}
        />
      </Tabs>
      <Box sx={{ flex: 1, minWidth: 0, minHeight: 0, overflow: "auto", pl: 1 }}>
        {activeView === "quickSummary" && <QuickSummaryTab playlistName={playlistName} />}
        {activeView === "fullReport" && <FullReportTab playlistName={playlistName} />}
        {activeView === "bitRate" && <BitRateTab playlistName={playlistName} />}
      </Box>
    </Box>
  );
}

export default function MainContent() {
  const { t } = useTranslation();
  const openTabs = useAppStore((state) => state.openTabs);
  const activeTabIndex = useAppStore((state) => state.activeTabIndex);
  const setActiveTabIndex = useAppStore((state) => state.setActiveTabIndex);
  const closeTab = useAppStore((state) => state.closeTab);
  const disc = useAppStore((state) => state.disc);
  const fullScanCompletedFor = useAppStore((state) => state.fullScanCompletedFor);
  const isBitRateAvailable = !!disc && fullScanCompletedFor === disc.path;

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

  useEffect(() => {
    if (openTabs[activeTabIndex]?.type === Protocol.TabType.BitRate && !isBitRateAvailable) {
      setActiveTabIndex(0);
    }
  }, [activeTabIndex, isBitRateAvailable, openTabs, setActiveTabIndex]);

  // Keyboard shortcuts.
  useEffect(() => {
    const handleKeyUp = (event: KeyboardEvent) => {
      if (event.ctrlKey && !event.altKey && !event.shiftKey) {
        if (event.key >= "1" && event.key <= "9") {
          const newTabIndex = parseInt(event.key) - 1;
          if (newTabIndex >= 0 && newTabIndex < openTabs.length) {
            event.stopPropagation();
            setActiveTabIndex(newTabIndex);
          }
        } else if (event.key === "w") {
          event.stopPropagation();
          closeTab(activeTabIndex);
        } else if (event.key === "Tab") {
          event.stopPropagation();
          const next = activeTabIndex >= openTabs.length - 1 ? 0 : activeTabIndex + 1;
          setActiveTabIndex(next);
        }
      } else if (event.ctrlKey && !event.altKey && event.shiftKey) {
        if (event.key === "Tab") {
          event.stopPropagation();
          const prev = activeTabIndex > 0 ? activeTabIndex - 1 : openTabs.length - 1;
          setActiveTabIndex(prev);
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
  }, [activeTabIndex, openTabs.length, setActiveTabIndex, closeTab]);

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

  // CLI launch args (e.g. `BDMaster.exe D:\Movie\BDMV` or a file inside it).
  // The backend resolves files to their parent folder so any path under the
  // disc tree works.
  useEffect(() => {
    getLaunchArgs().then((args) => {
      if (args.length > 0) scanDiscPaths(args);
    });
  }, []);

  const renderTabLabel = (tab: OpenTab) => {
    switch (tab.type) {
      case Protocol.TabType.About:
        return <span>{t("tabs.about")}</span>;
      case Protocol.TabType.Config:
        return <span>{t("tabs.settings")}</span>;
      case Protocol.TabType.DiscInfo:
        return <span>{t("tabs.discInfo")}</span>;
      case Protocol.TabType.Playlist:
        return <span>{tab.playlistName ?? ""}</span>;
      case Protocol.TabType.QuickSummary:
        return (
          <Tooltip title={t("tabs.quickSummary")}>
            <Box sx={{ display: "flex", alignItems: "center", gap: 0.5 }}>
              <SummarizeIcon sx={{ fontSize: 16 }} />
              <span>{tab.playlistName ?? ""}</span>
            </Box>
          </Tooltip>
        );
      case Protocol.TabType.FullReport:
        return (
          <Tooltip title={t("tabs.fullReport")}>
            <Box sx={{ display: "flex", alignItems: "center", gap: 0.5 }}>
              <DescriptionIcon sx={{ fontSize: 16 }} />
              <span>{tab.playlistName ?? ""}</span>
            </Box>
          </Tooltip>
        );
      case Protocol.TabType.BitRate:
        return (
          <Tooltip title={t("tabs.bitRate")}>
            <Box sx={{ display: "flex", alignItems: "center", gap: 0.5 }}>
              <ShowChartIcon sx={{ fontSize: 16 }} />
              <span>{tab.playlistName ?? ""}</span>
            </Box>
          </Tooltip>
        );
    }
  };

  const renderTabContent = (tab: OpenTab) => {
    switch (tab.type) {
      case Protocol.TabType.About:
        return <About />;
      case Protocol.TabType.Config:
        return <Config />;
      case Protocol.TabType.DiscInfo:
        return <DiscInfoTab />;
      case Protocol.TabType.Playlist:
        return <PlaylistDetailTab playlistName={tab.playlistName ?? null} />;
      case Protocol.TabType.QuickSummary:
        return <QuickSummaryTab playlistName={tab.playlistName ?? null} />;
      case Protocol.TabType.FullReport:
        return <FullReportTab playlistName={tab.playlistName ?? null} />;
      case Protocol.TabType.BitRate:
        return <BitRateTab playlistName={tab.playlistName ?? null} />;
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
          value={activeTabIndex}
          onChange={(_, v) => setActiveTabIndex(v)}
          variant="scrollable"
          scrollButtons="auto"
          sx={{ mt: 0, minHeight: "24px", "& .MuiTab-root": { textTransform: "none" } }}
        >
          {openTabs.map((tab, index) => (
            <Tab
              key={`${tab.type}:${tab.playlistName ?? ""}`}
              style={{ minHeight: "24px" }}
              label={
                <Box sx={{ display: "flex", alignItems: "center", gap: 0.5 }}>
                  {renderTabLabel(tab)}
                  {tab.type !== Protocol.TabType.DiscInfo && (
                    <Tooltip title={t("tabs.close")}>
                      <IconButton
                        size="small"
                        sx={{ ml: 0.5, p: 0.25 }}
                        onClick={(e) => {
                          e.stopPropagation();
                          closeTab(index);
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
        {openTabs.map((tab, index) => {
          const isVisible = index === activeTabIndex;
          return (
            <Box
              key={`content-${tab.type}:${tab.playlistName ?? ""}`}
              sx={{
                display: isVisible ? "flex" : "none",
                flexDirection: "column",
                flex: 1,
                minHeight: 0,
                overflow: "auto",
              }}
            >
              {renderTabContent(tab)}
            </Box>
          );
        })}
      </Box>
    </Box>
  );
}
