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
        for wrap_mode in wrap_modes {
            for filter_mode in &[FilterMode::Linear, FilterMode::Nearest] {
                for mipmap_filter in &[FilterMode::Linear, FilterMode::Nearest] {
                    samplers.insert(
                        (*wrap_mode, *filter_mode, *mipmap_filter),
                        Arc::new(device.create_sampler(&SamplerDescriptor {
                            label: None,
                            address_mode_u: (*wrap_mode).into(),
                            address_mode_v: (*wrap_mode).into(),
                            address_mode_w: (*wrap_mode).into(),
                            mag_filter: (*filter_mode).into(),
                            min_filter: (*filter_mode).into(),
                            mipmap_filter: (*mipmap_filter).into(),
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
