# bdrom/mod.rs

## Description

Top-level Blu-ray scanner for native folders and `.iso` images. This module corresponds mainly to BDInfo's `BDROM.cs`, with supporting behavior from `TSStreamFile.cs`, `TSPlaylistFile.cs`, `TSStreamClip.cs`, and `TSInterleavedFile.cs`.

## Implementation Progress

100%

## Implementation Details

- Exposes the public `scan(path_str)` entry point used by the Tauri command layer.
- Locates `BDMV`, `PLAYLIST`, `CLIPINF`, `STREAM`, optional `STREAM/SSIF`, `BDJO`, `META`, and `SNP` directories case-insensitively.
- Supports both native filesystem discs and ISO images via `udf.rs`.
- When the input folder itself is the `BDMV` directory (identified by root-level `index.bdmv`), native scans use that folder as the disc root instead of its parent.
- For ISO images the volume label is the UDF Logical Volume Identifier (matching DiscUtils/BDInfo), falling back to the ISO file name only when the LVD carries none; ISO scans now reject images missing `BDMV/PLAYLIST` or `BDMV/CLIPINF`, matching native-folder validation.
- Builds `BDRom` with disc flags, volume/title metadata, playlist map, stream file map, parsed CLPI metadata, and SSIF counterparts.
- Converts the internal model into frontend `DiscInfo`, `PlaylistInfo`, `PlaylistStreamClipInfo`, `StreamFileInfo`, and `TSStreamInfo` DTOs.
- Performs the lightweight codec initialization pass over unique angle-0 clips and distributes discovered codec metadata to all referencing playlists.
- Uses parsed CLPI program-info as a fallback stream-metadata source. For each playlist it chooses a reference angle-0 clip by richest valid CLPI stream table, then longest clip length; `clpi_language_for` fills language codes from that reference CLPI when MPLS leaves them blank.
- Adds hidden streams from the reference CLPI table before codec initialization, preserving CLPI language/video/audio attributes even when a hidden PID has no early PES payload. PMT/PES discovery during codec init can still add additional hidden PIDs not present in CLPI.
- Implements SSIF source selection, MVC extension recomputation, estimated stream-size caching, and native file path resolution helpers.
- ISO/UDF locking uses `lock().unwrap_or_else(|e| e.into_inner())` throughout, so the scan cannot cascade-panic on a poisoned mutex.
- Native folder size and metadata-title walks use canonical visited-directory sets, a depth cap, and symlink skipping, so scans do not recurse outside the disc tree or loop indefinitely through linked directories.
- Disc-title extraction now parses XML element local names, selecting `<title><name>...` metadata regardless of namespace prefix, decoding common XML entities, and ignoring unrelated `<name>` tags.
- Tested by a fixture-disc harness that writes a synthetic `BDMV` tree (index.bdmv, MPLS, CLPI, M2TS, META/bdmt_eng.xml, BDJO, SSIF, SNP, FilmIndex) to a temp dir and drives the full `scan()` pipeline, plus unit tests for `extract_title_from_xml`, `estimate_stream_size`, `playlist_has_loops`/validity, `recompute_mvc_extension`, `refresh_ssif_derived_metadata`, `clpi_language_for`, the `resolve_*_path` helpers, native symlink-loop sizing, root-`index.bdmv` handling, and the disc-flag/error branches. Current `src/bdrom` release coverage is 90.54% line / 93.29% function with the bdrom-focused llvm-cov filter.

## Design Notes (intentional differences from BDInfo)

- The native (folder) volume label is the root directory name rather than a Windows OS volume-label lookup, keeping the scanner cross-platform.
- Disc-title extraction is a lightweight XML local-name scanner over `META/DL/bdmt_eng.xml`; it is namespace-prefix tolerant for the title/name elements but still intentionally ignores other-language metadata files.
- `codec_init` scans angle-0 clips with an 8 MB per-clip budget. This is sufficient for retail streams; the deep `full_scan` pass covers anything the quick pass leaves uninitialized.
- Playlist grouping is by shared clip names, simpler than BDInfo's UI grouping/validity logic, and SSIF support trusts name matching plus the fixed MVC PID convention.
- `codec_init` shares codec-parser mutations across playlist collections via contained raw pointers (pointers are dropped before any vector mutation); this avoids a second keyed pass and is exercised by the codec/types/clpi/lang unit tests it orchestrates.
