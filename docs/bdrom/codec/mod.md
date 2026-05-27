# bdrom/codec/mod.rs

## Description

Codec dispatcher and description finalizer for PES payload parsing. This corresponds to BDInfo's `TSStreamFile.ScanStream` dispatch plus parts of `TSStream.cs` description formatting.

## Implementation Progress

100%

## Implementation Details

- Re-exports codec modules and `TSStreamBuffer`.
- Holds per-PID codec state in `CodecScanState`, which now persists both PGS caption state and HEVC parameter sets (`PersistentHevc`) across PES payloads.
- Dispatches based on `TSStreamType` to MPEG-2, AVC, MVC, HEVC, VC-1, MPA, AAC, AC3/E-AC3, TrueHD, LPCM, DTS, DTS-HD, and PGS parsers.
- Supports full-scan behavior for PGS so captions are counted only during deep scans.
- Handles LPCM bit-rate calculation from parsed header fields.
- Marks unknown stream types initialized to avoid infinite scan loops.
- Builds display descriptions for video, audio, and graphics streams after codec fields are populated. The builders mirror the exact `TSStream` getters, including:
  - **Graphics**: forced captions render as `( + N Forced Caption)` when normal captions are also present, and as `/ N Forced Caption` otherwise — matching BDInfo's "Fix PGS Caption count reporting" commit (`2581e58`).
  - **Audio channel description**: the `-EX` (Dolby Digital EX) / `-ES` (DTS-ES) suffix is appended for Extended `audio_mode`.
  - **Audio bitrate**: the embedded core's bitrate is subtracted from the displayed kbps **only** for TrueHD (`AC3_TRUE_HD_AUDIO`); DTS-HD HR/MA and DD+ display the full measured rate, as BDInfo does.
- Provides `refine_from_pes` for one-shot enrichment from a single PES sample.
- Tests cover the LPCM dispatch + bit-rate, dispatch over every codec arm, the unknown-type and quick-init/full-scan PGS paths, the video/audio/graphics description builders (including the forced-caption forms, `-EX`/`-ES` suffix, embedded-core labels, TrueHD-only core-bitrate subtraction, base-view eyes, and stereo audio modes), and `refine_from_pes`.

## Parity Notes (mirrors BDInfo by design)

- Persistent cross-PES parser state now exists for PGS and HEVC (the two codecs where BDInfo also accumulates stream-associated state). Other codecs init from a single PES, exactly as upstream.
- The description builder is a Rust rendering of BDInfo's `TSStream` property strings; it is intentionally a port of the formatting logic rather than the C# class hierarchy. The whole Rust port mutates the single `TSStreamInfo` DTO instead of BDInfo's `TSVideoStream`/`TSAudioStream` subclasses, and `extended_diagnostics` is a bool in place of BDInfo's settings object.
- Unknown stream types are marked initialized (no diagnostics) so scanning terminates, matching BDInfo's behavior of not blocking on unsupported codecs.
