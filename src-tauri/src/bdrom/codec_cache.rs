/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::bdrom::clpi::ClpiStream;
use crate::protocol::TSStreamInfo;

/// Cache key for one elementary stream.
///
/// - `disc_path` isolates discs (and parallel tests) from each other.
/// - `playlists` is the *set of playlists that reference the clip* the stream
///   came from. This is the crucial scoping bit: codec attributes are a
///   property of the stream, but bitstream-level details (e.g. AC3 dialnorm,
///   HDR metadata) are content-specific. Two playlists can declare audio with
///   an identical CLPI descriptor yet carry different content with different
///   dialnorm — they are *different* streams and must not share a cache entry.
///   Clips that belong to the same playlist set, however, feed the same
///   playlist stream entries, where the codec pass keeps the first clip's
///   values anyway — so reusing within a playlist set matches a full scan.
/// - `descriptor` is the clip's CLPI coding descriptor (PID + type + format
///   bytes); a different codec/resolution on the same PID is a distinct entry.
pub(crate) type StreamDescriptor = (u16, u8, u8, u8, u8, u8, u32);
pub(crate) type StreamCacheKey = (String, Vec<usize>, StreamDescriptor);

/// Per-disc codec cache.
///
/// A Blu-ray reuses the same elementary streams across many clips — a
/// "play-all" / menu-loop playlist can reference hundreds of clips that all
/// carry the same video + audio. So each distinct stream (per the key above)
/// is scanned once, cached here, and reused everywhere it recurs; a clip whose
/// every stream is already cached is never read.
///
/// Lifecycle: set up when a disc is opened (`open_codec_cache`) and disposed
/// when it is closed (`close_codec_cache`). This is a single-disc app, so
/// opening a disc also disposes any other disc's entries.
static CODEC_CACHE: OnceLock<Mutex<HashMap<StreamCacheKey, TSStreamInfo>>> = OnceLock::new();

pub(crate) fn codec_cache() -> &'static Mutex<HashMap<StreamCacheKey, TSStreamInfo>> {
  CODEC_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Set up the codec cache for a freshly opened disc. Entries for any other
/// disc are disposed; re-opening the same disc keeps its entries so
/// already-scanned streams are reused rather than re-read.
pub(crate) fn open_codec_cache(disc_path: &str) {
  let mut cache = codec_cache().lock().unwrap_or_else(|e| e.into_inner());
  cache.retain(|(p, _, _), _| p == disc_path);
}

/// Dispose the codec cache for a disc that has been closed in the app.
pub fn close_codec_cache(disc_path: &str) {
  let mut cache = codec_cache().lock().unwrap_or_else(|e| e.into_inner());
  cache.retain(|(p, _, _), _| p != disc_path);
}

pub(crate) fn clpi_stream_descriptor(s: &ClpiStream) -> StreamDescriptor {
  (
    s.pid,
    s.stream_type,
    s.video_format,
    s.frame_rate,
    s.aspect_ratio,
    s.channel_layout,
    s.sample_rate,
  )
}
