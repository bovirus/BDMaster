# bdrom/codec/mpa.rs

## Description

MPEG audio parser for MP1/MP2 style Blu-ray streams. This corresponds to BDInfo's `TSCodecMPA.cs`.

## Implementation Progress

90%

## Implementation Details

- Checks the 11-bit MPEG audio sync word.
- Reads MPEG version, layer, bitrate index, sampling-rate index, channel mode, and ancillary header flags.
- Maps bit rate, sample rate, audio mode, channel count, version text, and layer text.
- Marks streams CBR and initialized after a valid header-shaped payload.

## Open Issues

- Free-format and invalid bitrate/sample-rate combinations are not explicitly rejected.
- CRC/protection data is not validated.
- No VBR headers such as Xing/Info/VBRI are parsed.
- Only the first frame header is used for metadata.
- No tests cover all version/layer/bitrate matrix entries.

