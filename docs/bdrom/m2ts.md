# bdrom/m2ts.rs

## Description

M2TS/MPEG-TS packet scanner for Blu-ray 192-byte BDAV packets. This is a pragmatic Rust split of the packet parsing and stream accounting logic in BDInfo's `TSStreamFile.cs`.

## Implementation Progress

68%

## Implementation Details

- Parses 4-byte arrival timecode plus 188-byte MPEG-TS packets.
- Discovers PMT PIDs from PAT and elementary stream PIDs/types from PMT.
- Counts payload bytes and packets per PID.
- Reassembles PES payloads and dispatches them to a caller callback during streaming scans.
- Supports `PesAction::Continue`, `Stop`, and `SkipPid` so codec initialization and full scan can stop expensive PES assembly once a stream is initialized.
- Computes duration from PCR when available, with ATC as fallback.
- Produces one-second bitrate samples for charting.
- Provides both path/reader based scanners and progress snapshots for the full-scan worker.

## Open Issues

- Does not implement BDInfo's full PTS/DTS tracking, per-stream `PacketSeconds`, or PTS-window bitrate calculation.
- PAT and PMT parsing assumes the relevant table section is available in one payload; multi-section or fragmented PSI is not reassembled.
- Does not validate continuity counters, table CRCs, transport errors, scrambling state, or descriptor payloads.
- Resynchronization is minimal; packets with a missing sync byte are skipped rather than using BDInfo's more involved parser state.
- One-second samples are based on M2TS packet bytes, not per-video PID bytes or exact presentation intervals.
- `scan_inner` streaming results intentionally do not preserve PES samples; only the callback sees full PES payloads.
- Does not model variable packet sizes or non-BDAV TS inputs beyond basic skipping.
- No diagnostic log equivalent to BDInfo's `TSStreamDiagnostics`.

