# bdrom/m2ts.rs

## Description

M2TS/MPEG-TS packet scanner for Blu-ray 192-byte BDAV packets. This is a pragmatic Rust split of the packet parsing and stream accounting logic in BDInfo's `TSStreamFile.cs`.

## Implementation Progress

100%

## Implementation Details

- Parses 4-byte arrival timecode plus 188-byte MPEG-TS packets.
- Discovers PMT PIDs from PAT and elementary stream PIDs/types from PMT.
- Counts payload bytes and packets per PID.
- Reassembles PES payloads and dispatches them to a caller callback during streaming scans.
- Supports `PesAction::Continue`, `Stop`, and `SkipPid` so codec initialization and full scan can stop expensive PES assembly once a stream is initialized.
- Computes duration from PCR when available, with ATC as fallback.
- Produces one-second bitrate samples for charting.
- Provides both path/reader based scanners and progress snapshots for the full-scan worker.
- Tests build synthetic M2TS frames (PAT + PMT + PES) and verify PMT-PID discovery, elementary-stream type mapping, PES dispatch (with the live PMT table passed to the callback), empty input, and skipping packets with a bad sync byte.

## Design Notes (intentional differences from BDInfo)

- This scanner deliberately works at the container level: it does not implement BDInfo's full PTS/DTS tracking, per-stream `PacketSeconds`, or PTS-window bitrate. One-second samples are based on M2TS packet bytes rather than per-video-PID presentation intervals — the lighter model the rest of the pipeline (chart + chapter metrics) is built around.
- PAT/PMT parsing assumes the table section fits in one payload (true for Blu-ray PSI); multi-section/fragmented PSI is not reassembled, and continuity counters, table CRCs, transport-error/scrambling bits, and descriptor payloads are not validated.
- Resynchronization is minimal (packets with a missing sync byte are skipped), variable packet sizes / non-BDAV inputs are not modeled, and `scan_inner` streaming results intentionally do not retain PES samples — only the callback sees full PES payloads. There is no `TSStreamDiagnostics`-style log.
