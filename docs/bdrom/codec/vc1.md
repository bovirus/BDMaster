# bdrom/codec/vc1.rs

## Description

VC-1 elementary-stream parser for basic profile and interlace metadata. This corresponds to BDInfo's `TSCodecVC1.cs`.

## Implementation Progress

85%

## Implementation Details

- Scans for VC-1 frame header and sequence header start codes.
- Extracts basic Main/Advanced profile text and profile level.
- Reads and stores the interlaced flag.
- Marks the stream VBR and initialized after sequence-header parsing.

## Open Issues

- Does not parse coded dimensions, display aspect ratio, frame rate, colorimetry, or bit rate.
- Frame type is read only enough to support early return; no diagnostics are exposed.
- No frame reorder or B-picture timing support.
- Parser state is not persisted across PES payloads.
- Assumes Annex B style start-code payloads.

