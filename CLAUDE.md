# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

BDMaster is a Tauri 2 desktop app that inspects Blu-ray discs (folders or `.iso` images). The Rust backend (`src-tauri/`) parses Blu-ray structures and probes codec streams; the React + MUI frontend (`src/`) presents disc / playlist / stream / chapter views and bit-rate charts. Targets Linux x86_64, macOS x86_64 + arm64, and Windows x86_64.

## Commands

Package manager: **pnpm** (see `pnpm-workspace.yaml`, `pnpm-lock.yaml`).

```bash
pnpm install                  # install JS deps
pnpm tauri dev                # run the desktop app in dev mode (starts Vite on :1420, then Tauri)
pnpm tauri build              # production bundle (uses `pnpm build` then cargo)
pnpm build                    # frontend-only: `tsc -b && vite build`
pnpm dev                      # frontend-only Vite dev server (use `pnpm tauri dev` for the full app)
```

Rust side (run from `src-tauri/`):

```bash
cargo test --release          # what CI runs; no JS-side test suite exists
cargo build --release         # used by `pnpm tauri build` indirectly
cargo test --release bdrom::  # run a subset (e.g. all bdrom-module tests)
cargo test --release udf      # filter by name substring
```

There is currently **no lint, formatter, or frontend test runner configured** — don't invent commands like `pnpm lint` or `pnpm test`. CI (`.github/workflows/*_build.yml`) only runs `cargo test --release` plus `pnpm tauri build`.

When bumping the app version, update **three places** in lockstep: `package.json#version`, `src-tauri/Cargo.toml#package.version`, and `src-tauri/tauri.conf.json#version`. CI workflows also pin `BDMASTER_VERSION` in their `env:` block.

## Architecture

### Frontend ↔ backend boundary

Communication is exclusively via Tauri `invoke` commands. The contract is:

- Rust commands live in `src-tauri/src/lib.rs` (`#[tauri::command]`), thin-wrapping `src-tauri/src/controller.rs` and feature modules. Every command is also listed in the `invoke_handler![…]` block at the bottom of `lib.rs` — adding a new command requires editing both.
- Rust DTOs live in `src-tauri/src/protocol.rs` with `#[serde(rename = "...")]` for camelCase JSON, mirrored by TypeScript interfaces/enums in `src/lib/protocol.ts`. The two files must stay in sync; there is no codegen.
- The TS façade in `src/lib/service.ts` wraps `invoke<T>(...)` calls one-for-one. UI code calls `service.ts`, never `invoke` directly.

### Rust backend layout (`src-tauri/src/`)

- `lib.rs` — Tauri entry: window state, update-check background thread, command registration. `main.rs` is a one-liner that calls `bdmaster_lib::run()`.
- `controller.rs` — application logic glue called from commands (config, scan dispatch, update-check version comparison, file writes).
- `config.rs` — persisted user config (`Config` struct). Stored as JSON at platform-specific paths: `~/Library/Application Support/BDMaster/BDMaster.json` (macOS); `$XDG_CONFIG_HOME/BDMaster/` or `~/.config/BDMaster/` (Linux); `%APPDATA%\BDMaster\` when installed under `%LOCALAPPDATA%`/`%ProgramFiles%`, otherwise the exe directory (Windows). Held in a `OnceLock<RwLock<Config>>` global; `get_config()` clones, `set_config()` writes-through to disk then updates memory.
- `protocol.rs` — wire types plus shared state structs (`UpdateCheckState`, `FullScanState`) registered with `tauri::Manager`.
- `bdrom/` — Blu-ray parsing core. **The big-picture flow:**
  - `mod.rs` exposes `scan(path)` → `open_bdrom` (native folder or `.iso` via `udf.rs`) → parses `BDMV/PLAYLIST/*.mpls` (`mpls.rs`), `BDMV/CLIPINF/*.clpi` (`clpi.rs`), and `BDMV/STREAM/*.m2ts` (`m2ts.rs`). Optional `BDMV/STREAM/SSIF/*.ssif` (3D MVC) is opened in parallel when `config.scan.enable_ssif_support` is true; `effective_stream_source` swaps SSIF in for the matching M2TS so codec init sees AVC + MVC.
  - After parsing, `codec_init` runs each clip through `codec/` parsers (avc, hevc, mvc, mpeg2, vc1, ac3, dts, dtshd, truehd, lpcm, aac, mpa, pgs) on reassembled PES payloads via `TSStreamBuffer` until every PID reports `is_initialized`. This populates per-stream codec details and a fixed bit-rate when the codec defines one; VBR streams get an estimate from bytes/seconds observed.
  - `full_scan.rs` is the heavy background pass. `controller::start_full_scan` spawns a worker that reads every M2TS end-to-end via a `ProgressReader` (reports cumulative bytes ~4×/sec and short-circuits on `state.cancel`). Progress is exposed via the polled `get_scan_progress` command — there is **no event emission**; the UI polls. Cancellation is also wired into `WindowEvent::CloseRequested` so the worker exits before Tauri drops shared state.
- `bettermediainfo.rs`, `mkvtoolnix.rs` — locate optional external tools (configured path → process-table lookup of a running instance → platform-specific install heuristics) and spawn them with a file path. **macOS-specific:** to avoid a brief window flash on Launch Services activation, both modules spawn via `/usr/bin/open -a <bundle>.app --args <file>` when the binary lives inside a `.app`; direct binary spawn is used only as a fallback. Match this pattern in any new external-tool integration. On Windows, use `CREATE_NO_WINDOW` (0x08000000) when spawning console-attached children.
- `constants.rs` — `APP_NAME = "BDMaster"`, used as window title prefix, config-dir name, and User-Agent for the GitHub releases update check.

### Frontend layout (`src/`)

- `App.tsx` — theme (MUI palette switched from `Protocol.Theme` enum + light/dark mode auto-detect), i18n init, mounts `Layout` + `NotificationSnackbar`.
- `lib/store.tsx` — Zustand store. Holds `config`, `about`, single-`disc` state, full-scan progress, and an `openTabs` array driving the main content area. Tab index 0 is always `DiscInfo` and is non-closable. `setDisc` only resets tabs / scan state when the disc *path* changes, so live-updates from a running full scan don't blow away user state.
- `lib/service.ts` — Tauri `invoke` façade (one function per Rust command).
- `lib/protocol.ts` — TS mirror of `src-tauri/src/protocol.rs`. Update both sides together.
- `lib/report.ts`, `lib/reportI18n.ts` — BDInfo-compatible quick-summary / full-report text generation. Heavy logic; centralizes the report rendering used by `QuickSummaryTab`, `FullReportTab`, and `ReportDocumentView`.
- `lib/format.ts` — bit-rate / size formatting honoring `config.formatting` precision and unit (K/M/G/T, optionally binary IEC `Ki`/`Mi`/…).
- `components/` — MUI views: `Layout`, `Toolbar`, `MainContent` (tab host + drag-drop target), `DiscDetail` (left pane with playlists / streams / clips), per-playlist `QuickSummaryTab` / `FullReportTab` / `BitRateTab`, plus `Config`, `About`, `Welcome`. **Drag-and-drop is handled in `MainContent` via Tauri's `DragDropEvent`, not via HTML5 events.** Launch arguments (files passed on the CLI / open-with) are fetched via the `get_launch_args` command.
- `i18n/` — i18next with eight locales: `de`, `en-US`, `es`, `fr`, `ja`, `zh-CN`, `zh-HK`, `zh-TW`. Adding a UI string means adding a key to **all eight** JSON files; the `Language` enum in `src/lib/protocol.ts` and `src-tauri/src/config.rs` must agree on locale codes. Windows installer language lists in `tauri.conf.json` (NSIS + WiX) also enumerate the languages.

### Important cross-cutting notes

- **Update check** runs at startup on a background thread, throttled by `config.update.check_interval` (Daily/Weekly/Monthly). It hits `https://api.github.com/repos/caoccao/BDMaster/releases`, compares tags via `is_newer_version` in `controller.rs` (numeric dotted-version, `v` prefix tolerated), and stashes the result for the frontend to fetch via `get_update_result`. Users can suppress a specific version via `skip_version`.
- **Window state** (size + position) is persisted to `config.window` on every `Moved`/`Resized` event after first show. The `WINDOW_READY` `AtomicBool` gate in `lib.rs` prevents racing the initial restore.
- **SSIF (3D)** support is opt-in via config. When enabled, scanning treats the SSIF file as authoritative for the matching M2TS, which changes both codec detection and measured size. If you touch stream-source resolution, check both `effective_stream_source` and `refresh_ssif_derived_metadata`.
- **Tauri capabilities** are declared in `src-tauri/capabilities/default.json`. Adding a new plugin or core permission (e.g. a new dialog/clipboard/opener API) requires listing it here, not just enabling the plugin in `lib.rs`.
- The Rust crate is `bdmaster_lib` (`Cargo.toml#[lib].name`), produced as `staticlib + cdylib + rlib` for Tauri's mobile/desktop entry-point split.
