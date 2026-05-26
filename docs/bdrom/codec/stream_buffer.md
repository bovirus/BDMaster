# bdrom/codec/stream_buffer.rs

## Description

In-memory bit/byte reader used by codec parsers. This corresponds to BDInfo's `TSStreamBuffer.cs`.

## Implementation Progress

85%

## Implementation Details

- Wraps a PES payload slice and tracks byte position, bit offset, and skipped H.26x emulation-prevention bytes.
- Provides byte reads, bool reads, 16/32/64-bit bit reads, seeks, bit/byte skips, Exp-Golomb reads/skips, signed Exp-Golomb reads, and remaining-bit counters.
- Mirrors several BDInfo quirks, including `read_bytes` returning `None` at `pos + n >= len`.
- Supports optional H.26x emulation-prevention-byte skipping for AVC/HEVC parsing.

## Open Issues

- Does not implement BDInfo's streaming `Add`, `Reset`, `BeginRead`, `EndRead`, and transfer-length buffer lifecycle; it only wraps one payload slice.
- Under-reads return zero/false in many methods, so malformed payloads can silently produce default metadata.
- H.26x emulation prevention handling is intentionally simple and should be fixture-tested against edge cases.
- No explicit error reporting exists for invalid seeks or reads beyond the buffer.
- `read_bits8` is less exercised than the smaller read helpers.

