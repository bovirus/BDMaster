# bdrom/codec/avc.rs

## Description

H.264/AVC elementary-stream parser for profile and level discovery. This corresponds to BDInfo's `TSCodecAVC.cs`.

## Implementation Progress

90%

## Implementation Details

- Scans Annex B start codes in PES payloads.
- Detects access-unit delimiters and sequence parameter sets.
- Reads profile IDC, constraint set 3, and level IDC.
- Produces BDInfo-style encoding profile text.
- Marks the stream VBR and initialized after SPS profile/level discovery.

## Open Issues

- Does not parse SPS dimensions, cropping, VUI timing, colorimetry, or aspect ratio; the app relies on MPLS for those fields.
- Profile mapping covers BDInfo's small set and reports other profiles as unknown.
- Parser state is not persisted across PES payloads.
- Assumes Annex B start-code formatted payloads.
- Does not expose frame type diagnostics.

