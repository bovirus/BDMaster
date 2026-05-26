# bdrom/codec/mvc.rs

## Description

MVC stream marker/parser stub. This corresponds to BDInfo's `TSCodecMVC.cs`.

## Implementation Progress

95%

## Implementation Details

- Matches BDInfo's current behavior: mark MVC streams VBR and initialized.
- Actual 3D/base-view presentation is handled by MPLS metadata and SSIF handling in `mod.rs` and `full_scan.rs`.

## Open Issues

- Does not parse MVC NAL units, view identifiers, dependency information, or MVC-specific profile/level metadata.
- Relies on SSIF source selection and a fixed MVC PID convention for practical 3D handling.
- Does not validate that the MVC stream is paired with the expected AVC base view.

