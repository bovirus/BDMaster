# bdrom/full_scan.rs

## Description

Background full-disc worker corresponding primarily to BDInfo's full-scan path in `TSStreamFile.cs` and `BDROM.Scan()` orchestration.

## Implementation Progress

100%

## Implementation Details

- Starts one background worker, publishes snapshots through `FullScanState`, and supports atomic cancellation.
- Opens the disc once and immediately reads every unique main- and alternate-angle M2TS/SSIF file end-to-end. Codec initialization happens inline; the full phase does not rerun the configurable fast-scan budget.
- `ProgressReader` publishes cumulative byte progress and terminates reads on cancellation.
- Streams files through `m2ts::scan_m2ts_streaming_from_reader_with_progress`.
- Dispatches PES payloads in full-scan mode; PGS continues through the file so caption counts accumulate.
- Applies exact per-clip/per-stream payload and packet measurements from PTS/DTS windows. Timestamp-free or malformed files fall back to proportional duration attribution.
- Maintains separate alternate-angle video collections and measurements, matching BDInfo's `AngleStreams` model.
- Adds hidden PMT streams and special-cases SSIF MVC streams as visible MVC extensions.
- Builds playlist bitrate samples plus chapter average, 1-/5-/10-second peak, and frame-size metrics from primary-video timing diagnostics.
- Publishes updated disc snapshots between files and keeps scanning after individual file failures. Every failure is retained in `fileErrors` and shown by the frontend when scanning completes.
- Progress locks recover poisoned guards so polling remains available after an unrelated worker panic.

## Design Notes (intentional differences from BDInfo)

- Live snapshots within the current file use proportional byte estimates because incomplete timing windows cannot be attributed safely. The completed-file snapshot replaces them with exact measurements when timestamps are available.
- Provisional snapshots are built from clones so progress reporting cannot overwrite codec metadata being discovered in the live full-scan state.
- Video timing windows are treated as frame windows for public frame-size metrics. BDInfo additionally attaches codec-specific tags and documents a remaining B-pyramid frame-reorder TODO; BDMaster does not expose those tags.
- Stream records are shared through contained raw pointers while scanning a file. The vectors are never mutated while those pointers are live.

## Open Issues

- Codec-specific diagnostic tags, frame reordering, and the optional raw `TSStreamDiagnostics` text block are not part of the public protocol.
