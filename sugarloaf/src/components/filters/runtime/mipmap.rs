// This file was originally taken from https://github.com/SnowflakePowered/librashader
// The file has changed to avoid use atomic reference counter of wgpu Device and Queue structs
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use librashader_common::map::FastHashMap;
use std::borrow::Cow;
use tracing::debug;

pub struct MipmapGen {
    shader: wgpu::ShaderModule,
    pipeline_cache: FastHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl MipmapGen {
    fn create_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: None,
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "../shader/blit.wgsl"
            ))),
        });

        Self {
            shader,
            pipeline_cache: Default::default(),
        }
    }

    pub fn generate_mipmaps(
        &mut self,
        device: &wgpu::Device,
        cmd: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        sampler: &wgpu::Sampler,
        miplevels: u32,
    ) {
        let format = texture.format();
        let pipeline = &*self.pipeline_cache.entry(format).or_insert_with(|| {
            debug!("PipelineCache miss for texture format={:?}", format);
            Self::create_pipeline(device, &self.shader, format)
        });

        let views = (0..miplevels)
            .map(|mip| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("mip"),
                    format: None,
                    dimension: None,
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    base_array_layer: 0,
                    array_layer_count: None,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();

        for target_mip in 1..miplevels as usize {
            let bind_group_layout = pipeline.get_bind_group_layout(0);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &views[target_mip - 1],
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
                label: None,
            });

            let mut pass = cmd.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &views[target_mip],
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
