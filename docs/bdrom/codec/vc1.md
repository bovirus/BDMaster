# bdrom/codec/vc1.rs

## Description

VC-1 elementary-stream parser for basic profile and interlace metadata. This corresponds to BDInfo's `TSCodecVC1.cs`.

## Implementation Progress

100%

## Implementation Details

- Scans for VC-1 frame header and sequence header start codes.
- Extracts basic Main/Advanced profile text and profile level.
- Reads and stores the interlaced flag.
- Marks the stream VBR and initialized after sequence-header parsing.
- Tested against crafted Advanced/Main sequence headers (profile text + interlace flag) and a garbage-input robustness sweep.

## Parity Notes (mirrors BDInfo exactly)

- `TSCodecVC1.cs` itself only extracts profile and interlace; it does not parse coded dimensions, display aspect ratio, frame rate, colorimetry, or bit rate, and it has no frame reorder/B-picture timing. This port matches that scope.
- BDInfo's only frame-type output is the diagnostic `tag` string used by its chart UI; that diagnostic surface is intentionally not part of this port.
- Both implementations re-parse each PES payload from start codes (Annex B style) rather than persisting parser state.
