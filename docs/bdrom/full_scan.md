# bdrom/full_scan.rs

## Description

Background full-disc scanning worker. This maps primarily to BDInfo's full-scan path in `TSStreamFile.cs` and the `BDROM.Scan()` orchestration in `BDROM.cs`.

## Implementation Progress

70%

## Implementation Details

- Starts a single background worker, publishes progress through `FullScanState`, and supports cancellation through an atomic flag.
- Opens the disc once, builds initial `DiscInfo`, runs codec initialization, and then scans every unique angle-0 clip.
- Uses `ProgressReader` to update byte progress and to short-circuit reads on cancellation.
- Streams each M2TS/SSIF file through `m2ts::scan_m2ts_streaming_from_reader_with_progress`.
- Dispatches PES payloads through codec parsers in full-scan mode; PGS streams continue scanning so caption counts accumulate.
- Applies per-clip and per-stream measured sizes from per-PID byte totals.
- Adds hidden PMT streams and special-cases SSIF MVC streams as visible MVC extensions.
- Builds playlist bitrate samples and chapter bitrate metrics from one-second M2TS bitrate buckets.

## Open Issues

- Only angle-0 clips are scanned; angle-specific streams and angle byte totals are not fully modeled.
- Per-clip and per-stream byte attribution for partial clips is proportional by duration, not based on exact PTS windows.
- Chart samples are container-level M2TS byte rates, then scaled for chapter video metrics; BDInfo computes richer PTS-based per-stream activity.
- Chapter metrics leave `avg_frame_size`, `max_frame_size`, and `max_frame_time` at zero.
- Full scan does not implement BDInfo's frame reorder handling or frame-size diagnostics.
- File scan errors are logged and scanning continues; there is no per-file error surface in the progress DTO.
- Hidden stream discovery depends on PMT and PES visibility during the scan; no CLPI fallback exists.
- Uses raw pointers into playlist stream vectors during per-file scanning; correctness depends on not mutating those vectors until pointers are dropped.
- Mutex locks use `unwrap`, so poisoned progress locks can panic.

