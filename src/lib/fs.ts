/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useAppStore } from "./store";
import * as Protocol from "./protocol";
import { scanDisc } from "./service";

export async function scanDiscPaths(paths: string[]) {
  for (const path of paths) {
    await scanOneDisc(path);
  }
}

async function scanOneDisc(path: string) {
  const { setScanning, addDisc, setDialogNotification, setSelectedDiscPath } =
    useAppStore.getState();
  setScanning(path, true);
  try {
    const disc = await scanDisc(path);
    addDisc(disc);
    setSelectedDiscPath(disc.path);
  } catch (error) {
    setDialogNotification({
      title: typeof error === "string" ? error : `Failed to scan: ${path}`,
      type: Protocol.DialogNotificationType.Error,
    });
  } finally {
    setScanning(path, false);
  }
}
