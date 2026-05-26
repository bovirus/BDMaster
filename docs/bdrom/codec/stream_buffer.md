# bdrom/codec/stream_buffer.rs

## Description

In-memory bit/byte reader used by codec parsers. This corresponds to BDInfo's `TSStreamBuffer.cs`.

## Implementation Progress

100%

## Implementation Details

- Wraps a PES payload slice and tracks byte position, bit offset, and skipped H.26x emulation-prevention bytes.
- Provides byte reads, bool reads, 16/32/64-bit bit reads, seeks, bit/byte skips, Exp-Golomb reads/skips, signed Exp-Golomb reads, and remaining-bit counters.
- `begin_read` resets the cursor and bit state exactly like BDInfo's `BeginRead`.
- Mirrors several BDInfo quirks, including `read_bytes` returning `None` at `pos + n >= len`.
- Supports optional H.26x emulation-prevention-byte skipping for AVC/HEVC parsing.
- `read_bits8` reads the second 4-byte half using the original captured position for its bounds check, matching `ReadBits8` in the C# source (a porting bug that truncated 64-bit reads to ~32 bits was fixed and locked in by test).
- Tests cover MSB-first 16/32/64-bit reads (including partial high-half reads), sub-byte fields, bool walking, H.26x emulation skipping (skipped / kept / disabled), Exp-Golomb and signed Exp-Golomb decoding, `read_bytes` end-of-buffer behavior, seek clamping, `begin_read` reset, remaining counters, and panic-free under-reads.

## Parity Notes (mirrors BDInfo by design)

- BDInfo's `Add`, `Reset`, `EndRead`, and `TransferLength` belong to its incremental MemoryStream accumulation used by `TSStreamFile`. In this port the PES payload is reassembled in `m2ts.rs` before being wrapped as a slice, so those lifecycle methods are intentionally unnecessary while results stay equivalent.
- Under-reads return zero/false rather than raising errors, matching BDInfo's zero-filled backing buffer, so malformed payloads degrade gracefully instead of panicking.
