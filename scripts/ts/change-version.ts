/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import * as fs from "https://deno.land/std/fs/mod.ts";
import * as path from "https://deno.land/std/path/mod.ts";

class ChangeVersion {
  private productionVersion: string;
  private rootDirPath: string;
  private snapshotVersion: string;

  constructor(productionVersion: string, snapshotVersion: string) {
    this.productionVersion = productionVersion;
    this.snapshotVersion = snapshotVersion;
    this.rootDirPath = path.join(
      path.dirname(path.fromFileUrl(import.meta.url)),
      "../../"
    );
  }

  private _change(
    filePath: string,
    patterns: Array<RegExp>,
    isSnapshot = true
  ) {
    const sourceFilePath = path.join(this.rootDirPath, filePath);
    if (fs.existsSync(sourceFilePath)) {
      console.info(`Processing ${sourceFilePath}.`);
    } else {
      console.error(`%c${sourceFilePath} is not found.`, "color: red");
      return;
    }
    const newVersion = isSnapshot
      ? this.snapshotVersion
      : this.productionVersion;
    const positionGroups: Array<{ start: number; end: number }> = [];
    const currentContent = Deno.readTextFileSync(sourceFilePath);
    patterns.map((pattern) => {
      [...currentContent.matchAll(pattern)].map((match) => {
        const matchedString = match[0];
        const currentVersion = match.groups["version"];
        if (newVersion === currentVersion) {
          console.warn(`%c  Ignored ${matchedString}`, "color: yellow");
        } else {
          console.info(`  ${matchedString} => ${newVersion}`);
          const start = match.index + matchedString.indexOf(currentVersion);
          const end = start + currentVersion.length;
          positionGroups.push({ start: start, end: end });
        }
      });
    });
    if (positionGroups.length > 0) {
      let newContent = "";
      let lastEnd = 0;
      positionGroups.sort((pg1, pg2) => pg1.start - pg2.start);
      positionGroups.map((positionGroup) => {
        if (positionGroup.start > lastEnd) {
          newContent += currentContent.substring(lastEnd, positionGroup.start);
        }
        newContent += newVersion;
        lastEnd = positionGroup.end;
      });
      if (lastEnd < currentContent.length) {
        newContent += currentContent.substring(lastEnd, currentContent.length);
      }
      if (newContent !== currentContent) {
        Deno.writeTextFileSync(sourceFilePath, newContent);
      }
    }
  }

  change() {
    this._change("package.json", [
      /^  "version":\s*"(?<version>\d+\.\d+\.\d+)"/gim,
    ]);
    this._change(".github/workflows/linux_build.yml", [
      /^\s*BDMASTER_VERSION:\s*(?<version>\d+\.\d+\.\d+)/gim,
    ]);
    this._change(".github/workflows/macos_build.yml", [
      /^\s*BDMASTER_VERSION:\s*(?<version>\d+\.\d+\.\d+)/gim,
    ]);
    this._change(".github/workflows/windows_build.yml", [
      /^\s*BDMASTER_VERSION:\s*(?<version>\d+\.\d+\.\d+)/gim,
    ]);
    this._change("src-tauri/Cargo.toml", [
      /^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"/gim,
    ]);
    this._change("src-tauri/tauri.conf.json", [
      /^  "version":\s*"(?<version>\d+\.\d+\.\d+)"/gim,
    ]);
  }
}

const changeVersion = new ChangeVersion("0.2.0", "0.3.0");
changeVersion.change();
