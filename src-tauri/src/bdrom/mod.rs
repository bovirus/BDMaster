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

/*
 * Top-level Blu-ray disc scanner. Locates BDMV/PLAYLIST/CLIPINF/STREAM
 * directories under a path and aggregates parsed playlists, clips and
 * streams into a DiscInfo.
 */

pub mod clpi;
pub mod codec;
pub mod full_scan;
pub mod lang;
pub mod m2ts;
pub mod mpls;
pub mod types;
pub mod udf;

mod codec_cache;
mod codec_init;
mod disc_info;
mod disc_title;
mod model;
mod open;
mod paths;
mod scan;
#[cfg(test)]
mod tests;

// Public API surface used outside the bdrom module.
pub use codec_cache::close_codec_cache;
pub use paths::{resolve_playlist_path, resolve_stream_file_path};
pub use scan::scan;

// Crate-internal re-exports so other modules (full_scan, controller, lib, and
// sibling bdrom submodules) can keep referencing these items via
// `crate::bdrom::X` / `super::X`. Item names are unique across submodules, so
// glob re-exports don't collide.
pub(crate) use disc_info::*;
pub(crate) use model::*;
pub(crate) use open::*;
