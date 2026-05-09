/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useAppStore } from "./store";
import * as Protocol from "./protocol";
import { scanDisc } from "./service";

export async function scanDiscPaths(paths: string[]) {
  // Single-disc app: only the first path is inspected; any prior disc is replaced.
  if (paths.length === 0) return;
  await scanOneDisc(paths[0]);
}

async function scanOneDisc(path: string) {
  const { setScanningPath, setDisc, setDialogNotification } = useAppStore.getState();
  setScanningPath(path);
  try {
    const disc = await scanDisc(path);
    setDisc(disc);
  } catch (error) {
    setDialogNotification({
      title: typeof error === "string" ? error : `Failed to scan: ${path}`,
      type: Protocol.DialogNotificationType.Error,
    });
  } finally {
    setScanningPath(null);
  }
}
