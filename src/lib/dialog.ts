/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { open, save } from "@tauri-apps/plugin-dialog";
import i18n from "../i18n";
import { scanDiscPaths } from "./fs";

export async function openDiscDirectoryDialog() {
  const directory = await open({ directory: true, multiple: false });
  if (directory) {
    await scanDiscPaths([directory as string]);
  }
}

export async function openSaveReportDialog() {
  return await save({
    filters: [{ name: i18n.t("fileFilter.text"), extensions: ["txt"] }],
  });
}
