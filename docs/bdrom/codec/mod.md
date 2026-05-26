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
- Builds display descriptions for video, audio, and graphics streams after codec fields are populated.
- Provides `refine_from_pes` for one-shot enrichment from a single PES sample.
- Tests cover the LPCM dispatch + bit-rate, the unknown-type and quick-init PGS paths, the video/audio/graphics description builders, and `refine_from_pes`.

## Parity Notes (mirrors BDInfo by design)

- Persistent cross-PES parser state now exists for PGS and HEVC (the two codecs where BDInfo also accumulates stream-associated state). Other codecs init from a single PES, exactly as upstream.
- The description builder is a Rust rendering of BDInfo's `TSStream` property strings; it is intentionally a port of the formatting logic rather than the C# class hierarchy. The whole Rust port mutates the single `TSStreamInfo` DTO instead of BDInfo's `TSVideoStream`/`TSAudioStream` subclasses, and `extended_diagnostics` is a bool in place of BDInfo's settings object.
- Unknown stream types are marked initialized (no diagnostics) so scanning terminates, matching BDInfo's behavior of not blocking on unsupported codecs.
