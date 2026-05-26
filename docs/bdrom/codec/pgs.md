# bdrom/codec/pgs.rs

## Description

Presentation Graphics Stream parser for subtitle dimensions and caption counts. This corresponds to BDInfo's `TSCodecPGS.cs`.

## Implementation Progress

100%

## Implementation Details

- Defines `Frame` and `PgsState` to hold frame/caption state across PES calls.
- Reads segment types for Object Definition Segment, Presentation Composition Segment, and end-of-display marker.
- Extracts graphics width and height from the first PCS.
- Tracks forced-caption flag from composition objects.
- Increments normal or forced caption counters when ODS appears before the current frame is finished.
- Marks PGS streams VBR.
- Tests cover PCS dimension/initialization, forced vs. normal caption counting, the end-of-display marker stopping further counts, and ignored unknown segment types.

## Parity Notes (mirrors BDInfo exactly)

- `TSCodecPGS.cs` skips subtitle bitmap payloads, palettes, windows, timing, and object placement beyond the fields used for counting, and it populates `CaptionIDs` the same way (a `ContainsKey` insert) without further reporting. This port matches that behavior.
- Caption counting happens during the full stream scan in both implementations; quick codec-init marks PGS initialized without counting, as dispatched in `codec/mod.rs`.
