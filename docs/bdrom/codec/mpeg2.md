# bdrom/codec/mpeg2.rs

## Description

MPEG-2 video elementary-stream parser. This corresponds to BDInfo's `TSCodecMPEG2.cs`.

## Implementation Progress

85%

## Implementation Details

- Scans for picture start, sequence header, and extension start codes.
- In debug builds, can fill width, height, aspect ratio, frame rate, bit rate, and interlace flag from the elementary stream.
- In release builds, mirrors BDInfo's `#undef DEBUG` behavior by leaving several stream-property assignments gated off.
- Marks stream VBR and initialized after a sequence header is processed.

## Open Issues

- Release builds intentionally do not populate several parsed fields from MPEG-2 ES data, relying on MPLS metadata instead.
- Sequence extension handling is minimal and debug-gated.
- Does not parse GOP headers, chroma format, profile/level, VBV, closed captions, or frame type diagnostics.
- No frame reorder or per-frame bitrate support.
- Debug and release behavior differ, which can surprise tests or local diagnostics.

