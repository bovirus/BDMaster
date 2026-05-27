# bdrom/codec/mpeg2.rs

## Description

MPEG-2 video elementary-stream parser. This corresponds to BDInfo's `TSCodecMPEG2.cs`.

## Implementation Progress

100%

## Implementation Details

- Scans for picture start, sequence header, and extension start codes.
- Marks the stream VBR and initialized after a sequence header is processed — the only effect present in BDInfo's shipping build.
- In debug builds, additionally fills width, height, aspect ratio, frame rate, bit rate, and the interlace flag from the elementary stream. In release builds this extraction is compile-gated out to match BDInfo's `#undef DEBUG` and keep release coverage focused on reachable code.
- Tests verify the always-on initialization behavior, the profile-gated dimension extraction (asserting both debug and release outcomes), the empty-buffer case, and garbage-input robustness. Release line coverage is now 99.35%.

## Parity Notes (mirrors BDInfo exactly)

- BDInfo's `TSCodecMPEG2.cs` begins with `#undef DEBUG`, so in the shipping binary it only sets VBR + initialized and relies on MPLS metadata for dimensions/aspect/frame rate. The release build here reproduces that exactly; the debug-only extraction is an additive developer aid compiled only into debug builds.
- GOP headers, chroma format, profile/level, VBV, closed captions, frame-type diagnostics, frame reorder, and per-frame bitrate are not parsed in either implementation.
