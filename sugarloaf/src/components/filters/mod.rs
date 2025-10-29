mod builtin;
mod runtime;

use crate::context::Context;
use librashader_common::{Size, Viewport};
use librashader_presets::ShaderFeatures;
use std::sync::Arc;

pub type Filter = String;

/// A brush for applying RetroArch filters.
#[derive(Default)]
pub struct FiltersBrush {
    filter_chains: Vec<crate::components::filters::runtime::FilterChain>,
    filter_intermediates: Vec<Arc<wgpu::Texture>>,
    framecount: usize,
}

impl FiltersBrush {
    #[inline]
    pub fn update_filters(&mut self, ctx: &Context, filters: &[Filter]) {
        self.filter_chains.clear();
        self.filter_intermediates.clear();

        if filters.is_empty() {
            return;
        }

        let usage_caps = ctx.surface_caps().usages;

        if !usage_caps.contains(wgpu::TextureUsages::COPY_DST)
            || !usage_caps.contains(wgpu::TextureUsages::COPY_SRC)
        {
            tracing::warn!("The selected backend does not support the filter chains!");

            return;
        }

        for filter in filters {
            let configured_filter = filter.to_lowercase();
            match configured_filter.as_str() {
                "newpixiecrt" | "fubax_vr" => {
                    tracing::debug!("Loading builtin filter {}", configured_filter);

                    let builtin_filter = match configured_filter.as_str() {
                        "newpixiecrt" => builtin::newpixiecrt,
                        "fubax_vr" => builtin::fubaxvr,
                        _ => {
                            continue;
                        }
                    };

                    match builtin_filter() {
                        Ok(shader_preset) => {
                            match crate::components::filters::runtime::FilterChain::load_from_preset(
                                shader_preset,
                                &ctx.device,
                                &ctx.queue,
                                None,
                            ) {
                                Ok(f) => self.filter_chains.push(f),
                                Err(e) => tracing::error!("Failed to load builtin filter {}: {}", configured_filter, e),
                            }
                        },
                        Err(e) => {
                            tracing::error!("Failed to build shader preset from builtin filter {}: {}", configured_filter, e)
                        },
                    }
                }
                _ => {
                    tracing::debug!("Loading filter {}", filter);

                    match crate::components::filters::runtime::FilterChain::load_from_path(
                        filter,
                        ShaderFeatures::NONE,
                        &ctx.device,
                        &ctx.queue,
                        None,
                    ) {
                        Ok(f) => self.filter_chains.push(f),
                        Err(e) => {
                            tracing::error!("Failed to load filter {}: {}", filter, e)
                        }
                    }
                }
            }
        }

        self.filter_intermediates.reserve(self.filter_chains.len());

        // If we have an odd number of filters, the last filter can be
        // renderer directly to the output texture.
        let skip = if self.filter_chains.len() % 2 == 1 {
            1
        } else {
            0
        };

        let size = wgpu::Extent3d {
            depth_or_array_layers: 1,
            width: ctx.size.width as u32,
            height: ctx.size.height as u32,
        };

        for _ in self.filter_chains.iter().skip(skip) {
            let intermediate_texture =
                Arc::new(ctx.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Filter Intermediate Texture"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: ctx.format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[ctx.format],
                }));

            self.filter_intermediates.push(intermediate_texture);
        }
    }

    /// Render the filters on top of the src_texture to dst_texture.
    /// If the filters are not set, the src_texture is copied to dst_texture.
    #[inline]
    pub fn render(
        &mut self,
        ctx: &Context,
        encoder: &mut wgpu::CommandEncoder,
        src_texture: &wgpu::Texture,
        dst_texture: &wgpu::Texture,
    ) {
        let filters_count = self.filter_chains.len();
        if filters_count == 0 {
            return;
        }

        let usage_caps = ctx.surface_caps().usages;

        if !usage_caps.contains(wgpu::TextureUsages::COPY_SRC)
            || !usage_caps.contains(wgpu::TextureUsages::COPY_DST)
        {
            return;
        }

        // Some shaders can do some specific things for which WGPU (at least the Vulkan backend)
        // requires the src and dst textures to be different, otherwise it will crash.
        // Also librashader requires a texture to be in Arc, so we need to make a copy anyway
        let src_texture = {
            let new_src_texture =
                Arc::new(ctx.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Filters Source Texture"),
                    size: src_texture.size(),
                    mip_level_count: src_texture.mip_level_count(),
                    sample_count: src_texture.sample_count(),
                    dimension: src_texture.dimension(),
                    format: src_texture.format(),
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[src_texture.format()],
                }));

            encoder.copy_texture_to_texture(
                src_texture.as_image_copy(),
                new_src_texture.as_image_copy(),
                new_src_texture.size(),
            );

            new_src_texture
        };

        let view_size = Size::new(ctx.size.width as u32, ctx.size.height as u32);
        for (idx, filter) in self.filter_chains.iter_mut().enumerate() {
            let filter_src_texture: Arc<wgpu::Texture>;
            let filter_dst_texture: &wgpu::Texture;

            if idx == 0 {
                filter_src_texture = src_texture.clone();

                if filters_count == 1 {
                    filter_dst_texture = dst_texture;
                } else {
                    filter_dst_texture = &self.filter_intermediates[0];
                }
            } else if idx == filters_count - 1 {
                filter_src_texture = self.filter_intermediates[idx - 1].clone();
                filter_dst_texture = dst_texture;
            } else {
                filter_src_texture = self.filter_intermediates[idx - 1].clone();
                filter_dst_texture = &self.filter_intermediates[idx];
            }

            let dst_texture_view =
                filter_dst_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let dst_output_view =
                crate::components::filters::runtime::WgpuOutputView::new_from_raw(
                    &dst_texture_view,
                    view_size,
                    ctx.format,
                );
            let dst_viewport =
                Viewport::new_render_target_sized_origin(dst_output_view, None).unwrap();

            // Framecount should be added forever: https://github.com/raphamorim/rio/issues/753
            self.framecount = self.framecount.wrapping_add(1);
            if let Err(err) = filter.frame(
                filter_src_texture,
                &dst_viewport,
                encoder,
                self.framecount,
                None,
                ctx,
            ) {
                tracing::error!("Filter rendering failed: {err}");
            }
        }
    }
}
