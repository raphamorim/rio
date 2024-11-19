// This file was originally taken from https://github.com/SnowflakePowered/librashader
// The file has changed to avoid use atomic reference counter of wgpu Device and Queue structs
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use librashader_common::map::FastHashMap;
use librashader_common::{FilterMode, WrapMode};
use std::sync::Arc;
use wgpu::{Sampler, SamplerBorderColor, SamplerDescriptor};

pub struct SamplerSet {
    // todo: may need to deal with differences in mip filter.
    samplers: FastHashMap<(WrapMode, FilterMode, FilterMode), Arc<Sampler>>,
}

impl SamplerSet {
    #[inline(always)]
    pub fn get(
        &self,
        wrap: WrapMode,
        filter: FilterMode,
        mipmap: FilterMode,
    ) -> Arc<Sampler> {
        // eprintln!("{wrap}, {filter}, {mip}");
        // SAFETY: the sampler set is complete for the matrix
        // wrap x filter x mipmap
        unsafe {
            Arc::clone(
                self.samplers
                    .get(&(wrap, filter, mipmap))
                    .unwrap_unchecked(),
            )
        }
    }

    pub fn new(device: &wgpu::Device) -> SamplerSet {
        let mut samplers = FastHashMap::default();
        let wrap_modes = &[
            WrapMode::ClampToBorder,
            WrapMode::ClampToEdge,
            WrapMode::Repeat,
            WrapMode::MirroredRepeat,
        ];

        let has_clamp_to_border = device
            .features()
            .contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER);

        for wrap_mode in wrap_modes {
            for filter_mode in &[FilterMode::Linear, FilterMode::Nearest] {
                for mipmap_filter in &[FilterMode::Linear, FilterMode::Nearest] {
                    let wgpu_wrap_mode = match wrap_mode {
                        WrapMode::ClampToBorder => {
                            if !has_clamp_to_border {
                                wgpu::AddressMode::ClampToEdge
                            } else {
                                wgpu::AddressMode::ClampToBorder
                            }
                        }
                        WrapMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
                        WrapMode::Repeat => wgpu::AddressMode::Repeat,
                        WrapMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
                    };

                    let wgpu_filter_mode = match filter_mode {
                        FilterMode::Linear => wgpu::FilterMode::Linear,
                        FilterMode::Nearest => wgpu::FilterMode::Nearest,
                    };

                    let wgpu_mipmap_filter = match mipmap_filter {
                        FilterMode::Linear => wgpu::FilterMode::Linear,
                        FilterMode::Nearest => wgpu::FilterMode::Nearest,
                    };

                    samplers.insert(
                        (*wrap_mode, *filter_mode, *mipmap_filter),
                        Arc::new(device.create_sampler(&SamplerDescriptor {
                            label: None,
                            address_mode_u: wgpu_wrap_mode,
                            address_mode_v: wgpu_wrap_mode,
                            address_mode_w: wgpu_wrap_mode,
                            mag_filter: wgpu_filter_mode,
                            min_filter: wgpu_filter_mode,
                            mipmap_filter: wgpu_mipmap_filter,
                            lod_min_clamp: 0.0,
                            lod_max_clamp: 1000.0,
                            compare: None,
                            anisotropy_clamp: 1,
                            border_color: Some(SamplerBorderColor::TransparentBlack),
                        })),
                    );
                }
            }
        }

        // assert all samplers were created.
        assert_eq!(samplers.len(), wrap_modes.len() * 2 * 2);
        SamplerSet { samplers }
    }
}
