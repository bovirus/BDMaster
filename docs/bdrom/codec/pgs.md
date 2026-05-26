# bdrom/codec/pgs.rs

## Description

Presentation Graphics Stream parser for subtitle dimensions and caption counts. This corresponds to BDInfo's `TSCodecPGS.cs`.

## Implementation Progress

90%

## Implementation Details

- Defines `Frame` and `PgsState` to hold frame/caption state across PES calls.
- Reads segment types for Object Definition Segment, Presentation Composition Segment, and end-of-display marker.
- Extracts graphics width and height from the first PCS.
- Tracks forced-caption flag from composition objects.
- Increments normal or forced caption counters when ODS appears before the current frame is finished.
- Marks PGS streams VBR.

## Open Issues

- Does not parse subtitle bitmap payloads, palettes, windows, timing, or object placement beyond fields skipped for counting.
- `caption_ids` is populated but not used for de-duplication or reporting.
- Caption counts depend on full-scan mode; quick codec init marks PGS initialized without counting captions.
- No validation for malformed segment sizes or truncated composition objects.

