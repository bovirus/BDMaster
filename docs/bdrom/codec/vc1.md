# bdrom/codec/vc1.rs

## Description

VC-1 elementary-stream parser for basic profile and interlace metadata. This corresponds to BDInfo's `TSCodecVC1.cs`.

## Implementation Progress

100%

## Implementation Details

- Scans for VC-1 frame header and sequence header start codes.
- Extracts basic Main/Advanced profile text and profile level.
- Decodes the interlace flag locally to drive picture-type decoding, but does **not** assign it to the stream — `is_interlaced` is sourced from the MPLS video format, matching `TSCodecVC1.cs` (which keeps `isInterlaced` as a Scan-local and never writes `stream.IsInterlaced`).
- Marks the stream VBR and initialized after sequence-header parsing.
- Tested against crafted Advanced/Main sequence headers (profile text) plus a regression check that the codec leaves the MPLS-derived `is_interlaced` flag untouched, and a garbage-input robustness sweep.

## Parity Notes (mirrors BDInfo exactly)

- `TSCodecVC1.cs` itself only extracts profile (and a Scan-local interlace flag for picture-type decoding); it does not parse coded dimensions, display aspect ratio, frame rate, colorimetry, or bit rate, has no frame reorder/B-picture timing, and never propagates interlace to the stream. This port matches that scope (an earlier revision incorrectly wrote `is_interlaced`; that divergence is fixed).
- BDInfo's only frame-type output is the diagnostic `tag` string used by its chart UI; that diagnostic surface is intentionally not part of this port.
- Both implementations re-parse each PES payload from start codes (Annex B style) rather than persisting parser state.
