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

  // Discs
  discs: Protocol.DiscInfo[];
  selectedDiscPath: string | null;
  scanningPaths: Set<string>;

  // Tab status
  tabAboutStatus: Protocol.ControlStatus;
  tabSettingsStatus: Protocol.ControlStatus;

  // Actions
  initConfig: () => Promise<void>;
  initAbout: () => Promise<void>;
  setConfig: (config: Protocol.Config | null) => void;
  setDialogNotification: (n: DialogNotification | null) => void;
  setTabAboutStatus: (s: Protocol.ControlStatus) => void;
  setTabSettingsStatus: (s: Protocol.ControlStatus) => void;
  addDisc: (disc: Protocol.DiscInfo) => void;
  removeDisc: (path: string) => void;
  clearDiscs: () => void;
  setSelectedDiscPath: (path: string | null) => void;
  setScanning: (path: string, scanning: boolean) => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  config: null,
  about: null,
  dialogNotification: null,
  discs: [],
  selectedDiscPath: null,
  scanningPaths: new Set(),
  tabAboutStatus: Protocol.ControlStatus.Hidden,
  tabSettingsStatus: Protocol.ControlStatus.Hidden,

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

  addDisc: (disc) => {
    const { discs } = get();
    const filtered = discs.filter((d) => d.path !== disc.path);
    set({ discs: [...filtered, disc] });
  },

  removeDisc: (path) => {
    const { discs } = get();
    set({ discs: discs.filter((d) => d.path !== path) });
  },

  clearDiscs: () => set({ discs: [], selectedDiscPath: null }),

  setSelectedDiscPath: (selectedDiscPath) => set({ selectedDiscPath }),

  setScanning: (path, scanning) => {
    const { scanningPaths } = get();
    const next = new Set(scanningPaths);
    if (scanning) next.add(path);
    else next.delete(path);
    set({ scanningPaths: next });
  },
}));
