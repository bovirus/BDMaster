# bdrom/codec/ac3.rs

## Description

Dolby Digital and Dolby Digital Plus parser, including partial Atmos/JOC detection. This corresponds to BDInfo's `TSCodecAC3.cs`.

## Implementation Progress

85%

## Implementation Details

- Validates the AC3 sync word.
- Parses legacy AC3 headers for sample rate, frame-size code, bit rate, channel mode, LFE, surround/extended mode, and dialnorm.
- Parses E-AC3 frame fields, dependent-stream channel maps, frame size, number of blocks, and extended dialnorm behavior.
- Creates or updates embedded core stream metadata where applicable.
- Scans EMDF payloads to detect Dolby Atmos/JOC extensions.
- Marks streams CBR and handles the two-frame initialization pattern for some Dolby Digital Plus streams.

## Open Issues

- The `dheadphonmod` branch remains a TODO inherited from BDInfo.
- Dependent-stream handling clones current state into a core stream, which can be fragile if frame ordering is unusual.
- EMDF/JOC detection is heuristic and only checks the payload path BDInfo handles.
- Many bit reads return zero on under-run, so malformed short frames can silently produce partial metadata.
- No validation of AC3 CRC, frame-size table consistency, or illegal bitstream IDs.
- Multi-frame E-AC3 initialization can fail when the second frame is outside the codec-init byte budget.

