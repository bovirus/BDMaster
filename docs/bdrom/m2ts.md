# bdrom/m2ts.rs

## Description

M2TS/MPEG-TS packet scanner for Blu-ray 192-byte BDAV packets. This maps the transport parsing and stream accounting in BDInfo's `TSStreamFile.cs`.

## Implementation Progress

100%

## Implementation Details

- Parses the 4-byte arrival timecode plus 188-byte MPEG-TS packets.
- Reassembles PAT/PMT PSI sections across packets, then discovers PMT and elementary-stream PIDs/types.
- Counts 192-byte packets and elementary PES payload bytes per PID. PES headers/stuffing are excluded and bounded PES packet lengths are honored, matching BDInfo.
- Reassembles PES payloads and dispatches them to codec callbacks.
- Supports `PesAction::Continue`, `Stop`, and `SkipPid`, so initialized streams stop allocating PES buffers while byte/timing accounting continues.
- Parses and unwraps 33-bit PTS/DTS values. Successive video timestamps close BDInfo-style per-PID windows containing marker, interval, payload bytes, and packet counts.
- Computes duration from the video timestamp span when present, then PCR and arrival time as fallbacks.
- Uses one production parser (`scan_inner`) for native/UDF readers, codec callbacks, and progress snapshots.
- Retains arrival-time bitrate samples as a compatibility fallback for malformed or timestamp-free inputs.
- Synthetic tests cover fragmented PSI, bounded PES payloads, PTS windows, wraparound, PMT discovery/type mapping, PES dispatch, progress hooks, malformed packets, and reader failures.

## Design Notes (intentional differences from BDInfo)

- Timing diagnostics are retained for full-scan aggregation but are not exposed as BDInfo's optional textual `TSStreamDiagnostics` report.
- Scrambled elementary payload is skipped as in BDInfo. PSI continuity counters and CRCs, transport-error flags, and descriptor payloads are not validated or interpreted; BDInfo likewise leaves most descriptor interpretation as TODO.
- Resynchronization is minimal (a packet with a missing sync byte is skipped), and variable packet sizes/non-BDAV inputs are not modeled.

## Open Issues

- Table CRC/continuity validation, robust byte-level resynchronization, codec-specific diagnostic tags, and textual/extended diagnostics remain outside the current public scan model.
