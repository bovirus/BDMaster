# bdrom/codec/mvc.rs

## Description

MVC stream marker/parser. This corresponds to BDInfo's `TSCodecMVC.cs`.

## Implementation Progress

100%

## Implementation Details

- Faithful port of `TSCodecMVC.Scan`, which only marks MVC streams VBR and initialized (the C# source itself carries the `// TODO: Do something more interesting here...` comment).
- Actual 3D/base-view presentation is handled by MPLS metadata and SSIF handling in `mod.rs` and `full_scan.rs`.
- Tested to confirm the two flags are set.

## Parity Notes (mirrors BDInfo exactly)

- BDInfo does not parse MVC NAL units, view identifiers, dependency information, or MVC profile/level metadata; neither does this port. Practical 3D handling relies on SSIF source selection and the MVC PID convention, exactly as upstream.
