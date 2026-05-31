/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.

 *   Licensed under the Apache License, Version 2.0 (the "License");
 *   you may not use this file except in compliance with the License.
 *   You may obtain a copy of the License at

 *   http://www.apache.org/licenses/LICENSE-2.0

 *   Unless required by applicable law or agreed to in writing, software
 *   distributed under the License is distributed on an "AS IS" BASIS,
 *   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *   See the License for the specific language governing permissions and
 *   limitations under the License.
 */

import { useAppStore } from "./store";
import * as Protocol from "./protocol";
import { cancelFullScan, scanDisc } from "./service";

export async function scanDiscPaths(paths: string[]) {
  // Single-disc app: only the first path is inspected; any prior disc is replaced.
  if (paths.length === 0) {
    return;
  }
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
