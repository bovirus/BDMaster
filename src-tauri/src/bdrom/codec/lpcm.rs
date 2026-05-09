/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * BD LPCM 4-byte audio header parser. Port of TSCodecLPCM.cs.
 */

#[derive(Debug, Clone, Copy)]
pub struct ParsedLpcm {
    pub channels: u32,
    pub lfe: u32,
    pub bit_depth: u32,
    pub sample_rate: u32,
}

pub fn parse(payload: &[u8]) -> Option<ParsedLpcm> {
    if payload.len() < 4 {
        return None;
    }
    let flags = ((payload[2] as u32) << 8) | payload[3] as u32;

    let (channels, lfe) = match (flags & 0xF000) >> 12 {
        1 => (1, 0),
        3 => (2, 0),
        4 => (3, 0),
        5 => (3, 0),
        6 => (4, 0),
        7 => (4, 0),
        8 => (5, 0),
        9 => (5, 1),
        10 => (7, 0),
        11 => (7, 1),
        _ => (0, 0),
    };

    let bit_depth = match (flags & 0xC0) >> 6 {
        1 => 16,
        2 => 20,
        3 => 24,
        _ => 0,
    };

    let sample_rate = match (flags & 0xF00) >> 8 {
        1 => 48000,
        4 => 96000,
        5 => 192000,
        _ => 0,
    };

    Some(ParsedLpcm {
        channels,
        lfe,
        bit_depth,
        sample_rate,
    })
}
