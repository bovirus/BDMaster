# bdrom/codec/dts.rs

## Description

DTS core parser. This corresponds to BDInfo's `TSCodecDTS.cs`.

## Implementation Progress

100%

## Implementation Details

- Scans for the big-endian DTS core sync word `0x7FFE8001`.
- Parses frame size, sample-rate code, bit-rate code, LFE, PCM resolution, dialog normalization, and channel count, with bounds checks on every table index.
- Maps sample rates, bit depths, fixed bit rates, and open/variable/lossless bit-rate markers.
- Uses the caller-provided bit-rate hint for open bit-rate streams.
- Marks streams initialized once enough core metadata is available.
- Tests build synthetic core frames (with and without CRC) and verify the fixed/variable/open bit-rate paths, the frame-size guard, channel/LFE/bit-depth/dial-norm decoding, and the no-sync case.

## Parity Notes (mirrors BDInfo exactly)

- `TSCodecDTS.cs` only recognizes the 16-bit big-endian core sync (no 14-bit or byte-swapped variants), reduces channel layout to a total count plus LFE, and leaves extension substreams to `TSCodecDTSHD`. Open/variable/lossless bit rates rely on the caller's estimate, and the CRC is skipped rather than validated. This port matches that behavior.
