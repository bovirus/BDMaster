# bdrom/mpls.rs

## Description

MPLS movie-playlist parser corresponding to BDInfo's `TSPlaylistFile.cs` and the clip timing model in `TSStreamClip.cs`.

## Implementation Progress

100%

## Implementation Details

- Validates `MPLS0100`, `MPLS0200`, and `MPLS0300` signatures.
- Reads playlist, chapter, and extension offsets plus the MVC base-view flag.
- Parses play items, primary clips, multi-angle clip names, in/out times, and angle indices.
- Parses STN entries for video, audio, presentation/interactive graphics, text subtitles, secondary audio, and secondary video, including every stream-entry header type and type-specific attributes.
- Skips playlist-declared MVC (`0x20`) streams to match BDInfo's `CreatePlaylistStream` TODO; SSIF/PMT handling recovers MVC extension visibility.
- Advances secondary-audio/video extension bytes unconditionally, as the reference parser does.
- Deduplicates streams by PID, replacing metadata when a later clip remains significant relative to the preceding playlist length (BDInfo's 1% rule).
- Extracts type-1 chapter marks as playlist-relative seconds and drops final marks less than one second from the end.
- Uses checked/bounded reads so malformed or truncated input returns an error instead of panicking.
- Tests cover signatures/truncation, clips/timing, every stream category/header type, secondary skip bytes, MVC, multi-angle expansion, duplicate-PID significance, and chapters.

## Design Notes (intentional differences from BDInfo)

- Parsing produces flat clip/stream/chapter lists. `disc_info.rs` performs CLPI binding and constructs the typed public main/angle collections.
- Stream order follows STN order, matching BDInfo's default `KeepStreamOrder = True`. The optional non-default reorder mode is not reproduced.
- Chapters are public playlist-relative seconds; lower-level mark/file indices are consumed during parsing.
- BDInfo's current parser also reads and ignores subitem counts and extension/PIP structures (marked TODO), so not exposing them is not a reference-implementation gap.

## Open Issues

- Optional non-default stream reordering is not modeled.
