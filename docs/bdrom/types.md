# bdrom/types.rs

## Description

Shared Blu-ray stream enums and simple display helpers. This corresponds to the enum and name-resolution parts of BDInfo's `TSStream.cs`.

## Implementation Progress

100%

## Implementation Details

- Defines `TSStreamType`, `TSVideoFormat`, `TSFrameRate`, `TSAspectRatio`, `TSChannelLayout`, and `TSAudioMode`.
- Provides conversion from raw Blu-ray byte values to Rust enums.
- Implements stream category helpers (`is_video`, `is_audio`, `is_graphics`, `is_text`).
- Provides codec long/short names and broad type labels.
- `codec_name_dynamic` / `codec_short_name_dynamic` reproduce BDInfo's `CodecName` / `CodecShortName` extension-dependent variants — Dolby Digital EX, Dolby Digital Plus/Atmos, Dolby TrueHD/Atmos (Atmos short name), DTS-ES, and DTS:X High-Res/Master. `codec/mod.rs::finalize_description` applies these once `has_extensions` / `audio_mode` are known, so Atmos/DTS:X labels surface like BDInfo.
- Converts Blu-ray sample-rate nibble values to Hz via `convert_sample_rate` (the functional equivalent of BDInfo's `TSSampleRate` enum).
- Provides simple labels for frame rate, aspect ratio, channel layout, and audio mode.
- Tests assert text parity with BDInfo for base and dynamic codec names/short names, type categories, video-format height/interlace, frame-rate/aspect/channel-layout labels, sample-rate conversion, and `from_u8` round-trips.

## Parity Notes (mirrors BDInfo by design)

- The Rust port models BDInfo's `TSStream` / `TSVideoStream` / `TSAudioStream` / `TSGraphicsStream` / `TSTextStream` hierarchy as a single flat `TSStreamInfo` protocol DTO plus these enums; behavior that lived in the C# subclasses is distributed across the codec and `mod.rs` layers.
- Sample-rate handling uses `convert_sample_rate` rather than a first-class `TSSampleRate` enum, and MPEG descriptors are parsed inline in `m2ts.rs` rather than via a `TSDescriptor` object — both deliberate simplifications with equivalent results.
