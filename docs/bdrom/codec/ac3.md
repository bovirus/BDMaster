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

## Open Issues

- Not resolved in this pass: release line coverage remains 78.50% for this module, although the bdrom-focused coverage target is above 90% overall. I re-ran `cargo llvm-cov --release --lib` and confirmed the missed lines are still concentrated in optional legacy AC3 skip fields, bsid-6 EX/headphone branches, dual-mono dependent paths, and the deeper EMDF/JOC payload configuration loop.
- I did not add synthetic vectors for those branches because doing so safely requires carefully hand-authoring several unusual AC3/E-AC3 bitstreams and validating expected BDInfo parity; rushing those fixtures would risk locking in incorrect parser behavior. The current tests still cover representative AC3 and E-AC3 frames plus robustness sweeps, but not enough malformed/optional branches to make the "mirrors exactly" claim fully regression-proof.
