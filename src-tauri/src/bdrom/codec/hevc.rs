/*
 * Copyright (c) 2026. caoccao.com Sam Cao
 * All rights reserved.
 *
 * Faithful port of TSCodecHEVC.cs.
 */

#![allow(clippy::too_many_arguments)]

use super::stream_buffer::TSStreamBuffer;
use crate::protocol::TSStreamInfo;

#[derive(Debug, Clone, Default)]
pub struct VideoParamSet {
    pub vps_max_sub_layers: u16,
}

#[derive(Debug, Clone, Default)]
pub struct XXLData {
    pub bit_rate_value: u64,
    pub cpb_size_value: u64,
    pub cbr_flag: bool,
}

#[derive(Debug, Clone, Default)]
pub struct XXL {
    pub sched_sel: Vec<XXLData>,
}

#[derive(Debug, Clone, Default)]
pub struct XXLCommon {
    pub sub_pic_hrd_params_present_flag: bool,
    pub du_cpb_removal_delay_increment_length_minus1: u16,
    pub dpb_output_delay_du_length_minus1: u16,
    pub initial_cpb_removal_delay_length_minus1: u16,
    pub au_cpb_removal_delay_length_minus1: u16,
    pub dpb_output_delay_length_minus1: u16,
    pub valid: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VUIParameters {
    pub nal: Option<XXL>,
    pub vcl: Option<XXL>,
    pub xxl_common: Option<XXLCommon>,
    pub num_units_in_tick: u32,
    pub time_scale: u32,
    pub sar_width: u16,
    pub sar_height: u16,
    pub aspect_ratio_idc: u8,
    pub video_format: u8,
    pub video_full_range_flag: u8,
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub aspect_ratio_info_present_flag: bool,
    pub video_signal_type_present_flag: bool,
    pub frame_field_info_present_flag: bool,
    pub colour_description_present_flag: bool,
    pub timing_info_present_flag: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SeqParameterSet {
    pub vui_parameters: VUIParameters,
    pub profile_space: u32,
    pub tier_flag: bool,
    pub profile_idc: u32,
    pub level_idc: u32,
    pub pic_width_in_luma_samples: u32,
    pub pic_height_in_luma_samples: u32,
    pub conf_win_left_offset: u32,
    pub conf_win_right_offset: u32,
    pub conf_win_top_offset: u32,
    pub conf_win_bottom_offset: u32,
    pub video_parameter_set_id: u8,
    pub chroma_format_idc: u8,
    pub separate_colour_plane_flag: bool,
    pub log2_max_pic_order_cnt_lsb_minus4: u8,
    pub bit_depth_luma_minus8: u8,
    pub bit_depth_chroma_minus8: u8,
    pub general_progressive_source_flag: bool,
    pub general_interlaced_source_flag: bool,
    pub general_frame_only_constraint_flag: bool,
    pub valid: bool,
}

impl SeqParameterSet {
    pub fn nal_hrd_bp_present_flag(&self) -> bool {
        self.vui_parameters.nal.is_some()
    }
    pub fn vcl_hrd_pb_present_flag(&self) -> bool {
        self.vui_parameters.vcl.is_some()
    }
    pub fn cpb_dpb_delays_present_flag(&self) -> bool {
        self.vui_parameters.xxl_common.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PicParameterSet {
    pub seq_parameter_set_id: u8,
    pub num_ref_idx_l0_default_active_minus1: u8,
    pub num_ref_idx_l1_default_active_minus1: u8,
    pub num_extra_slice_header_bits: u8,
    pub dependent_slice_segments_enabled_flag: bool,
    pub valid: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ExtendedDataSet {
    pub video_param_sets: Vec<VideoParamSet>,
    pub seq_parameter_sets: Vec<SeqParameterSet>,
    pub pic_parameter_sets: Vec<PicParameterSet>,
    pub mastering_display_color_primaries: String,
    pub mastering_display_luminance: String,
    pub maximum_content_light_level: u32,
    pub maximum_frame_average_light_level: u32,
    pub light_level_available: bool,
    pub extended_format_info: Vec<String>,
    pub preferred_transfer_characteristics: u8,
    pub is_hdr10_plus: bool,
}

impl ExtendedDataSet {
    pub fn new() -> Self {
        Self {
            preferred_transfer_characteristics: 2,
            ..Default::default()
        }
    }
}

struct MasteringMetadata2086 {
    primaries: [u16; 8],
    luminance: [u32; 2],
}

impl MasteringMetadata2086 {
    fn new() -> Self {
        Self {
            primaries: [0; 8],
            luminance: [0; 2],
        }
    }
}

struct MasteringDisplayColorVolumeValue {
    code: u8,
    values: [u16; 8],
}

const MASTERING_DISPLAY_COLOR_VOLUME_VALUES: [MasteringDisplayColorVolumeValue; 4] = [
    MasteringDisplayColorVolumeValue { code: 1, values: [15000, 30000, 7500, 3000, 32000, 16500, 15635, 16450] },
    MasteringDisplayColorVolumeValue { code: 9, values: [8500, 39850, 6550, 2300, 35400, 14600, 15635, 16450] },
    MasteringDisplayColorVolumeValue { code: 11, values: [13250, 34500, 7500, 3000, 34000, 16000, 15700, 17550] },
    MasteringDisplayColorVolumeValue { code: 12, values: [13250, 34500, 7500, 3000, 34000, 16000, 15635, 16450] },
];

fn colour_primaries(c: u8) -> &'static str {
    match c {
        1 => "BT.709",
        4 => "BT.470 System M",
        5 => "BT.601 PAL",
        6 => "BT.601 NTSC",
        7 => "SMPTE 240M",
        8 => "Generic film",
        9 => "BT.2020",
        10 => "XYZ",
        11 => "DCI P3",
        12 => "Display P3",
        22 => "EBU Tech 3213",
        _ => "",
    }
}

fn transfer_characteristics(c: u8) -> &'static str {
    match c {
        1 => "BT.709",
        4 => "BT.470 System M",
        5 => "BT.470 System B/G",
        6 => "BT.601",
        7 => "SMPTE 240M",
        8 => "Linear",
        9 => "Logarithmic (100:1)",
        10 => "Logarithmic (316.22777:1)",
        11 => "xvYCC",
        12 => "BT.1361",
        13 => "sRGB/sYCC",
        14 => "BT.2020 (10-bit)",
        15 => "BT.2020 (12-bit)",
        16 => "PQ",
        17 => "SMPTE 428M",
        18 => "HLG",
        _ => "",
    }
}

fn matrix_coefficients(c: u8) -> &'static str {
    match c {
        0 => "Identity",
        1 => "BT.709",
        4 => "FCC 73.682",
        5 => "BT.470 System B/G",
        6 => "BT.601",
        7 => "SMPTE 240M",
        8 => "YCgCo",
        9 => "BT.2020 non-constant",
        10 => "BT.2020 constant",
        11 => "Y'D'zD'x",
        12 => "Chromaticity-derived non-constant",
        13 => "Chromaticity-derived constant",
        14 => "ICtCp",
        _ => "",
    }
}

struct State {
    is_initialized: bool,
    profile_space: u32,
    tier_flag: bool,
    profile_idc: u32,
    level_idc: u32,
    general_progressive_source_flag: bool,
    general_interlaced_source_flag: bool,
    general_frame_only_constraint_flag: bool,
    extended: ExtendedDataSet,
    is_hdr10_plus: bool,
    chroma_sample_loc_type_top_field: u32,
    chroma_sample_loc_type_bottom_field: u32,
    extended_diagnostics: bool,
}

pub fn scan(stream: &mut TSStreamInfo, buffer: &mut TSStreamBuffer, extended_diagnostics: bool) {
    let mut st = State {
        is_initialized: stream.is_initialized,
        profile_space: 0,
        tier_flag: false,
        profile_idc: 0,
        level_idc: 0,
        general_progressive_source_flag: false,
        general_interlaced_source_flag: false,
        general_frame_only_constraint_flag: false,
        extended: ExtendedDataSet::new(),
        is_hdr10_plus: false,
        chroma_sample_loc_type_top_field: 0,
        chroma_sample_loc_type_bottom_field: 0,
        extended_diagnostics,
    };

    let mut frame_type_read = false;

    loop {
        if buffer.position() + 3 >= buffer.len() {
            break;
        }
        if st.is_initialized && frame_type_read {
            break;
        }

        let mut sync_byte_found = false;
        loop {
            let stream_pos = buffer.position() as i64;
            if buffer.position() + 4 > buffer.len() {
                break;
            }
            if buffer.read_byte_default() == 0
                && buffer.read_byte_default() == 0
                && buffer.read_byte_default() == 0
                && buffer.read_byte_default() == 1
            {
                sync_byte_found = true;
                break;
            }
            buffer.bs_skip_bytes_default((stream_pos - buffer.position() as i64) as i32);
            if buffer.position() + 3 > buffer.len() {
                break;
            }
            if buffer.read_byte_default() == 0
                && buffer.read_byte_default() == 0
                && buffer.read_byte_default() == 1
            {
                sync_byte_found = true;
                break;
            }
            buffer.bs_skip_bytes_default((stream_pos - buffer.position() as i64 + 1) as i32);
            if buffer.position() + 3 >= buffer.len()
                || (st.is_initialized && frame_type_read)
            {
                break;
            }
        }

        if buffer.position() < buffer.len() && sync_byte_found {
            let last_stream_pos = buffer.position() as i64;
            buffer.bs_skip_bits(1, true);
            let nal_unit_type = buffer.read_bits2(6, true) as i64;
            buffer.bs_skip_bits(9, true);

            match nal_unit_type {
                0..=9 | 16..=21 => {
                    let r = slice_segment_layer(buffer, nal_unit_type, &st);
                    if r {
                        frame_type_read = true;
                    }
                }
                32 => video_parameter_set(buffer, &mut st),
                33 => seq_parameter_set(buffer, &mut st),
                34 => pic_parameter_set(buffer, &mut st),
                35 => access_unit_delimiter(buffer),
                39 | 40 => sei(buffer, &mut st),
                _ => {}
            }

            buffer.bs_skip_next_byte();
            buffer.bs_skip_bytes(
                (last_stream_pos - buffer.position() as i64) as i32,
                true,
            );
        }
    }

    // Apply collected SPS data to the stream.
    if !st.extended.seq_parameter_sets.is_empty() && !stream.is_initialized {
        let sps = st.extended.seq_parameter_sets[0].clone();
        if sps.profile_space == 0 {
            let profile = match sps.profile_idc {
                0 => "No profile",
                1 => "Main",
                2 => "Main 10",
                3 => "Main Still",
                _ => "",
            };
            let mut encoding = String::new();
            if sps.profile_idc > 0 {
                encoding.push_str(profile);
            }
            if sps.level_idc > 0 {
                let calc_level = sps.level_idc as f64 / 30.0;
                let dec = sps.level_idc % 10;
                let level_text = if dec >= 1 {
                    format!("{:.1}", calc_level)
                } else {
                    format!("{:.0}", calc_level)
                };
                encoding.push_str(&format!(" @ Level {} @ ", level_text));
                encoding.push_str(if sps.tier_flag { "High" } else { "Main" });
            }
            stream.encoding_profile = encoding;

            if sps.chroma_format_idc > 0 {
                let chroma = match sps.chroma_format_idc {
                    1 => "4:2:0",
                    2 => "4:2:2",
                    3 => "4:4:4",
                    _ => "",
                };
                if !chroma.is_empty() && st.extended_diagnostics {
                    st.extended.extended_format_info.push(chroma.to_string());
                }
            }
            if sps.bit_depth_luma_minus8 == sps.bit_depth_chroma_minus8 {
                st.extended.extended_format_info.push(
                    format!("{} bits", sps.bit_depth_luma_minus8 as u32 + 8),
                );
            }

            if sps.bit_depth_luma_minus8 + 8 == 10
                && sps.chroma_format_idc == 1
                && sps.vui_parameters.video_signal_type_present_flag
                && sps.vui_parameters.colour_description_present_flag
                && sps.vui_parameters.colour_primaries == 9
                && sps.vui_parameters.transfer_characteristics == 16
                && (sps.vui_parameters.matrix_coefficients == 9
                    || sps.vui_parameters.matrix_coefficients == 10)
                && !st.extended.mastering_display_color_primaries.is_empty()
            {
                let label = if stream.pid >= 4117 {
                    "Dolby Vision"
                } else if st.is_hdr10_plus {
                    "HDR10+"
                } else {
                    "HDR10"
                };
                st.extended.extended_format_info.push(label.to_string());
            }

            if sps.vui_parameters.video_signal_type_present_flag {
                if st.extended_diagnostics {
                    st.extended.extended_format_info.push(
                        if sps.vui_parameters.video_full_range_flag == 1 {
                            "Full Range".to_string()
                        } else {
                            "Limited Range".to_string()
                        },
                    );
                }
                if sps.vui_parameters.colour_description_present_flag {
                    let cp = colour_primaries(sps.vui_parameters.colour_primaries);
                    if !cp.is_empty() {
                        st.extended.extended_format_info.push(cp.to_string());
                    }
                    if st.extended_diagnostics {
                        let tc = transfer_characteristics(sps.vui_parameters.transfer_characteristics);
                        if !tc.is_empty() {
                            st.extended.extended_format_info.push(tc.to_string());
                        }
                        let mc = matrix_coefficients(sps.vui_parameters.matrix_coefficients);
                        if !mc.is_empty() {
                            st.extended.extended_format_info.push(mc.to_string());
                        }
                    }
                }
            }
        }
    }

    if st.extended_diagnostics && !stream.is_initialized {
        if !st.extended.mastering_display_color_primaries.is_empty() {
            st.extended.extended_format_info.push(format!(
                "Mastering display color primaries: {}",
                st.extended.mastering_display_color_primaries
            ));
        }
        if !st.extended.mastering_display_luminance.is_empty() {
            st.extended.extended_format_info.push(format!(
                "Mastering display luminance: {}",
                st.extended.mastering_display_luminance
            ));
        }
        if st.extended.light_level_available && st.extended.maximum_content_light_level > 0 {
            st.extended.extended_format_info.push(format!(
                "Maximum Content Light Level: {} cd / m2",
                st.extended.maximum_content_light_level
            ));
            st.extended.extended_format_info.push(format!(
                "Maximum Frame-Average Light Level: {} cd/m2",
                st.extended.maximum_frame_average_light_level
            ));
        }
    }

    stream.is_vbr = true;
    if !st.extended.seq_parameter_sets.is_empty() {
        stream.is_initialized = true;
    }
    stream.extended_format_info = st.extended.extended_format_info.clone();
}

fn slice_segment_layer(buffer: &mut TSStreamBuffer, nal_unit_type: i64, st: &State) -> bool {
    let _first_slice_segment_in_pic_flag = buffer.read_bool_default();
    if (16..=23).contains(&nal_unit_type) {
        let _no_output_of_prior_pics_flag = buffer.read_bool_default();
    }
    let slice_pic_parameter_set_id = buffer.read_exp(true);
    if slice_pic_parameter_set_id as usize >= st.extended.pic_parameter_sets.len() {
        return false;
    }
    if !_first_slice_segment_in_pic_flag {
        if st.extended.pic_parameter_sets[slice_pic_parameter_set_id as usize]
            .dependent_slice_segments_enabled_flag
        {
            let _dependent = buffer.read_bool(true);
        }
        return false;
    }
    buffer.bs_skip_bits_default(
        st.extended.pic_parameter_sets[slice_pic_parameter_set_id as usize]
            .num_extra_slice_header_bits as u32,
    );
    let slice_type = buffer.read_exp(true);
    matches!(slice_type, 0..=2)
}

fn video_parameter_set(buffer: &mut TSStreamBuffer, st: &mut State) {
    if st.is_initialized {
        return;
    }
    let vps_video_parameter_set_id = buffer.read_bits2(4, true) as usize;
    buffer.bs_skip_bits(8, true);
    let max_sub_layers = buffer.read_bits2(3, true) as u32;
    buffer.bs_skip_bits(17, true);

    profile_tier_level(buffer, max_sub_layers, st);

    let temp_b = buffer.read_bool(true);
    let from = if temp_b { 0 } else { max_sub_layers };
    for _ in from..=max_sub_layers {
        for _ in 0..3 {
            buffer.skip_exp(true);
        }
    }
    let vps_max_layer_id = buffer.read_bits2(6, true) as u32;
    let vps_num_layer_sets_minus1 = buffer.read_exp(true);

    for _layer_set_pos in 1..=vps_num_layer_sets_minus1 {
        for _ in 0..=vps_max_layer_id {
            buffer.bs_skip_bits(1, true);
        }
    }

    let vps_timing_info_present_flag = buffer.read_bool(true);
    if vps_timing_info_present_flag {
        buffer.bs_skip_bits(64, true);
        let vps_poc_proportional_to_timing_flag = buffer.read_bool(true);
        if !vps_poc_proportional_to_timing_flag {
            buffer.skip_exp(true);
        }
        let mut vps_num_hrd_parameters = buffer.read_exp(true);
        if vps_num_hrd_parameters > 1024 {
            vps_num_hrd_parameters = 0;
        }
        for hrd_pos in 0..vps_num_hrd_parameters {
            let mut xxl_common: Option<XXLCommon> = None;
            let mut nal: Option<XXL> = None;
            let mut vcl: Option<XXL> = None;
            buffer.skip_exp(true);
            let cprms_present_flag = hrd_pos == 0 || buffer.read_bool(true);
            hrd_parameters(
                buffer,
                cprms_present_flag,
                vps_num_layer_sets_minus1,
                &mut xxl_common,
                &mut nal,
                &mut vcl,
            );
        }
    }
    buffer.bs_skip_bits(1, true);

    if vps_video_parameter_set_id >= st.extended.video_param_sets.len() {
        let extra = vps_video_parameter_set_id + 1 - st.extended.video_param_sets.len();
        for _ in 0..extra {
            st.extended
                .video_param_sets
                .push(VideoParamSet { vps_max_sub_layers: 0 });
        }
    }
    st.extended.video_param_sets[vps_video_parameter_set_id] = VideoParamSet {
        vps_max_sub_layers: vps_num_layer_sets_minus1 as u16,
    };
}

fn seq_parameter_set(buffer: &mut TSStreamBuffer, st: &mut State) {
    if st.is_initialized {
        return;
    }
    let mut conf_win_left_offset = 0u32;
    let mut conf_win_right_offset = 0u32;
    let mut conf_win_top_offset = 0u32;
    let mut conf_win_bottom_offset = 0u32;
    let mut separate_colour_plane_flag = false;

    let video_parameter_set_id = buffer.read_bits2(4, true) as usize;
    if video_parameter_set_id >= st.extended.video_param_sets.len() {
        return;
    }
    let video_param_set_item = st.extended.video_param_sets[video_parameter_set_id].clone();

    let max_sub_layers_minus1 = buffer.read_bits2(3, true) as u32;
    buffer.bs_skip_bits(1, true);
    profile_tier_level(buffer, max_sub_layers_minus1, st);

    let sps_seq_parameter_set_id = buffer.read_exp(true);
    let chroma_format_idc = buffer.read_exp(true);
    if chroma_format_idc >= 4 {
        return;
    }
    if chroma_format_idc == 3 {
        separate_colour_plane_flag = buffer.read_bool(true);
    }
    let pic_width_in_luma_samples = buffer.read_exp(true);
    let pic_height_in_luma_samples = buffer.read_exp(true);
    if buffer.read_bool(true) {
        conf_win_left_offset = buffer.read_exp(true);
        conf_win_right_offset = buffer.read_exp(true);
        conf_win_top_offset = buffer.read_exp(true);
        conf_win_bottom_offset = buffer.read_exp(true);
    }

    let bit_depth_luma_minus8 = buffer.read_exp(true);
    if bit_depth_luma_minus8 > 6 {
        return;
    }
    let bit_depth_chroma_minus8 = buffer.read_exp(true);
    if bit_depth_chroma_minus8 > 6 {
        return;
    }
    let log2_max_pic_order_cnt_lsb_minus4 = buffer.read_exp(true);
    if log2_max_pic_order_cnt_lsb_minus4 > 12 {
        return;
    }
    let sps_sub_layer_ordering_info_present_flag = buffer.read_bool(true);
    let from = if sps_sub_layer_ordering_info_present_flag { 0 } else { max_sub_layers_minus1 };
    for _ in from..=max_sub_layers_minus1 {
        for _ in 0..3 {
            buffer.skip_exp(true);
        }
    }
    for _ in 0..6 {
        buffer.skip_exp(true);
    }

    if buffer.read_bool(true) {
        if buffer.read_bool(true) {
            scaling_list_data(buffer);
        }
    }

    buffer.bs_skip_bits(2, true);
    if buffer.read_bool(true) {
        buffer.bs_skip_bits(8, true);
        for _ in 0..2 {
            buffer.skip_exp(true);
        }
        buffer.bs_skip_bits(1, true);
    }
    let num_short_term_ref_pic_sets = buffer.read_exp(true);
    short_term_ref_pic_sets(buffer, num_short_term_ref_pic_sets);

    if buffer.read_bool(true) {
        let num_long_term_ref_pics_sps = buffer.read_exp(true);
        for _ in 0..num_long_term_ref_pics_sps {
            buffer.bs_skip_bits(log2_max_pic_order_cnt_lsb_minus4 + 4, true);
            buffer.bs_skip_bits(1, true);
        }
    }
    buffer.bs_skip_bits(2, true);

    let mut vui = VUIParameters::default();
    if buffer.read_bool(true) {
        vui_parameters(buffer, &video_param_set_item, &mut vui, st);
    }

    let new_sps = SeqParameterSet {
        vui_parameters: vui,
        profile_space: st.profile_space,
        tier_flag: st.tier_flag,
        profile_idc: st.profile_idc,
        level_idc: st.level_idc,
        pic_width_in_luma_samples,
        pic_height_in_luma_samples,
        conf_win_left_offset,
        conf_win_right_offset,
        conf_win_top_offset,
        conf_win_bottom_offset,
        video_parameter_set_id: video_parameter_set_id as u8,
        chroma_format_idc: chroma_format_idc as u8,
        separate_colour_plane_flag,
        log2_max_pic_order_cnt_lsb_minus4: log2_max_pic_order_cnt_lsb_minus4 as u8,
        bit_depth_luma_minus8: bit_depth_luma_minus8 as u8,
        bit_depth_chroma_minus8: bit_depth_chroma_minus8 as u8,
        general_progressive_source_flag: st.general_progressive_source_flag,
        general_interlaced_source_flag: st.general_interlaced_source_flag,
        general_frame_only_constraint_flag: st.general_frame_only_constraint_flag,
        valid: true,
    };

    let id = sps_seq_parameter_set_id as usize;
    if id >= st.extended.seq_parameter_sets.len() {
        let extra = id + 1 - st.extended.seq_parameter_sets.len();
        for _ in 0..extra {
            st.extended.seq_parameter_sets.push(SeqParameterSet::default());
        }
    }
    st.extended.seq_parameter_sets[id] = new_sps;
}

fn pic_parameter_set(buffer: &mut TSStreamBuffer, st: &mut State) {
    if st.is_initialized {
        return;
    }
    let pps_pic_parameter_set_id = buffer.read_exp(true);
    if pps_pic_parameter_set_id >= 64 {
        return;
    }
    let pps_seq_parameter_set_id = buffer.read_exp(true);
    if pps_seq_parameter_set_id >= 16 {
        return;
    }
    if (pps_seq_parameter_set_id as usize) >= st.extended.seq_parameter_sets.len()
        || !st.extended.seq_parameter_sets[pps_seq_parameter_set_id as usize].valid
    {
        return;
    }

    let dependent_slice_segments_enabled_flag = buffer.read_bool(true);
    buffer.bs_skip_bits(1, true);
    let num_extra_slice_header_bits = buffer.read_bits2(3, true) as u8;
    buffer.bs_skip_bits(2, true);
    let num_ref_idx_l0_default_active_minus1 = buffer.read_exp(true);
    let num_ref_idx_l1_default_active_minus1 = buffer.read_exp(true);
    buffer.skip_exp(true);
    buffer.bs_skip_bits(2, true);
    if buffer.read_bool(true) {
        buffer.skip_exp(true);
    }
    for _ in 0..2 {
        buffer.skip_exp(true);
    }
    buffer.bs_skip_bits(4, true);
    let tiles_enabled_flag = buffer.read_bool(true);
    buffer.bs_skip_bits(1, true);
    if tiles_enabled_flag {
        let num_tile_columns_minus1 = buffer.read_exp(true);
        let num_tile_rows_minus1 = buffer.read_exp(true);
        let uniform_spacing_flag = buffer.read_bool(true);
        if !uniform_spacing_flag {
            for _ in 0..num_tile_columns_minus1 {
                buffer.skip_exp(true);
            }
            for _ in 0..num_tile_rows_minus1 {
                buffer.skip_exp(true);
            }
        }
        buffer.bs_skip_bits(1, true);
    }
    buffer.bs_skip_bits(1, true);
    if buffer.read_bool(true) {
        buffer.bs_skip_bits(1, true);
        if !buffer.read_bool(true) {
            for _ in 0..2 {
                buffer.skip_exp(true);
            }
        }
    }
    if buffer.read_bool(true) {
        scaling_list_data(buffer);
    }
    buffer.bs_skip_bits(1, true);
    buffer.skip_exp(true);
    buffer.bs_skip_bits(1, true);
    if buffer.read_bool(true) {
        buffer.bs_skip_next_byte();
    }

    let id = pps_pic_parameter_set_id as usize;
    if id >= st.extended.pic_parameter_sets.len() {
        let extra = id + 1 - st.extended.pic_parameter_sets.len();
        for _ in 0..extra {
            st.extended.pic_parameter_sets.push(PicParameterSet::default());
        }
    }
    st.extended.pic_parameter_sets[id] = PicParameterSet {
        seq_parameter_set_id: pps_seq_parameter_set_id as u8,
        num_ref_idx_l0_default_active_minus1: num_ref_idx_l0_default_active_minus1 as u8,
        num_ref_idx_l1_default_active_minus1: num_ref_idx_l1_default_active_minus1 as u8,
        num_extra_slice_header_bits,
        dependent_slice_segments_enabled_flag,
        valid: true,
    };
}

fn access_unit_delimiter(buffer: &mut TSStreamBuffer) {
    buffer.bs_skip_bits(3, true);
}

fn sei(buffer: &mut TSStreamBuffer, st: &mut State) {
    if st.is_initialized {
        return;
    }
    let element_start = buffer.position() as i64;
    let mut num_bytes;
    loop {
        let stream_pos = buffer.position() as i64;
        num_bytes = 0;
        if buffer.position() + 4 > buffer.len() {
            break;
        }
        if buffer.read_byte_default() == 0
            && buffer.read_byte_default() == 0
            && buffer.read_byte_default() == 0
            && buffer.read_byte_default() == 1
        {
            num_bytes = 4;
            break;
        }
        buffer.bs_skip_bytes_default((stream_pos - buffer.position() as i64) as i32);
        if buffer.position() + 3 > buffer.len() {
            break;
        }
        if buffer.read_byte_default() == 0
            && buffer.read_byte_default() == 0
            && buffer.read_byte_default() == 1
        {
            num_bytes = 3;
            break;
        }
        buffer.bs_skip_bytes_default((stream_pos - buffer.position() as i64 + 1) as i32);
        if buffer.position() >= buffer.len() {
            break;
        }
    }

    let mut element_size = buffer.position() as i64 - element_start;
    buffer.bs_skip_bytes_default((-element_size) as i32);
    element_size -= num_bytes as i64 + 1;

    loop {
        if buffer.position() as i64 >= element_start + element_size {
            break;
        }
        let mut seq_parameter_set_id: u32 = u32::MAX;
        let mut payload_type: u32 = 0;
        let mut payload_size: u32 = 0;
        loop {
            let b = buffer.read_byte(true);
            payload_type += b as u32;
            if b != 0xFF {
                break;
            }
        }
        loop {
            let b = buffer.read_byte(true);
            payload_size += b as u32;
            if b != 0xFF {
                break;
            }
        }
        let saved_pos = buffer.position() as u64 + payload_size as u64;
        if saved_pos > buffer.len() as u64 {
            return;
        }
        match payload_type {
            0 => sei_buffering_period(buffer, &mut seq_parameter_set_id, payload_size, st),
            1 => sei_pic_timing(buffer, &mut seq_parameter_set_id, payload_size, st),
            6 => {
                buffer.skip_exp(true);
                buffer.bs_skip_bits(2, true);
            }
            129 => {
                buffer.bs_skip_bits(6, true);
                let num_sps_ids_minus1 = buffer.read_exp(true);
                for _ in 0..(num_sps_ids_minus1 + 1) {
                    buffer.skip_exp(true);
                }
            }
            137 => sei_mastering_display(buffer, st),
            144 => {
                st.extended.maximum_content_light_level = buffer.read_bits2(16, true) as u32;
                st.extended.maximum_frame_average_light_level = buffer.read_bits2(16, true) as u32;
                st.extended.light_level_available = true;
            }
            147 => {
                st.extended.preferred_transfer_characteristics = buffer.read_bits2(8, true) as u8;
            }
            4 => sei_user_data_t35(buffer, payload_size, st),
            _ => {
                buffer.bs_skip_bytes(payload_size as i32, true);
            }
        }
        if saved_pos > buffer.position() as u64 {
            buffer.bs_skip_bytes((saved_pos - buffer.position() as u64) as i32, true);
        }
    }
}

fn sei_user_data_t35(buffer: &mut TSStreamBuffer, mut payload_size: u32, st: &mut State) {
    let country_code = buffer.read_bits2(8, true);
    let terminal_provider_code = buffer.read_bits2(16, true);
    let terminal_provider_oriented_code = buffer.read_bits2(16, true);
    let application_id = buffer.read_bits4(8, true);
    let application_version = buffer.read_bits4(8, true);
    let num_windows = buffer.read_bits4(2, true);
    buffer.bs_skip_bits(6, true);
    if country_code == 0xB5
        && terminal_provider_code == 0x003C
        && terminal_provider_oriented_code == 0x0001
    {
        if application_id == 4
            && (application_version == 0 || application_version == 1)
            && num_windows == 1
        {
            st.is_hdr10_plus = true;
            st.extended.is_hdr10_plus = true;
        }
    }
    payload_size = payload_size.saturating_sub(8);
    buffer.bs_skip_bytes(payload_size as i32, true);
}

fn sei_buffering_period(
    buffer: &mut TSStreamBuffer,
    seq_parameter_set_id: &mut u32,
    payload_size: u32,
    st: &State,
) {
    *seq_parameter_set_id = buffer.read_exp(true);
    if (*seq_parameter_set_id as usize) >= st.extended.seq_parameter_sets.len()
        || !st.extended.seq_parameter_sets[*seq_parameter_set_id as usize].valid
    {
        buffer.bs_skip_bits(payload_size * 8, true);
        return;
    }
    let sps = &st.extended.seq_parameter_sets[*seq_parameter_set_id as usize];
    let sub_pic_hrd_params_present_flag = false;
    let mut irap_cpb_params_present_flag = sps
        .vui_parameters
        .xxl_common
        .as_ref()
        .map(|x| x.sub_pic_hrd_params_present_flag)
        .unwrap_or(false);
    if !sub_pic_hrd_params_present_flag {
        irap_cpb_params_present_flag = buffer.read_bool(true);
    }
    let au = sps
        .vui_parameters
        .xxl_common
        .as_ref()
        .map(|x| x.au_cpb_removal_delay_length_minus1)
        .unwrap_or(23);
    let dpb = sps
        .vui_parameters
        .xxl_common
        .as_ref()
        .map(|x| x.dpb_output_delay_length_minus1)
        .unwrap_or(23);
    if irap_cpb_params_present_flag {
        buffer.bs_skip_bits(au as u32 + dpb as u32 + 2, true);
    }
    buffer.bs_skip_bits(au as u32 + 2, true);
    if sps.nal_hrd_bp_present_flag() {
        sei_buffering_period_xxl(
            buffer,
            sps.vui_parameters.xxl_common.as_ref(),
            irap_cpb_params_present_flag,
            sps.vui_parameters.nal.as_ref(),
            payload_size,
        );
    }
    if sps.vcl_hrd_pb_present_flag() {
        sei_buffering_period_xxl(
            buffer,
            sps.vui_parameters.xxl_common.as_ref(),
            irap_cpb_params_present_flag,
            sps.vui_parameters.vcl.as_ref(),
            payload_size,
        );
    }
}

fn sei_buffering_period_xxl(
    buffer: &mut TSStreamBuffer,
    xxl_common: Option<&XXLCommon>,
    irap: bool,
    xxl: Option<&XXL>,
    payload_size: u32,
) {
    let xxl_common = match xxl_common {
        Some(x) => x,
        None => {
            buffer.bs_skip_bits(payload_size * 8, true);
            return;
        }
    };
    let xxl = match xxl {
        Some(x) => x,
        None => {
            buffer.bs_skip_bits(payload_size * 8, true);
            return;
        }
    };
    for _ in 0..xxl.sched_sel.len() {
        buffer.bs_skip_bits(
            xxl_common.initial_cpb_removal_delay_length_minus1 as u32 + 1,
            true,
        );
        buffer.bs_skip_bits(
            xxl_common.initial_cpb_removal_delay_length_minus1 as u32 + 1,
            true,
        );
        if xxl_common.sub_pic_hrd_params_present_flag || irap {
            buffer.bs_skip_bits(
                xxl_common.initial_cpb_removal_delay_length_minus1 as u32 + 1,
                true,
            );
            buffer.bs_skip_bits(
                xxl_common.initial_cpb_removal_delay_length_minus1 as u32 + 1,
                true,
            );
        }
    }
}

fn sei_pic_timing(
    buffer: &mut TSStreamBuffer,
    seq_parameter_set_id: &mut u32,
    payload_size: u32,
    st: &State,
) {
    if *seq_parameter_set_id == u32::MAX && st.extended.seq_parameter_sets.len() == 1 {
        *seq_parameter_set_id = 0;
    }
    if (*seq_parameter_set_id as usize) >= st.extended.seq_parameter_sets.len()
        || !st.extended.seq_parameter_sets[*seq_parameter_set_id as usize].valid
    {
        buffer.bs_skip_bits(payload_size * 8, true);
        return;
    }
    let sps = &st.extended.seq_parameter_sets[*seq_parameter_set_id as usize];
    let frame_field = sps.vui_parameters.frame_field_info_present_flag
        || (sps.general_progressive_source_flag && sps.general_interlaced_source_flag);
    if frame_field {
        buffer.bs_skip_bits(7, true);
    }
    if sps.cpb_dpb_delays_present_flag() {
        let xxl_common = sps.vui_parameters.xxl_common.as_ref().unwrap();
        let au = xxl_common.au_cpb_removal_delay_length_minus1 as u32;
        let dpb = xxl_common.dpb_output_delay_length_minus1 as u32;
        let sub = xxl_common.sub_pic_hrd_params_present_flag;
        buffer.bs_skip_bits(au + dpb + 2, true);
        if sub {
            let dpb_du = xxl_common.dpb_output_delay_du_length_minus1 as u32;
            buffer.bs_skip_bits(dpb_du + 1, true);
        }
    }
}

fn sei_mastering_display(buffer: &mut TSStreamBuffer, st: &mut State) {
    let mut meta = MasteringMetadata2086::new();
    buffer.bs_reset_bits();
    for i in 0..3 {
        meta.primaries[i * 2] = buffer.read_bits2(16, true);
        meta.primaries[(i * 2) + 1] = buffer.read_bits2(16, true);
    }
    meta.primaries[3 * 2] = buffer.read_bits2(16, true);
    meta.primaries[(3 * 2) + 1] = buffer.read_bits2(16, true);

    meta.luminance[1] = buffer.read_bits4(32, true);
    meta.luminance[0] = buffer.read_bits4(32, true);

    let (mut r, mut g, mut b) = (4i32, 4i32, 4i32);
    for c in 0..3 {
        if meta.primaries[c * 2] < 17500 && meta.primaries[(c * 2) + 1] < 17500 {
            b = c as i32;
        } else if meta.primaries[(c * 2) + 1] as i32 - meta.primaries[c * 2] as i32 >= 0 {
            g = c as i32;
        } else {
            r = c as i32;
        }
    }
    if (r | b | g) >= 4 {
        g = 0;
        b = 1;
        r = 2;
    }

    let mut not_valid = false;
    for c in 0..8 {
        if meta.primaries[c] == u16::MAX {
            not_valid = true;
        }
    }

    let mut primaries_str = String::new();
    let mut human_readable = false;

    if !not_valid {
        for v in MASTERING_DISPLAY_COLOR_VOLUME_VALUES.iter() {
            let mut code = v.code;
            for j in 0..2 {
                let g_lo = v.values[0 * 2 + j] as i32 - 25;
                let g_hi = v.values[0 * 2 + j] as i32 + 25;
                let b_lo = v.values[1 * 2 + j] as i32 - 25;
                let b_hi = v.values[1 * 2 + j] as i32 + 25;
                let r_lo = v.values[2 * 2 + j] as i32 - 25;
                let r_hi = v.values[2 * 2 + j] as i32 + 25;
                let w_lo = v.values[3 * 2 + j] as i32 - 2;
                let w_hi = v.values[3 * 2 + j] as i32 + 3;

                let pg = meta.primaries[(g as usize) * 2 + j] as i32;
                let pb = meta.primaries[(b as usize) * 2 + j] as i32;
                let pr = meta.primaries[(r as usize) * 2 + j] as i32;
                let pw = meta.primaries[3 * 2 + j] as i32;

                if pg < g_lo || pg >= g_hi { code = 0; }
                if pb < b_lo || pb >= b_hi { code = 0; }
                if pr < r_lo || pr >= r_hi { code = 0; }
                if pw < w_lo || pw >= w_hi { code = 0; }
            }
            if code > 0 {
                primaries_str = colour_primaries(code).to_string();
                human_readable = true;
                break;
            }
        }
        if !human_readable {
            primaries_str = format!(
                "R: x={:.6} y={:.6}, G: x={:.6} y={:.6}, B: x={:.6} y={:.6}, White point: x={:.6} y={:.6}",
                meta.primaries[(r as usize) * 2] as f64 / 50000.0,
                meta.primaries[(r as usize) * 2 + 1] as f64 / 50000.0,
                meta.primaries[(g as usize) * 2] as f64 / 50000.0,
                meta.primaries[(g as usize) * 2 + 1] as f64 / 50000.0,
                meta.primaries[(b as usize) * 2] as f64 / 50000.0,
                meta.primaries[(b as usize) * 2 + 1] as f64 / 50000.0,
                meta.primaries[3 * 2] as f64 / 50000.0,
                meta.primaries[3 * 2 + 1] as f64 / 50000.0,
            );
        }
    }

    st.extended.mastering_display_color_primaries = primaries_str;
    let lum_max = meta.luminance[1] as f64 / 10000.0;
    let lum_max_str = if (lum_max - lum_max.floor()).abs() < f64::EPSILON {
        format!("{:.0}", lum_max)
    } else {
        format!("{:.4}", lum_max)
    };
    st.extended.mastering_display_luminance = format!(
        "min: {:.4} cd/m2, max: {} cd/m2",
        meta.luminance[0] as f64 / 10000.0,
        lum_max_str
    );
}

fn vui_parameters(
    buffer: &mut TSStreamBuffer,
    video_param_set_item: &VideoParamSet,
    out: &mut VUIParameters,
    st: &mut State,
) {
    let mut xxl_common = XXLCommon::default();
    let mut nal_o: Option<XXL> = Some(XXL::default());
    let mut vcl_o: Option<XXL> = Some(XXL::default());

    let mut num_units_in_tick = u32::MAX;
    let mut time_scale = u32::MAX;
    let mut sar_width = u16::MAX;
    let mut sar_height = u16::MAX;
    let mut aspect_ratio_idc: u8 = 0;
    let mut video_format: u8 = 5;
    let mut video_full_range_flag: u8 = 0;
    let mut colour_primaries: u8 = 2;
    let mut transfer_characteristics: u8 = 2;
    let mut matrix_coefficients: u8 = 2;
    let mut colour_description_present_flag = false;

    let aspect_ratio_info_present_flag = buffer.read_bool(true);
    if aspect_ratio_info_present_flag {
        aspect_ratio_idc = buffer.read_bits2(8, true) as u8;
        if aspect_ratio_idc == 0xFF {
            sar_width = buffer.read_bits4(16, true) as u16;
            sar_height = buffer.read_bits4(16, true) as u16;
        }
    }
    if buffer.read_bool(true) {
        buffer.bs_skip_bits(1, true);
    }
    let video_signal_type_present_flag = buffer.read_bool(true);
    if video_signal_type_present_flag {
        video_format = buffer.read_bits2(3, true) as u8;
        video_full_range_flag = buffer.read_bits2(1, true) as u8;
        colour_description_present_flag = buffer.read_bool(true);
        if colour_description_present_flag {
            colour_primaries = buffer.read_bits2(8, true) as u8;
            transfer_characteristics = buffer.read_bits2(8, true) as u8;
            matrix_coefficients = buffer.read_bits2(8, true) as u8;
        }
    }
    if buffer.read_bool(true) {
        st.chroma_sample_loc_type_top_field = buffer.read_exp(true);
        st.chroma_sample_loc_type_bottom_field = buffer.read_exp(true);
    }
    buffer.bs_skip_bits(2, true);
    let frame_field_info_present_flag = buffer.read_bool(true);
    if buffer.read_bool(true) {
        for _ in 0..4 {
            buffer.skip_exp(true);
        }
    }
    let timing_info_present_flag = buffer.read_bool(true);
    if timing_info_present_flag {
        num_units_in_tick = buffer.read_bits8(32, true) as u32;
        time_scale = buffer.read_bits8(32, true) as u32;
        if buffer.read_bool(true) {
            buffer.skip_exp(true);
        }
        if buffer.read_bool(true) {
            let mut common: Option<XXLCommon> = None;
            hrd_parameters(
                buffer,
                true,
                video_param_set_item.vps_max_sub_layers as u32,
                &mut common,
                &mut nal_o,
                &mut vcl_o,
            );
            if let Some(c) = common {
                xxl_common = c;
            }
        }
    }
    if buffer.read_bool(true) {
        buffer.bs_skip_bits(3, true);
        for _ in 0..5 {
            buffer.skip_exp(true);
        }
    }

    *out = VUIParameters {
        nal: nal_o.filter(|x| !x.sched_sel.is_empty()),
        vcl: vcl_o.filter(|x| !x.sched_sel.is_empty()),
        xxl_common: if xxl_common.valid { Some(xxl_common) } else { None },
        num_units_in_tick,
        time_scale,
        sar_width,
        sar_height,
        aspect_ratio_idc,
        video_format,
        video_full_range_flag,
        colour_primaries,
        transfer_characteristics,
        matrix_coefficients,
        aspect_ratio_info_present_flag,
        video_signal_type_present_flag,
        frame_field_info_present_flag,
        colour_description_present_flag,
        timing_info_present_flag,
    };
}

fn short_term_ref_pic_sets(buffer: &mut TSStreamBuffer, num_short_term_ref_pic_sets: u32) {
    let mut num_pics: u32 = 0;
    for st_rps_idx in 0..num_short_term_ref_pic_sets {
        let mut inter_ref_pic_set_prediction_flag = false;
        if st_rps_idx > 0 {
            inter_ref_pic_set_prediction_flag = buffer.read_bool(true);
        }
        if inter_ref_pic_set_prediction_flag {
            let mut delta_idx_minus1 = 0u32;
            if st_rps_idx == num_short_term_ref_pic_sets {
                delta_idx_minus1 = buffer.read_exp(true);
            }
            if delta_idx_minus1 + 1 > st_rps_idx {
                return;
            }
            buffer.bs_skip_bits(1, true);
            buffer.skip_exp(true);
            let mut num_pics_new: u32 = 0;
            for _ in 0..=num_pics {
                if buffer.read_bool(true) {
                    num_pics_new += 1;
                } else if buffer.read_bool(true) {
                    num_pics_new += 1;
                }
            }
            num_pics = num_pics_new;
        } else {
            let num_negative_pics = buffer.read_exp(true);
            let num_positive_pics = buffer.read_exp(true);
            num_pics = num_negative_pics + num_positive_pics;
            for _ in 0..num_negative_pics {
                buffer.skip_exp(true);
                buffer.bs_skip_bits(1, true);
            }
            for _ in 0..num_positive_pics {
                buffer.skip_exp(true);
                buffer.bs_skip_bits(1, true);
            }
        }
    }
}

fn scaling_list_data(buffer: &mut TSStreamBuffer) {
    for size_id in 0..4 {
        let outer = if size_id == 3 { 2 } else { 6 };
        for _ in 0..outer {
            if !buffer.read_bool(true) {
                buffer.skip_exp(true);
            } else {
                let coef_num = std::cmp::min(64u32, 1u32 << (4 + (size_id << 1)));
                if size_id > 1 {
                    buffer.skip_exp(true);
                }
                for _ in 0..coef_num {
                    buffer.skip_exp(true);
                }
            }
        }
    }
}

fn profile_tier_level(buffer: &mut TSStreamBuffer, sub_layer_count: u32, st: &mut State) {
    st.profile_space = buffer.read_bits2(2, true) as u32;
    st.tier_flag = buffer.read_bool(true);
    st.profile_idc = buffer.read_bits2(5, true) as u32;

    buffer.bs_skip_bits(32, true);

    st.general_progressive_source_flag = buffer.read_bool(true);
    st.general_interlaced_source_flag = buffer.read_bool(true);
    buffer.bs_skip_bits(1, true);
    st.general_frame_only_constraint_flag = buffer.read_bool(true);
    buffer.bs_skip_bits(44, true);
    st.level_idc = buffer.read_bits2(8, true) as u32;

    let mut sub_layer_profile_present_flags: Vec<bool> = Vec::new();
    let mut sub_layer_level_present_flags: Vec<bool> = Vec::new();
    for _ in 0..sub_layer_count {
        sub_layer_profile_present_flags.push(buffer.read_bool(true));
        sub_layer_level_present_flags.push(buffer.read_bool(true));
    }
    if sub_layer_count > 0 {
        let to_skip = ((8u32 - sub_layer_count) * 2) as u32;
        // the C# code skips 2 reserved bits before per-sublayer profile/level data
        buffer.bs_skip_bits(2, true);
        // pad sub-layer parsing; original BDInfo expects 8 sublayers worth in 2-bit units already covered
        let _ = to_skip;
    }
    for sub_layer_pos in 0..sub_layer_count as usize {
        if sub_layer_profile_present_flags[sub_layer_pos] {
            buffer.bs_skip_bits(88, true);
        }
        if sub_layer_level_present_flags[sub_layer_pos] {
            buffer.bs_skip_bits(8, true);
        }
    }
}

fn hrd_parameters(
    buffer: &mut TSStreamBuffer,
    common_inf_present_flag: bool,
    max_num_sub_layers_minus1: u32,
    xxl_common: &mut Option<XXLCommon>,
    nal: &mut Option<XXL>,
    vcl: &mut Option<XXL>,
) {
    let mut bit_rate_scale: u8 = 0;
    let mut cpb_size_scale: u8 = 0;
    let mut du_cpb_removal_delay_increment_length_minus1: u8 = 0;
    let mut dpb_output_delay_du_length_minus1: u8 = 0;
    let mut initial_cpb_removal_delay_length_minus1: u8 = 0;
    let mut au_cpb_removal_delay_length_minus1: u8 = 0;
    let mut dpb_output_delay_length_minus1: u8 = 0;
    let mut nal_hrd_parameters_present_flag = false;
    let mut vcl_hrd_parameters_present_flag = false;
    let mut sub_pic_hrd_params_present_flag = false;

    if common_inf_present_flag {
        nal_hrd_parameters_present_flag = buffer.read_bool(true);
        vcl_hrd_parameters_present_flag = buffer.read_bool(true);
        if nal_hrd_parameters_present_flag || vcl_hrd_parameters_present_flag {
            sub_pic_hrd_params_present_flag = buffer.read_bool(true);
            if sub_pic_hrd_params_present_flag {
                buffer.bs_skip_bits(8, true);
                du_cpb_removal_delay_increment_length_minus1 = buffer.read_bits2(5, true) as u8;
                buffer.bs_skip_bits(1, true);
                dpb_output_delay_du_length_minus1 = buffer.read_bits2(5, true) as u8;
            }
            bit_rate_scale = buffer.read_bits2(4, true) as u8;
            cpb_size_scale = buffer.read_bits2(4, true) as u8;
            if sub_pic_hrd_params_present_flag {
                buffer.bs_skip_bits(4, true);
            }
            initial_cpb_removal_delay_length_minus1 = buffer.read_bits2(5, true) as u8;
            au_cpb_removal_delay_length_minus1 = buffer.read_bits2(5, true) as u8;
            dpb_output_delay_length_minus1 = buffer.read_bits2(5, true) as u8;
        }
    }

    for _num_sub_layer in 0..=max_num_sub_layers_minus1 {
        let mut cpb_cnt_minus1: u32 = 0;
        let mut fixed_pic_rate_within_cvs_flag = true;
        let mut low_delay_hrd_flag = false;
        let fixed_pic_rate_general_flag = buffer.read_bool(true);
        if !fixed_pic_rate_general_flag {
            fixed_pic_rate_within_cvs_flag = buffer.read_bool(true);
        }
        if fixed_pic_rate_within_cvs_flag {
            buffer.skip_exp(true);
        } else {
            low_delay_hrd_flag = buffer.read_bool(true);
        }
        if !low_delay_hrd_flag {
            cpb_cnt_minus1 = buffer.read_exp(true);
            if cpb_cnt_minus1 > 31 {
                return;
            }
        }
        if nal_hrd_parameters_present_flag || vcl_hrd_parameters_present_flag {
            *xxl_common = Some(XXLCommon {
                sub_pic_hrd_params_present_flag,
                du_cpb_removal_delay_increment_length_minus1: du_cpb_removal_delay_increment_length_minus1
                    as u16,
                dpb_output_delay_du_length_minus1: dpb_output_delay_du_length_minus1 as u16,
                initial_cpb_removal_delay_length_minus1: initial_cpb_removal_delay_length_minus1
                    as u16,
                au_cpb_removal_delay_length_minus1: au_cpb_removal_delay_length_minus1 as u16,
                dpb_output_delay_length_minus1: dpb_output_delay_length_minus1 as u16,
                valid: true,
            });
        }
        if nal_hrd_parameters_present_flag {
            sub_layer_hrd_parameters(
                buffer,
                xxl_common.as_ref(),
                bit_rate_scale,
                cpb_size_scale,
                cpb_cnt_minus1,
                nal,
            );
        }
        if vcl_hrd_parameters_present_flag {
            sub_layer_hrd_parameters(
                buffer,
                xxl_common.as_ref(),
                bit_rate_scale,
                cpb_size_scale,
                cpb_cnt_minus1,
                vcl,
            );
        }
    }
}

fn sub_layer_hrd_parameters(
    buffer: &mut TSStreamBuffer,
    xxl_common: Option<&XXLCommon>,
    bit_rate_scale: u8,
    cpb_size_scale: u8,
    cpb_cnt_minus1: u32,
    out: &mut Option<XXL>,
) {
    let mut sched_sel: Vec<XXLData> = Vec::with_capacity(cpb_cnt_minus1 as usize + 1);
    for _ in 0..=cpb_cnt_minus1 {
        let bit_rate_value_minus1 = buffer.read_exp(true);
        let bit_rate_value = ((bit_rate_value_minus1 as u64 + 1)
            * (1u64 << (6 + bit_rate_scale as u32))) as u64;
        let cpb_size_value_minus1 = buffer.read_exp(true);
        let cpb_size_value = ((cpb_size_value_minus1 as u64 + 1)
            * (1u64 << (4 + cpb_size_scale as u32))) as u64;
        if xxl_common
            .map(|x| x.sub_pic_hrd_params_present_flag)
            .unwrap_or(false)
        {
            buffer.skip_exp(true);
            buffer.skip_exp(true);
        }
        let cbr_flag = buffer.read_bool(true);
        sched_sel.push(XXLData {
            bit_rate_value,
            cpb_size_value,
            cbr_flag,
        });
    }
    *out = Some(XXL { sched_sel });
}
