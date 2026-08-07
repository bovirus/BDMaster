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
- For ISO images the volume label is the UDF Logical Volume Identifier (matching DiscUtils/BDInfo), falling back to the ISO file name only when the LVD carries none; ISO scans reject images missing `BDMV/PLAYLIST` or `BDMV/CLIPINF`, matching native-folder validation.
- Builds `BDRom` with disc flags, volume/title metadata, playlist map, stream file map, parsed CLPI metadata, and SSIF counterparts.
- Converts the internal model into frontend `DiscInfo`, `PlaylistInfo`, `PlaylistStreamClipInfo`, `StreamFileInfo`, and `TSStreamInfo` DTOs. Alternate angles retain their own video-stream collections, matching BDInfo's `AngleStreams` model.
- Performs lightweight codec initialization over unique main- and alternate-angle clips and distributes discovered metadata to every referencing collection.
- Bounds the codec-initialization phase with the configurable `scan.fastScanSeconds` wall-clock deadline (default 10 seconds, clamped to 1–3600). Reference clips are scanned first so a short budget produces deterministic, useful metadata; unresolved streams are completed by full scan.
- Uses parsed CLPI program-info as a fallback stream-metadata source. Reference-clip replacement follows BDInfo's exact order: a present stream file replaces a missing one, a significant clip with more CLPI streams replaces the current reference, and otherwise a longer present clip wins. `clpi_language_for` fills language codes when MPLS leaves them blank.
- Adds hidden streams from the reference CLPI table before codec initialization. PMT/PES discovery can still add hidden PIDs absent from CLPI.
- Implements SSIF source selection, MVC extension recomputation, estimated stream-size caching, and native file path resolution helpers.
- ISO/UDF locking recovers poisoned mutex guards rather than cascading a panic.
- Native folder size and metadata-title walks use canonical visited-directory sets, a depth cap, and symlink skipping.
- Disc-title extraction parses XML local names, decodes common entities, and ignores unrelated `<name>` tags.

## Design Notes (intentional differences from BDInfo)

- The native folder volume label is the root directory name rather than a Windows volume-label lookup, keeping the scanner cross-platform.
- Disc-title extraction reads `META/DL/bdmt_eng.xml`; other-language metadata files remain outside the current DTO model.
- The fast-scan deadline applies to potentially expensive M2TS/SSIF reads. Structural MPLS/CLPI parsing happens before codec initialization and is retained when the deadline expires.
- Playlist grouping is by shared clip names, simpler than BDInfo's UI grouping logic, and SSIF support trusts name matching plus the fixed MVC PID convention.
- Codec initialization shares parser mutations through contained raw pointers. The vectors are not mutated while those pointers are live.
