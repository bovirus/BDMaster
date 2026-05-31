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

import { open, save } from "@tauri-apps/plugin-dialog";
import i18n from "../i18n";
import { scanDiscPaths } from "./fs";

export async function openDiscFileDialog() {
  const file = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "ISO", extensions: ["iso"] }],
  });
  if (file) {
    await scanDiscPaths([file as string]);
  }
}

export async function openDiscDirectoryDialog() {
  const directory = await open({ directory: true, multiple: false });
  if (directory) {
    await scanDiscPaths([directory as string]);
  }
}

export async function openSaveReportDialog(kind: "text" | "html" = "text") {
  const filter =
    kind === "html"
      ? { name: i18n.t("fileFilter.html"), extensions: ["html", "htm"] }
      : { name: i18n.t("fileFilter.text"), extensions: ["txt"] };
  return await save({ filters: [filter] });
}

export async function openSaveChartDialog(defaultPath?: string) {
  return await save({
    defaultPath,
    filters: [{ name: "PNG", extensions: ["png"] }],
  });
}
