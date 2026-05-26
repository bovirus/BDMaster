# bdrom/codec/mpa.rs

## Description

MPEG audio parser for MP1/MP2 style Blu-ray streams. This corresponds to BDInfo's `TSCodecMPA.cs`.

## Implementation Progress

100%

## Implementation Details

- Checks the 11-bit MPEG audio sync word.
- Reads MPEG version, layer, bitrate index, sampling-rate index, channel mode, and ancillary header flags.
- Maps bit rate, sample rate, audio mode, channel count, version text, and layer text.
- Marks streams CBR and initialized after a valid header-shaped payload.
- Tests pack synthetic frame headers and verify the full version × layer × bitrate × sample-rate × channel-mode matrix against the lookup tables, plus targeted MPEG-1 cases and sync-word rejection.

## Parity Notes (mirrors BDInfo exactly)

- `TSCodecMPA.cs` does not reject free-format/invalid bitrate combinations (the tables simply contain zeros), does not validate CRC/protection, does not parse Xing/Info/VBRI VBR headers, and uses only the first frame header. This port matches that behavior cell-for-cell.
