# bdrom/codec/dtshd.rs

## Description

DTS-HD extension substream parser with DTS core fallback. This corresponds to BDInfo's `TSCodecDTSHD.cs`.

## Implementation Progress

100%

## Implementation Details

- Looks for the DTS-HD sync word `0x64582025`.
- Falls back to parsing a DTS core stream when no DTS-HD payload is found.
- Parses substream index, blown-up header flag, static fields, asset sizes, sample rate, bit depth, speaker activity mask, channel count, and LFE count.
- Detects selected DTS-HD extension markers and sets `has_extensions`.
- Handles Master Audio as VBR and High Resolution/secondary streams using caller-provided bit-rate hints plus core bit rate.
- The sample-rate index is a 4-bit field looked up in a 16-entry table through a checked `get`, so a malformed bitstream cannot panic the parser.
- Tests cover the core fallback, Master Audio VBR initialization, the already-initialized early return, and a robustness sweep over truncated/garbage payloads.

## Parity Notes (mirrors BDInfo exactly)

- Multi-asset DTS-HD parsing breaks after the first asset, matching the `// TODO...` in `TSCodecDTSHD.cs`.
- Core dialnorm is not copied into the DTS-HD stream because that block is commented out in BDInfo; the same code is preserved as a comment here.
- Extension detection is marker-based and channel/speaker layout is reduced to channel/LFE counts, exactly as in the C# source (which carries its own `// TODO...`).
- Non-Master-Audio initialization depends on the caller's bit-rate estimate, as designed in BDInfo.
