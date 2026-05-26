# bdrom/codec/mod.rs

## Description

Codec dispatcher and description finalizer for PES payload parsing. This corresponds to BDInfo's `TSStreamFile.ScanStream` dispatch plus parts of `TSStream.cs` description formatting.

## Implementation Progress

78%

## Implementation Details

- Re-exports codec modules and `TSStreamBuffer`.
- Holds per-PID codec state in `CodecScanState`; currently this is mainly PGS caption state.
- Dispatches based on `TSStreamType` to MPEG-2, AVC, MVC, HEVC, VC-1, MPA, AAC, AC3/E-AC3, TrueHD, LPCM, DTS, DTS-HD, and PGS parsers.
- Supports full-scan behavior for PGS so captions are counted only during deep scans.
- Handles LPCM bit-rate calculation from parsed header fields.
- Marks unknown stream types initialized to avoid infinite scan loops.
- Builds display descriptions for video, audio, and graphics streams after codec fields are populated.
- Provides `refine_from_pes` for one-shot enrichment from a single PES sample.

## Open Issues

- Persistent parser state across PES calls exists only for PGS; BDInfo keeps richer stream-associated state for codecs such as HEVC.
- Description formatting is a Rust approximation of BDInfo's stream properties and may differ in edge cases.
- Unknown stream types are marked initialized without diagnostics, which can hide unsupported codecs.
- LPCM parsing is inlined in the dispatcher rather than having a `scan` function matching the other codecs.
- The dispatcher mutates protocol DTOs directly instead of using typed `TSVideoStream`/`TSAudioStream` classes.
- Extended diagnostics are passed as a boolean rather than mirroring BDInfo's settings object.

