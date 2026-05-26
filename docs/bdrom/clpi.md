# bdrom/clpi.rs

## Description

Minimal CLPI reader for `BDMV/CLIPINF/*.clpi`. The corresponding BDInfo module is `TSStreamClipFile.cs`.

## Implementation Progress

15%

## Implementation Details

- Defines `StreamClipFile` with only `name` and `size`.
- `parse_clpi(path)` verifies that the file can be statted, uppercases the file name, and records file size.
- Native folder scans call this parser; ISO scans synthesize the same minimal struct from UDF file-entry metadata.

## Open Issues

- Does not validate CLPI file signatures (`HDMV0100`, `HDMV0200`, `HDMV0300`).
- Does not parse the clip info section, stream count, PID list, video attributes, audio attributes, graphics/text languages, or stream types.
- Does not expose `FileType`, `IsValid`, or the `Streams` dictionary that BDInfo builds.
- Does not use CLPI stream metadata to supplement or cross-check MPLS stream entries.
- Does not report malformed CLPI contents; any readable file is treated as a valid clip-info entry.

