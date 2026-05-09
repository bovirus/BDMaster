/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Codec dispatcher. Each codec is invoked on a reassembled PES payload via
 * the TSStreamBuffer abstraction. Parsers may be called multiple times for
 * the same stream until is_initialized turns true.
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

use crate::bdrom::types::TSStreamType;
use crate::protocol::TSStreamInfo;

pub use pgs::PgsState;
pub use stream_buffer::TSStreamBuffer;

/// Per-PID codec scanning state held across PES invocations. This mirrors
/// the per-stream state TSStreamFile.cs maintains in the C# original.
#[derive(Default)]
pub struct CodecScanState {
    pub pgs: PgsState,
}

pub fn scan_stream(
    stream: &mut TSStreamInfo,
    state: &mut CodecScanState,
    payload: &[u8],
    bitrate: i64,
    extended_diagnostics: bool,
    is_full_scan: bool,
) {
    let mut buffer = TSStreamBuffer::new(payload);
    let st = TSStreamType::from_u8(stream.stream_type);

    match st {
        TSStreamType::MPEG2Video => {
            mpeg2::scan(stream, &mut buffer);
        }
        TSStreamType::AVCVideo => {
            avc::scan(stream, &mut buffer);
        }
        TSStreamType::MVCVideo => {
            mvc::scan(stream, &mut buffer);
        }
        TSStreamType::HEVCVideo => {
            hevc::scan(stream, &mut buffer, extended_diagnostics);
        }
        TSStreamType::VC1Video => {
            vc1::scan(stream, &mut buffer);
        }
        TSStreamType::MPEG1Audio | TSStreamType::MPEG2Audio => {
            mpa::scan(stream, &mut buffer);
        }
        TSStreamType::MPEG2AacAudio | TSStreamType::MPEG4AacAudio => {
            aac::scan(stream, &mut buffer);
        }
        TSStreamType::AC3Audio
        | TSStreamType::AC3PlusAudio
        | TSStreamType::AC3PlusSecondaryAudio => {
            ac3::scan(stream, &mut buffer);
        }
        TSStreamType::AC3TrueHDAudio => {
            truehd::scan(stream, &mut buffer);
        }
        TSStreamType::LpcmAudio => {
            if let Some(p) = lpcm::parse(payload) {
                stream.channel_count = p.channels;
                stream.lfe = p.lfe;
                stream.sample_rate = p.sample_rate;
                stream.bit_depth = p.bit_depth;
                let total = p.channels + p.lfe;
                stream.bit_rate = p.sample_rate as u64 * p.bit_depth as u64 * total as u64;
                stream.is_vbr = false;
                stream.is_initialized = true;
            }
        }
        TSStreamType::DTSAudio => {
            dts::scan(stream, &mut buffer, bitrate);
        }
        TSStreamType::DTSHDAudio
        | TSStreamType::DTSHDMasterAudio
        | TSStreamType::DTSHDSecondaryAudio => {
            dtshd::scan(stream, &mut buffer, bitrate);
        }
        TSStreamType::PresentationGraphics => {
            if is_full_scan {
                pgs::scan(stream, &mut buffer, &mut state.pgs);
            } else {
                stream.is_initialized = true;
            }
        }
        _ => {
            stream.is_initialized = true;
        }
    }
}

/// Update audio/video description strings using the populated parameters.
pub fn finalize_description(stream: &mut TSStreamInfo) {
    let st = TSStreamType::from_u8(stream.stream_type);
    if st.is_video() {
        let mut parts: Vec<String> = Vec::new();
        if let Some(bv) = stream.base_view {
            parts.push(if bv { "Right Eye".to_string() } else { "Left Eye".to_string() });
        }
        if stream.height > 0 {
            parts.push(format!(
                "{}{}",
                stream.height,
                if stream.is_interlaced { "i" } else { "p" }
            ));
        }
        if stream.frame_rate_enumerator > 0 && stream.frame_rate_denominator > 0 {
            if stream.frame_rate_enumerator % stream.frame_rate_denominator == 0 {
                parts.push(format!(
                    "{} fps",
                    stream.frame_rate_enumerator / stream.frame_rate_denominator
                ));
            } else {
                parts.push(format!(
                    "{:.3} fps",
                    stream.frame_rate_enumerator as f64
                        / stream.frame_rate_denominator as f64
                ));
            }
        } else if !stream.framerate.is_empty() {
            parts.push(format!("{} fps", stream.framerate));
        }
        if !stream.aspect_ratio.is_empty() {
            parts.push(stream.aspect_ratio.clone());
        }
        if !stream.encoding_profile.is_empty() {
            parts.push(stream.encoding_profile.clone());
        }
        if !stream.extended_format_info.is_empty() {
            parts.push(stream.extended_format_info.join(" / "));
        }
        stream.description = parts.join(" / ");
    } else if st.is_audio() {
        let mut parts: Vec<String> = Vec::new();
        if stream.channel_count > 0 {
            parts.push(format!("{}.{}", stream.channel_count, stream.lfe));
        } else if !stream.channel_layout.is_empty() {
            parts.push(stream.channel_layout.clone());
        }
        if stream.sample_rate > 0 {
            parts.push(format!("{} kHz", stream.sample_rate / 1000));
        }
        if stream.bit_rate > 0 {
            let core_br = stream.core.as_ref().map(|c| c.bit_rate).unwrap_or(0);
            let net = if stream.bit_rate > core_br {
                stream.bit_rate - core_br
            } else {
                stream.bit_rate
            };
            parts.push(format!("{} kbps", (net + 500) / 1000));
        }
        if stream.bit_depth > 0 {
            parts.push(format!("{}-bit", stream.bit_depth));
        }
        if stream.dial_norm != 0 {
            parts.push(format!("DN {}dB", stream.dial_norm));
        }
        if stream.channel_count == 2 {
            match stream.audio_mode.as_str() {
                "DualMono" => parts.push("Dual Mono".to_string()),
                "Surround" => parts.push("Dolby Surround".to_string()),
                "JointStereo" => parts.push("Joint Stereo".to_string()),
                _ => {}
            }
        }
        let mut desc = parts.join(" / ");
        if let Some(core) = &stream.core {
            let core_st = TSStreamType::from_u8(core.stream_type);
            let codec = match core_st {
                TSStreamType::AC3Audio => "AC3 Embedded",
                TSStreamType::DTSAudio => "DTS Core",
                TSStreamType::AC3PlusAudio => "DD+ Embedded",
                _ => "",
            };
            if !codec.is_empty() {
                desc = format!("{} ({}: {})", desc, codec, core.description);
            }
        }
        stream.description = desc;
    } else if st.is_graphics() {
        let mut parts: Vec<String> = Vec::new();
        if stream.width > 0 || stream.height > 0 {
            parts.push(format!("{}x{}", stream.width, stream.height));
        }
        if stream.captions > 0 {
            parts.push(format!(
                "{} Caption{}",
                stream.captions,
                if stream.captions > 1 { "s" } else { "" }
            ));
        }
        if stream.forced_captions > 0 {
            parts.push(format!(
                "{} Forced Caption{}",
                stream.forced_captions,
                if stream.forced_captions > 1 { "s" } else { "" }
            ));
        }
        stream.description = parts.join(" / ");
    }
}

/// Convenience to refine an entire stream from a single PES sample. Used by
/// the lightweight enrichment path before deep per-PES scanning.
pub fn refine_from_pes(stream: &mut TSStreamInfo, sample: &[u8]) {
    let mut state = CodecScanState::default();
    scan_stream(stream, &mut state, sample, 0, false, false);
    finalize_description(stream);
}
