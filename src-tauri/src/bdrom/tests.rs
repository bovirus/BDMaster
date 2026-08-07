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

use super::*;
use crate::bdrom::disc_title::{extract_title_from_xml, read_disc_title_native};
use crate::bdrom::mpls::{self, PlaylistFile};
use crate::bdrom::types::*;
use crate::protocol::{DiscInfo, PlaylistInfo, TSStreamInfo};
use std::path::{Path, PathBuf};
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::time::{SystemTime, UNIX_EPOCH};

  // ---- Temp-dir scaffolding (no external crates). -----------------------

  static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

  /// A unique temp directory that removes itself (recursively) on drop, so
  /// the BDMV tree we build is cleaned up even if a test panics.
  struct TempDir {
    path: PathBuf,
  }

  impl TempDir {
    fn new(tag: &str) -> Self {
      let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
      let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
      let name = format!("bdmaster_modtest_{}_{}_{}_{}", tag, std::process::id(), n, nanos);
      let path = std::env::temp_dir().join(name);
      std::fs::create_dir_all(&path).expect("create temp dir");
      TempDir { path }
    }

    fn path(&self) -> &Path {
      &self.path
    }

    /// Create a file (creating parent dirs) under the temp root.
    fn write(&self, rel: &str, bytes: &[u8]) {
      let p = self.path.join(rel);
      if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
      }
      std::fs::write(&p, bytes).expect("write file");
    }

    fn mkdir(&self, rel: &str) {
      std::fs::create_dir_all(self.path.join(rel)).expect("create dir");
    }
  }

  impl Drop for TempDir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.path);
    }
  }

  // ---- MPLS builder (mirrors mpls.rs `build_mpls`, parameterized). ------

  /// One stream entry for the STN table: (header_type byte stays 1) PID +
  /// coding-info bytes already shaped for the requested stream type.
  struct StreamSpec {
    pid: u16,
    coding: Vec<u8>,
  }

  fn avc_video(pid: u16) -> StreamSpec {
    StreamSpec {
      pid,
      // length 3: 0x1b AVC, video format 1080p / 23.976, aspect 16:9
      coding: vec![3, 0x1b, (6 << 4) | 1, 3 << 4],
    }
  }

  fn ac3_audio(pid: u16, lang: &[u8; 3]) -> StreamSpec {
    StreamSpec {
      pid,
      coding: vec![5, 0x81, (6 << 4) | 1, lang[0], lang[1], lang[2]],
    }
  }

  /// Build an MPLS with a single play item (clip 00001.M2TS) carrying the
  /// supplied video + audio streams, an out_time, and one chapter.
  fn build_mpls_custom(
    out_time_45k: u32,
    videos: &[StreamSpec],
    audios: &[StreamSpec],
    mvc_base_view_r: bool,
  ) -> Vec<u8> {
    let mut d: Vec<u8> = Vec::new();
    d.extend_from_slice(b"MPLS0200");
    d.extend_from_slice(&[0u8; 4]); // playlist_offset @8
    d.extend_from_slice(&[0u8; 4]); // chapters_offset @12
    d.extend_from_slice(&[0u8; 4]); // extensions_offset @16
    while d.len() < 0x38 {
      d.push(0);
    }
    d.push(if mvc_base_view_r { 0x10 } else { 0x00 }); // misc flags @0x38

    let playlist_offset = d.len() as u32;
    d[8..12].copy_from_slice(&playlist_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes()); // playlist_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.extend_from_slice(&1u16.to_be_bytes()); // item_count
    d.extend_from_slice(&0u16.to_be_bytes()); // subitem_count

    let item_start = d.len();
    d.extend_from_slice(&0u16.to_be_bytes()); // item_length placeholder
    d.extend_from_slice(b"00001"); // item name
    d.extend_from_slice(b"M2TS"); // item type
    d.push(0x00);
    d.push(0x00); // no multiangle
    d.push(0x00);
    d.extend_from_slice(&0u32.to_be_bytes()); // in_time
    d.extend_from_slice(&out_time_45k.to_be_bytes()); // out_time
    d.extend_from_slice(&[0u8; 12]); // reserved skip

    d.extend_from_slice(&0u16.to_be_bytes()); // stn_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.push(videos.len() as u8); // video count
    d.push(audios.len() as u8); // audio count
    d.push(0); // pg
    d.push(0); // ig
    d.push(0); // secondary audio
    d.push(0); // secondary video
    d.push(0); // pip
    d.extend_from_slice(&[0u8; 5]); // reserved skip

    for v in videos {
      d.push(3); // stream-entry header length
      d.push(1); // header type 1 -> PID follows
      d.extend_from_slice(&v.pid.to_be_bytes());
      d.extend_from_slice(&v.coding);
    }
    for a in audios {
      d.push(3);
      d.push(1);
      d.extend_from_slice(&a.pid.to_be_bytes());
      d.extend_from_slice(&a.coding);
    }

    let item_len = (d.len() - item_start - 2) as u16;
    d[item_start..item_start + 2].copy_from_slice(&item_len.to_be_bytes());

    let chapters_offset = d.len() as u32;
    d[12..16].copy_from_slice(&chapters_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes()); // length (skipped)
    d.extend_from_slice(&1u16.to_be_bytes()); // chapter count
    let mut chapter = vec![0u8; 14];
    chapter[1] = 1; // chapter type 1
    chapter[4..8].copy_from_slice(&(45000u32 * 10).to_be_bytes()); // 10 s
    d.extend_from_slice(&chapter);

    d
  }

  fn build_mpls_default() -> Vec<u8> {
    build_mpls_custom(4_500_000, &[avc_video(0x1011)], &[ac3_audio(0x1100, b"eng")], true)
  }

  // ---- CLPI builder (mirrors clpi.rs test helpers). ---------------------

  fn clpi_stream_entry(pid: u16, coding_type: u8, attrs: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&pid.to_be_bytes());
    v.push((1 + attrs.len()) as u8);
    v.push(coding_type);
    v.extend_from_slice(attrs);
    v
  }

  fn build_clpi(file_type: &str, streams: &[Vec<u8>]) -> Vec<u8> {
    let mut clip = vec![0u8; 10];
    clip[8] = streams.len() as u8;
    for s in streams {
      clip.extend_from_slice(s);
    }
    let clip_len = clip.len() as u32;

    let mut data = Vec::new();
    data.extend_from_slice(file_type.as_bytes());
    data.extend_from_slice(&[0, 0, 0, 0]);
    let clip_index = 16u32;
    data.extend_from_slice(&clip_index.to_be_bytes());
    data.extend_from_slice(&clip_len.to_be_bytes());
    data.extend_from_slice(&clip);
    data
  }

  /// A CLPI with an AVC video (no MPLS-side language), an AC3 audio carrying
  /// a language code, and a PGS graphics entry — so `clpi_language_for` has
  /// something to fall back to.
  fn build_clpi_default() -> Vec<u8> {
    let video = clpi_stream_entry(0x1011, 0x1b, &[(6 << 4) | 1, 3 << 4]);
    let audio = clpi_stream_entry(0x1100, 0x81, &[(6 << 4) | 1, b'e', b'n', b'g']);
    let pgs = clpi_stream_entry(0x1200, 0x90, &[b'j', b'p', b'n']);
    build_clpi("HDMV0200", &[video, audio, pgs])
  }

  // ---- M2TS builder (mirrors m2ts.rs test helpers). ---------------------

  const TS_PACKET_SIZE: usize = 188;
  const SYNC_BYTE: u8 = 0x47;

  fn ts_packet(pusi: bool, pid: u16, payload: &[u8]) -> Vec<u8> {
    let mut ts = vec![0xFFu8; TS_PACKET_SIZE];
    ts[0] = SYNC_BYTE;
    ts[1] = ((pid >> 8) as u8 & 0x1F) | if pusi { 0x40 } else { 0 };
    ts[2] = (pid & 0xFF) as u8;
    ts[3] = 0x10; // payload only
    let n = payload.len().min(TS_PACKET_SIZE - 4);
    ts[4..4 + n].copy_from_slice(&payload[..n]);
    ts
  }

  fn m2ts_frame(ts: &[u8]) -> Vec<u8> {
    let mut p = vec![0u8; 4];
    p.extend_from_slice(ts);
    p
  }

  fn pat_payload(program: u16, pmt_pid: u16) -> Vec<u8> {
    vec![
      0x00,
      0x00,
      0xB0,
      0x0D,
      0x00,
      0x01,
      0x01,
      0x00,
      0x00,
      (program >> 8) as u8,
      (program & 0xFF) as u8,
      0xE0 | (pmt_pid >> 8) as u8,
      (pmt_pid & 0xFF) as u8,
      0x00,
      0x00,
      0x00,
      0x00,
    ]
  }

  /// Build a PMT payload listing arbitrary (stream_type, pid) elementary
  /// streams. The section_length is computed so the scanner accepts it.
  fn pmt_payload_multi(streams: &[(u8, u16)]) -> Vec<u8> {
    // Body after section_length field: program_number(2) version/section/last(3)
    // PCR(2) program_info_length(2) = 9 bytes, plus 5 per ES, plus 4 CRC.
    let es_bytes = streams.len() * 5;
    let section_length = 9 + es_bytes + 4;
    let mut v = vec![0x00u8, 0x02]; // pointer, table_id (PMT)
    v.push(0xB0 | ((section_length >> 8) as u8 & 0x0F));
    v.push((section_length & 0xFF) as u8);
    v.extend_from_slice(&[0x00, 0x01]); // program_number
    v.extend_from_slice(&[0x01, 0x00, 0x00]); // version / section / last
    v.extend_from_slice(&[0xE0, 0x00]); // PCR PID
    v.extend_from_slice(&[0xF0, 0x00]); // program_info_length = 0
    for (st, pid) in streams {
      v.push(*st);
      v.push(0xE0 | ((pid >> 8) as u8 & 0x1F));
      v.push((pid & 0xFF) as u8);
      v.push(0xF0);
      v.push(0x00);
    }
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // CRC (not validated)
    v
  }

  fn pes_payload(stream_id: u8, es: &[u8]) -> Vec<u8> {
    let mut v = vec![0x00, 0x00, 0x01, stream_id, 0x00, 0x00, 0x80, 0x00, 0x00];
    v.extend_from_slice(es);
    v
  }

  /// Build an M2TS image: PAT, a multi-stream PMT (AVC/AC3/MVC/PGS), plus a
  /// few PES for each so codec_init dispatches them. `extra_pids` are PIDs in
  /// the PMT that the MPLS doesn't declare (hidden tracks / SSIF MVC).
  fn build_m2ts() -> Vec<u8> {
    let streams: &[(u8, u16)] = &[
      (0x1b, 0x1011),       // AVC (declared)
      (0x81, 0x1100),       // AC3 (declared)
      (0x20, SSIF_MVC_PID), // MVC (hidden / SSIF)
      (0x90, 0x1200),       // PGS (hidden)
      (0x92, 0x1300),       // Subtitle (hidden)
    ];
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts_frame(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts_frame(&ts_packet(true, 0x0100, &pmt_payload_multi(streams))));
    // A handful of PES per ES PID so the codec parsers get fed and the
    // byte accounting accumulates.
    for _ in 0..4 {
      for (st, pid) in streams {
        let sid = if TSStreamType::from_u8(*st).is_video() {
          0xE0
        } else {
          0xC0
        };
        data.extend_from_slice(&m2ts_frame(&ts_packet(
          true,
          *pid,
          &pes_payload(sid, &[0x00, 0x00, 0x01, 0x09, 0x10, 0xAA, 0xBB, 0xCC]),
        )));
      }
    }
    data
  }

  // ---- Disc-tree assembly. ----------------------------------------------

  struct DiscOpts {
    uhd: bool,
    with_ssif: bool,
    with_bdjo: bool,
    with_snp: bool,
    with_filmindex: bool,
    with_bdsvm: bool,
    with_meta: bool,
    meta_title: &'static str,
  }

  impl Default for DiscOpts {
    fn default() -> Self {
      DiscOpts {
        uhd: false,
        with_ssif: true,
        with_bdjo: true,
        with_snp: true,
        with_filmindex: true,
        with_bdsvm: true,
        with_meta: true,
        meta_title: "My Movie Title",
      }
    }
  }

  /// Build a complete native disc tree under a fresh temp dir.
  fn make_disc(opts: &DiscOpts) -> TempDir {
    let dir = TempDir::new("disc");

    dir.write("BDMV/PLAYLIST/00800.mpls", &build_mpls_default());
    dir.write("BDMV/CLIPINF/00001.clpi", &build_clpi_default());
    dir.write("BDMV/STREAM/00001.m2ts", &build_m2ts());

    let index_header: &[u8] = if opts.uhd { b"INDX0300extra" } else { b"INDX0200extra" };
    dir.write("BDMV/index.bdmv", index_header);

    if opts.with_meta {
      let xml = format!(
        "<?xml version=\"1.0\"?><disclib><di:title><di:name>{}</di:name></di:title></disclib>",
        opts.meta_title
      );
      dir.write("BDMV/META/DL/bdmt_eng.xml", xml.as_bytes());
    }
    if opts.with_bdjo {
      dir.write("BDMV/BDJO/00000.bdjo", b"BDJO");
    } else {
      dir.mkdir("BDMV/BDJO");
    }
    if opts.with_ssif {
      dir.write("BDMV/STREAM/SSIF/00001.ssif", &build_m2ts());
    }
    if opts.with_snp {
      dir.write("SNP/clip.mnv", b"mnv");
    } else {
      dir.mkdir("SNP");
    }
    if opts.with_filmindex {
      dir.write("FilmIndex.xml", b"<FilmIndex/>");
    }
    if opts.with_bdsvm {
      dir.mkdir("BDSVM");
    }

    dir
  }

  fn find_pl<'a>(disc: &'a DiscInfo, name: &str) -> &'a PlaylistInfo {
    disc
      .playlists
      .iter()
      .find(|p| p.name == name)
      .expect("playlist present")
  }

  // ====================================================================
  // 1. Full native-disc integration test (SSIF on, via default config).
  // ====================================================================

  #[test]
  fn scan_full_native_disc_ssif_on() {
    // Default config has enable_ssif_support = true.
    assert!(crate::config::get_config().scan.enable_ssif_support);

    let dir = make_disc(&DiscOpts {
      uhd: true,
      ..Default::default()
    });
    let root = dir.path().to_string_lossy().to_string();

    let disc = scan(&root).expect("scan succeeds");

    // Disc-level flags from the tree.
    assert!(disc.is_uhd, "INDX0300 header => UHD");
    assert!(disc.is_4k, "UHD implies 4k");
    assert!(disc.has_uhd_disc_marker);
    assert!(disc.is_bd_java, "BDJO with a file => BD-Java");
    assert!(disc.is_psp, "SNP/*.mnv => PSP");
    assert!(disc.is_dbox, "FilmIndex.xml => D-BOX");
    assert!(disc.is_bd_plus, "BDSVM => BD+");
    assert!(disc.is_3d, "SSIF dir with file => 3D");
    assert_eq!(disc.disc_title, "My Movie Title");
    assert_eq!(disc.meta_title.as_deref(), Some("My Movie Title"));
    assert!(!disc.volume_label.is_empty());
    assert_eq!(disc.volume_label, disc.disc_name);
    assert!(disc.size > 0, "directory_size summed something");

    // One playlist, group index assigned.
    assert_eq!(disc.playlists.len(), 1);
    let pl = &disc.playlists[0];
    assert_eq!(pl.name, "00800.MPLS");
    assert_eq!(pl.group_index, 1);
    assert_eq!(pl.total_angles, 0);
    assert_eq!(pl.total_length, 4_500_000);
    assert_eq!(pl.stream_clips.len(), 1);
    assert_eq!(pl.stream_clips[0].name, "00001.M2TS");
    // SSIF on => the clip display name uses the SSIF extension.
    assert_eq!(pl.stream_clips[0].display_name, "00001.SSIF");
    assert!(pl.stream_clips[0].interleaved_file_size > 0);

    // Declared streams from MPLS.
    assert_eq!(pl.video_streams.iter().filter(|s| !s.is_hidden).count() >= 1, true);
    let avc = pl.video_streams.iter().find(|s| s.pid == 0x1011).expect("AVC present");
    assert_eq!(avc.stream_type, TSStreamType::AVCVideo as u8);
    let ac3 = pl.audio_streams.iter().find(|s| s.pid == 0x1100).expect("AC3 present");
    assert_eq!(ac3.stream_type, TSStreamType::AC3Audio as u8);
    assert_eq!(ac3.language_code, "eng");

    // Hidden streams synthesized from PMT (PGS + Subtitle were not in MPLS).
    let pgs = pl.graphics_streams.iter().find(|s| s.pid == 0x1200);
    assert!(pgs.is_some(), "PGS hidden track added");
    assert!(pgs.unwrap().is_hidden);
    assert_eq!(pgs.unwrap().language_code, "jpn");
    let sub = pl.text_streams.iter().find(|s| s.pid == 0x1300);
    assert!(sub.is_some(), "subtitle hidden track added");
    assert!(pl.has_hidden_tracks);

    // MVC (PID 0x1012) under SSIF mode is promoted to a non-hidden video
    // stream, not a hidden track.
    let mvc = pl.video_streams.iter().find(|s| s.pid == SSIF_MVC_PID);
    assert!(mvc.is_some(), "MVC stream present under SSIF");
    assert!(!mvc.unwrap().is_hidden, "MVC promoted, not hidden");
    assert_eq!(mvc.unwrap().stream_type, TSStreamType::MVCVideo as u8);
    assert!(disc.has_mvc_extension, "MVC present => mvc extension flag");

    // Stream files / clip files in the DiscInfo.
    assert_eq!(disc.stream_files.len(), 1);
    assert_eq!(disc.stream_files[0].name, "00001.M2TS");
    assert!(disc.stream_files[0].interleaved);
    assert!(disc.stream_files[0].interleaved_file_size > 0);
    assert_eq!(disc.stream_files[0].display_name, "00001.SSIF");
    assert_eq!(disc.stream_clip_files.len(), 1);
    assert_eq!(disc.stream_clip_files[0].name, "00001.CLPI");

    // estimated_size cached for streams with a bit rate.
    let any_estimate = pl
      .video_streams
      .iter()
      .chain(pl.audio_streams.iter())
      .any(|s| s.estimated_size > 0);
    assert!(any_estimate, "at least one stream got an estimated size");
  }

  // ====================================================================
  // Non-UHD variant: INDX0200, no SSIF/BDJO/SNP/FilmIndex/BDSVM/META.
  // ====================================================================

  #[test]
  fn scan_minimal_non_uhd_disc() {
    let dir = make_disc(&DiscOpts {
      uhd: false,
      with_ssif: false,
      with_bdjo: false,
      with_snp: false,
      with_filmindex: false,
      with_bdsvm: false,
      with_meta: false,
      meta_title: "",
    });
    let root = dir.path().to_string_lossy().to_string();
    let disc = scan(&root).expect("scan succeeds");

    assert!(!disc.is_uhd);
    assert!(!disc.is_3d);
    assert!(!disc.is_bd_java, "empty BDJO dir => not BD-Java");
    assert!(!disc.is_psp, "empty SNP dir => not PSP");
    assert!(!disc.is_dbox);
    assert!(!disc.is_bd_plus);
    assert!(disc.disc_title.is_empty());
    assert!(disc.meta_title.is_none());

    // SSIF off => no interleaved counterpart, display name == clip name.
    let pl = &disc.playlists[0];
    assert_eq!(pl.stream_clips[0].display_name, "00001.M2TS");
    assert!(!disc.stream_files[0].interleaved);
    assert_eq!(disc.stream_files[0].interleaved_file_size, 0);

    // The CLPI language fallback fills the AC3 language (MPLS also had it,
    // but assert it survives).
    let ac3 = pl.audio_streams.iter().find(|s| s.pid == 0x1100).unwrap();
    assert_eq!(ac3.language_code, "eng");
  }

  // ====================================================================
  // 2a. Dragged-as-file: pass STREAM/00001.m2ts; open_bdrom walks up.
  // ====================================================================

  #[test]
  fn scan_file_inside_disc_walks_up() {
    let dir = make_disc(&DiscOpts::default());
    let m2ts_path = dir.path().join("BDMV/STREAM/00001.m2ts");
    let disc = scan(&m2ts_path.to_string_lossy()).expect("scan from inner file");
    assert_eq!(disc.playlists.len(), 1);
    assert_eq!(disc.playlists[0].name, "00800.MPLS");
  }

  // ====================================================================
  // 2b. Non-existent path => Err.
  // ====================================================================

  #[test]
  fn scan_nonexistent_path_errors() {
    let missing = std::env::temp_dir().join("bdmaster_modtest_does_not_exist_xyz");
    let _ = std::fs::remove_dir_all(&missing);
    assert!(scan(&missing.to_string_lossy()).is_err());
    assert!(open_bdrom(&missing, false).is_err());
  }

  // ====================================================================
  // 2c. open_bdrom on a file inside a disc, with SSIF off, directly.
  //     Exercises open_bdrom_native with use_ssif=false branch in
  //     effective_stream_source / build_playlist_info / stream_display_name.
  // ====================================================================

  #[test]
  fn open_bdrom_native_ssif_off_branches() {
    let dir = make_disc(&DiscOpts::default());
    let bd = open_bdrom(dir.path(), false).expect("open native");
    assert!(!bd.use_ssif);
    assert!(bd.is_3d, "SSIF files still present => is_3d true");
    assert!(!bd.interleaved_files.is_empty(), "interleaved map populated");

    // effective_stream_source with SSIF off returns the M2TS, not the SSIF.
    let src = effective_stream_source(&bd, "00001.M2TS").expect("source");
    match &src.0 {
      StreamSource::Native(p) => {
        assert!(p.to_string_lossy().to_uppercase().ends_with("00001.M2TS"))
      }
      _ => panic!("expected native source"),
    }

    // stream_display_name keeps the M2TS name when SSIF is off.
    assert_eq!(stream_display_name(&bd, "00001.M2TS"), "00001.M2TS");

    // to_disc_info on a SSIF-off BDRom: clips report the M2TS size, not SSIF.
    let disc = to_disc_info(&bd);
    let pl = &disc.playlists[0];
    assert_eq!(pl.stream_clips[0].display_name, "00001.M2TS");
    // file_size is the m2ts size.
    let m2ts_size = bd.stream_files.get("00001.M2TS").unwrap().1;
    assert_eq!(pl.stream_clips[0].file_size, m2ts_size);

    // is_ssif_mvc_stream is false when use_ssif is off.
    let mvc = TSStreamInfo::new(SSIF_MVC_PID, TSStreamType::MVCVideo as u8);
    assert!(!is_ssif_mvc_stream(&bd, "00001.M2TS", SSIF_MVC_PID, &mvc));
  }

  #[test]
  fn effective_stream_source_ssif_on_prefers_ssif() {
    let dir = make_disc(&DiscOpts::default());
    let bd = open_bdrom(dir.path(), true).expect("open native");
    assert!(bd.use_ssif);
    let src = effective_stream_source(&bd, "00001.M2TS").expect("source");
    match &src.0 {
      StreamSource::Native(p) => {
        assert!(p.to_string_lossy().to_uppercase().ends_with("00001.SSIF"))
      }
      _ => panic!("expected SSIF source"),
    }
    // display name swaps to SSIF.
    assert_eq!(stream_display_name(&bd, "00001.M2TS"), "00001.SSIF");

    // is_ssif_mvc_stream true path.
    let mvc = TSStreamInfo::new(SSIF_MVC_PID, TSStreamType::MVCVideo as u8);
    assert!(is_ssif_mvc_stream(&bd, "00001.M2TS", SSIF_MVC_PID, &mvc));
    // Wrong PID / wrong type / unknown clip are false.
    assert!(!is_ssif_mvc_stream(&bd, "00001.M2TS", 0x1011, &mvc));
    let avc = TSStreamInfo::new(SSIF_MVC_PID, TSStreamType::AVCVideo as u8);
    assert!(!is_ssif_mvc_stream(&bd, "00001.M2TS", SSIF_MVC_PID, &avc));
    assert!(!is_ssif_mvc_stream(&bd, "NOPE.M2TS", SSIF_MVC_PID, &mvc));
  }

  // ====================================================================
  // 4. Error/edge branches: missing PLAYLIST / CLIPINF, empty disc.
  // ====================================================================

  #[test]
  fn open_bdrom_missing_playlist_dir_errors() {
    let dir = TempDir::new("noplaylist");
    dir.write("BDMV/index.bdmv", b"INDX0200");
    dir.write("BDMV/CLIPINF/00001.clpi", &build_clpi_default());
    let err = open_bdrom(dir.path(), false).err().expect("expected error");
    assert!(err.to_string().contains("PLAYLIST or CLIPINF"));
  }

  #[test]
  fn open_bdrom_missing_clipinf_dir_errors() {
    let dir = TempDir::new("noclipinf");
    dir.write("BDMV/index.bdmv", b"INDX0200");
    dir.write("BDMV/PLAYLIST/00800.mpls", &build_mpls_default());
    assert!(open_bdrom(dir.path(), false).is_err());
  }

  #[test]
  fn locate_bdmv_via_index_at_root() {
    // A folder that IS the BDMV (index.bdmv at root, no BDMV ancestor).
    let dir = TempDir::new("rootbdmv");
    dir.write("index.bdmv", b"INDX0200");
    dir.write("PLAYLIST/00800.mpls", &build_mpls_default());
    dir.write("CLIPINF/00001.clpi", &build_clpi_default());
    dir.write("STREAM/00001.m2ts", &build_m2ts());
    let bd = open_bdrom(dir.path(), false).expect("open via index.bdmv root");
    assert_eq!(bd.playlists.len(), 1);
  }

  #[test]
  fn locate_bdmv_fails_when_absent() {
    let dir = TempDir::new("nobdmv");
    dir.write("random.txt", b"hi");
    assert!(open_bdrom(dir.path(), false).is_err());
  }

  // ====================================================================
  // 3. Pure-helper unit tests.
  // ====================================================================

  #[test]
  fn extract_title_from_xml_variants() {
    assert_eq!(
      extract_title_from_xml("<di:title><di:name>Hello World</di:name></di:title>").as_deref(),
      Some("Hello World")
    );
    assert_eq!(
      extract_title_from_xml("<x:title><y:name>Rock &amp; Roll</y:name></x:title>").as_deref(),
      Some("Rock & Roll")
    );
    assert_eq!(
      extract_title_from_xml("<di:title><di:name>blu-ray</di:name></di:title>"),
      None
    );
    assert_eq!(extract_title_from_xml("<di:title><di:name></di:name></di:title>"), None);
    // A name tag outside the title element is ignored, which avoids
    // unrelated metadata fields being treated as the disc title.
    assert_eq!(
      extract_title_from_xml("<di:other><di:name>Wrong</di:name></di:other>"),
      None
    );
    // No name tag => None.
    assert_eq!(extract_title_from_xml("<other>x</other>"), None);
    // Unterminated tag => None (no closing </).
    assert_eq!(extract_title_from_xml("<di:title><di:name>oops</di:title>"), None);
  }

  #[test]
  fn estimate_stream_size_paths() {
    let mut s = TSStreamInfo::new(0x1011, 0x1b);
    // No bit rate => 0.
    assert_eq!(estimate_stream_size(&s, 100.0), 0);
    // bit_rate used when > 0.
    s.bit_rate = 8_000_000;
    assert_eq!(estimate_stream_size(&s, 10.0), 10_000_000);
    // total_seconds 0 => 0.
    assert_eq!(estimate_stream_size(&s, 0.0), 0);
    // falls back to active_bit_rate when bit_rate == 0.
    s.bit_rate = 0;
    s.active_bit_rate = 8_000;
    assert_eq!(estimate_stream_size(&s, 1.0), 1_000);
  }

  fn mk_clip(name: &str, time_in: i64, time_out: i64, angle: u32) -> mpls::PlaylistStreamClip {
    mpls::PlaylistStreamClip {
      name: name.to_string(),
      time_in,
      time_out,
      angle_index: angle,
    }
  }

  fn mk_playlist(name: &str, clips: Vec<mpls::PlaylistStreamClip>) -> PlaylistFile {
    PlaylistFile {
      name: name.to_string(),
      file_type: "MPLS0200".to_string(),
      mvc_base_view_r: false,
      stream_clips: clips,
      chapters: Vec::new(),
      angle_count: 0,
      playlist_streams: Vec::new(),
    }
  }

  #[test]
  fn playlist_length_and_loops_and_validity() {
    // Two angle-0 clips of 45000 (1s) each = 90000.
    let pl = mk_playlist(
      "P.MPLS",
      vec![
        mk_clip("00001.M2TS", 0, 45000, 0),
        mk_clip("00002.M2TS", 0, 45000, 0),
        // angle 1 clip is ignored by length / loop computation.
        mk_clip("00003.M2TS", 0, 90000, 1),
      ],
    );
    assert_eq!(playlist_total_length_45k(&pl), 90000);
    assert!(!playlist_has_loops(&pl));

    // Looping playlist: same (name, time_in) appears twice on angle 0.
    let looped = mk_playlist(
      "L.MPLS",
      vec![mk_clip("00001.M2TS", 0, 45000, 0), mk_clip("00001.M2TS", 0, 45000, 0)],
    );
    assert!(playlist_has_loops(&looped));

    // Validity: short playlist filtered out.
    // total seconds = 90000/45000 = 2.0; threshold 20 => invalid.
    assert!(!playlist_is_valid_for_scan(&pl, true, true, 20));
    // threshold 1 => valid (2.0 >= 1).
    assert!(playlist_is_valid_for_scan(&pl, true, true, 1));
    // looping filter on => looped playlist invalid.
    assert!(!playlist_is_valid_for_scan(&looped, true, false, 0));
    // looping filter off => looped playlist valid.
    assert!(playlist_is_valid_for_scan(&looped, false, false, 0));
    // both filters off => always valid.
    assert!(playlist_is_valid_for_scan(&pl, false, false, 0));
  }

  #[test]
  fn alternate_angles_inherit_their_main_play_item_offset() {
    let mut pl = mk_playlist(
      "A.MPLS",
      vec![
        mk_clip("00001.M2TS", 0, 45_000, 0),
        mk_clip("00101.M2TS", 0, 45_000, 1),
        mk_clip("00002.M2TS", 90_000, 180_000, 0),
        mk_clip("00102.M2TS", 90_000, 180_000, 1),
      ],
    );
    pl.angle_count = 1;
    let bd = BDRom {
      path: PathBuf::new(),
      source: DiscSource::Native,
      volume_label: String::new(),
      disc_title: None,
      size: 0,
      is_uhd: false,
      is_bd_plus: false,
      is_bd_java: false,
      is_dbox: false,
      is_psp: false,
      is_3d: false,
      is_50_hz: false,
      playlists: std::collections::HashMap::new(),
      stream_files: std::collections::HashMap::new(),
      stream_clip_files: std::collections::HashMap::new(),
      interleaved_files: std::collections::HashMap::new(),
      use_ssif: false,
    };

    let info = build_playlist_info(&pl, &bd, 0);
    assert_eq!(info.stream_clips[0].relative_time_in, 0);
    assert_eq!(info.stream_clips[1].relative_time_in, 0);
    assert_eq!(info.stream_clips[2].relative_time_in, 45_000);
    assert_eq!(info.stream_clips[3].relative_time_in, 45_000);
  }

  #[test]
  fn reference_clip_selection_matches_bdinfo_priority_order() {
    let pl = mk_playlist(
      "R.MPLS",
      vec![
        mk_clip("00001.M2TS", 0, 4_500_000, 0),
        mk_clip("00002.M2TS", 0, 9_000_000, 0),
      ],
    );
    let mut bd = BDRom {
      path: PathBuf::new(),
      source: DiscSource::Native,
      volume_label: String::new(),
      disc_title: None,
      size: 0,
      is_uhd: false,
      is_bd_plus: false,
      is_bd_java: false,
      is_dbox: false,
      is_psp: false,
      is_3d: false,
      is_50_hz: false,
      playlists: std::collections::HashMap::new(),
      stream_files: std::collections::HashMap::new(),
      stream_clip_files: std::collections::HashMap::new(),
      interleaved_files: std::collections::HashMap::new(),
      use_ssif: false,
    };
    for name in ["00001.M2TS", "00002.M2TS"] {
      bd.stream_files
        .insert(name.to_string(), (StreamSource::Native(PathBuf::new()), 0));
    }
    bd.stream_clip_files.insert(
      "00001.CLPI".into(),
      clpi::StreamClipFile {
        name: "00001.CLPI".into(),
        is_valid: true,
        streams: vec![clpi::ClpiStream::default(), clpi::ClpiStream::default()],
        ..Default::default()
      },
    );
    bd.stream_clip_files.insert(
      "00002.CLPI".into(),
      clpi::StreamClipFile {
        name: "00002.CLPI".into(),
        is_valid: true,
        streams: vec![clpi::ClpiStream::default()],
        ..Default::default()
      },
    );

    assert_eq!(
      reference_clip_name_for_playlist(&pl, &bd).as_deref(),
      Some("00002.M2TS"),
      "BDInfo lets a longer present clip replace a richer reference"
    );
  }

  #[test]
  fn playlist_stream_to_info_video_and_audio() {
    // Video stream.
    let vs = mpls::PlaylistStream {
      pid: 0x1011,
      stream_type: TSStreamType::AVCVideo,
      video_format: TSVideoFormat::Video1080p,
      frame_rate: TSFrameRate::F23_976,
      aspect_ratio: TSAspectRatio::Aspect16_9,
      channel_layout: TSChannelLayout::Unknown,
      sample_rate_hz: 0,
      language_code: String::new(),
    };
    let vi = playlist_stream_to_info(&vs);
    assert!(vi.is_video_stream);
    assert_eq!(vi.height, 1080);
    assert_eq!(vi.width, 1920);
    assert!(!vi.is_interlaced);
    assert_eq!(vi.framerate, "23.976");
    assert_eq!(vi.aspect_ratio, "16:9");
    assert_eq!(vi.video_format, "1080p");
    assert!(vi.description.contains("1080p"));

    // Interlaced video to hit the "i" branch and a different width.
    let vs2 = mpls::PlaylistStream {
      pid: 0x1011,
      stream_type: TSStreamType::MPEG2Video,
      video_format: TSVideoFormat::Video480i,
      frame_rate: TSFrameRate::Unknown,
      aspect_ratio: TSAspectRatio::Unknown,
      channel_layout: TSChannelLayout::Unknown,
      sample_rate_hz: 0,
      language_code: "fra\0".to_string(),
    };
    let vi2 = playlist_stream_to_info(&vs2);
    assert!(vi2.is_interlaced);
    assert_eq!(vi2.width, 720);
    assert_eq!(vi2.video_format, "480i");
    // language code trailing NUL trimmed.
    assert_eq!(vi2.language_code, "fra");

    // Audio stream.
    let as_ = mpls::PlaylistStream {
      pid: 0x1100,
      stream_type: TSStreamType::AC3Audio,
      video_format: TSVideoFormat::Unknown,
      frame_rate: TSFrameRate::Unknown,
      aspect_ratio: TSAspectRatio::Unknown,
      channel_layout: TSChannelLayout::Multi,
      sample_rate_hz: 48000,
      language_code: "eng".to_string(),
    };
    let ai = playlist_stream_to_info(&as_);
    assert!(ai.is_audio_stream);
    assert_eq!(ai.channel_layout, "5.1");
    assert_eq!(ai.sample_rate, 48000);
    assert!(ai.description.contains("48 kHz"));
  }

  #[test]
  fn recompute_mvc_extension_toggles() {
    let mut disc = DiscInfo {
      path: String::new(),
      disc_name: String::new(),
      disc_title: String::new(),
      volume_label: String::new(),
      size: 0,
      is_bd_plus: false,
      is_bd_java: false,
      is_3d: false,
      is_4k: false,
      is_50hz: false,
      is_dbox: false,
      is_psp: false,
      is_uhd: false,
      has_mvc_extension: false,
      has_hevc_streams: false,
      has_uhd_disc_marker: false,
      meta_title: None,
      meta_disc_number: None,
      file_set_identifier: None,
      playlists: Vec::new(),
      stream_files: Vec::new(),
      stream_clip_files: Vec::new(),
    };
    recompute_mvc_extension(&mut disc);
    assert!(!disc.has_mvc_extension);

    let mut pl = PlaylistInfo {
      name: "P".into(),
      group_index: 1,
      file_size: 0,
      measured_size: 0,
      total_length: 0,
      has_hidden_tracks: false,
      has_loops: false,
      is_custom: false,
      chapters: Vec::new(),
      chapter_metrics: Vec::new(),
      bitrate_samples: Vec::new(),
      stream_clips: Vec::new(),
      video_streams: Vec::new(),
      audio_streams: Vec::new(),
      graphics_streams: Vec::new(),
      text_streams: Vec::new(),
      angle_streams: Vec::new(),
      total_angles: 0,
    };
    pl.video_streams
      .push(TSStreamInfo::new(SSIF_MVC_PID, TSStreamType::MVCVideo as u8));
    disc.playlists.push(pl);
    recompute_mvc_extension(&mut disc);
    assert!(disc.has_mvc_extension);
  }

  #[test]
  fn refresh_ssif_derived_metadata_sets_base_view() {
    // Build a 3D BDRom and a DiscInfo with two video streams (AVC + MVC)
    // so refresh_ssif_derived_metadata assigns base_view per the source's
    // mvc_base_view_r flag.
    let dir = make_disc(&DiscOpts::default());
    let bd = open_bdrom(dir.path(), true).expect("open");
    assert!(bd.is_3d);

    let mut disc = to_disc_info(&bd);
    // to_disc_info yields only the MPLS-declared AVC video stream; the SSIF
    // MVC counterpart is normally appended by codec_init. Inject it here so
    // refresh_ssif_derived_metadata has both AVC + MVC to assign base_view.
    {
      let pl = disc
        .playlists
        .iter_mut()
        .find(|p| p.name == "00800.MPLS")
        .expect("playlist present");
      pl.video_streams
        .push(TSStreamInfo::new(SSIF_MVC_PID, TSStreamType::MVCVideo as u8));
    }

    refresh_ssif_derived_metadata(&mut disc, &bd);

    let pl = find_pl(&disc, "00800.MPLS");
    let avc = pl.video_streams.iter().find(|s| s.pid == 0x1011).unwrap();
    let mvc = pl.video_streams.iter().find(|s| s.pid == SSIF_MVC_PID).unwrap();
    // mvc_base_view_r is true in our MPLS => AVC base_view = true, MVC = false.
    let src = bd.playlists.get("00800.MPLS").unwrap();
    assert_eq!(avc.base_view, Some(src.mvc_base_view_r));
    assert_eq!(mvc.base_view, Some(!src.mvc_base_view_r));
    assert!(disc.has_mvc_extension);
  }

  #[test]
  fn refresh_ssif_derived_metadata_no_op_when_not_3d() {
    // A disc without SSIF: is_3d false => the base_view loop is skipped,
    // recompute_mvc_extension still runs.
    let dir = make_disc(&DiscOpts {
      with_ssif: false,
      ..Default::default()
    });
    let bd = open_bdrom(dir.path(), true).expect("open");
    assert!(!bd.is_3d);
    let mut disc = to_disc_info(&bd);
    refresh_ssif_derived_metadata(&mut disc, &bd);
    // No MVC promotion happened (no SSIF), so no mvc extension.
    // (Codec init wasn't run here, so video stream list is just the AVC.)
    assert!(!disc.has_mvc_extension);
  }

  #[test]
  fn copy_codec_metadata_copies_fields() {
    let mut src = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
    src.is_initialized = true;
    src.codec_name = "AVC".into();
    src.width = 1920;
    src.height = 1080;
    src.bit_rate = 20_000_000;
    src.active_bit_rate = 21_000_000;
    src.is_vbr = true;

    let mut dst = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
    super::codec_init::copy_codec_metadata(&mut dst, &src);
    assert!(dst.is_initialized);
    assert_eq!(dst.width, 1920);
    assert_eq!(dst.height, 1080);
    assert_eq!(dst.bit_rate, 20_000_000);
    assert_eq!(dst.active_bit_rate, 21_000_000);
    assert!(dst.is_vbr);

    // Not-initialized source is a no-op.
    let uninit = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
    let mut dst2 = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
    super::codec_init::copy_codec_metadata(&mut dst2, &uninit);
    assert!(!dst2.is_initialized);
  }

  #[test]
  fn clpi_language_for_lookup() {
    let dir = make_disc(&DiscOpts::default());
    let bd = open_bdrom(dir.path(), false).expect("open");
    // AC3 at PID 0x1100 has language "eng" in CLPI.
    assert_eq!(clpi_language_for(&bd, "00001.M2TS", 0x1100).as_deref(), Some("eng"));
    // PGS at 0x1200 has "jpn".
    assert_eq!(clpi_language_for(&bd, "00001.M2TS", 0x1200).as_deref(), Some("jpn"));
    // Unknown PID => None.
    assert!(clpi_language_for(&bd, "00001.M2TS", 0x9999).is_none());
    // Unknown clip => None.
    assert!(clpi_language_for(&bd, "99999.M2TS", 0x1100).is_none());
  }

  #[test]
  fn cache_estimated_stream_sizes_fills_estimates() {
    let mut disc = DiscInfo {
      path: String::new(),
      disc_name: String::new(),
      disc_title: String::new(),
      volume_label: String::new(),
      size: 0,
      is_bd_plus: false,
      is_bd_java: false,
      is_3d: false,
      is_4k: false,
      is_50hz: false,
      is_dbox: false,
      is_psp: false,
      is_uhd: false,
      has_mvc_extension: false,
      has_hevc_streams: false,
      has_uhd_disc_marker: false,
      meta_title: None,
      meta_disc_number: None,
      file_set_identifier: None,
      playlists: Vec::new(),
      stream_files: Vec::new(),
      stream_clip_files: Vec::new(),
    };
    let mut pl = PlaylistInfo {
      name: "P".into(),
      group_index: 1,
      file_size: 0,
      measured_size: 0,
      total_length: 45000 * 100, // 100 s
      has_hidden_tracks: false,
      has_loops: false,
      is_custom: false,
      chapters: Vec::new(),
      chapter_metrics: Vec::new(),
      bitrate_samples: Vec::new(),
      stream_clips: Vec::new(),
      video_streams: Vec::new(),
      audio_streams: Vec::new(),
      graphics_streams: Vec::new(),
      text_streams: Vec::new(),
      angle_streams: Vec::new(),
      total_angles: 0,
    };
    let mut v = TSStreamInfo::new(0x1011, TSStreamType::AVCVideo as u8);
    v.bit_rate = 8_000_000; // 8 Mbps => 100 MB over 100 s
    pl.video_streams.push(v);
    disc.playlists.push(pl);

    cache_estimated_stream_sizes(&mut disc);
    assert_eq!(disc.playlists[0].video_streams[0].estimated_size, 100_000_000);
  }

  // ====================================================================
  // resolve_playlist_path / resolve_stream_file_path.
  // ====================================================================

  #[test]
  fn resolve_playlist_path_success_and_errors() {
    let dir = make_disc(&DiscOpts::default());
    let root = dir.path().to_string_lossy().to_string();

    // Success (case-insensitive name match: stored uppercase, on-disk lowercase).
    let p = resolve_playlist_path(&root, "00800.MPLS").expect("resolve");
    assert!(p.to_string_lossy().to_lowercase().ends_with("00800.mpls"));

    // Not found.
    assert!(resolve_playlist_path(&root, "99999.MPLS").is_err());

    // Non-existent disc path.
    let missing = dir.path().join("nope");
    assert!(resolve_playlist_path(&missing.to_string_lossy(), "00800.MPLS").is_err());

    // is_file path (point at a real file) => Err (ISO-style rejection).
    let m2ts = dir.path().join("BDMV/STREAM/00001.m2ts");
    let err = resolve_playlist_path(&m2ts.to_string_lossy(), "00800.MPLS").unwrap_err();
    assert!(err.to_string().contains(".iso"));
  }

  #[test]
  fn resolve_playlist_path_no_playlist_dir_errors() {
    // BDMV present (index.bdmv) but no PLAYLIST subdir.
    let dir = TempDir::new("noplaylistdir");
    dir.write("index.bdmv", b"INDX0200");
    let root = dir.path().to_string_lossy().to_string();
    let err = resolve_playlist_path(&root, "00800.MPLS").unwrap_err();
    assert!(err.to_string().contains("PLAYLIST"));
  }

  #[test]
  fn resolve_stream_file_path_success_and_errors() {
    let dir = make_disc(&DiscOpts::default());
    let root = dir.path().to_string_lossy().to_string();

    // Default config has SSIF on, so the SSIF file is returned for the clip.
    let p = resolve_stream_file_path(&root, "00001.M2TS").expect("resolve stream");
    let upper = p.to_string_lossy().to_uppercase();
    assert!(upper.ends_with("00001.SSIF") || upper.ends_with("00001.M2TS"));

    // Unknown stream => Err.
    assert!(resolve_stream_file_path(&root, "99999.M2TS").is_err());

    // Non-existent disc path => Err.
    let missing = dir.path().join("nope");
    assert!(resolve_stream_file_path(&missing.to_string_lossy(), "00001.M2TS").is_err());

    // is_file path => Err (ISO-style rejection).
    let m2ts = dir.path().join("BDMV/STREAM/00001.m2ts");
    let err = resolve_stream_file_path(&m2ts.to_string_lossy(), "00001.M2TS").unwrap_err();
    assert!(err.to_string().contains(".iso"));
  }

  // ====================================================================
  // directory_size / dir_has_files / dir_has_extension / find_subdir.
  // ====================================================================

  #[test]
  fn directory_helpers() {
    let dir = TempDir::new("helpers");
    dir.write("a/file1.bin", &[0u8; 100]);
    dir.write("a/sub/file2.bin", &[0u8; 200]);
    dir.write("a/skip.ssif", &[0u8; 9999]); // excluded from size
    dir.mkdir("a/EmptyDir");

    let a = dir.path().join("a");
    // directory_size excludes .ssif files.
    assert_eq!(directory_size(&a), 300);
    #[cfg(unix)]
    {
      std::os::unix::fs::symlink(&a, a.join("sub/loop")).expect("create symlink loop");
      assert_eq!(directory_size(&a), 300);
    }
    // dir_has_files: 'a' has files; EmptyDir does not.
    assert!(dir_has_files(&a));
    assert!(!dir_has_files(&a.join("EmptyDir")));
    // dir_has_extension (case-insensitive).
    assert!(dir_has_extension(&a, "SSIF"));
    assert!(dir_has_extension(&a, "bin"));
    assert!(!dir_has_extension(&a, "mnv"));
    // find_subdir case-insensitive, returns None for missing.
    assert!(find_subdir(&a, "sub").is_some());
    assert!(find_subdir(&a, "SUB").is_some());
    assert!(find_subdir(&a, "missing").is_none());
    // directory_size on a non-existent dir is 0.
    assert_eq!(directory_size(&dir.path().join("nope")), 0);
    assert!(!dir_has_files(&dir.path().join("nope")));
    assert!(!dir_has_extension(&dir.path().join("nope"), "bin"));
  }

  #[test]
  fn read_disc_title_native_walks_meta() {
    let dir = TempDir::new("meta");
    dir.write(
      "META/DL/bdmt_eng.xml",
      b"<x><di:title><di:name>Nested Title</di:name></di:title></x>",
    );
    let title = read_disc_title_native(&dir.path().join("META"));
    assert_eq!(title.as_deref(), Some("Nested Title"));

    // No bdmt_eng.xml => None.
    let empty = TempDir::new("metaempty");
    empty.mkdir("META");
    assert!(read_disc_title_native(&empty.path().join("META")).is_none());
  }

  #[test]
  fn open_stream_reader_native_round_trip() {
    let dir = make_disc(&DiscOpts::default());
    let bd = open_bdrom(dir.path(), false).expect("open");
    let entry = bd.stream_files.get("00001.M2TS").expect("m2ts entry");
    // Buffered reader.
    let mut r = open_stream_reader(&bd, &entry.0).expect("reader");
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut r, &mut buf).expect("read");
    assert!(!buf.is_empty());
    // Raw reader.
    let mut r2 = open_stream_reader_raw(&bd, &entry.0).expect("raw reader");
    let mut buf2 = Vec::new();
    std::io::Read::read_to_end(&mut r2, &mut buf2).expect("read raw");
    assert_eq!(buf.len(), buf2.len());
  }
