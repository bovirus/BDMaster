# bdrom/codec/hevc.rs

## Description

H.265/HEVC parser for profile, level, HDR, and selected SEI/VUI metadata. This corresponds to BDInfo's `TSCodecHEVC.cs`.

## Implementation Progress

82%

## Implementation Details

- Ports BDInfo's VPS, SPS, PPS, slice, VUI, HRD, and SEI parsing structure.
- Tracks parameter sets, profile/tier/level, bit depth, chroma format, range, color primaries, transfer characteristics, matrix coefficients, mastering display metadata, MaxCLL/MaxFALL, alternative transfer characteristics, and HDR10+ detection.
- Builds BDInfo-style HEVC encoding profile text.
- Adds extended format strings such as bit depth, HDR10/HDR10+/Dolby Vision, BT.2020, range, and optional diagnostics.
- Marks HEVC streams VBR and initialized after an SPS is found.

## Open Issues

- Parser state is recreated for each PES call; BDInfo stores `ExtendedData` on the stream, so parameter sets split across PES payloads can be missed.
- Parsed SPS dimensions, cropping, timing, and aspect data are not applied to `TSStreamInfo`; the app relies on MPLS values.
- Dolby Vision labeling uses the same PID heuristic as the ported logic and is not a full Dolby Vision RPU/profile parser.
- Extended data is flattened into strings; structured HDR metadata is not exposed through the protocol DTO.
- The inherited BDInfo `TODO: profile to string` area remains only partially mapped.
- Several malformed-bitstream paths return early without diagnostics.
- No focused HEVC bitstream fixture tests exist for HDR10, HDR10+, Dolby Vision, or unusual VPS/SPS ordering.

