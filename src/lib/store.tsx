/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { create } from "zustand";
import * as Protocol from "./protocol";
import { getAbout, getConfig } from "./service";

interface DialogNotification {
  title: string;
  type: Protocol.DialogNotificationType;
}

interface AppState {
  config: Protocol.Config | null;
  about: Protocol.About | null;
  dialogNotification: DialogNotification | null;

  // Single Blu-ray disc currently being inspected.
  disc: Protocol.DiscInfo | null;
  scanningPath: string | null;

  // Tab status
  tabAboutStatus: Protocol.ControlStatus;
  tabSettingsStatus: Protocol.ControlStatus;
  tabChaptersStatus: Protocol.ControlStatus;
  tabQuickSummaryStatus: Protocol.ControlStatus;
  // Which playlist's chapters the Chapters tab should display.
  chapterPlaylist: string | null;
  // Which playlist's quick summary the QuickSummary tab should display.
  quickSummaryPlaylist: string | null;

  // Actions
  initConfig: () => Promise<void>;
  initAbout: () => Promise<void>;
  setConfig: (config: Protocol.Config | null) => void;
  setDialogNotification: (n: DialogNotification | null) => void;
  setTabAboutStatus: (s: Protocol.ControlStatus) => void;
  setTabSettingsStatus: (s: Protocol.ControlStatus) => void;
  setTabChaptersStatus: (s: Protocol.ControlStatus) => void;
  setTabQuickSummaryStatus: (s: Protocol.ControlStatus) => void;
  setChapterPlaylist: (name: string | null) => void;
  setQuickSummaryPlaylist: (name: string | null) => void;
  setDisc: (disc: Protocol.DiscInfo | null) => void;
  clearDisc: () => void;
  setScanningPath: (path: string | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
  config: null,
  about: null,
  dialogNotification: null,
  disc: null,
  scanningPath: null,
  tabAboutStatus: Protocol.ControlStatus.Hidden,
  tabSettingsStatus: Protocol.ControlStatus.Hidden,
  tabChaptersStatus: Protocol.ControlStatus.Hidden,
  tabQuickSummaryStatus: Protocol.ControlStatus.Hidden,
  chapterPlaylist: null,
  quickSummaryPlaylist: null,

  initConfig: async () => {
    try {
      const config = await getConfig();
      set({ config });
    } catch (error) {
      console.error("Failed to load config:", error);
    }
  },

  initAbout: async () => {
    try {
      const about = await getAbout();
      set({ about });
    } catch (error) {
      console.error("Failed to load about:", error);
    }
  },

  setConfig: (config) => set({ config }),
  setDialogNotification: (dialogNotification) => set({ dialogNotification }),
  setTabAboutStatus: (tabAboutStatus) => set({ tabAboutStatus }),
  setTabSettingsStatus: (tabSettingsStatus) => set({ tabSettingsStatus }),
  setTabChaptersStatus: (tabChaptersStatus) => set({ tabChaptersStatus }),
  setTabQuickSummaryStatus: (tabQuickSummaryStatus) => set({ tabQuickSummaryStatus }),
  setChapterPlaylist: (chapterPlaylist) => set({ chapterPlaylist }),
  setQuickSummaryPlaylist: (quickSummaryPlaylist) => set({ quickSummaryPlaylist }),
  setDisc: (disc) => set({ disc }),
  clearDisc: () => set({ disc: null }),
  setScanningPath: (scanningPath) => set({ scanningPath }),
}));
