# Release Notes

## 0.2.0

* External tool integration: open a playlist or stream file directly in MPC-HC (Windows).
* Localization: added Italian (it) — UI, reports, and Windows installer.

## 0.1.0 (Initial Release)

BDMaster is a modern, cross-platform GUI for inspecting Blu-ray discs.

### Platforms

* Linux (x86_64)
* macOS (x86_64 + arm64)
* Windows (x86_64)

### Disc Loading

* Open Blu-ray content from a folder (`BDMV/` layout) or directly from a `.iso` image via a built-in UDF 2.x reader — no external mounting required.
* Drag-and-drop a disc folder, `.iso`, or any file inside `BDMV/` onto the window (powered by Tauri's native drag-drop, so it works even when the dragged item isn't an HTML5 file).
* Pass a path on the command line or via "Open With" to launch straight into a disc.

### Disc, Playlist, Stream, and Chapter Inspection

* Parses `BDMV/PLAYLIST/*.mpls`, `BDMV/CLIPINF/*.clpi`, and `BDMV/STREAM/*.m2ts` to surface the full disc structure.
* Per-playlist views: **Quick Summary**, **Full Report**, **Bit Rate**, and **Chapters**.
* Sortable playlist list with video / audio / subtitle / chapter counts at a glance.
* Track table with hidden-track support and per-stream bit-rate estimates.
* Optional 3D Blu-ray (SSIF / MVC) support — opt-in via configuration; when enabled, the matching SSIF file is used as the authoritative source for codec detection and size measurement.

### Codec Coverage

The codec engine initializes every stream by parsing reassembled PES payloads until each PID reports complete metadata:

* Video — AVC, HEVC, MPEG-2, VC-1, MVC (3D)
* Audio — AC-3, DTS, DTS-HD, TrueHD, LPCM, AAC, MPA
* Subtitles — PGS

Fixed bit rates are reported when the codec defines one; variable-bit-rate streams are estimated from observed bytes per second.

### Full Disc Scan

* Background full scan reads every M2TS end-to-end for precise size and bit-rate measurement.
* Live progress reporting (~4 updates/sec) with elapsed and remaining-time tracking.
* Cancellable from the UI and on window close — the worker exits cleanly before shared state is dropped.
* Bit-rate samples power the per-playlist chart, and accurate sizes feed the Quick Summary / Full Report numbers.

### Reporting and Export

* **Quick Summary** and **Full Report** text generation, localized into all supported languages.
* Bit-rate chart powered by Apache ECharts, exportable as PNG.
* Save report text to a file from the UI.

### External Tool Integration

* **MKVToolNix** — open a playlist or individual stream file directly in MKVToolNix GUI.
* **BetterMediaInfo** — open a playlist or individual stream file directly in BetterMediaInfo.
* Tools are auto-located via configured path → running-process lookup → platform-specific install heuristics. On macOS, `.app` bundles are spawned via `open -a` to avoid window flash; on Windows, console children are spawned with `CREATE_NO_WINDOW`.

### Configuration

* User config persisted as JSON at platform-standard locations (macOS `Application Support`, Linux `$XDG_CONFIG_HOME` / `~/.config`, Windows `%APPDATA%` or the install directory).
* Configurable size / bit-rate formatting (precision, decimal K/M/G/T or binary IEC Ki/Mi/Gi/Ti units).
* SSIF support toggle and other scan options.
* Window size and position are remembered between sessions.
* Resizable disc-info splitter, with its position persisted.

### Localization

Full UI and report localization in eight languages:

* English (en-US)
* German (de)
* Spanish (es)
* French (fr)
* Japanese (ja)
* Simplified Chinese (zh-CN)
* Traditional Chinese — Hong Kong (zh-HK)
* Traditional Chinese — Taiwan (zh-TW)

Windows installers (NSIS + WiX) are shipped in all eight languages.

### Updates

* Background update check on startup against GitHub Releases, throttled by configurable interval (Daily / Weekly / Monthly).
* In-app notification when a newer version is available, with the option to skip a specific version.

### Theming

* Light / dark mode with automatic OS detection, plus configurable palette.
