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
- After every secondary-audio / secondary-video entry the parser always advances past the 2- / 6-byte extension field, regardless of whether a stream was produced — matching `TSPlaylistFile.cs` (`pos += 2` / `pos += 6` run unconditionally after `CreatePlaylistStream`).
- Deduplicates playlist stream entries by PID.
- Extracts chapter timestamps in seconds (only type-1 chapter marks).
- Tests build synthetic MPLS images covering: the signature check (and rejection / truncation handling), clip name/in/out times, every stream category and header type, the secondary-stream skip bytes, the MVC flag, multi-angle clip expansion (angle count + per-angle clips), and chapter timing.

## Design Notes (intentional differences from BDInfo)

- The parser produces flat `stream_clips`, `playlist_streams`, and `chapters` lists; BDInfo's `Streams` / `PlaylistStreams` / `AngleStreams` / `AngleClips` / `SortedStreams` and its `LoadStreamClips` clip↔CLPI binding are handled in `mod.rs` (which builds the typed DTO lists, fills missing language codes from CLPI, and applies validity/grouping). Splitting parse from model-building this way is a deliberate architectural choice.
- Stream order follows the MPLS STN-table order. This matches BDInfo's **default** behavior: `BDInfoSettings.KeepStreamOrder` defaults to `True`, so `SortStreams` (the height/channel-count/English-first comparators) is skipped by default. The optional reorder mode is not reproduced.
- Chapters are stored as absolute seconds (sufficient for the chapter table/markers); the stream-file-index/relative-clip mapping BDInfo carries is not reconstructed.
- Secondary audio/video and PIP entries are parsed only enough to advance correctly; deep subpath/subclip relationships, playlist extensions, and custom playlists are out of scope, matching the app's feature set.

## Open Issues

- `create_stream` registers MVC (`0x20`) as a normal video stream, but BDInfo's `TSPlaylistFile.CreatePlaylistStream` skips MVC with its own TODO. Playlist-level MVC visibility and base-view handling can diverge from BDInfo.
- Chapter parsing ignores `stream_file_index` and stores absolute clip seconds. BDInfo converts chapter marks to playlist-relative seconds and ignores final chapter marks less than one second from the end, so multi-clip chapter tables can be wrong here.
- Duplicate playlist streams use a first-PID-wins rule. BDInfo replaces an existing PID when a later significant clip (`RelativeLength > 0.01`) supplies better metadata, so multi-clip playlists can choose different stream attributes or languages.
- Subpath/subclip IDs, subitem tables, extension blocks, custom playlists, and optional stream sorting are still not modeled. These are documented simplifications, but they remain BDInfo parity gaps.
- Several parser paths still use direct `data[r.pos]` indexing or unchecked skip advances after partially validating offsets. Some malformed/truncated MPLS inputs can still panic instead of returning a graceful parse error.
