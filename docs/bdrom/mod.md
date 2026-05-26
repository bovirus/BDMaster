# bdrom/mod.rs

## Description

Top-level Blu-ray scanner for native folders and `.iso` images. This module corresponds mainly to BDInfo's `BDROM.cs`, with supporting behavior from `TSStreamFile.cs`, `TSPlaylistFile.cs`, `TSStreamClip.cs`, and `TSInterleavedFile.cs`.

## Implementation Progress

82%

## Implementation Details

- Exposes the public `scan(path_str)` entry point used by the Tauri command layer.
- Locates `BDMV`, `PLAYLIST`, `CLIPINF`, `STREAM`, optional `STREAM/SSIF`, `BDJO`, `META`, and `SNP` directories case-insensitively.
- Supports both native filesystem discs and ISO images via `udf.rs`.
- Builds `BDRom` with disc flags, volume/title metadata, playlist map, stream file map, CLPI metadata, and SSIF counterparts.
- Converts the internal model into frontend `DiscInfo`, `PlaylistInfo`, `PlaylistStreamClipInfo`, `StreamFileInfo`, and `TSStreamInfo` DTOs.
- Performs the lightweight codec initialization pass over unique angle-0 clips and distributes discovered codec metadata to all referencing playlists.
- Adds hidden streams from PMT PIDs that are not declared in MPLS.
- Implements SSIF source selection, MVC extension recomputation, estimated stream-size caching, and native file path resolution helpers.

## Open Issues

- ISO volume label is derived from the ISO file name, not the UDF logical volume identifier used by DiscUtils/BDInfo.
- Native volume label is derived from the root directory name, not the OS volume label lookup that BDInfo uses on Windows.
- Disc title extraction is a string scan for `bdmt_eng.xml`; it is not a full XML namespace-aware parser and ignores non-English metadata files.
- Codec initialization only scans angle-0 clips and has an 8 MB per-clip budget, so late PMT entries, delayed parameter sets, or uncommon hidden streams can remain uninitialized.
- Playlist grouping is based on shared clip names; BDInfo's UI grouping and playlist validity logic are richer.
- CLPI data is not used as an alternate stream source; stream metadata primarily comes from MPLS and M2TS PMT/PES parsing.
- SSIF support trusts name matching and a fixed MVC PID convention; it does not deeply validate the interleaved file relationship before replacing the M2TS source.
- `codec_init` uses raw pointers into playlist stream vectors to share codec parser mutations across collections; this is contained but harder to audit than a keyed mutation pass.
- Mutex poisoning in the ISO/UDF path is handled with `unwrap`, which can panic after an earlier panic while holding the lock.

