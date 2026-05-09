/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
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

use anyhow::{anyhow, Result};
use std::collections::HashMap;
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
    let candidates: Vec<u64> = vec![
        256,
        last_lba.saturating_sub(1),
        last_lba.saturating_sub(257),
    ];
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

        let main_vds_length =
            u32::from_le_bytes([avdp[16], avdp[17], avdp[18], avdp[19]]) as usize;
        let main_vds_location =
            u32::from_le_bytes([avdp[20], avdp[21], avdp[22], avdp[23]]) as u64;
        let reserve_vds_length =
            u32::from_le_bytes([avdp[24], avdp[25], avdp[26], avdp[27]]) as usize;
        let reserve_vds_location =
            u32::from_le_bytes([avdp[28], avdp[29], avdp[30], avdp[31]]) as u64;

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
                        let vdsn = u32::from_le_bytes([
                            sector[16], sector[17], sector[18], sector[19],
                        ]);
                        let number =
                            u16::from_le_bytes([sector[22], sector[23]]);
                        let starting_location = u32::from_le_bytes([
                            sector[188], sector[189], sector[190], sector[191],
                        ]);
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
                        let vdsn = u32::from_le_bytes([
                            sector[16], sector[17], sector[18], sector[19],
                        ]);
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
        let map_table_length = u32::from_le_bytes([
            lvd_sector[264], lvd_sector[265], lvd_sector[266], lvd_sector[267],
        ]) as usize;
        let n_partition_maps = u32::from_le_bytes([
            lvd_sector[268], lvd_sector[269], lvd_sector[270], lvd_sector[271],
        ]) as usize;

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
                let partition_number =
                    u16::from_le_bytes([lvd_sector[p + 4], lvd_sector[p + 5]]);
                partition_maps.push(PartitionMap::Type1 { partition_number });
            } else if map_type == 2 && map_length >= 64 {
                // Type 2: read the PartitionTypeIdentifier (EntityID) at offset 4.
                // Identifier string occupies offset 4+1..4+24 (skip the 1-byte flags).
                let id_off = p + 4 + 1;
                let id_str = String::from_utf8_lossy(&lvd_sector[id_off..id_off + 23])
                    .trim_end_matches(['\0', ' '])
                    .to_string();
                let underlying_partition_number =
                    u16::from_le_bytes([lvd_sector[p + 38], lvd_sector[p + 39]]);
                if id_str.contains("Metadata Partition") {
                    let metadata_file_lba = u32::from_le_bytes([
                        lvd_sector[p + 40],
                        lvd_sector[p + 41],
                        lvd_sector[p + 42],
                        lvd_sector[p + 43],
                    ]);
                    // Resolve the metadata file's File Entry, then capture
                    // its physical extents.
                    let meta_partition = partitions
                        .get(&underlying_partition_number)
                        .ok_or_else(|| anyhow!("UDF: metadata partition references unknown partition {}", underlying_partition_number))?;
                    let meta_file_phys_lba = meta_partition.starting_location as u64
                        + metadata_file_lba as u64;
                    let meta_fe = read_file_entry_at_phys_lba(
                        &mut file,
                        meta_partition.starting_location,
                        meta_file_phys_lba,
                    )?;
                    let mut metadata_extents: Vec<(u32, u32)> = Vec::new();
                    for ad in &meta_fe.allocation_descriptors {
                        match ad {
                            AllocDesc::Short(s) => {
                                let phys =
                                    meta_partition.starting_location + s.position;
                                metadata_extents.push((phys, s.length()));
                            }
                            AllocDesc::Long(l) => {
                                let phys = meta_partition.starting_location
                                    + l.location.logical_block_number;
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
        };

        // Resolve the FSD physical LBA via the partition maps.
        let fsd_phys_lba = img.resolve_phys_lba(
            fsd_long_ad.location.partition_reference_number,
            fsd_long_ad.location.logical_block_number,
        )?;
        let fsd = read_sector(&mut img.file, fsd_phys_lba)?;
        let tid = u16::from_le_bytes([fsd[0], fsd[1]]);
        if tid != TAG_FSD {
            return Err(anyhow!(
                "UDF: expected FSD, got tag {} at LBA {}",
                tid,
                fsd_phys_lba
            ));
        }
        let root_icb = read_long_ad(&fsd[400..416]);

        let root = img.read_file_entry(&root_icb)?;
        img.root = root;
        Ok(img)
    }

    fn resolve_phys_lba(&self, prn: u16, lbn: u32) -> Result<u64> {
        let pmap = self.partition_maps.get(prn as usize).ok_or_else(|| {
            anyhow!(
                "UDF: partition_reference_number {} out of range",
                prn
            )
        })?;
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
                Err(anyhow!(
                    "UDF: lbn {} not covered by metadata extents",
                    lbn
                ))
            }
        }
    }

    fn resolve_phys_byte_run(
        &self,
        prn: u16,
        lbn: u32,
        length_bytes: usize,
    ) -> Result<Vec<(u64 /* phys LBA */, u64 /* run bytes */)>> {
        let pmap = self.partition_maps.get(prn as usize).ok_or_else(|| {
            anyhow!(
                "UDF: partition_reference_number {} out of range",
                prn
            )
        })?;
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
        directory_size_inner(self, fe)
    }
}

fn directory_size_inner(image: &mut UdfImage, fe: &UdfFile) -> Result<u64> {
    let mut total: u64 = 0;
    if !fe.is_directory {
        return Ok(fe.size);
    }
    let entries = image.list_dir(fe)?;
    for e in entries {
        if e.is_parent || e.is_deleted {
            continue;
        }
        let child = image.read_file_entry(&e.icb)?;
        if child.is_directory {
            total += directory_size_inner(image, &child)?;
        } else if !e.name.to_ascii_lowercase().ends_with(".ssif") {
            total += child.size;
        }
    }
    Ok(total)
}

/// Read a File Entry / Extended File Entry from a sector that may not yet be
/// covered by the `UdfImage` partition map (used while bootstrapping the
/// metadata partition's metadata file).
fn read_file_entry_at_phys_lba(
    file: &mut File,
    partition_start_lba: u32,
    phys_lba: u64,
) -> Result<UdfFile> {
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
            let img = image.lock().unwrap();
            for ad in &fe.allocation_descriptors {
                let (prn, lbn, len) = match ad {
                    AllocDesc::Short(s) => {
                        (fe.partition_reference, s.position, s.length() as usize)
                    }
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
        while self.run_index < self.runs.len() {
            let (lba, length) = self.runs[self.run_index];
            if self.run_offset >= length {
                self.run_index += 1;
                self.run_offset = 0;
                continue;
            }
            let remain_in_run = length - self.run_offset;
            let want = (buf.len() as u64)
                .min(remain_in_run)
                .min(self.total_remaining) as usize;
            if want == 0 {
                return Ok(0);
            }
            let abs_byte = lba * SECTOR_SIZE as u64 + self.run_offset;
            let mut img = self.image.lock().unwrap();
            img.file.seek(SeekFrom::Start(abs_byte))?;
            let n = img.file.read(&mut buf[..want])?;
            self.run_offset += n as u64;
            self.total_remaining -= n as u64;
            return Ok(n);
        }
        Ok(0)
    }
}
