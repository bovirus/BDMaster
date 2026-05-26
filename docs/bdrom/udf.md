# bdrom/udf.rs

## Description

Minimal UDF 2.x image reader used for Blu-ray ISO support. BDInfo uses DiscUtils (`DiscUtils.Udf`) for this responsibility, so this module is a Rust replacement rather than a direct BDInfo source-file port.

## Implementation Progress

75%

## Implementation Details

- Finds the Anchor Volume Descriptor Pointer at canonical UDF locations.
- Walks main and reserve volume descriptor sequences.
- Selects the newest Partition Descriptor and Logical Volume Descriptor by sequence number.
- Supports Type 1 physical partition maps and Type 2 Metadata Partition maps used by UHD Blu-rays.
- Reads File Set Descriptor, File Entry, Extended File Entry, short/long allocation descriptors, embedded data, and File Identifier Descriptors.
- Resolves paths case-insensitively and lists directories.
- Computes directory sizes while skipping `.ssif` files to match BDInfo disc-size behavior.
- Provides `UdfFileReader`, a streaming `Read` implementation over allocation extents for M2TS scanning without loading whole files.

## Open Issues

- Does not validate descriptor checksums, tag CRCs, descriptor versions, or logical volume integrity data.
- Unsupported Type 2 maps such as sparable or virtual partitions fall back to direct mapping and may read incorrect data.
- Metadata mirror files, bitmap files, sparing tables, and defect management are not implemented.
- Allocation descriptor type bits are mostly ignored beyond short, long, and embedded data.
- `UdfFileReader` does not stream `embedded_data`; this is fine for M2TS files but incomplete as a generic reader.
- D-string handling only covers compression IDs 8 and 16 and does not cover every OSTA compressed Unicode edge case.
- Symlinks, file versions, permissions, timestamps, and extended attributes are ignored.
- Recursive directory-size traversal has no explicit loop protection.
- Shared ISO access uses `Mutex::lock().unwrap()`, so lock poisoning can panic.

