# bdrom/lang.rs

## Description

ISO 639 language-code lookup used to populate stream language names. This corresponds to BDInfo's `LanguageCodes.cs`.

## Implementation Progress

100%

## Implementation Details

- Provides `language_name(code)` for three-letter Blu-ray language codes.
- Backed by `LANGUAGE_CODES`, a static table ported verbatim from BDInfo's `GetName` switch (all 459 cases, including ISO 639-2/B and ISO 639-2/T aliases). Parity with BDInfo is preserved by construction.
- Trims trailing NULs and lowercases the input before lookup.
- Unknown or empty codes fall back to the trimmed raw code, matching BDInfo's `default: return code;`.
- The Norwegian Bokmål label is stored as valid UTF-8 and verified by test.
- Tests iterate the whole table to assert each code resolves to its BDInfo name, and cover the fallback, NUL trimming, case-insensitivity, B/T aliases, and table uniqueness/size.
