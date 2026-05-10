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
