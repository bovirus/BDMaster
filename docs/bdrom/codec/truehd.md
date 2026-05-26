# bdrom/codec/truehd.rs

## Description

Dolby TrueHD parser with embedded AC3 core fallback. This corresponds to BDInfo's `TSCodecTrueHD.cs`.

## Implementation Progress

82%

## Implementation Details

- Scans for the TrueHD sync word `0xF8726FBA`.
- Falls back to AC3 core parsing when TrueHD sync is not found.
- Parses sample-rate ratebits, channel-present flags, LFE count, peak bitrate, derived bit depth, and extension-content markers.
- Marks TrueHD streams VBR.
- Initializes the TrueHD stream once an embedded core stream is available and initialized.

## Open Issues

- Does not copy AC3 core dialnorm into the TrueHD stream, unlike BDInfo.
- TrueHD metadata dialnorm remains unimplemented, matching BDInfo's TODO.
- A TrueHD sync found before AC3 core initialization can leave the parent stream uninitialized until a later core parse.
- Extension parsing only determines whether extension content exists; it does not classify Atmos or other metadata.
- No validation for impossible channel/sample-rate combinations or truncated headers.

