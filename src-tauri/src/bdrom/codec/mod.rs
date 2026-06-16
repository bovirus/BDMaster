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
 * Codec module: per-codec parsers plus the dispatcher that routes a
 * reassembled PES payload to the matching parser. See `dispatch` for the
 * dispatch logic; each `*` submodule is a single codec parser.
 */

pub mod aac;
pub mod ac3;
pub mod avc;
pub mod dts;
pub mod dtshd;
pub mod hevc;
pub mod lpcm;
pub mod mpa;
pub mod mpeg2;
pub mod mvc;
pub mod pgs;
pub mod stream_buffer;
pub mod truehd;
pub mod vc1;

mod dispatch;

pub use dispatch::{CodecScanState, finalize_description, scan_stream};
pub use pgs::PgsState;
pub use stream_buffer::TSStreamBuffer;
