# bdrom/codec/avc.rs

## Description

H.264/AVC elementary-stream parser for profile and level discovery. This corresponds to BDInfo's `TSCodecAVC.cs`.

## Implementation Progress

100%

## Implementation Details

- Scans Annex B start codes in PES payloads (emulation-prevention bytes are stripped via `read_byte(true)`).
- Detects access-unit delimiters and sequence parameter sets.
- Reads profile IDC, constraint set 3, and level IDC.
- Produces BDInfo-style encoding profile text, including the special `1b` level case.
- Marks the stream VBR and initialized after SPS profile/level discovery.
- Tested against crafted SPS payloads for High/Baseline profiles, the `1b` level case, an unknown profile IDC, and a garbage-input robustness sweep.

## Parity Notes (mirrors BDInfo exactly)

- `TSCodecAVC.cs` extracts only profile and level from the SPS; it does not parse dimensions, cropping, VUI timing, colorimetry, or aspect ratio. Those fields come from the MPLS `video_format` byte in both implementations.
- The profile table is BDInfo's exact set; other profile IDCs map to "Unknown Profile", as upstream.
- BDInfo's only frame-type output is the chart-UI diagnostic `tag`; that surface is intentionally not part of this port. Both re-parse each PES payload from start codes.
