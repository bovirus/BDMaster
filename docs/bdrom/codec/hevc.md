# bdrom/codec/hevc.rs

## Description

H.265/HEVC parser for profile, level, HDR, and selected SEI/VUI metadata. This corresponds to BDInfo's `TSCodecHEVC.cs`.

## Implementation Progress

100%

## Implementation Details

- Ports BDInfo's VPS, SPS, PPS, slice, VUI, HRD, and SEI parsing structure.
- Tracks parameter sets, profile/tier/level, bit depth, chroma format, range, color primaries, transfer characteristics, matrix coefficients, mastering display metadata, MaxCLL/MaxFALL, alternative transfer characteristics, and HDR10+ detection.
- Builds BDInfo-style HEVC encoding profile text.
- Adds extended format strings such as bit depth, HDR10/HDR10+/Dolby Vision, BT.2020, range, and optional diagnostics.
- Marks HEVC streams VBR and initialized after an SPS is found.
- **Parameter-set state now persists across PES payloads** via `PersistentHevc` in `CodecScanState` (keyed per PID). This mirrors BDInfo storing `ExtendedData` on the stream, so a VPS in one PES and an SPS/SEI in a later PES resolve correctly instead of being dropped.
- Tests cover the colour-primaries / transfer-characteristics / matrix-coefficients tables; empty/garbage robustness; the cross-PES persistence path; HDR10 / HDR10+ / Dolby-Vision labeling and the profile/level/bit-depth/colour application logic (driven by pre-seeded `SeqParameterSet` structs across profile, level, tier, chroma and extended-diagnostics variants); and **synthesized VPS+SPS+VUI NAL bitstreams** (a bit-writer with Exp-Golomb encoding and emulation-prevention insertion) that drive the real `video_parameter_set` / `seq_parameter_set` / `profile_tier_level` / `vui_parameters` parsers end-to-end, including the SPS-without-VPS rejection.

## Parity Notes (mirrors BDInfo exactly)

- Parsed SPS dimensions, cropping, timing, and aspect data are not pushed onto `TSStreamInfo`; like AVC, both implementations take those from MPLS.
- Dolby Vision labeling uses BDInfo's `PID >= 4117` heuristic, not a full RPU/profile parser.
- Extended data is flattened into strings (BDInfo's `ExtendedData` model); structured HDR metadata is not exposed through the protocol DTO.
- The `TODO: profile to string` area and the early-return-without-diagnostics paths for malformed bitstreams are preserved from `TSCodecHEVC.cs`.

## Open Issues

- Release line coverage is 66.05% and function coverage is 85.90%, both below the 90% target. The untested surface is mostly the real VPS/SPS/PPS/VUI/HRD/SEI parser branches rather than the pre-seeded application logic.
- Coverage is especially thin around short-term reference picture sets, scaling lists, HRD/sub-layer HRD parsing, buffering-period and picture-timing SEI, alternate T.35 user-data payload shapes, VUI aspect/timing branches, and malformed NAL recovery paths.
- The doc says the module mirrors BDInfo exactly, but the low parser coverage means parity is not well protected against regressions in unusual HEVC/HDR streams.
