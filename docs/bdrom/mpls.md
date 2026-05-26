# bdrom/mpls.rs

## Description

MPLS movie playlist parser. This corresponds to the parsing portions of BDInfo's `TSPlaylistFile.cs` and the clip timing model in `TSStreamClip.cs`.

## Implementation Progress

65%

## Implementation Details

- Validates playlist signatures `MPLS0100`, `MPLS0200`, and `MPLS0300`.
- Reads playlist, chapter, and extension offsets.
- Extracts MVC base-view flag from the misc flags byte.
- Parses play items, primary clips, multi-angle clip names, in/out times, and angle indices.
- Parses STN table stream entries for video, audio, presentation graphics, interactive graphics, subtitles, secondary audio, and secondary video.
- Deduplicates playlist stream entries by PID.
- Extracts simple chapter timestamps in seconds.

## Open Issues

- Does not build BDInfo's `Streams`, `PlaylistStreams`, `AngleStreams`, `AngleClips`, `SortedStreams`, or typed stream lists in the parser itself.
- Does not call a `LoadStreamClips` equivalent to bind playlist clips to stream files and CLPI stream maps.
- Chapter parsing records absolute chapter seconds and ignores the stream-file index/relative-clip mapping that BDInfo uses.
- Secondary audio/video and PIP entries are only skipped enough to continue parsing; detailed subpath/subclip relationships are not modeled.
- Playlist extensions are not parsed.
- Custom playlists are not supported.
- Loop detection, validity filtering, and stream sorting live in `mod.rs` and are simpler than BDInfo's `IsValid` and comparer logic.
- Malformed lengths can still lead to partial parses that miss data instead of preserving BDInfo-style debug diagnostics.

