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
 * UDF 2.x reader sufficient to navigate Blu-ray Disc images stored as .iso
 * files. Implements:
 *   - Anchor Volume Descriptor Pointer (AVDP) at LBA 256
 *   - Volume Descriptor Sequence walk picking the latest Partition / Logical
 *     Volume Descriptors by VolumeDescriptorSequenceNumber
 *   - Type 1 (physical) and Type 2 Metadata partition maps — UHD BDs use
 *     Metadata Partitions, so the FSD lives inside a metadata file rather
 *     than directly at `partition_start + logical_block_number`
 *   - File Set Descriptor → root directory ICB
 *   - File Entry (FE) and Extended File Entry (EFE) with short/long
 *     allocation descriptors and embedded data
 *   - File Identifier Descriptor (FID) directory listings
 *   - UdfFileReader: an `impl Read` that streams bytes through allocation
 *     extents (used to feed the M2TS scanner without buffering whole files)
 */

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub const SECTOR_SIZE: usize = 2048;

const TAG_AVDP: u16 = 2;
const TAG_PD: u16 = 5;
const TAG_LVD: u16 = 6;
const TAG_TD: u16 = 8;
const TAG_FSD: u16 = 256;
const TAG_FID: u16 = 257;
const TAG_FE: u16 = 261;
const TAG_EFE: u16 = 266;

#[derive(Debug, Clone, Copy)]
pub struct LbAddr {
  pub logical_block_number: u32,
  pub partition_reference_number: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct LongAd {
  pub length_and_type: u32,
  pub location: LbAddr,
}

impl LongAd {
  pub fn length(&self) -> u32 {
    self.length_and_type & 0x3FFF_FFFF
  }
}

#[derive(Debug, Clone, Copy)]
pub struct ShortAd {
  pub length_and_type: u32,
  pub position: u32,
}

impl ShortAd {
  pub fn length(&self) -> u32 {
    self.length_and_type & 0x3FFF_FFFF
  }
}

#[derive(Debug, Clone)]
pub enum AllocDesc {
  Short(ShortAd),
  Long(LongAd),
}

#[derive(Debug, Clone)]
struct Partition {
  number: u16,
  starting_location: u32,
  vdsn: u32,
}

#[derive(Debug, Clone)]
enum PartitionMap {
  /// Type 1: direct mapping to a physical partition.
  Type1 { partition_number: u16 },
  /// Type 2 Metadata Partition. Logical addresses are resolved through a
  /// metadata file whose extents are loaded once at open time.
  Metadata {
    partition_number: u16,
    metadata_extents: Vec<(u32 /* phys LBA */, u32 /* length bytes */)>,
    metadata_size: u64,
  },
  /// Unsupported partition map (e.g. Sparable). Falls back to a direct
  /// mapping; works for simple discs and matches BDInfo's behavior on
  /// non-metadata Type 2 maps.
  Other { partition_number: u16 },
}

#[derive(Debug, Clone)]
pub struct UdfFile {
  pub size: u64,
  pub is_directory: bool,
  pub embedded_data: Option<Vec<u8>>,
  pub allocation_descriptors: Vec<AllocDesc>,
  pub partition_reference: u16,
}

#[derive(Debug, Clone)]
pub struct UdfDirEntry {
  pub name: String,
  pub icb: LongAd,
  pub is_directory: bool,
  pub is_parent: bool,
  pub is_hidden: bool,
  pub is_deleted: bool,
}

pub struct UdfImage {
  pub(crate) file: File,
  partitions: HashMap<u16, Partition>,
  partition_maps: Vec<PartitionMap>,
  pub root: UdfFile,
  /// Logical Volume Identifier from the LVD (UDF's disc volume label).
  pub volume_label: String,
}

fn read_long_ad(buf: &[u8]) -> LongAd {
  LongAd {
    length_and_type: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
    location: LbAddr {
      logical_block_number: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
      partition_reference_number: u16::from_le_bytes([buf[8], buf[9]]),
    },
  }
}

fn read_short_ad(buf: &[u8]) -> ShortAd {
  ShortAd {
    length_and_type: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
    position: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
  }
}

fn parse_d_string(buf: &[u8]) -> String {
  if buf.is_empty() {
    return String::new();
  }
  let kind = buf[0];
  let body = &buf[1..];
  match kind {
    8 => body.iter().take_while(|b| **b != 0).map(|b| *b as char).collect(),
    16 => {
      let mut s = String::new();
      let mut i = 0;
      while i + 1 < body.len() {
        let cu = u16::from_be_bytes([body[i], body[i + 1]]);
        if cu == 0 {
          break;
        }
        if let Some(ch) = char::from_u32(cu as u32) {
          s.push(ch);
        }
        i += 2;
      }
      s
    }
    _ => String::new(),
  }
}

fn read_sector(file: &mut File, lba: u64) -> Result<Vec<u8>> {
  let mut sector = vec![0u8; SECTOR_SIZE];
  file.seek(SeekFrom::Start(lba * SECTOR_SIZE as u64))?;
  file.read_exact(&mut sector)?;
  Ok(sector)
}

fn read_run(file: &mut File, lba: u64, length_bytes: usize) -> Result<Vec<u8>> {
  let mut buf = vec![0u8; length_bytes];
  file.seek(SeekFrom::Start(lba * SECTOR_SIZE as u64))?;
  file.read_exact(&mut buf)?;
  Ok(buf)
}

/// Try to read the AVDP from one of the canonical locations. UDF 2.x mandates
/// LBA 256 and (last_lba) and (last_lba - 256). The image may have any of
/// these depending on how it was authored.
fn find_avdp(file: &mut File, file_size: u64) -> Result<Vec<u8>> {
  let last_lba = file_size / SECTOR_SIZE as u64;
  let candidates: Vec<u64> = vec![256, last_lba.saturating_sub(1), last_lba.saturating_sub(257)];
  for lba in candidates {
    if (lba + 1) * SECTOR_SIZE as u64 > file_size {
      continue;
    }
    if let Ok(sector) = read_sector(file, lba) {
      let tag_id = u16::from_le_bytes([sector[0], sector[1]]);
      if tag_id == TAG_AVDP {
        return Ok(sector);
      }
    }
  }
  Err(anyhow!("Not a valid UDF image (no AVDP found)"))
}

impl UdfImage {
  pub fn open(path: &Path) -> Result<Self> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();

    let avdp = find_avdp(&mut file, file_size)?;

    let main_vds_length = u32::from_le_bytes([avdp[16], avdp[17], avdp[18], avdp[19]]) as usize;
    let main_vds_location = u32::from_le_bytes([avdp[20], avdp[21], avdp[22], avdp[23]]) as u64;
    let reserve_vds_length = u32::from_le_bytes([avdp[24], avdp[25], avdp[26], avdp[27]]) as usize;
    let reserve_vds_location = u32::from_le_bytes([avdp[28], avdp[29], avdp[30], avdp[31]]) as u64;

    let mut partitions: HashMap<u16, Partition> = HashMap::new();
    let mut latest_lvd: Option<(u32, Vec<u8>)> = None;

    // Walk both VDS sequences (main first, reserve as fallback).
    let sequences = [
      (main_vds_location, main_vds_length / SECTOR_SIZE),
      (reserve_vds_location, reserve_vds_length / SECTOR_SIZE),
    ];
    for (start_lba, n_sectors) in sequences {
      if n_sectors == 0 {
        continue;
      }
      for i in 0..n_sectors {
        let sector = match read_sector(&mut file, start_lba + i as u64) {
          Ok(s) => s,
          Err(_) => break,
        };
        let tid = u16::from_le_bytes([sector[0], sector[1]]);
        match tid {
          TAG_PD => {
            let vdsn = u32::from_le_bytes([sector[16], sector[17], sector[18], sector[19]]);
            let number = u16::from_le_bytes([sector[22], sector[23]]);
            let starting_location = u32::from_le_bytes([sector[188], sector[189], sector[190], sector[191]]);
            let entry = partitions.entry(number).or_insert(Partition {
              number,
              starting_location,
              vdsn,
            });
            if vdsn >= entry.vdsn {
              entry.starting_location = starting_location;
              entry.vdsn = vdsn;
            }
          }
          TAG_LVD => {
            let vdsn = u32::from_le_bytes([sector[16], sector[17], sector[18], sector[19]]);
            if latest_lvd.as_ref().map(|(v, _)| vdsn >= *v).unwrap_or(true) {
              latest_lvd = Some((vdsn, sector.clone()));
            }
          }
          TAG_TD => break,
          _ => {}
        }
      }
    }

    if partitions.is_empty() {
      return Err(anyhow!("UDF: no Partition Descriptor found"));
    }
    let lvd_sector = latest_lvd
      .map(|(_, s)| s)
      .ok_or_else(|| anyhow!("UDF: no Logical Volume Descriptor"))?;

    // Parse LVD: LogicalVolumeContentsUse (16 bytes long_ad → FSD) at offset 248.
    // MapTableLength at offset 264, NumberOfPartitionMaps at offset 268.
    // Partition maps start at offset 440.
    let fsd_long_ad = read_long_ad(&lvd_sector[248..264]);
    let map_table_length =
      u32::from_le_bytes([lvd_sector[264], lvd_sector[265], lvd_sector[266], lvd_sector[267]]) as usize;
    let n_partition_maps =
      u32::from_le_bytes([lvd_sector[268], lvd_sector[269], lvd_sector[270], lvd_sector[271]]) as usize;

    let mut partition_maps: Vec<PartitionMap> = Vec::with_capacity(n_partition_maps);
    let mut p = 440usize;
    let map_end = (440 + map_table_length).min(lvd_sector.len());
    for _ in 0..n_partition_maps {
      if p + 2 > map_end {
        break;
      }
      let map_type = lvd_sector[p];
      let map_length = lvd_sector[p + 1] as usize;
      if map_type == 1 && map_length >= 6 {
        let partition_number = u16::from_le_bytes([lvd_sector[p + 4], lvd_sector[p + 5]]);
        partition_maps.push(PartitionMap::Type1 { partition_number });
      } else if map_type == 2 && map_length >= 64 {
        // Type 2: read the PartitionTypeIdentifier (EntityID) at offset 4.
        // Identifier string occupies offset 4+1..4+24 (skip the 1-byte flags).
        let id_off = p + 4 + 1;
        let id_str = String::from_utf8_lossy(&lvd_sector[id_off..id_off + 23])
          .trim_end_matches(['\0', ' '])
          .to_string();
        let underlying_partition_number = u16::from_le_bytes([lvd_sector[p + 38], lvd_sector[p + 39]]);
        if id_str.contains("Metadata Partition") {
          let metadata_file_lba = u32::from_le_bytes([
            lvd_sector[p + 40],
            lvd_sector[p + 41],
            lvd_sector[p + 42],
            lvd_sector[p + 43],
          ]);
          // Resolve the metadata file's File Entry, then capture
          // its physical extents.
          let meta_partition = partitions.get(&underlying_partition_number).ok_or_else(|| {
            anyhow!(
              "UDF: metadata partition references unknown partition {}",
              underlying_partition_number
            )
          })?;
          let meta_file_phys_lba = meta_partition.starting_location as u64 + metadata_file_lba as u64;
          let meta_fe = read_file_entry_at_phys_lba(&mut file, meta_partition.starting_location, meta_file_phys_lba)?;
          let mut metadata_extents: Vec<(u32, u32)> = Vec::new();
          for ad in &meta_fe.allocation_descriptors {
            match ad {
              AllocDesc::Short(s) => {
                let phys = meta_partition.starting_location + s.position;
                metadata_extents.push((phys, s.length()));
              }
              AllocDesc::Long(l) => {
                let phys = meta_partition.starting_location + l.location.logical_block_number;
                metadata_extents.push((phys, l.length()));
              }
            }
          }
          partition_maps.push(PartitionMap::Metadata {
            partition_number: underlying_partition_number,
            metadata_extents,
            metadata_size: meta_fe.size,
          });
        } else {
          // Sparable / Virtual / unknown — fall back to direct mapping.
          partition_maps.push(PartitionMap::Other {
            partition_number: underlying_partition_number,
          });
        }
      } else {
        break;
      }
      p += map_length.max(1);
    }

    if partition_maps.is_empty() {
      // Some images set NumberOfPartitionMaps = 0 even though they have
      // exactly one physical partition; synthesize a Type 1 map for it.
      if let Some(part) = partitions.values().next() {
        partition_maps.push(PartitionMap::Type1 {
          partition_number: part.number,
        });
      }
    }

    // Logical Volume Identifier: a 128-byte d-string at offset 84 of the LVD.
    let volume_label = parse_d_string(&lvd_sector[84..212]);

    // Build the image with what we have so we can use its resolver.
    let mut img = UdfImage {
      file,
      partitions,
      partition_maps,
      root: UdfFile {
        size: 0,
        is_directory: true,
        embedded_data: None,
        allocation_descriptors: Vec::new(),
        partition_reference: 0,
      },
      volume_label,
    };

    // Resolve the FSD physical LBA via the partition maps.
    let fsd_phys_lba = img.resolve_phys_lba(
      fsd_long_ad.location.partition_reference_number,
      fsd_long_ad.location.logical_block_number,
    )?;
    let fsd = read_sector(&mut img.file, fsd_phys_lba)?;
    let tid = u16::from_le_bytes([fsd[0], fsd[1]]);
    if tid != TAG_FSD {
      return Err(anyhow!("UDF: expected FSD, got tag {} at LBA {}", tid, fsd_phys_lba));
    }
    let root_icb = read_long_ad(&fsd[400..416]);

    let root = img.read_file_entry(&root_icb)?;
    img.root = root;
    Ok(img)
  }

  fn resolve_phys_lba(&self, prn: u16, lbn: u32) -> Result<u64> {
    let pmap = self
      .partition_maps
      .get(prn as usize)
      .ok_or_else(|| anyhow!("UDF: partition_reference_number {} out of range", prn))?;
    match pmap {
      PartitionMap::Type1 { partition_number } | PartitionMap::Other { partition_number } => {
        let part = self
          .partitions
          .get(partition_number)
          .ok_or_else(|| anyhow!("UDF: unknown partition {}", partition_number))?;
        Ok(part.starting_location as u64 + lbn as u64)
      }
      PartitionMap::Metadata {
        metadata_extents,
        metadata_size,
        ..
      } => {
        let mut byte_offset_in_meta = lbn as u64 * SECTOR_SIZE as u64;
        if byte_offset_in_meta >= *metadata_size {
          return Err(anyhow!(
            "UDF: lbn {} beyond metadata file (size {} bytes)",
            lbn,
            metadata_size
          ));
        }
        for (phys_lba, length_bytes) in metadata_extents {
          let length = *length_bytes as u64;
          if byte_offset_in_meta < length {
            return Ok(*phys_lba as u64 + byte_offset_in_meta / SECTOR_SIZE as u64);
          }
          byte_offset_in_meta -= length;
        }
        Err(anyhow!("UDF: lbn {} not covered by metadata extents", lbn))
      }
    }
  }

  fn resolve_phys_byte_run(
    &self,
    prn: u16,
    lbn: u32,
    length_bytes: usize,
  ) -> Result<Vec<(u64 /* phys LBA */, u64 /* run bytes */)>> {
    let pmap = self
      .partition_maps
      .get(prn as usize)
      .ok_or_else(|| anyhow!("UDF: partition_reference_number {} out of range", prn))?;
    match pmap {
      PartitionMap::Type1 { partition_number } | PartitionMap::Other { partition_number } => {
        let part = self
          .partitions
          .get(partition_number)
          .ok_or_else(|| anyhow!("UDF: unknown partition {}", partition_number))?;
        let phys = part.starting_location as u64 + lbn as u64;
        Ok(vec![(phys, length_bytes as u64)])
      }
      PartitionMap::Metadata {
        metadata_extents,
        metadata_size,
        ..
      } => {
        let mut runs: Vec<(u64, u64)> = Vec::new();
        let mut remaining = length_bytes as u64;
        let mut byte_offset = lbn as u64 * SECTOR_SIZE as u64;
        if byte_offset >= *metadata_size {
          return Err(anyhow!("UDF: lbn {} beyond metadata file", lbn));
        }
        for (phys_lba, ext_length) in metadata_extents {
          let ext_length = *ext_length as u64;
          if byte_offset >= ext_length {
            byte_offset -= ext_length;
            continue;
          }
          let phys_start = *phys_lba as u64 + byte_offset / SECTOR_SIZE as u64;
          let in_ext_remaining = ext_length - byte_offset;
          let take = in_ext_remaining.min(remaining);
          runs.push((phys_start, take));
          remaining -= take;
          byte_offset = 0;
          if remaining == 0 {
            break;
          }
        }
        Ok(runs)
      }
    }
  }

  pub fn list_dir(&mut self, fe: &UdfFile) -> Result<Vec<UdfDirEntry>> {
    let bytes = self.read_file(fe)?;
    parse_fids(&bytes)
  }

  pub fn resolve(&mut self, path: &str) -> Result<UdfFile> {
    let mut current = self.root.clone();
    for part in path.split(['/', '\\']).filter(|s| !s.is_empty()) {
      if !current.is_directory {
        return Err(anyhow!("UDF: not a directory at component {}", part));
      }
      let entries = self.list_dir(&current)?;
      let entry = entries
        .iter()
        .find(|e| !e.is_parent && !e.is_deleted && e.name.eq_ignore_ascii_case(part))
        .ok_or_else(|| anyhow!("UDF: path component not found: {}", part))?;
      current = self.read_file_entry(&entry.icb)?;
    }
    Ok(current)
  }

  pub fn try_resolve(&mut self, path: &str) -> Option<UdfFile> {
    self.resolve(path).ok()
  }

  pub fn read_file(&mut self, fe: &UdfFile) -> Result<Vec<u8>> {
    if let Some(data) = &fe.embedded_data {
      let mut out = data.clone();
      out.truncate(fe.size as usize);
      return Ok(out);
    }
    let mut out: Vec<u8> = Vec::with_capacity(fe.size as usize);
    let mut remaining = fe.size as usize;
    for ad in fe.allocation_descriptors.clone() {
      if remaining == 0 {
        break;
      }
      let (prn, lbn, len) = match ad {
        AllocDesc::Short(s) => (fe.partition_reference, s.position, s.length() as usize),
        AllocDesc::Long(l) => (
          l.location.partition_reference_number,
          l.location.logical_block_number,
          l.length() as usize,
        ),
      };
      let runs = self.resolve_phys_byte_run(prn, lbn, len)?;
      let mut consumed_in_ad = 0usize;
      for (phys_lba, run_bytes) in runs {
        let take = (run_bytes as usize).min(remaining).min(len - consumed_in_ad);
        if take == 0 {
          break;
        }
        let aligned = ((take + SECTOR_SIZE - 1) / SECTOR_SIZE) * SECTOR_SIZE;
        let chunk = read_run(&mut self.file, phys_lba, aligned.max(SECTOR_SIZE))?;
        out.extend_from_slice(&chunk[..take]);
        remaining -= take;
        consumed_in_ad += take;
        if remaining == 0 {
          break;
        }
      }
    }
    Ok(out)
  }

  pub fn read_file_entry(&mut self, icb: &LongAd) -> Result<UdfFile> {
    let lba = self.resolve_phys_lba(
      icb.location.partition_reference_number,
      icb.location.logical_block_number,
    )?;
    let len = icb.length();
    let to_read = if len == 0 {
      SECTOR_SIZE
    } else {
      ((len as usize + SECTOR_SIZE - 1) / SECTOR_SIZE) * SECTOR_SIZE
    };
    let buf = read_run(&mut self.file, lba, to_read)?;
    parse_file_entry(&buf, icb.location.partition_reference_number)
  }

  pub fn directory_size(&mut self, fe: &UdfFile) -> Result<u64> {
    let mut visited: HashSet<(u32, u16)> = HashSet::new();
    directory_size_inner(self, fe, &mut visited, 0)
  }
}

/// Recursive directory-byte tally with explicit loop protection: a `visited`
/// set of (block, partition) ICB locations prevents cycles from a malformed
/// disc, and `MAX_DIR_DEPTH` caps pathological nesting.
fn directory_size_inner(
  image: &mut UdfImage,
  fe: &UdfFile,
  visited: &mut HashSet<(u32, u16)>,
  depth: u32,
) -> Result<u64> {
  const MAX_DIR_DEPTH: u32 = 100;
  let mut total: u64 = 0;
  if !fe.is_directory {
    return Ok(fe.size);
  }
  if depth >= MAX_DIR_DEPTH {
    return Ok(0);
  }
  let entries = image.list_dir(fe)?;
  for e in entries {
    if e.is_parent || e.is_deleted {
      continue;
    }
    let child = image.read_file_entry(&e.icb)?;
    if child.is_directory {
      let key = (
        e.icb.location.logical_block_number,
        e.icb.location.partition_reference_number,
      );
      // Don't descend into a directory ICB we've already seen.
      if !visited.insert(key) {
        continue;
      }
      total += directory_size_inner(image, &child, visited, depth + 1)?;
    } else if !e.name.to_ascii_lowercase().ends_with(".ssif") {
      total += child.size;
    }
  }
  Ok(total)
}

/// Read a File Entry / Extended File Entry from a sector that may not yet be
/// covered by the `UdfImage` partition map (used while bootstrapping the
/// metadata partition's metadata file).
fn read_file_entry_at_phys_lba(file: &mut File, partition_start_lba: u32, phys_lba: u64) -> Result<UdfFile> {
  let buf = read_run(file, phys_lba, SECTOR_SIZE * 2)?;
  // The metadata file is in the underlying physical partition: any short_ads
  // it contains are relative to that partition's start LBA, and all
  // long_ads carry partition_reference_number 0 (the underlying partition),
  // so we synthesize a one-partition pseudo-image during parse.
  let _ = partition_start_lba;
  parse_file_entry(&buf, 0)
}

fn parse_file_entry(buf: &[u8], partition_reference: u16) -> Result<UdfFile> {
  let tid = u16::from_le_bytes([buf[0], buf[1]]);
  if tid != TAG_FE && tid != TAG_EFE {
    return Err(anyhow!("UDF: expected FE/EFE, got tag {}", tid));
  }
  // ICB Tag at offset 16 (20 bytes); flags at 18..20 within the ICB tag,
  // i.e. buf[34..36]. Bottom 3 bits of flags = AD type.
  let file_type = buf[27]; // 16 + 11
  let icb_flags = u16::from_le_bytes([buf[34], buf[35]]);
  let ad_type = icb_flags & 0x7;
  let is_directory = file_type == 4;

  let (info_length_off, length_ea_off, length_ad_off, body_start) = if tid == TAG_FE {
    (56usize, 168usize, 172usize, 176usize)
  } else {
    (56usize, 208usize, 212usize, 216usize)
  };

  let size = u64::from_le_bytes([
    buf[info_length_off],
    buf[info_length_off + 1],
    buf[info_length_off + 2],
    buf[info_length_off + 3],
    buf[info_length_off + 4],
    buf[info_length_off + 5],
    buf[info_length_off + 6],
    buf[info_length_off + 7],
  ]);
  let length_ea = u32::from_le_bytes([
    buf[length_ea_off],
    buf[length_ea_off + 1],
    buf[length_ea_off + 2],
    buf[length_ea_off + 3],
  ]) as usize;
  let length_ad = u32::from_le_bytes([
    buf[length_ad_off],
    buf[length_ad_off + 1],
    buf[length_ad_off + 2],
    buf[length_ad_off + 3],
  ]) as usize;

  let ad_start = body_start + length_ea;
  let ad_end = (ad_start + length_ad).min(buf.len());

  let mut allocation_descriptors: Vec<AllocDesc> = Vec::new();
  let mut embedded_data: Option<Vec<u8>> = None;

  match ad_type {
    0 => {
      let mut p = ad_start;
      while p + 8 <= ad_end {
        let ad = read_short_ad(&buf[p..p + 8]);
        if ad.length() == 0 {
          break;
        }
        allocation_descriptors.push(AllocDesc::Short(ad));
        p += 8;
      }
    }
    1 => {
      let mut p = ad_start;
      while p + 16 <= ad_end {
        let ad = read_long_ad(&buf[p..p + 16]);
        if ad.length() == 0 {
          break;
        }
        allocation_descriptors.push(AllocDesc::Long(ad));
        p += 16;
      }
    }
    3 => {
      if ad_end <= buf.len() {
        embedded_data = Some(buf[ad_start..ad_end].to_vec());
      }
    }
    _ => {}
  }

  Ok(UdfFile {
    size,
    is_directory,
    embedded_data,
    allocation_descriptors,
    partition_reference,
  })
}

pub fn read_file_entry_at(image: &mut UdfImage, icb: &LongAd) -> Result<UdfFile> {
  image.read_file_entry(icb)
}

fn parse_fids(buf: &[u8]) -> Result<Vec<UdfDirEntry>> {
  let mut out: Vec<UdfDirEntry> = Vec::new();
  let mut p = 0;
  while p + 38 <= buf.len() {
    let tid = u16::from_le_bytes([buf[p], buf[p + 1]]);
    if tid == 0 {
      let next = ((p / SECTOR_SIZE) + 1) * SECTOR_SIZE;
      if next <= p {
        break;
      }
      p = next;
      continue;
    }
    if tid != TAG_FID {
      let next = ((p / SECTOR_SIZE) + 1) * SECTOR_SIZE;
      if next <= p {
        break;
      }
      p = next;
      continue;
    }
    let characteristics = buf[p + 18];
    let l_fi = buf[p + 19] as usize;
    let icb = read_long_ad(&buf[p + 20..p + 36]);
    let l_iu = u16::from_le_bytes([buf[p + 36], buf[p + 37]]) as usize;
    let fi_off = p + 38 + l_iu;
    if fi_off + l_fi > buf.len() {
      break;
    }
    let name_bytes = &buf[fi_off..fi_off + l_fi];
    let name = parse_d_string(name_bytes);
    let is_parent = (characteristics & 0x8) != 0;
    out.push(UdfDirEntry {
      name,
      icb,
      is_directory: (characteristics & 0x2) != 0,
      is_parent,
      is_hidden: (characteristics & 0x1) != 0,
      is_deleted: (characteristics & 0x4) != 0,
    });
    let total = 38 + l_iu + l_fi;
    let padded = (total + 3) & !3;
    p += padded;
  }
  Ok(out)
}

/// Streaming reader that walks a UdfFile's allocation descriptors and pulls
/// bytes through the partition-map resolver. Used to feed the M2TS scanner
/// without buffering whole files.
pub struct UdfFileReader {
  image: Arc<Mutex<UdfImage>>,
  embedded_data: Option<Vec<u8>>,
  embedded_offset: usize,
  /// (physical LBA, run length in bytes) for every byte the file references.
  runs: Vec<(u64, u64)>,
  run_index: usize,
  run_offset: u64,
  total_remaining: u64,
}

impl UdfFileReader {
  pub fn new(image: Arc<Mutex<UdfImage>>, fe: &UdfFile) -> Result<Self> {
    let mut runs: Vec<(u64, u64)> = Vec::new();
    {
      let img = image.lock().unwrap_or_else(|e| e.into_inner());
      for ad in &fe.allocation_descriptors {
        let (prn, lbn, len) = match ad {
          AllocDesc::Short(s) => (fe.partition_reference, s.position, s.length() as usize),
          AllocDesc::Long(l) => (
            l.location.partition_reference_number,
            l.location.logical_block_number,
            l.length() as usize,
          ),
        };
        runs.extend(img.resolve_phys_byte_run(prn, lbn, len)?);
      }
    }
    Ok(Self {
      image,
      embedded_data: fe.embedded_data.clone(),
      embedded_offset: 0,
      runs,
      run_index: 0,
      run_offset: 0,
      total_remaining: fe.size,
    })
  }
}

impl Read for UdfFileReader {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    if self.total_remaining == 0 {
      return Ok(0);
    }
    if let Some(data) = &self.embedded_data {
      let available = data.len().saturating_sub(self.embedded_offset);
      let remaining = usize::try_from(self.total_remaining).unwrap_or(usize::MAX);
      let want = buf.len().min(available).min(remaining);
      if want == 0 {
        return Ok(0);
      }
      buf[..want].copy_from_slice(&data[self.embedded_offset..self.embedded_offset + want]);
      self.embedded_offset += want;
      self.total_remaining -= want as u64;
      return Ok(want);
    }
    while self.run_index < self.runs.len() {
      let (lba, length) = self.runs[self.run_index];
      if self.run_offset >= length {
        self.run_index += 1;
        self.run_offset = 0;
        continue;
      }
      let remain_in_run = length - self.run_offset;
      let want = (buf.len() as u64).min(remain_in_run).min(self.total_remaining) as usize;
      if want == 0 {
        return Ok(0);
      }
      let abs_byte = lba * SECTOR_SIZE as u64 + self.run_offset;
      let mut img = self.image.lock().unwrap_or_else(|e| e.into_inner());
      img.file.seek(SeekFrom::Start(abs_byte))?;
      let n = img.file.read(&mut buf[..want])?;
      self.run_offset += n as u64;
      self.total_remaining -= n as u64;
      return Ok(n);
    }
    Ok(0)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;
  use std::path::PathBuf;
  use std::sync::atomic::{AtomicU64, Ordering};

  // ---------------------------------------------------------------------
  // Temp-file scaffolding (no `tempfile` dev-dependency available).
  // ---------------------------------------------------------------------

  static UNIQUE: AtomicU64 = AtomicU64::new(0);

  /// A path under the system temp dir that is removed on drop. Works for
  /// both single files and directory trees.
  struct TempPath {
    path: PathBuf,
    is_dir: bool,
  }

  impl TempPath {
    fn unique(prefix: &str, ext: &str) -> PathBuf {
      let n = UNIQUE.fetch_add(1, Ordering::Relaxed);
      let pid = std::process::id();
      let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
      let mut name = format!("bdmaster_udf_{}_{}_{}_{}", prefix, pid, nanos, n);
      if !ext.is_empty() {
        name.push('.');
        name.push_str(ext);
      }
      std::env::temp_dir().join(name)
    }

    fn new_file(prefix: &str, ext: &str) -> Self {
      TempPath {
        path: Self::unique(prefix, ext),
        is_dir: false,
      }
    }

    fn new_dir(prefix: &str) -> std::io::Result<Self> {
      let path = Self::unique(prefix, "");
      std::fs::create_dir_all(&path)?;
      Ok(TempPath { path, is_dir: true })
    }
  }

  impl Drop for TempPath {
    fn drop(&mut self) {
      if self.is_dir {
        let _ = std::fs::remove_dir_all(&self.path);
      } else {
        let _ = std::fs::remove_file(&self.path);
      }
    }
  }

  // ---------------------------------------------------------------------
  // Synthetic Blu-ray structure builders (mirroring the per-module test
  // helpers in mpls.rs / clpi.rs / m2ts.rs).
  // ---------------------------------------------------------------------

  fn build_mpls() -> Vec<u8> {
    let mut d: Vec<u8> = Vec::new();
    d.extend_from_slice(b"MPLS0200");
    d.extend_from_slice(&[0u8; 4]); // playlist_offset @8
    d.extend_from_slice(&[0u8; 4]); // chapters_offset @12
    d.extend_from_slice(&[0u8; 4]); // extensions_offset @16
    while d.len() < 0x38 {
      d.push(0);
    }
    d.push(0x10); // misc flags @0x38 -> mvc_base_view_r = true

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
    d.push(0x00);
    d.push(0x00);
    d.extend_from_slice(&0u32.to_be_bytes()); // in_time
    d.extend_from_slice(&4_500_000u32.to_be_bytes()); // out_time (100 s)
    d.extend_from_slice(&[0u8; 12]);

    // STN table header.
    d.extend_from_slice(&0u16.to_be_bytes()); // stn_length
    d.extend_from_slice(&0u16.to_be_bytes()); // reserved
    d.push(1); // video count
    d.push(1); // audio count
    d.push(0);
    d.push(0);
    d.push(0);
    d.push(0);
    d.push(0);
    d.extend_from_slice(&[0u8; 5]);

    // Video stream entry.
    d.push(3);
    d.push(1);
    d.extend_from_slice(&0x1011u16.to_be_bytes());
    d.push(3);
    d.push(0x1b); // AVC
    d.push((6 << 4) | 1); // 1080p / 23.976
    d.push(3 << 4); // 16:9

    // Audio stream entry.
    d.push(3);
    d.push(1);
    d.extend_from_slice(&0x1100u16.to_be_bytes());
    d.push(5);
    d.push(0x81); // AC3
    d.push((6 << 4) | 1);
    d.extend_from_slice(b"eng");

    let item_len = (d.len() - item_start - 2) as u16;
    d[item_start..item_start + 2].copy_from_slice(&item_len.to_be_bytes());

    let chapters_offset = d.len() as u32;
    d[12..16].copy_from_slice(&chapters_offset.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes());
    d.extend_from_slice(&1u16.to_be_bytes());
    let mut chapter = vec![0u8; 14];
    chapter[1] = 1;
    chapter[4..8].copy_from_slice(&(45000u32 * 10).to_be_bytes());
    d.extend_from_slice(&chapter);
    d
  }

  fn build_clpi() -> Vec<u8> {
    // A coding-info entry: PID (2 bytes), length byte, coding type, attributes.
    fn stream_entry(pid: u16, coding_type: u8, attrs: &[u8]) -> Vec<u8> {
      let mut v = Vec::new();
      v.extend_from_slice(&pid.to_be_bytes());
      v.push((1 + attrs.len()) as u8);
      v.push(coding_type);
      v.extend_from_slice(attrs);
      v
    }
    let video = stream_entry(0x1011, 0x1b, &[(6 << 4) | 1, 3 << 4]);
    let audio = stream_entry(0x1100, 0x81, &[(6 << 4) | 1, b'e', b'n', b'g']);
    let streams = [video, audio];

    let mut clip = vec![0u8; 10];
    clip[8] = streams.len() as u8;
    for s in &streams {
      clip.extend_from_slice(s);
    }
    let clip_len = clip.len() as u32;

    let mut data = Vec::new();
    data.extend_from_slice(b"HDMV0200");
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&16u32.to_be_bytes());
    data.extend_from_slice(&clip_len.to_be_bytes());
    data.extend_from_slice(&clip);
    data
  }

  // M2TS builders (mirroring m2ts.rs test helpers).
  const TS_PACKET_SIZE: usize = 188;
  const SYNC_BYTE: u8 = 0x47;

  fn ts_packet(pusi: bool, pid: u16, payload: &[u8]) -> Vec<u8> {
    let mut ts = vec![0xFFu8; TS_PACKET_SIZE];
    ts[0] = SYNC_BYTE;
    ts[1] = ((pid >> 8) as u8 & 0x1F) | if pusi { 0x40 } else { 0 };
    ts[2] = (pid & 0xFF) as u8;
    ts[3] = 0x10;
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

  fn pmt_payload() -> Vec<u8> {
    vec![
      0x00, 0x02, 0xB0, 0x17, 0x00, 0x01, 0x01, 0x00, 0x00, 0xE0, 0x00, 0xF0, 0x00, 0x1b, 0xF0, 0x11, 0xF0,
      0x00, // AVC PID 0x1011
      0x81, 0xF1, 0x00, 0xF0, 0x00, // AC3 PID 0x1100
      0x00, 0x00, 0x00, 0x00,
    ]
  }

  fn pes_payload(es: &[u8]) -> Vec<u8> {
    let mut v = vec![0x00, 0x00, 0x01, 0xE0, 0x00, 0x00, 0x80, 0x00, 0x00];
    v.extend_from_slice(es);
    v
  }

  /// Build an M2TS that spans several sectors so the UDF reader has to walk
  /// multiple allocation runs. Contains a valid PAT, PMT and a couple of PES.
  fn build_m2ts() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&m2ts_frame(&ts_packet(true, 0x0000, &pat_payload(1, 0x0100))));
    data.extend_from_slice(&m2ts_frame(&ts_packet(true, 0x0100, &pmt_payload())));
    data.extend_from_slice(&m2ts_frame(&ts_packet(true, 0x1011, &pes_payload(&[0xAA, 0xBB]))));
    data.extend_from_slice(&m2ts_frame(&ts_packet(true, 0x1100, &pes_payload(&[0xCC, 0xDD]))));
    // Pad with null TS packets so the file is several KiB (multi-sector).
    while data.len() < 8 * 1024 {
      data.extend_from_slice(&m2ts_frame(&ts_packet(false, 0x1FFF, &[])));
    }
    data
  }

  /// Lay down a minimal BDMV tree under `root`.
  fn build_bdmv_tree(root: &Path) -> std::io::Result<()> {
    let bdmv = root.join("BDMV");
    std::fs::create_dir_all(bdmv.join("PLAYLIST"))?;
    std::fs::create_dir_all(bdmv.join("CLIPINF"))?;
    std::fs::create_dir_all(bdmv.join("STREAM"))?;
    std::fs::create_dir_all(bdmv.join("META").join("DL"))?;

    // index.bdmv (INDX0200 = non-UHD).
    std::fs::write(bdmv.join("index.bdmv"), b"INDX0200payload")?;
    std::fs::write(bdmv.join("PLAYLIST").join("00800.mpls"), build_mpls())?;
    std::fs::write(bdmv.join("CLIPINF").join("00001.clpi"), build_clpi())?;
    std::fs::write(bdmv.join("STREAM").join("00001.m2ts"), build_m2ts())?;

    // A disc-title metadata file so read_disc_title_iso has something to find.
    let bdmt = "<?xml version=\"1.0\"?><di:disclib xmlns:di=\"x\">\
            <di:discinfo><di:title><di:name>My Test Disc</di:name></di:title>\
            </di:discinfo></di:disclib>";
    std::fs::write(bdmv.join("META").join("DL").join("bdmt_eng.xml"), bdmt)?;
    Ok(())
  }

  /// Run hdiutil to make a UDF image from `srcdir`; returns false if the tool
  /// is unavailable or failed (so tests can skip gracefully).
  fn make_udf_image(srcdir: &Path, out: &Path) -> bool {
    let status = std::process::Command::new("hdiutil")
      .arg("makehybrid")
      .arg("-udf")
      .arg("-udf-volume-name")
      .arg("BDTEST")
      .arg("-ov")
      .arg("-o")
      .arg(out)
      .arg(srcdir)
      .stdout(std::process::Stdio::null())
      .stderr(std::process::Stdio::null())
      .status();
    match status {
      Ok(s) if s.success() => out.exists() && std::fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false),
      _ => false,
    }
  }

  /// Generate (and cache) a real UDF image of a minimal Blu-ray tree, run the
  /// closure with its path, and clean up. Returns false if hdiutil is missing.
  fn with_generated_iso<F: FnOnce(&Path)>(f: F) -> bool {
    let srcdir = match TempPath::new_dir("src") {
      Ok(d) => d,
      Err(_) => return false,
    };
    if build_bdmv_tree(&srcdir.path).is_err() {
      return false;
    }
    let iso = TempPath::new_file("img", "iso");
    if !make_udf_image(&srcdir.path, &iso.path) {
      return false;
    }
    f(&iso.path);
    true
  }

  // =====================================================================
  // End-to-end: parse a real hdiutil-generated UDF Blu-ray image.
  // =====================================================================

  #[test]
  fn open_and_navigate_generated_udf_image() {
    let ran = with_generated_iso(|iso_path| {
      let mut img = UdfImage::open(iso_path).expect("UDF image opens");

      // Volume label comes from the LVD Logical Volume Identifier.
      assert!(
        img.volume_label.to_uppercase().contains("BDTEST"),
        "volume label should contain BDTEST, got {:?}",
        img.volume_label
      );

      // Root is a directory.
      assert!(img.root.is_directory);

      // Root listing contains BDMV.
      let root = img.root.clone();
      let root_entries = img.list_dir(&root).expect("root listing");
      assert!(
        root_entries.iter().any(|e| e.name.eq_ignore_ascii_case("BDMV")),
        "root should list BDMV: {:?}",
        root_entries.iter().map(|e| &e.name).collect::<Vec<_>>()
      );

      // Resolve and verify each Blu-ray subdirectory.
      let bdmv = img.resolve("BDMV").expect("BDMV resolves");
      assert!(bdmv.is_directory);
      assert!(img.resolve("BDMV/PLAYLIST").unwrap().is_directory);
      assert!(img.resolve("BDMV/CLIPINF").unwrap().is_directory);
      assert!(img.resolve("BDMV/STREAM").unwrap().is_directory);

      // Backslash separators and case-insensitivity both work.
      assert!(img.resolve("bdmv\\stream").unwrap().is_directory);

      // Read a file by full content and check the magic bytes.
      let index = img.resolve("BDMV/index.bdmv").expect("index.bdmv resolves");
      assert!(!index.is_directory);
      let index_bytes = img.read_file(&index).expect("read index.bdmv");
      assert_eq!(&index_bytes[..8], b"INDX0200");
      assert_eq!(index_bytes.len() as u64, index.size);

      // The MPLS file resolves and round-trips.
      let mpls = img.resolve("BDMV/PLAYLIST/00800.mpls").expect("mpls resolves");
      let mpls_bytes = img.read_file(&mpls).expect("read mpls");
      assert_eq!(&mpls_bytes[..8], b"MPLS0200");
      assert_eq!(mpls_bytes, build_mpls());

      // The CLPI file round-trips.
      let clpi = img.resolve("BDMV/CLIPINF/00001.clpi").expect("clpi resolves");
      let clpi_bytes = img.read_file(&clpi).expect("read clpi");
      assert_eq!(clpi_bytes, build_clpi());

      // The (multi-sector) M2TS round-trips exactly.
      let m2ts = img.resolve("BDMV/STREAM/00001.m2ts").expect("m2ts resolves");
      let expected_m2ts = build_m2ts();
      assert_eq!(m2ts.size, expected_m2ts.len() as u64);
      let m2ts_bytes = img.read_file(&m2ts).expect("read m2ts");
      assert_eq!(m2ts_bytes, expected_m2ts);

      // read_file_entry via read_file_entry_at (public re-export).
      let stream_dir = img.resolve("BDMV/STREAM").unwrap();
      let stream_entries = img.list_dir(&stream_dir).unwrap();
      let m2ts_fid = stream_entries
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case("00001.m2ts"))
        .expect("m2ts FID present");
      let fe = read_file_entry_at(&mut img, &m2ts_fid.icb).expect("FE reads");
      assert_eq!(fe.size, expected_m2ts.len() as u64);

      // directory_size sums all files but skips .ssif (none here).
      let root = img.root.clone();
      let total = img.directory_size(&root).expect("directory size");
      let sum = index.size + mpls.size + clpi.size + m2ts.size;
      // bdmt_eng.xml also counts; total must be at least the four files.
      assert!(total >= sum, "total {} < sum {}", total, sum);

      // try_resolve: present and absent.
      assert!(img.try_resolve("BDMV").is_some());
      assert!(img.try_resolve("BDMV/NOPE/missing.bin").is_none());

      // resolve errors on a missing component.
      assert!(img.resolve("BDMV/PLAYLIST/missing.mpls").is_err());

      // resolve errors when a path descends through a non-directory.
      assert!(img.resolve("BDMV/index.bdmv/inner").is_err());
    });
    if !ran {
      eprintln!("hdiutil unavailable; skipping generated-UDF test");
    }
  }

  #[test]
  fn udf_file_reader_streams_full_m2ts() {
    let ran = with_generated_iso(|iso_path| {
      let img = UdfImage::open(iso_path).expect("opens");
      let image = Arc::new(Mutex::new(img));
      let fe = {
        let mut g = image.lock().unwrap();
        g.resolve("BDMV/STREAM/00001.m2ts").expect("m2ts resolves")
      };
      let mut reader = UdfFileReader::new(image.clone(), &fe).expect("reader builds");

      // Read everything through the streaming reader and compare.
      let mut out = Vec::new();
      let mut chunk = [0u8; 777]; // odd size to cross run boundaries
      loop {
        let n = reader.read(&mut chunk).expect("read");
        if n == 0 {
          break;
        }
        out.extend_from_slice(&chunk[..n]);
      }
      assert_eq!(out, build_m2ts());

      // A second read after exhaustion returns 0.
      assert_eq!(reader.read(&mut chunk).unwrap(), 0);
    });
    if !ran {
      eprintln!("hdiutil unavailable; skipping reader test");
    }
  }

  #[test]
  fn udf_file_reader_zero_length_file() {
    let ran = with_generated_iso(|iso_path| {
      let img = UdfImage::open(iso_path).expect("opens");
      let image = Arc::new(Mutex::new(img));
      // Synthesize a zero-size UdfFile (no runs) and confirm reader yields 0.
      let empty = UdfFile {
        size: 0,
        is_directory: false,
        embedded_data: None,
        allocation_descriptors: Vec::new(),
        partition_reference: 0,
      };
      let mut reader = UdfFileReader::new(image, &empty).expect("reader builds");
      let mut buf = [0u8; 16];
      assert_eq!(reader.read(&mut buf).unwrap(), 0);
    });
    if !ran {
      eprintln!("hdiutil unavailable; skipping zero-length reader test");
    }
  }

  // =====================================================================
  // End-to-end through the top-level bdrom open / scan paths.
  // =====================================================================

  #[test]
  fn open_bdrom_reads_generated_iso() {
    let ran = with_generated_iso(|iso_path| {
      let bd = crate::bdrom::open_bdrom(iso_path, false).expect("open_bdrom on iso");
      assert!(bd.volume_label.to_uppercase().contains("BDTEST"));
      assert!(!bd.is_uhd, "INDX0200 should not be UHD");
      assert!(bd.size > 0);
      assert_eq!(bd.disc_title.as_deref(), Some("My Test Disc"));
      // One playlist, one clip, one stream from the synthetic tree.
      assert_eq!(bd.playlists.len(), 1);
      assert!(bd.stream_files.contains_key("00001.M2TS"));
      assert!(bd.stream_clip_files.contains_key("00001.CLPI"));
    });
    if !ran {
      eprintln!("hdiutil unavailable; skipping open_bdrom iso test");
    }
  }

  #[test]
  fn scan_drives_iso_end_to_end() {
    let ran = with_generated_iso(|iso_path| {
      let disc = crate::bdrom::scan(&iso_path.to_string_lossy()).expect("scan succeeds on iso");
      assert!(disc.volume_label.to_uppercase().contains("BDTEST"));
      assert_eq!(disc.playlists.len(), 1);
      assert_eq!(disc.stream_files.len(), 1);
      assert_eq!(disc.disc_title, "My Test Disc");
    });
    if !ran {
      eprintln!("hdiutil unavailable; skipping scan iso test");
    }
  }

  // =====================================================================
  // Error paths against malformed/empty images.
  // =====================================================================

  #[test]
  fn open_rejects_non_udf_file() {
    // A file with no AVDP anywhere -> find_avdp error.
    let tmp = TempPath::new_file("notudf", "iso");
    let mut f = File::create(&tmp.path).expect("create");
    // Make it large enough that the AVDP candidate sectors are in range,
    // but never carry the AVDP tag.
    f.write_all(&vec![0u8; SECTOR_SIZE * 300]).expect("write");
    drop(f);
    let err = match UdfImage::open(&tmp.path) {
      Ok(_) => panic!("expected open to fail on non-UDF file"),
      Err(e) => e,
    };
    assert!(err.to_string().contains("no AVDP"), "got {}", err);
  }

  #[test]
  fn open_missing_file_errors() {
    let missing = std::env::temp_dir().join("bdmaster_udf_definitely_missing_xyz.iso");
    assert!(UdfImage::open(&missing).is_err());
  }

  // =====================================================================
  // Pure-helper unit tests (cover edge branches the image won't hit).
  // =====================================================================

  #[test]
  fn d_string_compression_8_is_ascii() {
    let mut buf = vec![8u8];
    buf.extend_from_slice(b"BD_VIDEO\0\0");
    assert_eq!(parse_d_string(&buf), "BD_VIDEO");
  }

  #[test]
  fn d_string_compression_16_is_utf16_be() {
    let mut buf = vec![16u8];
    for ch in "Café".chars() {
      buf.extend_from_slice(&(ch as u16).to_be_bytes());
    }
    assert_eq!(parse_d_string(&buf), "Café");
  }

  #[test]
  fn d_string_compression_16_stops_at_nul_and_odd_tail() {
    // Embedded NUL terminates; trailing odd byte is ignored without panic.
    let buf = vec![16u8, 0x00, b'A', 0x00, 0x00, 0xFF];
    assert_eq!(parse_d_string(&buf), "A");
  }

  #[test]
  fn d_string_empty_or_unknown_compression() {
    assert_eq!(parse_d_string(&[]), "");
    assert_eq!(parse_d_string(&[5, 1, 2, 3]), "");
  }

  #[test]
  fn long_ad_length_masks_type_bits() {
    let buf = [0x00, 0x08, 0x00, 0xC0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let ad = read_long_ad(&buf);
    assert_eq!(ad.length(), 0x0000_0800);
  }

  #[test]
  fn short_ad_length_masks_type_bits() {
    let buf = [0x00, 0x10, 0x00, 0x40, 0, 0, 0, 0];
    let ad = read_short_ad(&buf);
    assert_eq!(ad.length(), 0x0000_1000);
  }

  #[test]
  fn read_long_ad_extracts_location() {
    let buf = [
      0x00, 0x08, 0x00, 0x00, // length 0x800
      0x05, 0x00, 0x00, 0x00, // logical block 5
      0x02, 0x00, // partition ref 2
      0, 0, 0, 0, 0, 0,
    ];
    let ad = read_long_ad(&buf);
    assert_eq!(ad.length(), 0x800);
    assert_eq!(ad.location.logical_block_number, 5);
    assert_eq!(ad.location.partition_reference_number, 2);
  }

  #[test]
  fn read_short_ad_extracts_position() {
    let buf = [0x00, 0x10, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00];
    let ad = read_short_ad(&buf);
    assert_eq!(ad.length(), 0x1000);
    assert_eq!(ad.position, 7);
  }

  // ---- parse_file_entry: each AD type + error branch -----------------

  /// Build a File Entry buffer with a chosen ICB AD type and body.
  /// `efe` selects EFE (266) vs FE (261). `file_type` 4 => directory.
  fn build_fe(efe: bool, file_type: u8, ad_type: u16, info_length: u64, body: &[u8]) -> Vec<u8> {
    let tid: u16 = if efe { TAG_EFE } else { TAG_FE };
    let (info_off, ea_off, ad_off, body_start) = if efe {
      (56usize, 208usize, 212usize, 216usize)
    } else {
      (56usize, 168usize, 172usize, 176usize)
    };
    let mut buf = vec![0u8; body_start + body.len() + 64];
    buf[0..2].copy_from_slice(&tid.to_le_bytes());
    buf[27] = file_type; // 16 + 11
    buf[34..36].copy_from_slice(&ad_type.to_le_bytes()); // ICB flags
    buf[info_off..info_off + 8].copy_from_slice(&info_length.to_le_bytes());
    buf[ea_off..ea_off + 4].copy_from_slice(&0u32.to_le_bytes()); // length_ea = 0
    buf[ad_off..ad_off + 4].copy_from_slice(&(body.len() as u32).to_le_bytes()); // length_ad
    buf[body_start..body_start + body.len()].copy_from_slice(body);
    buf
  }

  #[test]
  fn parse_file_entry_short_ads() {
    // Two short ADs followed by a zero-length terminator.
    let mut body = Vec::new();
    body.extend_from_slice(&100u32.to_le_bytes()); // len
    body.extend_from_slice(&3u32.to_le_bytes()); // position
    body.extend_from_slice(&200u32.to_le_bytes());
    body.extend_from_slice(&10u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes()); // zero-length -> stop
    body.extend_from_slice(&0u32.to_le_bytes());
    let buf = build_fe(false, 5, 0, 300, &body);
    let fe = parse_file_entry(&buf, 1).expect("parses");
    assert!(!fe.is_directory);
    assert_eq!(fe.size, 300);
    assert_eq!(fe.partition_reference, 1);
    assert_eq!(fe.allocation_descriptors.len(), 2);
    match &fe.allocation_descriptors[0] {
      AllocDesc::Short(s) => {
        assert_eq!(s.length(), 100);
        assert_eq!(s.position, 3);
      }
      _ => panic!("expected short ad"),
    }
  }

  #[test]
  fn parse_file_entry_long_ads_directory() {
    let mut body = Vec::new();
    // long ad #1
    body.extend_from_slice(&500u32.to_le_bytes()); // len
    body.extend_from_slice(&7u32.to_le_bytes()); // lbn
    body.extend_from_slice(&0u16.to_le_bytes()); // prn
    body.extend_from_slice(&[0u8; 6]); // implementation use
    // terminator (zero length)
    body.extend_from_slice(&[0u8; 16]);
    let buf = build_fe(true, 4, 1, 500, &body);
    let fe = parse_file_entry(&buf, 0).expect("parses");
    assert!(fe.is_directory, "file_type 4 => directory");
    assert_eq!(fe.allocation_descriptors.len(), 1);
    match &fe.allocation_descriptors[0] {
      AllocDesc::Long(l) => {
        assert_eq!(l.length(), 500);
        assert_eq!(l.location.logical_block_number, 7);
      }
      _ => panic!("expected long ad"),
    }
  }

  #[test]
  fn parse_file_entry_embedded_data() {
    let payload = b"embedded-file-content".to_vec();
    let buf = build_fe(false, 5, 3, payload.len() as u64, &payload);
    let fe = parse_file_entry(&buf, 0).expect("parses");
    assert_eq!(fe.size, payload.len() as u64);
    assert_eq!(fe.embedded_data.as_deref(), Some(&payload[..]));
    assert!(fe.allocation_descriptors.is_empty());
  }

  #[test]
  fn parse_file_entry_unknown_ad_type_is_ignored() {
    // AD type 2 (extended) is not handled -> no ADs, no embedded data.
    let buf = build_fe(false, 5, 2, 0, &[1, 2, 3, 4]);
    let fe = parse_file_entry(&buf, 0).expect("parses");
    assert!(fe.allocation_descriptors.is_empty());
    assert!(fe.embedded_data.is_none());
  }

  #[test]
  fn parse_file_entry_rejects_wrong_tag() {
    let mut buf = vec![0u8; 256];
    buf[0..2].copy_from_slice(&TAG_FSD.to_le_bytes()); // wrong tag
    let err = parse_file_entry(&buf, 0).unwrap_err();
    assert!(err.to_string().contains("expected FE/EFE"), "got {}", err);
  }

  // ---- parse_fids ----------------------------------------------------

  /// Build a single File Identifier Descriptor.
  fn build_fid(characteristics: u8, name: &str, lbn: u32) -> Vec<u8> {
    // d-string (compression 8) name.
    let mut fi: Vec<u8> = vec![8u8];
    fi.extend_from_slice(name.as_bytes());
    let l_fi = fi.len() as u8;
    let mut fid = vec![0u8; 38];
    fid[0..2].copy_from_slice(&TAG_FID.to_le_bytes());
    fid[18] = characteristics;
    fid[19] = l_fi;
    // icb long_ad at 20..36: length 0, lbn, prn 0.
    fid[24..28].copy_from_slice(&lbn.to_le_bytes());
    fid[36..38].copy_from_slice(&0u16.to_le_bytes()); // l_iu = 0
    fid.extend_from_slice(&fi);
    let total = fid.len();
    let padded = (total + 3) & !3;
    fid.resize(padded, 0);
    fid
  }

  #[test]
  fn parse_fids_lists_entries_with_flags() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&build_fid(0x08, "", 0)); // parent
    buf.extend_from_slice(&build_fid(0x02, "DIR", 5)); // directory
    buf.extend_from_slice(&build_fid(0x00, "file.bin", 9)); // normal file
    buf.extend_from_slice(&build_fid(0x01, "hidden.txt", 11)); // hidden
    buf.extend_from_slice(&build_fid(0x04, "gone.tmp", 13)); // deleted

    let entries = parse_fids(&buf).expect("parses");
    assert_eq!(entries.len(), 5);
    assert!(entries[0].is_parent);
    assert!(entries[1].is_directory && entries[1].name == "DIR");
    assert!(!entries[2].is_directory && entries[2].name == "file.bin");
    assert!(entries[3].is_hidden);
    assert!(entries[4].is_deleted);
    assert_eq!(entries[2].icb.location.logical_block_number, 9);
  }

  #[test]
  fn parse_fids_skips_padding_and_non_fid_tags() {
    // Leading zero tag should advance to the next sector boundary; with a
    // short buffer that ends the loop. A non-FID, non-zero tag likewise
    // jumps to the next sector.
    let mut buf = vec![0u8; 38]; // all-zero tag -> jump to sector boundary
    // Place a real FID at sector boundary (offset 2048).
    buf.resize(SECTOR_SIZE, 0);
    buf.extend_from_slice(&build_fid(0x00, "real.bin", 3));
    let entries = parse_fids(&buf).expect("parses");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "real.bin");

    // A buffer of only a non-FID tag short of 38 bytes yields nothing.
    let short = vec![0u8; 10];
    assert!(parse_fids(&short).unwrap().is_empty());
  }

  #[test]
  fn parse_fids_non_fid_tag_jumps_sector() {
    // A non-zero, non-FID tag at offset 0; valid FID at sector 1.
    let mut buf = vec![0u8; SECTOR_SIZE];
    buf[0..2].copy_from_slice(&TAG_FE.to_le_bytes()); // 261, not FID
    buf.extend_from_slice(&build_fid(0x00, "after.bin", 2));
    let entries = parse_fids(&buf).expect("parses");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "after.bin");
  }

  // ---- read_sector / read_run --------------------------------------

  #[test]
  fn read_sector_and_run_helpers() {
    let tmp = TempPath::new_file("sectors", "bin");
    // Two sectors: first 0xAA, second 0xBB.
    let mut data = vec![0xAAu8; SECTOR_SIZE];
    data.extend_from_slice(&vec![0xBBu8; SECTOR_SIZE]);
    std::fs::write(&tmp.path, &data).expect("write");
    let mut f = File::open(&tmp.path).expect("open");

    let sec0 = read_sector(&mut f, 0).expect("sector 0");
    assert!(sec0.iter().all(|&b| b == 0xAA));
    let sec1 = read_sector(&mut f, 1).expect("sector 1");
    assert!(sec1.iter().all(|&b| b == 0xBB));

    let run = read_run(&mut f, 0, SECTOR_SIZE + 4).expect("run");
    assert_eq!(run.len(), SECTOR_SIZE + 4);
    assert_eq!(run[0], 0xAA);
    assert_eq!(run[SECTOR_SIZE], 0xBB);

    // Reading past EOF errors.
    assert!(read_sector(&mut f, 99).is_err());
  }

  // ---- find_avdp candidate selection -------------------------------

  #[test]
  fn find_avdp_locates_tag_at_lba_256() {
    let tmp = TempPath::new_file("avdp", "bin");
    let total = SECTOR_SIZE * 300;
    let mut data = vec![0u8; total];
    // Place an AVDP tag at LBA 256.
    let off = 256 * SECTOR_SIZE;
    data[off..off + 2].copy_from_slice(&TAG_AVDP.to_le_bytes());
    std::fs::write(&tmp.path, &data).expect("write");
    let mut f = File::open(&tmp.path).expect("open");
    let sector = find_avdp(&mut f, total as u64).expect("found avdp");
    assert_eq!(u16::from_le_bytes([sector[0], sector[1]]), TAG_AVDP);
  }

  #[test]
  fn find_avdp_locates_tag_near_end() {
    let tmp = TempPath::new_file("avdp_end", "bin");
    // Small file: LBA 256 out of range; AVDP placed at last_lba - 1.
    let n_sectors = 10usize;
    let total = SECTOR_SIZE * n_sectors;
    let mut data = vec![0u8; total];
    let last_lba = n_sectors as u64; // file_size / SECTOR_SIZE
    let candidate = (last_lba - 1) as usize; // matches find_avdp's last_lba-1
    let off = candidate * SECTOR_SIZE;
    data[off..off + 2].copy_from_slice(&TAG_AVDP.to_le_bytes());
    std::fs::write(&tmp.path, &data).expect("write");
    let mut f = File::open(&tmp.path).expect("open");
    let sector = find_avdp(&mut f, total as u64).expect("found avdp near end");
    assert_eq!(u16::from_le_bytes([sector[0], sector[1]]), TAG_AVDP);
  }

  #[test]
  fn find_avdp_errors_when_absent() {
    let tmp = TempPath::new_file("avdp_none", "bin");
    let total = SECTOR_SIZE * 300;
    std::fs::write(&tmp.path, &vec![0u8; total]).expect("write");
    let mut f = File::open(&tmp.path).expect("open");
    assert!(find_avdp(&mut f, total as u64).is_err());
  }

  // =====================================================================
  // Hand-built UDF images. These give full control over the descriptor
  // layout so we can drive code paths a hdiutil image never produces:
  // Type-2 Metadata partition maps, the synthesized-map fallback, the FSD
  // tag-mismatch error, multi-PD / multi-LVD VolumeDescriptorSequenceNumber
  // selection, embedded-data files, and directory recursion with .ssif
  // skipping.
  //
  // The parser only inspects the descriptor *tag id* (LE u16 at offset 0)
  // and never validates checksums, so each sector just needs the right tag
  // and the fields the parser reads.
  // =====================================================================

  struct ImageBuilder {
    sectors: Vec<u8>,
  }

  impl ImageBuilder {
    fn new(n_sectors: usize) -> Self {
      ImageBuilder {
        sectors: vec![0u8; n_sectors * SECTOR_SIZE],
      }
    }

    fn sector_mut(&mut self, lba: usize) -> &mut [u8] {
      &mut self.sectors[lba * SECTOR_SIZE..(lba + 1) * SECTOR_SIZE]
    }

    /// Write raw bytes starting at an absolute byte offset.
    fn put(&mut self, lba: usize, offset: usize, bytes: &[u8]) {
      let base = lba * SECTOR_SIZE + offset;
      self.sectors[base..base + bytes.len()].copy_from_slice(bytes);
    }

    fn tag(&mut self, lba: usize, tag_id: u16) {
      self.put(lba, 0, &tag_id.to_le_bytes());
    }

    /// AVDP at LBA 256 pointing the main VDS at `vds_lba` for `vds_sectors`.
    fn avdp(&mut self, vds_lba: u32, vds_sectors: u32) {
      self.tag(256, TAG_AVDP);
      let len_bytes = (vds_sectors as usize * SECTOR_SIZE) as u32;
      self.put(256, 16, &len_bytes.to_le_bytes()); // main vds length
      self.put(256, 20, &vds_lba.to_le_bytes()); // main vds location
      // reserve vds left at 0 -> n_sectors 0 -> skipped.
    }

    /// Partition descriptor at `lba`.
    fn pd(&mut self, lba: usize, number: u16, starting_location: u32, vdsn: u32) {
      self.tag(lba, TAG_PD);
      self.put(lba, 16, &vdsn.to_le_bytes());
      self.put(lba, 22, &number.to_le_bytes());
      self.put(lba, 188, &starting_location.to_le_bytes());
    }

    /// Terminating descriptor.
    fn td(&mut self, lba: usize) {
      self.tag(lba, TAG_TD);
    }

    /// Logical Volume Descriptor at `lba`. `maps` are pre-built partition
    /// map byte blocks laid out starting at offset 440.
    fn lvd(&mut self, lba: usize, vdsn: u32, volume_label: &str, fsd: &LongAdSpec, maps: &[Vec<u8>]) {
      self.tag(lba, TAG_LVD);
      self.put(lba, 16, &vdsn.to_le_bytes());
      // Volume label d-string (compression 8) at offset 84.
      let mut dstr = vec![8u8];
      dstr.extend_from_slice(volume_label.as_bytes());
      self.put(lba, 84, &dstr);
      // FSD long_ad at offset 248.
      self.put(lba, 248, &fsd.bytes());
      // map_table_length @264, n_partition_maps @268.
      let mut map_bytes = Vec::new();
      for m in maps {
        map_bytes.extend_from_slice(m);
      }
      self.put(lba, 264, &(map_bytes.len() as u32).to_le_bytes());
      self.put(lba, 268, &(maps.len() as u32).to_le_bytes());
      self.put(lba, 440, &map_bytes);
    }

    /// File Set Descriptor at `lba` with the root-directory ICB at offset 400.
    fn fsd(&mut self, lba: usize, root_icb: &LongAdSpec) {
      self.tag(lba, TAG_FSD);
      self.put(lba, 400, &root_icb.bytes());
    }

    /// File Entry (FE) at `lba`. Short ADs only. `file_type` 4 => directory.
    fn fe_short(&mut self, lba: usize, file_type: u8, size: u64, ads: &[(u32, u32)]) {
      self.tag(lba, TAG_FE);
      self.put(lba, 27, &[file_type]);
      self.put(lba, 34, &0u16.to_le_bytes()); // icb flags -> ad_type 0 (short)
      self.put(lba, 56, &size.to_le_bytes()); // info length
      self.put(lba, 168, &0u32.to_le_bytes()); // length_ea
      // body at 176; build short ADs (len, position).
      let mut body = Vec::new();
      for (len, pos) in ads {
        body.extend_from_slice(&len.to_le_bytes());
        body.extend_from_slice(&pos.to_le_bytes());
      }
      self.put(lba, 172, &(body.len() as u32).to_le_bytes()); // length_ad
      self.put(lba, 176, &body);
    }

    /// File Entry with Long ADs. Each AD: (length_bytes, lbn, prn).
    fn fe_long(&mut self, lba: usize, file_type: u8, size: u64, ads: &[(u32, u32, u16)]) {
      self.tag(lba, TAG_FE);
      self.put(lba, 27, &[file_type]);
      self.put(lba, 34, &1u16.to_le_bytes()); // icb flags -> ad_type 1 (long)
      self.put(lba, 56, &size.to_le_bytes());
      self.put(lba, 168, &0u32.to_le_bytes());
      let mut body = Vec::new();
      for (len, lbn, prn) in ads {
        body.extend_from_slice(
          &LongAdSpec {
            length: *len,
            lbn: *lbn,
            prn: *prn,
          }
          .bytes(),
        );
      }
      self.put(lba, 172, &(body.len() as u32).to_le_bytes());
      self.put(lba, 176, &body);
    }

    fn write_temp(&self) -> TempPath {
      let tmp = TempPath::new_file("handbuilt", "iso");
      std::fs::write(&tmp.path, &self.sectors).expect("write image");
      tmp
    }
  }

  struct LongAdSpec {
    length: u32,
    lbn: u32,
    prn: u16,
  }

  impl LongAdSpec {
    fn bytes(&self) -> [u8; 16] {
      let mut b = [0u8; 16];
      b[0..4].copy_from_slice(&self.length.to_le_bytes());
      b[4..8].copy_from_slice(&self.lbn.to_le_bytes());
      b[8..10].copy_from_slice(&self.prn.to_le_bytes());
      b
    }
  }

  /// A Type-1 partition map block (6 bytes): type=1, length=6, then 2 bytes
  /// reserved + 2-byte partition number at offset 4.
  fn type1_map(partition_number: u16) -> Vec<u8> {
    let mut m = vec![0u8; 6];
    m[0] = 1;
    m[1] = 6;
    m[4..6].copy_from_slice(&partition_number.to_le_bytes());
    m
  }

  /// A Type-2 map block (64 bytes). `ident` is placed at offset 5..; the
  /// underlying partition number at offset 38; metadata file LBA at offset 40.
  fn type2_map(ident: &str, underlying_partition: u16, metadata_file_lba: u32) -> Vec<u8> {
    let mut m = vec![0u8; 64];
    m[0] = 2;
    m[1] = 64;
    // EntityID identifier string starts at offset 4+1 = 5.
    let id = ident.as_bytes();
    m[5..5 + id.len()].copy_from_slice(id);
    m[38..40].copy_from_slice(&underlying_partition.to_le_bytes());
    m[40..44].copy_from_slice(&metadata_file_lba.to_le_bytes());
    m
  }

  /// Layout helper to write a directory's FID stream (one sector) into the
  /// image at `lba`. Each entry: (characteristics, name, icb_lbn).
  fn write_dir_sector(b: &mut ImageBuilder, lba: usize, entries: &[(u8, &str, u32)]) {
    let mut stream = Vec::new();
    for (ch, name, icb_lbn) in entries {
      stream.extend_from_slice(&build_fid(*ch, name, *icb_lbn));
    }
    assert!(stream.len() <= SECTOR_SIZE, "dir stream exceeds one sector");
    let dst = b.sector_mut(lba);
    dst[..stream.len()].copy_from_slice(&stream);
  }

  /// Build a complete Type-1 image with this directory tree:
  ///   /              (root)
  ///     FILE.BIN     (data file, multi-sector)
  ///     MOVIE.SSIF   (data file, skipped by directory_size)
  ///     SUB/         (subdir)
  ///       INNER.BIN
  /// Returns (TempPath, expected_file_bytes, expected_inner_bytes).
  fn build_type1_image() -> (TempPath, Vec<u8>, Vec<u8>) {
    // Sector map (LBAs):
    //   256 AVDP
    //   257 PD (vdsn 0), 258 PD (vdsn 1, wins), 259 LVD (vdsn 0),
    //   260 LVD (vdsn 1, wins), 261 TD
    //   partition start = 280
    //   rel 0 (280) FSD
    //   rel 1 (281) root FE -> data rel 2
    //   rel 2 (282) root dir FIDs
    //   rel 3 (283) FILE.BIN FE -> data rel 4..6 (2 sectors)
    //   rel 4..5 (284..285) FILE.BIN data
    //   rel 6 (286) MOVIE.SSIF FE -> data rel 7
    //   rel 7 (287) MOVIE.SSIF data
    //   rel 8 (288) SUB FE -> data rel 9
    //   rel 9 (289) SUB dir FIDs
    //   rel 10 (290) INNER.BIN FE -> data rel 11
    //   rel 11 (291) INNER.BIN data
    const PSTART: u32 = 280;
    let mut b = ImageBuilder::new((PSTART as usize) + 16);
    b.avdp(257, 5); // main VDS at 257, 5 sectors (257..262)
    b.pd(257, 0, PSTART, 0);
    b.pd(258, 0, PSTART, 1); // higher vdsn wins (covers vdsn>=entry.vdsn branch)
    b.lvd(
      259,
      0,
      "OLDLABEL",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type1_map(0)],
    );
    b.lvd(
      260,
      1, // higher vdsn -> this LVD wins
      "HANDBUILT",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      }, // FSD at partition rel 0
      &[type1_map(0)],
    );
    b.td(261);

    let p = |rel: u32| (PSTART + rel) as usize;

    // FSD at rel 0 -> root ICB at rel 1.
    b.fsd(
      p(0),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    );
    // root FE (directory) -> dir data at rel 2.
    b.fe_short(p(1), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    // root dir FIDs: parent, FILE.BIN(rel3), MOVIE.SSIF(rel6), SUB(rel8).
    write_dir_sector(
      &mut b,
      p(2),
      &[
        (0x08, "", 1),           // parent
        (0x00, "FILE.BIN", 3),   // file
        (0x00, "MOVIE.SSIF", 6), // ssif file
        (0x02, "SUB", 8),        // subdir
      ],
    );

    // FILE.BIN: 2 sectors of data (covers multi-sector read run).
    let file_bytes: Vec<u8> = (0..(SECTOR_SIZE + 500)).map(|i| (i % 251) as u8).collect();
    b.fe_short(p(3), 5, file_bytes.len() as u64, &[(file_bytes.len() as u32, 4)]);
    b.put(p(4), 0, &file_bytes);

    // MOVIE.SSIF: one sector payload.
    let ssif_bytes = vec![0x5Au8; 600];
    b.fe_short(p(6), 5, ssif_bytes.len() as u64, &[(ssif_bytes.len() as u32, 7)]);
    b.put(p(7), 0, &ssif_bytes);

    // SUB directory -> dir data at rel 9.
    b.fe_short(p(8), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 9)]);
    write_dir_sector(&mut b, p(9), &[(0x08, "", 8), (0x00, "INNER.BIN", 10)]);

    // INNER.BIN: small file.
    let inner_bytes = b"inner-data-1234567890".to_vec();
    b.fe_short(p(10), 5, inner_bytes.len() as u64, &[(inner_bytes.len() as u32, 11)]);
    b.put(p(11), 0, &inner_bytes);

    (b.write_temp(), file_bytes, inner_bytes)
  }

  #[test]
  fn handbuilt_type1_image_full_navigation() {
    let (tmp, file_bytes, inner_bytes) = build_type1_image();
    let mut img = UdfImage::open(&tmp.path).expect("opens hand-built image");

    // Latest LVD (vdsn 1) won -> "HANDBUILT", not "OLDLABEL".
    assert_eq!(img.volume_label, "HANDBUILT");
    assert!(img.root.is_directory);

    // List root.
    let root = img.root.clone();
    let entries = img.list_dir(&root).expect("root list");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"FILE.BIN"));
    assert!(names.contains(&"MOVIE.SSIF"));
    assert!(names.contains(&"SUB"));

    // Read FILE.BIN exactly (multi-sector run via short AD).
    let file_fe = img.resolve("FILE.BIN").expect("FILE.BIN resolves");
    assert_eq!(file_fe.size, file_bytes.len() as u64);
    assert_eq!(img.read_file(&file_fe).unwrap(), file_bytes);

    // Nested resolution.
    let inner = img.resolve("SUB/INNER.BIN").expect("nested resolves");
    assert_eq!(img.read_file(&inner).unwrap(), inner_bytes);

    // directory_size: counts FILE.BIN + INNER.BIN but skips MOVIE.SSIF.
    let root = img.root.clone();
    let total = img.directory_size(&root).expect("dir size");
    assert_eq!(total, file_bytes.len() as u64 + inner_bytes.len() as u64);

    // Streaming reader over a multi-sector short-AD file.
    let image = Arc::new(Mutex::new(img));
    let fe = {
      let mut g = image.lock().unwrap();
      g.resolve("FILE.BIN").unwrap()
    };
    let mut reader = UdfFileReader::new(image, &fe).unwrap();
    let mut out = Vec::new();
    let mut chunk = [0u8; 333];
    loop {
      let n = reader.read(&mut chunk).unwrap();
      if n == 0 {
        break;
      }
      out.extend_from_slice(&chunk[..n]);
    }
    assert_eq!(out, file_bytes);
  }

  #[test]
  fn handbuilt_fsd_tag_mismatch_errors() {
    // Same as the Type-1 image but the FSD sector carries the wrong tag.
    let (tmp, _f, _i) = build_type1_image();
    // Corrupt the FSD tag at partition rel 0 (LBA 280).
    let mut data = std::fs::read(&tmp.path).unwrap();
    let fsd_off = 280 * SECTOR_SIZE;
    data[fsd_off..fsd_off + 2].copy_from_slice(&TAG_FE.to_le_bytes()); // not FSD
    let tmp2 = TempPath::new_file("badfsd", "iso");
    std::fs::write(&tmp2.path, &data).unwrap();
    let err = match UdfImage::open(&tmp2.path) {
      Ok(_) => panic!("expected FSD tag mismatch error"),
      Err(e) => e,
    };
    assert!(err.to_string().contains("expected FSD"), "got {}", err);
  }

  #[test]
  fn handbuilt_lower_vdsn_descriptors_are_ignored() {
    // First PD/LVD carry the higher VolumeDescriptorSequenceNumber; the
    // later ones are lower and must be ignored (the !(vdsn >= entry.vdsn)
    // and !(vdsn >= latest) branches).
    const PSTART: u32 = 280;
    const STALE: u32 = 999; // a wrong starting_location for the stale PD
    let mut b = ImageBuilder::new((PSTART as usize) + 8);
    b.avdp(257, 6); // VDS sectors 257..263
    // PD vdsn 5 (correct), then PD vdsn 1 (stale -> ignored).
    b.pd(257, 0, PSTART, 5);
    b.pd(258, 0, STALE, 1);
    // LVD vdsn 5 (correct label), then LVD vdsn 1 (stale label -> ignored).
    b.lvd(
      259,
      5,
      "WINNER",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type1_map(0)],
    );
    b.lvd(
      260,
      1,
      "STALE",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type1_map(0)],
    );
    b.td(261);
    let p = |rel: u32| (PSTART + rel) as usize;
    b.fsd(
      p(0),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    );
    b.fe_short(p(1), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    write_dir_sector(&mut b, p(2), &[(0x08, "", 1), (0x00, "W.BIN", 3)]);
    let payload = b"winner-payload".to_vec();
    b.fe_short(p(3), 5, payload.len() as u64, &[(payload.len() as u32, 4)]);
    b.put(p(4), 0, &payload);

    let tmp = b.write_temp();
    let mut img = UdfImage::open(&tmp.path).expect("opens");
    // The higher-vdsn LVD won.
    assert_eq!(img.volume_label, "WINNER");
    // The higher-vdsn PD's starting_location (PSTART) was kept, so the file
    // resolves correctly (a stale start would point at the wrong sectors).
    let fe = img.resolve("W.BIN").expect("W.BIN resolves with correct PD");
    assert_eq!(img.read_file(&fe).unwrap(), payload);
  }

  #[test]
  fn handbuilt_bad_map_type_stops_parsing() {
    // A partition map with an unrecognized type byte breaks the map loop
    // (covering the `else => break` arm). With no maps parsed the fallback
    // synthesizes a Type-1 map from the single partition.
    const PSTART: u32 = 280;
    let mut b = ImageBuilder::new((PSTART as usize) + 8);
    b.avdp(257, 4);
    b.pd(257, 0, PSTART, 0);
    // Map type 7 is unknown -> break. n_partition_maps says 1.
    let mut bad_map = vec![0u8; 8];
    bad_map[0] = 7;
    bad_map[1] = 8;
    b.lvd(
      258,
      0,
      "BADMAP",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[bad_map],
    );
    b.td(259);
    let p = |rel: u32| (PSTART + rel) as usize;
    b.fsd(
      p(0),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    );
    b.fe_short(p(1), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    write_dir_sector(&mut b, p(2), &[(0x08, "", 1), (0x00, "B.BIN", 3)]);
    let payload = b"badmap-payload".to_vec();
    b.fe_short(p(3), 5, payload.len() as u64, &[(payload.len() as u32, 4)]);
    b.put(p(4), 0, &payload);

    let tmp = b.write_temp();
    let mut img = UdfImage::open(&tmp.path).expect("opens despite bad map (synthesized)");
    let fe = img.resolve("B.BIN").expect("resolves via synthesized map");
    assert_eq!(img.read_file(&fe).unwrap(), payload);
  }

  #[test]
  fn handbuilt_no_partition_descriptor_errors() {
    // VDS with an LVD but no PD -> "no Partition Descriptor".
    let mut b = ImageBuilder::new(270);
    b.avdp(257, 3);
    b.lvd(
      257,
      0,
      "X",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type1_map(0)],
    );
    b.td(258);
    let tmp = b.write_temp();
    let err = match UdfImage::open(&tmp.path) {
      Ok(_) => panic!("expected no-PD error"),
      Err(e) => e,
    };
    assert!(err.to_string().contains("no Partition Descriptor"), "got {}", err);
  }

  #[test]
  fn handbuilt_no_lvd_errors() {
    // VDS with a PD but no LVD -> "no Logical Volume Descriptor".
    let mut b = ImageBuilder::new(290);
    b.avdp(257, 3);
    b.pd(257, 0, 280, 0);
    b.td(258);
    let tmp = b.write_temp();
    let err = match UdfImage::open(&tmp.path) {
      Ok(_) => panic!("expected no-LVD error"),
      Err(e) => e,
    };
    assert!(err.to_string().contains("no Logical Volume Descriptor"), "got {}", err);
  }

  /// Build a Type-1 image but declare 0 partition maps, forcing the
  /// synthesize-one-Type1-map fallback.
  #[test]
  fn handbuilt_zero_maps_synthesizes_type1() {
    const PSTART: u32 = 280;
    let mut b = ImageBuilder::new((PSTART as usize) + 8);
    b.avdp(257, 4);
    b.pd(257, 0, PSTART, 0);
    // LVD with zero maps.
    b.lvd(
      258,
      0,
      "SYNTH",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[],
    );
    b.td(259);
    let p = |rel: u32| (PSTART + rel) as usize;
    b.fsd(
      p(0),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    );
    b.fe_short(p(1), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    write_dir_sector(&mut b, p(2), &[(0x08, "", 1), (0x00, "A.BIN", 3)]);
    let payload = b"synth-map-payload".to_vec();
    b.fe_short(p(3), 5, payload.len() as u64, &[(payload.len() as u32, 4)]);
    b.put(p(4), 0, &payload);

    let tmp = b.write_temp();
    let mut img = UdfImage::open(&tmp.path).expect("opens with synthesized map");
    assert_eq!(img.volume_label, "SYNTH");
    let fe = img.resolve("A.BIN").expect("A.BIN resolves via synthesized map");
    assert_eq!(img.read_file(&fe).unwrap(), payload);
  }

  /// Build a Type-2 "Sparable"-style (non-metadata) map -> Other fallback.
  #[test]
  fn handbuilt_type2_other_map_falls_back_to_direct() {
    const PSTART: u32 = 280;
    let mut b = ImageBuilder::new((PSTART as usize) + 8);
    b.avdp(257, 4);
    b.pd(257, 0, PSTART, 0);
    b.lvd(
      258,
      0,
      "OTHERMAP",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type2_map("*UDF Sparable Partition", 0, 0)],
    );
    b.td(259);
    let p = |rel: u32| (PSTART + rel) as usize;
    b.fsd(
      p(0),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    );
    b.fe_short(p(1), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    write_dir_sector(&mut b, p(2), &[(0x08, "", 1), (0x00, "S.BIN", 3)]);
    let payload = b"sparable-direct".to_vec();
    b.fe_short(p(3), 5, payload.len() as u64, &[(payload.len() as u32, 4)]);
    b.put(p(4), 0, &payload);

    let tmp = b.write_temp();
    let mut img = UdfImage::open(&tmp.path).expect("opens with Other map");
    let fe = img.resolve("S.BIN").expect("resolves through Other map");
    assert_eq!(img.read_file(&fe).unwrap(), payload);
  }

  /// Build a Type-2 *Metadata* partition image: the FSD, root dir and files
  /// live "inside" the metadata file, whose physical extents are declared by
  /// the metadata-file FE. Exercises the Metadata arm of open(),
  /// resolve_phys_lba, and resolve_phys_byte_run.
  #[test]
  fn handbuilt_metadata_partition_image() {
    // Physical layout:
    //   256 AVDP, 257 PD(start=PSTART), 258 LVD(metadata map), 259 TD
    //   PSTART + 0  : metadata-file FE (points at the metadata extent)
    //   PSTART + 1  : (gap)
    //   META_EXT (PSTART+2) start of the metadata partition's logical
    //                space. Logical block 0 of the metadata partition maps
    //                here.
    //     logical 0 : FSD            (META_EXT + 0)
    //     logical 1 : root FE        (META_EXT + 1)
    //     logical 2 : root dir FIDs  (META_EXT + 2)
    //     logical 3 : file FE        (META_EXT + 3)
    //     logical 4 : file data      (META_EXT + 4)
    const PSTART: u32 = 280;
    const META_REL: u32 = 2; // metadata extent starts at partition rel 2
    const META_SECTORS: u32 = 6;
    let mut b = ImageBuilder::new((PSTART as usize) + (META_REL as usize) + 16);
    b.avdp(257, 4);
    b.pd(257, 0, PSTART, 0);
    // Metadata file FE lives at partition rel 0 (metadata_file_lba = 0).
    b.lvd(
      258,
      0,
      "METADISC",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      }, // FSD at metadata logical 0
      &[type2_map("*UDF Metadata Partition", 0, 0)],
    );
    b.td(259);

    let phys = |rel: u32| (PSTART + rel) as usize;
    // The metadata file FE: a single short AD covering META_SECTORS
    // sectors starting at partition-relative position META_REL.
    b.fe_short(
      phys(0),
      5,
      (META_SECTORS as usize * SECTOR_SIZE) as u64,
      &[(META_SECTORS * SECTOR_SIZE as u32, META_REL)],
    );

    // Now lay out the metadata partition's logical content at physical
    // META_REL onward. logical L -> physical (PSTART + META_REL + L).
    let mlog = |l: u32| (PSTART + META_REL + l) as usize;
    b.fsd(
      mlog(0),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    );
    // root FE -> dir data at metadata logical block 2. The short AD
    // `position` is a logical block number (resolve_phys_byte_run turns it
    // into a byte offset within the metadata file), so it must be 2.
    b.fe_short(mlog(1), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    write_dir_sector(&mut b, mlog(2), &[(0x08, "", 1), (0x00, "M.BIN", 3)]);
    // file FE (logical 3) -> data at logical 4. Position in short AD is a
    // byte... no: short AD `position` is a logical block number for the
    // partition. For the metadata partition resolve, lbn is converted to a
    // byte offset within the metadata file. So position = logical block 4.
    let payload: Vec<u8> = (0..1000).map(|i| (i % 97) as u8).collect();
    b.fe_short(mlog(3), 5, payload.len() as u64, &[(payload.len() as u32, 4)]);
    b.put(mlog(4), 0, &payload);

    let tmp = b.write_temp();
    let mut img = UdfImage::open(&tmp.path).expect("opens metadata-partition image");
    assert_eq!(img.volume_label, "METADISC");
    assert!(img.root.is_directory);
    let entries = {
      let root = img.root.clone();
      img.list_dir(&root).expect("root listing")
    };
    assert!(entries.iter().any(|e| e.name == "M.BIN"));
    let fe = img.resolve("M.BIN").expect("M.BIN resolves through metadata map");
    assert_eq!(fe.size, payload.len() as u64);
    // read_file -> resolve_phys_byte_run (Metadata arm).
    assert_eq!(img.read_file(&fe).unwrap(), payload);

    // Streaming reader also walks the metadata extents.
    let image = Arc::new(Mutex::new(img));
    let fe2 = {
      let mut g = image.lock().unwrap();
      g.resolve("M.BIN").unwrap()
    };
    let mut reader = UdfFileReader::new(image, &fe2).unwrap();
    let mut out = Vec::new();
    let mut chunk = [0u8; 128];
    loop {
      let n = reader.read(&mut chunk).unwrap();
      if n == 0 {
        break;
      }
      out.extend_from_slice(&chunk[..n]);
    }
    assert_eq!(out, payload);
  }

  /// Metadata partition whose metadata file is split across two
  /// non-contiguous physical extents. Drives the extent-skip and
  /// extent-boundary logic in both resolve_phys_lba and resolve_phys_byte_run.
  #[test]
  fn handbuilt_metadata_two_extents() {
    const PSTART: u32 = 280;
    // Two extents: extent1 = partition rel 2..5 (3 sectors),
    //              extent2 = partition rel 10..13 (3 sectors).
    // Metadata logical blocks: 0,1,2 -> phys 282,283,284 (extent1);
    //                          3,4,5 -> phys 290,291,292 (extent2).
    let mut b = ImageBuilder::new((PSTART as usize) + 16);
    b.avdp(257, 4);
    b.pd(257, 0, PSTART, 0);
    b.lvd(
      258,
      0,
      "TWOEXT",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type2_map("*UDF Metadata Partition", 0, 0)],
    );
    b.td(259);

    let phys = |rel: u32| (PSTART + rel) as usize;
    // Metadata file FE at partition rel 0 with two short ADs.
    let ext_bytes = 3 * SECTOR_SIZE as u32;
    b.fe_short(phys(0), 5, (2 * ext_bytes) as u64, &[(ext_bytes, 2), (ext_bytes, 10)]);

    // Metadata logical layout.
    // extent1 (logical 0,1,2) at phys 282,283,284.
    b.fsd(
      phys(2),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    ); // logical 0
    // root FE (logical 1) -> dir data logical 2.
    b.fe_short(phys(3), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    write_dir_sector(&mut b, phys(4), &[(0x08, "", 1), (0x00, "T.BIN", 3)]); // logical 2
    // extent2 (logical 3,4,5) at phys 290,291,292.
    // file FE (logical 3) -> data spanning logical 4..5 (4096 bytes).
    let payload: Vec<u8> = (0..(2 * SECTOR_SIZE)).map(|i| (i % 131) as u8).collect();
    b.fe_short(phys(10), 5, payload.len() as u64, &[(payload.len() as u32, 4)]); // logical 3
    b.put(phys(11), 0, &payload[..SECTOR_SIZE]); // logical 4
    b.put(phys(12), 0, &payload[SECTOR_SIZE..]); // logical 5

    let tmp = b.write_temp();
    let mut img = UdfImage::open(&tmp.path).expect("opens two-extent metadata image");
    assert_eq!(img.volume_label, "TWOEXT");
    let fe = img.resolve("T.BIN").expect("T.BIN resolves across extents");
    assert_eq!(fe.size, payload.len() as u64);
    assert_eq!(img.read_file(&fe).unwrap(), payload);

    // resolve_phys_lba for a logical block in the second extent: must skip
    // extent1 then match extent2.
    let phys_l4 = img.resolve_phys_lba(0, 4).expect("logical 4 resolves");
    assert_eq!(phys_l4, phys(11) as u64);

    // A logical block beyond the metadata file size errors.
    assert!(img.resolve_phys_lba(0, 6).is_err());
    assert!(img.resolve_phys_byte_run(0, 6, 10).is_err());
  }

  #[test]
  fn handbuilt_metadata_unknown_partition_errors() {
    // Type-2 Metadata map referencing an underlying partition that has no
    // PD -> the "metadata partition references unknown partition" error.
    const PSTART: u32 = 280;
    let mut b = ImageBuilder::new((PSTART as usize) + 8);
    b.avdp(257, 4);
    b.pd(257, 0, PSTART, 0); // partition number 0
    b.lvd(
      258,
      0,
      "BADMETA",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type2_map("*UDF Metadata Partition", 9 /* unknown */, 0)],
    );
    b.td(259);
    let tmp = b.write_temp();
    let err = match UdfImage::open(&tmp.path) {
      Ok(_) => panic!("expected unknown-partition error"),
      Err(e) => e,
    };
    assert!(err.to_string().contains("references unknown partition"), "got {}", err);
  }

  /// Build a Type-1 image whose single file uses two Long ADs pointing at
  /// two separate one-sector extents. Covers the Long-AD arms of read_file
  /// and UdfFileReader::new plus the multi-run advance in the reader.
  #[test]
  fn handbuilt_long_ad_multi_extent_file() {
    const PSTART: u32 = 280;
    let mut b = ImageBuilder::new((PSTART as usize) + 12);
    b.avdp(257, 4);
    b.pd(257, 0, PSTART, 0);
    b.lvd(
      258,
      0,
      "LONGAD",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type1_map(0)],
    );
    b.td(259);
    let p = |rel: u32| (PSTART + rel) as usize;
    b.fsd(
      p(0),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    );
    b.fe_short(p(1), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    write_dir_sector(&mut b, p(2), &[(0x08, "", 1), (0x00, "BIG.BIN", 3)]);

    // BIG.BIN: total 3000 bytes split across two extents (rel 4 and rel 6).
    let part_a: Vec<u8> = (0..1500u32).map(|i| (i % 200) as u8).collect();
    let part_b: Vec<u8> = (0..1500u32).map(|i| ((i + 7) % 211) as u8).collect();
    let mut expected = part_a.clone();
    expected.extend_from_slice(&part_b);

    b.fe_long(
      p(3),
      5,
      expected.len() as u64,
      &[
        (part_a.len() as u32, 4, 0), // extent A at partition rel 4
        (part_b.len() as u32, 6, 0), // extent B at partition rel 6
      ],
    );
    b.put(p(4), 0, &part_a);
    b.put(p(6), 0, &part_b);

    let tmp = b.write_temp();
    let mut img = UdfImage::open(&tmp.path).expect("opens long-AD image");
    let fe = img.resolve("BIG.BIN").expect("BIG.BIN resolves");
    assert_eq!(fe.size, expected.len() as u64);
    assert_eq!(img.read_file(&fe).unwrap(), expected);

    // Stream it: the reader holds two runs, so the run-advance path runs.
    let image = Arc::new(Mutex::new(img));
    let fe2 = {
      let mut g = image.lock().unwrap();
      g.resolve("BIG.BIN").unwrap()
    };
    let mut reader = UdfFileReader::new(image, &fe2).unwrap();
    let mut out = Vec::new();
    let mut chunk = [0u8; 1000];
    loop {
      let n = reader.read(&mut chunk).unwrap();
      if n == 0 {
        break;
      }
      out.extend_from_slice(&chunk[..n]);
    }
    assert_eq!(out, expected);
  }

  #[test]
  fn udf_file_reader_size_exceeds_runs_returns_zero() {
    // A UdfFile whose declared size is larger than its allocation extent.
    // The reader yields the run bytes, then returns Ok(0) once runs are
    // exhausted even though total_remaining is still > 0.
    let (tmp, _f, _i) = build_type1_image();
    let img = UdfImage::open(&tmp.path).expect("opens");
    // Use a real short AD that resolves (partition rel 4, the FILE.BIN data
    // region from build_type1_image), but claim a size much larger.
    let fe = UdfFile {
      size: 1_000_000, // far more than one sector
      is_directory: false,
      embedded_data: None,
      allocation_descriptors: vec![AllocDesc::Short(ShortAd {
        length_and_type: 100, // only 100 bytes of run
        position: 4,
      })],
      partition_reference: 0,
    };
    let image = Arc::new(Mutex::new(img));
    let mut reader = UdfFileReader::new(image, &fe).unwrap();
    let mut buf = [0u8; 4096];
    let mut total = 0usize;
    loop {
      let n = reader.read(&mut buf).unwrap();
      if n == 0 {
        break;
      }
      total += n;
    }
    // We only had 100 bytes of run; the reader stops there.
    assert_eq!(total, 100);
  }

  // ---- parse_fids truncation edge -----------------------------------

  #[test]
  fn parse_fids_truncated_name_breaks() {
    // A FID claims l_fi larger than the remaining buffer -> break.
    let mut fid = vec![0u8; 38];
    fid[0..2].copy_from_slice(&TAG_FID.to_le_bytes());
    fid[18] = 0x00; // characteristics
    fid[19] = 50; // l_fi = 50 but no name bytes follow
    fid[36..38].copy_from_slice(&0u16.to_le_bytes()); // l_iu = 0
    let entries = parse_fids(&fid).expect("parses");
    assert!(entries.is_empty(), "truncated FID name should yield nothing");
  }

  // ---- resolve_phys_lba / resolve_phys_byte_run direct error paths ----

  #[test]
  fn resolve_phys_lba_out_of_range_prn() {
    let (tmp, _f, _i) = build_type1_image();
    let img = UdfImage::open(&tmp.path).expect("opens");
    // prn 5 has no map.
    assert!(img.resolve_phys_lba(5, 0).is_err());
    assert!(img.resolve_phys_byte_run(5, 0, 10).is_err());
  }

  #[test]
  fn read_file_entry_zero_length_icb_reads_one_sector() {
    // icb.length() == 0 -> to_read = SECTOR_SIZE branch in read_file_entry.
    let (tmp, _f, _i) = build_type1_image();
    let mut img = UdfImage::open(&tmp.path).expect("opens");
    // Root ICB is at partition rel 1 with length 0 in the FSD, so the root
    // read already exercised that branch; re-read it explicitly here.
    let icb = LongAd {
      length_and_type: 0, // length 0
      location: LbAddr {
        logical_block_number: 1,
        partition_reference_number: 0,
      },
    };
    let fe = img.read_file_entry(&icb).expect("reads root FE with zero-length icb");
    assert!(fe.is_directory);
  }

  #[test]
  fn directory_size_on_non_directory_returns_file_size() {
    // directory_size called on a plain file short-circuits to fe.size.
    let (tmp, _f, _i) = build_type1_image();
    let mut img = UdfImage::open(&tmp.path).expect("opens");
    let file_fe = img.resolve("FILE.BIN").expect("FILE.BIN resolves");
    assert!(!file_fe.is_directory);
    let size = img.directory_size(&file_fe).expect("size");
    assert_eq!(size, file_fe.size);
  }

  #[test]
  fn directory_size_breaks_cycles() {
    // A subdir whose FID points back at the root ICB forms a cycle; the
    // visited-set guard stops re-descending.
    const PSTART: u32 = 280;
    let mut b = ImageBuilder::new((PSTART as usize) + 12);
    b.avdp(257, 4);
    b.pd(257, 0, PSTART, 0);
    b.lvd(
      258,
      0,
      "CYCLE",
      &LongAdSpec {
        length: 0,
        lbn: 0,
        prn: 0,
      },
      &[type1_map(0)],
    );
    b.td(259);
    let p = |rel: u32| (PSTART + rel) as usize;
    // FSD -> root ICB at logical block 1.
    b.fsd(
      p(0),
      &LongAdSpec {
        length: 0,
        lbn: 1,
        prn: 0,
      },
    );
    // root FE (lbn 1) -> dir data at rel 2.
    b.fe_short(p(1), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 2)]);
    // root dir: parent + SUB (subdir, ICB lbn 3) + LEAF.BIN (ICB lbn 5).
    write_dir_sector(&mut b, p(2), &[(0x08, "", 1), (0x02, "SUB", 3), (0x00, "LEAF.BIN", 5)]);
    // SUB FE (lbn 3) -> dir data at rel 4.
    b.fe_short(p(3), 4, SECTOR_SIZE as u64, &[(SECTOR_SIZE as u32, 4)]);
    // SUB dir: parent + SELF pointing at SUB's own ICB (lbn 3) -> self
    // cycle, caught by the visited-set guard on the second encounter.
    write_dir_sector(&mut b, p(4), &[(0x08, "", 3), (0x02, "SELF", 3)]);
    // LEAF.BIN data at rel 6.
    let leaf = b"leaf-bytes".to_vec();
    b.fe_short(p(5), 5, leaf.len() as u64, &[(leaf.len() as u32, 6)]);
    b.put(p(6), 0, &leaf);

    let tmp = b.write_temp();
    let mut img = UdfImage::open(&tmp.path).expect("opens cyclic image");
    let root = img.root.clone();
    // Should terminate (cycle broken) and count LEAF.BIN once.
    let total = img.directory_size(&root).expect("size terminates");
    assert_eq!(total, leaf.len() as u64);
  }

  #[test]
  fn read_file_embedded_data_truncates_to_size() {
    // read_file with an embedded-data FE whose stored buffer is larger than
    // `size` should truncate to `size`.
    let fe = UdfFile {
      size: 4,
      is_directory: false,
      embedded_data: Some(vec![1, 2, 3, 4, 5, 6, 7, 8]),
      allocation_descriptors: Vec::new(),
      partition_reference: 0,
    };
    // We need an image to call read_file; reuse a hand-built one.
    let (tmp, _f, _i) = build_type1_image();
    let mut img = UdfImage::open(&tmp.path).expect("opens");
    let out = img.read_file(&fe).expect("reads embedded");
    assert_eq!(out, vec![1, 2, 3, 4]);

    let image = Arc::new(Mutex::new(img));
    let mut reader = UdfFileReader::new(image, &fe).expect("streaming reader");
    let mut streamed = Vec::new();
    reader.read_to_end(&mut streamed).expect("streams embedded");
    assert_eq!(streamed, vec![1, 2, 3, 4]);
  }
}
