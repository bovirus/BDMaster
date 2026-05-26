# bdrom/types.rs

## Description

Shared Blu-ray stream enums and simple display helpers. This corresponds to the enum and name-resolution parts of BDInfo's `TSStream.cs`.

## Implementation Progress

70%

## Implementation Details

- Defines `TSStreamType`, `TSVideoFormat`, `TSFrameRate`, `TSAspectRatio`, `TSChannelLayout`, and `TSAudioMode`.
- Provides conversion from raw Blu-ray byte values to Rust enums.
- Implements stream category helpers (`is_video`, `is_audio`, `is_graphics`, `is_text`).
- Provides codec long/short names and broad type labels.
- Converts Blu-ray sample-rate nibble values to Hz.
- Provides simple labels for frame rate, aspect ratio, channel layout, and audio mode.

## Open Issues

- Does not include BDInfo's `TSSampleRate` enum as a first-class type.
- Does not model `TSDescriptor` or descriptor cloning.
- The class hierarchy from BDInfo (`TSStream`, `TSVideoStream`, `TSAudioStream`, `TSGraphicsStream`, `TSTextStream`) is represented elsewhere as protocol DTO fields, so behavior is split across modules.
- Codec name and description formatting is simplified compared with `TSStream.cs` properties.
- No tests assert exact text parity with BDInfo's user-facing names.
- Unknown language/stream metadata handling is intentionally sparse.

