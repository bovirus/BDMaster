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
- Parses STN table stream entries for video, audio, presentation graphics, interactive graphics, subtitles, secondary audio, and secondary video (with type-specific attributes: video format/frame rate/aspect, audio channel layout/sample rate/language, graphics/subtitle languages).
- Deduplicates playlist stream entries by PID.
- Extracts chapter timestamps in seconds.
- An end-to-end test builds a synthetic MPLS and verifies the signature check, clip name/in/out times, video & audio stream attributes, the MVC flag, and chapter timing.

## Design Notes (intentional differences from BDInfo)

- The parser produces flat `stream_clips`, `playlist_streams`, and `chapters` lists; BDInfo's `Streams` / `PlaylistStreams` / `AngleStreams` / `AngleClips` / `SortedStreams` and its `LoadStreamClips` clip↔CLPI binding are handled in `mod.rs` (which builds the typed DTO lists, fills missing language codes from CLPI, and applies validity/grouping/sorting). Splitting parse from model-building this way is a deliberate architectural choice.
- Chapters are stored as absolute seconds (sufficient for the chapter table/markers); the stream-file-index/relative-clip mapping BDInfo carries is not reconstructed.
- Secondary audio/video and PIP entries are parsed only enough to advance correctly; deep subpath/subclip relationships, playlist extensions, and custom playlists are out of scope, matching the app's feature set.
