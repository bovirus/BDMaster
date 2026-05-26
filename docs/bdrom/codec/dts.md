# bdrom/codec/dts.rs

## Description

DTS core parser. This corresponds to BDInfo's `TSCodecDTS.cs`.

## Implementation Progress

90%

## Implementation Details

- Scans for the big-endian DTS core sync word `0x7FFE8001`.
- Parses frame size, sample-rate code, bit-rate code, LFE, PCM resolution, dialog normalization, and channel count.
- Maps sample rates, bit depths, fixed bit rates, and open/variable/lossless bit-rate markers.
- Uses the caller-provided bit-rate hint for open bit-rate streams.
- Marks streams initialized once enough core metadata is available.

## Open Issues

- Does not handle all DTS sync variants, such as 14-bit or byte-swapped encodings.
- Channel layout is simplified to a total channel count plus LFE.
- Extension substreams are handled in `dtshd.rs`, not here.
- Open/variable/lossless bit-rate handling depends on the caller's estimate.
- No CRC or frame consistency validation.

