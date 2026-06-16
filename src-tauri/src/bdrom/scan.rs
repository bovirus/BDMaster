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

use std::path::Path;

use anyhow::Result;

use crate::bdrom::codec_cache::open_codec_cache;
use crate::bdrom::codec_init::codec_init;
use crate::bdrom::disc_info::{cache_estimated_stream_sizes, refresh_ssif_derived_metadata, to_disc_info};
use crate::bdrom::open::open_bdrom;
use crate::protocol::DiscInfo;

pub fn scan(path_str: &str) -> Result<DiscInfo> {
  let path = Path::new(path_str);
  let use_ssif = crate::config::get_config().scan.enable_ssif_support;
  let bdrom = open_bdrom(path, use_ssif)?;
  let mut disc = to_disc_info(&bdrom);
  // Set up the per-disc codec cache for this open (disposing any other
  // disc's entries). codec_init reads each distinct stream once and reuses
  // the cached result for every clip that repeats it.
  open_codec_cache(&disc.path);
  // Codec initialization pass — mirrors BDInfo's `streamFile.Scan(playlists,
  // isFullScan: false)`. For every unique M2TS clip we open the stream once
  // and feed its PES payloads to the codec parsers until every relevant PID
  // has reported `is_initialized`, at which point the scan early-stops. This
  // populates per-stream codec details (codec_name, height, frame rate,
  // encoding profile, channel layout, sample rate, bit depth, …) and the
  // codec-fixed bit_rate (LPCM, AC3, DTS, MPA, …). For VBR streams that
  // codec parsers can't pin down, we estimate bit_rate from the running
  // total of payload bytes / elapsed seconds collected during the scan.
  codec_init(&mut disc, &bdrom);
  refresh_ssif_derived_metadata(&mut disc, &bdrom);
  cache_estimated_stream_sizes(&mut disc);
  Ok(disc)
}
