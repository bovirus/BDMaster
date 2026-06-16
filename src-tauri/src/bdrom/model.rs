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
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};

use crate::bdrom::clpi::StreamClipFile;
use crate::bdrom::mpls::PlaylistFile;
use crate::bdrom::udf::{UdfFile, UdfFileReader, UdfImage};

pub(crate) const SSIF_MVC_PID: u16 = 0x1012;

#[derive(Clone)]
pub enum StreamSource {
  Native(PathBuf),
  Iso(UdfFile),
}

#[derive(Clone)]
pub enum DiscSource {
  Native,
  Iso(Arc<Mutex<UdfImage>>),
}

pub struct BDRom {
  pub path: PathBuf,
  pub source: DiscSource,
  pub volume_label: String,
  pub disc_title: Option<String>,
  pub size: u64,
  pub is_uhd: bool,
  pub is_bd_plus: bool,
  pub is_bd_java: bool,
  pub is_dbox: bool,
  pub is_psp: bool,
  pub is_3d: bool,
  pub is_50_hz: bool,
  pub playlists: HashMap<String, PlaylistFile>,
  pub stream_files: HashMap<String, (StreamSource, u64)>,
  pub stream_clip_files: HashMap<String, StreamClipFile>,
  /// SSIF (interleaved stereoscopic) counterparts keyed by the matching
  /// `.M2TS` clip name (uppercase). Populated from `BDMV/STREAM/SSIF/*.ssif`
  /// whenever the directory exists, regardless of the `use_ssif` flag —
  /// callers that don't want SSIF simply ignore the map.
  pub interleaved_files: HashMap<String, (StreamSource, u64)>,
  /// When true, `effective_stream_source` returns the SSIF reader / size for
  /// any clip with an interleaved counterpart, so codec init and the full
  /// scan see the AVC + MVC payload instead of the AVC-only `.m2ts`. Set
  /// from `config.scan.enable_ssif_support` at open time.
  pub use_ssif: bool,
}

/// Pick the stream source (and size) for a given clip, honoring the
/// `use_ssif` flag on the BDRom. When SSIF is enabled and the clip has an
/// interleaved counterpart (`<stem>.SSIF` next to `<stem>.M2TS`), the SSIF
/// is returned — codec parsers and the full-scan worker then see the AVC +
/// MVC payload instead of the AVC-only base file. Falls back to the M2TS in
/// every other case.
pub(crate) fn effective_stream_source<'a>(bd: &'a BDRom, clip_name: &str) -> Option<&'a (StreamSource, u64)> {
  if bd.use_ssif {
    if let Some(ssif) = bd.interleaved_files.get(clip_name) {
      return Some(ssif);
    }
  }
  bd.stream_files.get(clip_name)
}

/// Open a streaming reader for an M2TS stream entry, regardless of whether
/// the disc source is a directory or an ISO image.
pub(crate) fn open_stream_reader(bd: &BDRom, src: &StreamSource) -> Result<Box<dyn std::io::Read + Send>> {
  match src {
    StreamSource::Native(p) => {
      let f = std::fs::File::open(p)?;
      Ok(Box::new(std::io::BufReader::with_capacity(1 << 20, f)))
    }
    StreamSource::Iso(fe) => {
      if let DiscSource::Iso(image) = &bd.source {
        // Wrap with BufReader: every UdfFileReader::read locks the
        // shared image mutex, seeks, and reads. Without buffering,
        // a 5 MB codec-init scan triggers tens of thousands of
        // mutex+seek+read cycles. A 1 MB buffer cuts that to a
        // handful of refills.
        Ok(Box::new(std::io::BufReader::with_capacity(
          1 << 20,
          UdfFileReader::new(image.clone(), fe)?,
        )))
      } else {
        Err(anyhow!("ISO stream source without ISO disc source"))
      }
    }
  }
}

/// Like `open_stream_reader` but returns the raw, *unbuffered* reader. The
/// full-scan worker uses this so it can interpose its own ProgressReader
/// below a single large BufReader — that way per-packet reads in the m2ts
/// loop hit the buffer (a memcpy) instead of the progress wrapper (atomic
/// load + addition + clock check), removing tens of seconds of overhead on
/// disc-sized inputs.
pub(crate) fn open_stream_reader_raw(bd: &BDRom, src: &StreamSource) -> Result<Box<dyn std::io::Read + Send>> {
  match src {
    StreamSource::Native(p) => {
      let f = std::fs::File::open(p)?;
      Ok(Box::new(f))
    }
    StreamSource::Iso(fe) => {
      if let DiscSource::Iso(image) = &bd.source {
        Ok(Box::new(UdfFileReader::new(image.clone(), fe)?))
      } else {
        Err(anyhow!("ISO stream source without ISO disc source"))
      }
    }
  }
}
