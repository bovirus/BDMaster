# bdrom/clpi.rs

## Description

CLPI reader for `BDMV/CLIPINF/*.clpi`. This corresponds to BDInfo's `TSStreamClipFile.cs`.

## Implementation Progress

100%

## Implementation Details

- `StreamClipFile` exposes `name`, `size`, `file_type`, `is_valid`, and the parsed `streams` list (mirroring BDInfo's `FileType`, `IsValid`, and `Streams`).
- `parse_clpi(path)` reads the file; `parse_clpi_bytes(name, size, data)` does the parse and is shared by the native-folder and ISO/UDF paths (the ISO path now reads the CLPI content via the UDF reader instead of only stat-ing it).
- Validates the `HDMV0100` / `HDMV0200` / `HDMV0300` signature and rejects anything else with `is_valid = false` (no panic), matching BDInfo's "unknown file type" rejection.
- Follows the clip-info offset at byte 12 and the program-info stream table: stream count, then per-stream PID, coding type, and type-specific attributes — video format / frame rate / aspect ratio, audio channel layout / sample rate / language, and graphics & subtitle languages. MVC entries are skipped without registering a stream, exactly as upstream.
- All offset math is bounds-checked, so truncated or malformed tables degrade gracefully.
- CLPI stream metadata supplements MPLS: when a playlist stream has no language code, `build_playlist_info` fills it from the matching clip's CLPI program-info table by PID (`clpi_language_for`).
- Tests build synthetic CLPI images and cover video/audio/PGS attribute parsing, signature rejection, the too-short-file guard, MVC skipping with continued parsing, and a truncated-table robustness case.
