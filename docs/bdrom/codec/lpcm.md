# bdrom/codec/lpcm.rs

## Description

Blu-ray LPCM audio header parser. This corresponds to BDInfo's `TSCodecLPCM.cs`.

## Implementation Progress

95%

## Implementation Details

- Parses the 4-byte LPCM payload header.
- Maps channel assignment to channel count and LFE count.
- Maps bit-depth codes to 16, 20, or 24 bits.
- Maps sample-rate codes to 48, 96, or 192 kHz.
- Returns `ParsedLpcm`; the dispatcher writes fields and calculates fixed bit rate.

## Open Issues

- The parser returns `Some` with zero fields for unknown channel, bit-depth, or sample-rate codes instead of rejecting the header.
- Bit-rate calculation and stream initialization live in `codec/mod.rs`, so this module is not a one-for-one `Scan` function.
- Does not validate any framing beyond the first four payload bytes.

