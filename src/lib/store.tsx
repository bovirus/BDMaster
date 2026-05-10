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

/**
 * One open tab in the main content area. Playlist tabs are keyed by playlist
 * name; their internal report/chapter/bitrate views are handled by a nested
 * tab container. About/Config/DiscInfo have no playlist.
 */
export interface OpenTab {
  type: Protocol.TabType;
  playlistName?: string;
}

interface AppState {
  config: Protocol.Config | null;
  about: Protocol.About | null;
  dialogNotification: DialogNotification | null;

  // Single Blu-ray disc currently being inspected.
  disc: Protocol.DiscInfo | null;
  scanningPath: string | null;

  // Full-scan state. `fullScanProgress` is the latest snapshot from the
  // backend's polling endpoint; `fullScanCompletedFor` records the disc path
  // for which a full scan has finished so the badge sticks across tab
  // switches until a new disc is loaded.
  fullScanProgress: Protocol.ScanProgress | null;
  fullScanCompletedFor: string | null;

  // Tabs: index 0 is always DiscInfo and is non-closable.
  openTabs: OpenTab[];
  activeTabIndex: number;

  // Actions
  initConfig: () => Promise<void>;
  initAbout: () => Promise<void>;
  setConfig: (config: Protocol.Config | null) => void;
  setDialogNotification: (n: DialogNotification | null) => void;
  setDisc: (disc: Protocol.DiscInfo | null) => void;
  clearDisc: () => void;
  setScanningPath: (path: string | null) => void;
  setFullScanProgress: (p: Protocol.ScanProgress | null) => void;
  setFullScanCompletedFor: (path: string | null) => void;

  /** Open or focus a tab. Reuses an existing tab with the same
   *  `(type, playlistName)` key; otherwise appends a new tab and selects it. */
  openTab: (type: Protocol.TabType, playlistName?: string) => void;
  closeTab: (index: number) => void;
  setActiveTabIndex: (index: number) => void;
}

const DEFAULT_TABS: OpenTab[] = [{ type: Protocol.TabType.DiscInfo }];

function tabsEqual(a: OpenTab, type: Protocol.TabType, playlistName?: string): boolean {
  return a.type === type && (a.playlistName ?? null) === (playlistName ?? null);
}

export const useAppStore = create<AppState>((set) => ({
  config: null,
  about: null,
  dialogNotification: null,
  disc: null,
  scanningPath: null,
  fullScanProgress: null,
  fullScanCompletedFor: null,

  openTabs: DEFAULT_TABS,
  activeTabIndex: 0,

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
  setDisc: (disc) =>
    set((state) => {
      // Updating the same disc (e.g. live updates from a running full scan)
      // mustn't reset the scan progress / completed badge or the tabs the
      // user has open. Only loading a different disc should clear that
      // session state.
      const samePath =
        state.disc !== null && disc !== null && state.disc.path === disc.path;
      if (samePath) {
        return { disc };
      }
      return {
        disc,
        openTabs: DEFAULT_TABS,
        activeTabIndex: 0,
        fullScanProgress: null,
        fullScanCompletedFor: null,
      };
    }),
  clearDisc: () =>
    set(() => ({
      disc: null,
      openTabs: DEFAULT_TABS,
      activeTabIndex: 0,
      fullScanProgress: null,
      fullScanCompletedFor: null,
    })),
  setScanningPath: (scanningPath) => set({ scanningPath }),
  setFullScanProgress: (fullScanProgress) => set({ fullScanProgress }),
  setFullScanCompletedFor: (fullScanCompletedFor) => set({ fullScanCompletedFor }),

  openTab: (type, playlistName) =>
    set((state) => {
      const existing = state.openTabs.findIndex((t) => tabsEqual(t, type, playlistName));
      if (existing >= 0) {
        return { activeTabIndex: existing };
      }
      const next: OpenTab[] = [...state.openTabs, { type, playlistName }];
      return { openTabs: next, activeTabIndex: next.length - 1 };
    }),

  closeTab: (index) =>
    set((state) => {
      // DiscInfo at index 0 is permanent.
      if (index <= 0 || index >= state.openTabs.length) return {};
      const nextTabs = state.openTabs.filter((_, i) => i !== index);
      let nextActive = state.activeTabIndex;
      if (nextActive === index) {
        nextActive = Math.max(0, index - 1);
      } else if (nextActive > index) {
        nextActive -= 1;
      }
      return { openTabs: nextTabs, activeTabIndex: nextActive };
    }),

  setActiveTabIndex: (activeTabIndex) =>
    set((state) => {
      if (activeTabIndex < 0 || activeTabIndex >= state.openTabs.length) return {};
      return { activeTabIndex };
    }),
}));
