# bdrom/mpls.rs

## Description

MPLS movie playlist parser. This corresponds to the parsing portions of BDInfo's `TSPlaylistFile.cs` and the clip timing model in `TSStreamClip.cs`.

## Implementation Progress

100%

## Implementation Details

- Validates playlist signatures `MPLS0100`, `MPLS0200`, and `MPLS0300`, rejecting anything else with an error.
- Reads playlist, chapter, and extension offsets.
- Extracts the MVC base-view flag from the misc flags byte.
- Parses play items, primary clips, multi-angle clip names, in/out times, and angle indices.
- Parses STN table stream entries for video, audio, presentation graphics, interactive graphics, subtitles, secondary audio, and secondary video (with type-specific attributes: video format/frame rate/aspect, audio channel layout/sample rate/language, graphics/subtitle languages), across all four stream-entry header types (1–4).
- Skips playlist-declared MVC (`0x20`) streams to match BDInfo's `TSPlaylistFile.CreatePlaylistStream` TODO; MVC extension visibility is still recovered from SSIF/PMT handling in `mod.rs` / `full_scan.rs`.
- After every secondary-audio / secondary-video entry the parser always advances past the 2- / 6-byte extension field, regardless of whether a stream was produced — matching `TSPlaylistFile.cs` (`pos += 2` / `pos += 6` run unconditionally after `CreatePlaylistStream`).
- Deduplicates playlist stream entries by PID, replacing an existing PID when a later significant clip supplies equal-or-richer metadata.
- Extracts chapter timestamps in playlist-relative seconds (only type-1 chapter marks), using the mark's stream-file index and dropping final marks less than one second from the playlist end.
- Parser offset advances now go through checked reader helpers or bounded stream-entry skips, so malformed/truncated MPLS input returns `Err`/`None` instead of panicking on unchecked indexing.
- Tests build synthetic MPLS images covering: the signature check (and rejection / truncation handling), clip name/in/out times, every stream category and header type, the secondary-stream skip bytes, MVC skipping, the MVC base-view flag, multi-angle clip expansion (angle count + per-angle clips), duplicate PID replacement, and chapter timing.

## Design Notes (intentional differences from BDInfo)

- The parser produces flat `stream_clips`, `playlist_streams`, and `chapters` lists; BDInfo's `Streams` / `PlaylistStreams` / `AngleStreams` / `AngleClips` / `SortedStreams` and its `LoadStreamClips` clip↔CLPI binding are handled in `mod.rs` (which builds the typed DTO lists, fills missing language codes from CLPI, and applies validity/grouping). Splitting parse from model-building this way is a deliberate architectural choice.
- Stream order follows the MPLS STN-table order. This matches BDInfo's **default** behavior: `BDInfoSettings.KeepStreamOrder` defaults to `True`, so `SortStreams` (the height/channel-count/English-first comparators) is skipped by default. The optional reorder mode is not reproduced.
- Chapters are stored as playlist-relative seconds for the frontend chapter table/markers; lower-level stream-file-index detail is consumed during parsing and is not retained on the public `PlaylistFile`.
- Secondary audio/video and PIP entries are parsed only enough to advance correctly; deep subpath/subclip relationships, playlist extensions, and custom playlists are out of scope, matching the app's feature set.

## Open Issues

- Subpath/subclip IDs, subitem tables, extension blocks, custom playlists, and optional stream sorting are still not modeled. I reviewed these while hardening STN parsing, but resolving them would require extending the internal playlist model and frontend DTOs rather than a local parser bug fix; they remain documented parity gaps.
