# bdrom/codec/dtshd.rs

## Description

DTS-HD extension substream parser with DTS core fallback. This corresponds to BDInfo's `TSCodecDTSHD.cs`.

## Implementation Progress

78%

## Implementation Details

- Looks for the DTS-HD sync word `0x64582025`.
- Falls back to parsing a DTS core stream when no DTS-HD payload is found.
- Parses substream index, blown-up header flag, static fields, asset sizes, sample rate, bit depth, speaker activity mask, channel count, and LFE count.
- Detects selected DTS-HD extension markers and sets `has_extensions`.
- Handles Master Audio as VBR and High Resolution/secondary streams using caller-provided bit-rate hints plus core bit rate.

## Open Issues

- Multi-asset DTS-HD handling is still a TODO and exits after the first asset.
- Does not copy core dialnorm into the DTS-HD stream, unlike BDInfo.
- Extension detection is marker-based and does not fully parse extension payloads.
- Malformed sample-rate indexes can panic because `nu_max_sample_rate` is used as an array index without a bounds check.
- Channel layout and speaker mask details are reduced to channel/LFE counts.
- Initialization for non-MA streams depends on external bit-rate estimates.

