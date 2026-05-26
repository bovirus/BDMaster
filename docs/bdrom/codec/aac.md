# bdrom/codec/aac.rs

## Description

AAC ADTS header parser. This corresponds to BDInfo's `TSCodecAAC.cs`.

## Implementation Progress

100%

## Implementation Details

- Looks for the 12-bit ADTS sync word.
- Reads MPEG version, profile object type, sampling-rate index, and channel configuration.
- Maps common AAC profile names, sample rates, channel counts, and audio modes.
- Sets codec name, sample rate, channel count, LFE flag, audio mode, VBR flag, and initialized state.
- Tests pack synthetic ADTS headers and verify the full version × profile × sample-rate-index × channel-mode matrix against the lookup tables, the profile-name table directly, and sync-word rejection.

## Parity Notes (mirrors BDInfo exactly)

- `TSCodecAAC.cs` parses only the ADTS fixed header: it does not decode SBR/PS extensions or explicit AudioSpecificConfig, does not parse channel configuration 0 (program config element), and skips CRC/protection. Invalid/reserved combinations collapse to zero/empty fields rather than errors. This port matches that behavior.
