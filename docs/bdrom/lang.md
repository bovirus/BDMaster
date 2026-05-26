# bdrom/lang.rs

## Description

ISO 639 language-code lookup used to populate stream language names. This corresponds to BDInfo's `LanguageCodes.cs`.

## Implementation Progress

20%

## Implementation Details

- Provides `language_name(code)` for three-letter Blu-ray language codes.
- Trims trailing NULs, lowercases the input, and maps a subset of common ISO 639-2/B and ISO 639-2/T aliases.
- Returns an empty string for unknown or empty codes.

## Open Issues

- BDInfo contains roughly 459 language-code cases; this module only includes a small common subset.
- The comment says unknown codes fall back to the raw code, but the implementation returns an empty string.
- The Norwegian Bokmal label is mojibake in the source.
- No generated data source or test coverage ensures parity with BDInfo or ISO 639 updates.
- Region/script variants and many legacy bibliographic/terminologic aliases are missing.
