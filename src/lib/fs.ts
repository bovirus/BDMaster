/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useAppStore } from "./store";
import * as Protocol from "./protocol";
import { cancelFullScan, scanDisc } from "./service";

export async function scanDiscPaths(paths: string[]) {
  // Single-disc app: only the first path is inspected; any prior disc is replaced.
  if (paths.length === 0) return;
  // Loading a new disc supersedes any in-flight full scan on the previous
  // disc — fire cancel before kicking off the lightweight scan so the
  // worker thread releases the old M2TS reader.
  await cancelFullScan().catch(() => {});
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
