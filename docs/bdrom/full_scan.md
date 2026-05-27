# bdrom/full_scan.rs

## Description

Background full-disc scanning worker. This maps primarily to BDInfo's full-scan path in `TSStreamFile.cs` and the `BDROM.Scan()` orchestration in `BDROM.cs`.

## Implementation Progress

100%

## Implementation Details

- Starts a single background worker, publishes progress through `FullScanState`, and supports cancellation through an atomic flag.
- Opens the disc once, builds initial `DiscInfo`, runs codec initialization, and then scans every unique angle-0 clip.
- Uses `ProgressReader` to update byte progress and to short-circuit reads on cancellation.
- Streams each M2TS/SSIF file through `m2ts::scan_m2ts_streaming_from_reader_with_progress`.
- Dispatches PES payloads through codec parsers in full-scan mode; PGS streams continue scanning so caption counts accumulate.
- Applies per-clip and per-stream measured sizes from per-PID byte totals.
- Adds hidden PMT streams and special-cases SSIF MVC streams as visible MVC extensions.
- Builds playlist bitrate samples and chapter bitrate metrics from one-second M2TS bitrate buckets.
- All progress-mutex access uses `lock().unwrap_or_else(|e| e.into_inner())`, so a panic in one part of the worker can no longer poison the progress lock and bring down the polling path.

## Design Notes (intentional differences from BDInfo)

- The worker scans angle-0 clips; angle-specific streams and per-angle byte totals are not separately modeled, matching the angle-0 codec-init model.
- Per-clip / per-stream byte attribution for partial clips is proportional by duration rather than computed from exact PTS windows, and chart samples are container-level M2TS byte rates (then scaled for chapter video metrics). This is the project's deliberate, lighter-weight alternative to BDInfo's PTS-based per-stream activity tracking.
- Because frames are not individually parsed, `avg_frame_size` / `max_frame_size` / `max_frame_time` stay zero and there is no frame-reorder or frame-size diagnostic — consistent with the container-level scanning model.
- Per-file scan errors are logged and scanning continues (no per-file error field in the progress DTO), and hidden-stream discovery uses PMT/PES visibility during the scan.
- Per-file scanning shares stream records through raw pointers into the playlist stream vectors; the vectors are never mutated while those pointers are live, so the borrow is sound and avoids cloning every stream per PES payload.

## Open Issues

- Full scan still uses proportional duration-based byte attribution and container-level bitrate buckets instead of BDInfo's exact PTS-window per-stream accounting. Chapter/video bitrate numbers can differ on partial clips or sparse streams.
- Angle-specific streams and per-angle byte totals are not scanned separately. BDInfo carries angle collections, while this worker only measures angle-0 clips.
- Frame-level diagnostics (`avg_frame_size`, `max_frame_size`, `max_frame_time`, frame reorder, and `TSStreamDiagnostics`) remain unavailable because the M2TS layer does not expose the underlying PTS/frame model.
- Per-file scan errors are only logged; the progress DTO has no per-file error list or BDInfo-style callback result, so the UI cannot distinguish a clean scan from a scan with skipped files.
- Line coverage is above the target at 92.97%, but function coverage is 88.78%. Worker error/cancellation and less common measurement branches still need targeted tests if function coverage is treated as part of the 90% requirement.
