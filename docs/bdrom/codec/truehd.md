# bdrom/codec/truehd.rs

## Description

Dolby TrueHD parser with embedded AC3 core fallback. This corresponds to BDInfo's `TSCodecTrueHD.cs`.

## Implementation Progress

100%

## Implementation Details

- Scans for the TrueHD sync word `0xF8726FBA`.
- Falls back to AC3 core parsing when TrueHD sync is not found.
- Parses sample-rate ratebits, channel-present flags, LFE count, peak bitrate, derived bit depth, and extension-content markers.
- Marks TrueHD streams VBR.
- Initializes the TrueHD stream once an embedded core stream is available and initialized.
- The peak-bit-depth divisor is guarded with `max(1)`, so an all-zero header cannot produce a divide-by-zero (a small robustness improvement over the raw C# arithmetic).
- Tests cover the AC3 core fallback, the empty-buffer case, the TrueHD sync path (sample rate, channels, LFE, derived bit depth), and a garbage-input robustness sweep.

## Parity Notes (mirrors BDInfo exactly)

- BDInfo's TrueHD-metadata dialnorm copy is commented out in `TSCodecTrueHD.cs` (its own `// TODO: Get THD dialnorm from metadata`); the same block is preserved here as a comment, so neither implementation copies core dialnorm.
- A TrueHD sync seen before the AC3 core initializes leaves the parent uninitialized until a later core parse — BDInfo's identical two-call pattern.
- Extension parsing only flags whether extension content exists (no Atmos classification) and there is no validation of impossible channel/sample-rate combinations, matching upstream.
