# bdrom/codec/lpcm.rs

## Description

Blu-ray LPCM audio header parser. This corresponds to BDInfo's `TSCodecLPCM.cs`.

## Implementation Progress

100%

## Implementation Details

- Parses the 4-byte LPCM payload header.
- Maps channel assignment to channel count and LFE count.
- Maps bit-depth codes to 16, 20, or 24 bits.
- Maps sample-rate codes to 48, 96, or 192 kHz.
- Returns `ParsedLpcm`; the dispatcher in `codec/mod.rs` writes the fields and computes the fixed bit rate as `sample_rate * bit_depth * (channels + lfe)`, exactly as BDInfo does inline.
- Tests cover the full channel-assignment, bit-depth, and sample-rate code maps, the too-short-payload guard, and the bit-rate formula.

## Parity Notes (mirrors BDInfo exactly)

- `TSCodecLPCM.cs` reads only the first four payload bytes and, for unknown channel/bit-depth/sample-rate codes, sets the corresponding field to zero while still initializing the stream. This port matches that behavior; the only difference is that the fixed bit-rate computation lives in the dispatcher rather than inline.
