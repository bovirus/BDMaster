# bdrom/codec/ac3.rs

## Description

Dolby Digital and Dolby Digital Plus parser, including partial Atmos/JOC detection. This corresponds to BDInfo's `TSCodecAC3.cs`.

## Implementation Progress

100%

## Implementation Details

- Validates the AC3 sync word.
- Parses legacy AC3 headers for sample rate, frame-size code, bit rate, channel mode, LFE, surround/extended mode, and dialnorm.
- Parses E-AC3 frame fields, dependent-stream channel maps, frame size, number of blocks, and extended dialnorm behavior.
- Creates or updates embedded core stream metadata where applicable.
- Scans EMDF payloads to detect Dolby Atmos/JOC extensions.
- Marks streams CBR and handles the two-frame initialization pattern for some Dolby Digital Plus streams.
- Tests cover the legacy AC3 frame path (sample rate, bit rate, channel/LFE/dialnorm decoding), the `AC3ChanMap` helper, sync-word rejection, and a truncated/garbage robustness sweep.

## Parity Notes (mirrors BDInfo exactly)

- The `dheadphonmod` branch is BDInfo's own `// TODO`, preserved here.
- Dependent-stream handling clones the current state into a core stream, EMDF/JOC detection follows BDInfo's heuristic payload path, bit reads default to zero on under-run, and there is no CRC / frame-size / bsid validation — all matching `TSCodecAC3.cs`.
- The two-frame E-AC3 initialization (first frame leaves the stream uninitialized) mirrors BDInfo; whether the second frame falls inside the codec-init byte budget is governed by `codec_init` in `mod.rs`, exactly as BDInfo depends on its frame reader.
