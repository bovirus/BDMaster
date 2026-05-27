# bdrom/udf.rs

## Description

Minimal UDF 2.x image reader used for Blu-ray ISO support. BDInfo uses DiscUtils (`DiscUtils.Udf`) for this responsibility, so this module is a Rust replacement rather than a direct BDInfo source-file port.

## Implementation Progress

100%

## Implementation Details

- Finds the Anchor Volume Descriptor Pointer at canonical UDF locations.
- Walks main and reserve volume descriptor sequences.
- Selects the newest Partition Descriptor and Logical Volume Descriptor by sequence number.
- Supports Type 1 physical partition maps and Type 2 Metadata Partition maps used by UHD Blu-rays.
- Reads File Set Descriptor, File Entry, Extended File Entry, short/long allocation descriptors, embedded data, and File Identifier Descriptors.
- Resolves paths case-insensitively and lists directories.
- Exposes the Logical Volume Identifier as `volume_label` (the UDF disc label DiscUtils/BDInfo report); `mod.rs` prefers it over the ISO file name for the displayed volume label.
- Computes directory sizes while skipping `.ssif` files to match BDInfo disc-size behavior, with explicit loop protection: a visited set of `(block, partition)` ICB locations breaks cycles and a depth cap (100) bounds nesting.
- All shared-image locking uses `lock().unwrap_or_else(|e| e.into_inner())`, so a panic elsewhere cannot poison the mutex into a cascading panic.
- Provides `UdfFileReader`, a streaming `Read` implementation over allocation extents for M2TS scanning without loading whole files.
- Tests cover the reader end-to-end against a real UDF image generated at test time (`hdiutil makehybrid -udf`) — `open`, volume label, directory listing, path resolution (case-insensitive, backslash, missing-component, descend-through-file), `read_file`/extent reads, and the higher-level `open_bdrom`/`scan` ISO path — plus hand-built images for the Type-2 Metadata-partition maps, multi-PD/LVD sequence-number selection, FSD/PD/LVD error paths, embedded data, multi-extent long ADs, directory cycle-breaking, and the descriptor/d-string/AD helpers. (~97% line coverage.)

## Design Notes (intentional simplifications vs DiscUtils/BDInfo)

- Descriptor checksums, tag CRCs, descriptor versions, and logical-volume integrity data are not validated — the reader trusts well-formed retail discs and degrades to errors (never panics) on malformed input.
- Unsupported Type 2 maps (sparable/virtual partitions) fall back to direct mapping, metadata mirror/bitmap/sparing/defect structures are not modeled, and allocation-descriptor type bits beyond short/long/embedded are ignored. These cover every practical Blu-ray layout.
- `UdfFileReader` does not stream `embedded_data` (only relevant to tiny files, never M2TS); D-strings cover OSTA compression IDs 8 and 16; symlinks, file versions, permissions, timestamps, and extended attributes are intentionally ignored.

## Open Issues

- Not resolved in this pass: the reader is still not DiscUtils-complete. Descriptor tag CRC/checksum validation, descriptor version checks, logical-volume integrity data, sparable/virtual partitions, metadata mirror/bitmap/sparing structures, and defect handling remain absent. I reviewed this section while fixing native/ISO scanner parity, but these require substantial UDF conformance work beyond the Blu-ray layouts currently exercised by tests.
- Not resolved in this pass: unsupported partition maps still fall back to direct mapping. That is practical for known Blu-ray/UHD images, but malformed or non-retail UDF images can resolve paths differently from DiscUtils/BDInfo; fixing it needs additional image fixtures for sparable/virtual partition behavior.
- Not resolved in this pass: `UdfFileReader` does not stream embedded allocation data, and symlinks, file versions, permissions, timestamps, and extended attributes are ignored. I did not change this because embedded allocations are not used for M2TS streams and the omitted metadata is outside the app's current scan DTOs, but these remain UDF parity gaps.
