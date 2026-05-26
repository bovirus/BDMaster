# bdrom/codec/aac.rs

## Description

AAC ADTS header parser. This corresponds to BDInfo's `TSCodecAAC.cs`.

## Implementation Progress

90%

## Implementation Details

- Looks for the 12-bit ADTS sync word.
- Reads MPEG version, profile object type, sampling-rate index, and channel configuration.
- Maps common AAC profile names, sample rates, channel counts, and audio modes.
- Sets codec name, sample rate, channel count, LFE flag, audio mode, VBR flag, and initialized state.

## Open Issues

- Does not parse AAC extensions such as SBR/PS or explicit AudioSpecificConfig data.
- Channel configuration 0 (program config element) is not parsed.
- ADTS CRC/protection fields are skipped and not validated.
- Invalid/reserved combinations become zero/empty fields instead of reported errors.
- No tests compare all BDInfo sample-rate and channel-mode cases.

