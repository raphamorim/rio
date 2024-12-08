// This file was originally taken from https://github.com/SnowflakePowered/librashader
// The file has changed to avoid use atomic reference counter of wgpu Device and Queue structs
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use crate::components::filters::runtime::buffer::WgpuStagedBuffer;
use crate::components::filters::runtime::error;
use crate::components::filters::runtime::filter_chain::FilterCommon;
use crate::components::filters::runtime::framebuffer::WgpuOutputView;
use crate::components::filters::runtime::graphics_pipeline::WgpuGraphicsPipeline;
use crate::components::filters::runtime::options::FrameOptionsWgpu;
use crate::components::filters::runtime::samplers::SamplerSet;
use crate::components::filters::runtime::texture::InputImage;
use librashader_common::map::FastHashMap;
use librashader_common::{ImageFormat, Size, Viewport};
use librashader_preprocess::ShaderSource;
use librashader_presets::PassMeta;
use librashader_reflect::reflect::semantics::{
    BindingStage, MemberOffset, TextureBinding, UniformBinding,
};
use librashader_reflect::reflect::ShaderReflection;
use librashader_runtime::binding::{BindSemantics, TextureInput, UniformInputs};
use librashader_runtime::filter_pass::FilterPassMeta;
use librashader_runtime::quad::QuadType;
use librashader_runtime::render_target::RenderTarget;
use librashader_runtime::uniforms::{
    NoUniformBinder, UniformStorage, UniformStorageAccess,
};
use std::sync::Arc;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindingResource, BufferBinding, ShaderStages,
};

pub struct FilterPass {
    pub reflection: ShaderReflection,
    pub(crate) uniform_storage: UniformStorage<
        NoUniformBinder,
        Option<()>,
        WgpuStagedBuffer,
        WgpuStagedBuffer,
        wgpu::Device,
    >,
    pub uniform_bindings: FastHashMap<UniformBinding, MemberOffset>,
    pub source: ShaderSource,
    pub meta: PassMeta,
    pub graphics_pipeline: WgpuGraphicsPipeline,
}

impl TextureInput for InputImage {
    fn size(&self) -> Size<u32> {
        let size = self.image.size();
        Size {
            width: size.width,
            height: size.height,
        }
    }
}

pub struct WgpuArcBinding<T> {
    binding: u32,
    resource: Arc<T>,
}

impl BindSemantics<NoUniformBinder, Option<()>, WgpuStagedBuffer, WgpuStagedBuffer>
    for FilterPass
{
    type InputTexture = InputImage;
    type SamplerSet = SamplerSet;
    type DescriptorSet<'a> = (
        &'a mut FastHashMap<u32, WgpuArcBinding<wgpu::TextureView>>,
        &'a mut FastHashMap<u32, WgpuArcBinding<wgpu::Sampler>>,
    );
    type DeviceContext = wgpu::Device;
    type UniformOffset = MemberOffset;

    #[inline(always)]
    fn bind_texture(
        descriptors: &mut Self::DescriptorSet<'_>,
        samplers: &Self::SamplerSet,
        binding: &TextureBinding,
        texture: &Self::InputTexture,
        _device: &Self::DeviceContext,
    ) {
        let sampler =
            samplers.get(texture.wrap_mode, texture.filter_mode, texture.mip_filter);

        let (texture_binding, sampler_binding) = descriptors;
        texture_binding.insert(
            binding.binding,
            WgpuArcBinding {
                binding: binding.binding,
                resource: Arc::clone(&texture.view),
            },
        );

        sampler_binding.insert(
            binding.binding,
            WgpuArcBinding {
                binding: binding.binding,
                resource: sampler,
            },
        );
    }
}

impl FilterPass {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        cmd: &mut wgpu::CommandEncoder,
        pass_index: usize,
        parent: &FilterCommon,
        frame_count: u32,
        options: &FrameOptionsWgpu,
        viewport: &Viewport<WgpuOutputView>,
        original: &InputImage,
        source: &InputImage,
        output: &RenderTarget<WgpuOutputView>,
        vbo_type: QuadType,
        context: &crate::context::Context,
    ) -> error::Result<()> {
        let mut main_heap = FastHashMap::default();
        let mut sampler_heap = FastHashMap::default();

        self.build_semantics(
            pass_index,
            parent,
            output.mvp,
            frame_count,
            options,
            output.output.size,
            viewport.output.size,
            original,
            source,
            &mut main_heap,
            &mut sampler_heap,
            context,
        );

        let mut main_heap_array = Vec::with_capacity(main_heap.len() + 1);
        let mut sampler_heap_array = Vec::with_capacity(sampler_heap.len() + 1);

        for binding in main_heap.values() {
            main_heap_array.push(BindGroupEntry {
                binding: binding.binding,
                resource: BindingResource::TextureView(&binding.resource),
            })
        }

        for binding in sampler_heap.values() {
            sampler_heap_array.push(BindGroupEntry {
                binding: binding.binding,
                resource: BindingResource::Sampler(&binding.resource),
            })
        }

        if let Some(ubo) = &self.reflection.ubo {
            main_heap_array.push(BindGroupEntry {
                binding: ubo.binding,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: self.uniform_storage.inner_ubo().buffer(),
                    offset: 0,
                    size: None,
                }),
            });
        }

        let mut has_pcb_buffer = false;
        if let Some(pcb) = &self.reflection.push_constant {
            if let Some(binding) = pcb.binding {
                main_heap_array.push(BindGroupEntry {
                    binding,
                    resource: BindingResource::Buffer(BufferBinding {
                        buffer: self.uniform_storage.inner_push().buffer(),
                        offset: 0,
                        size: None,
                    }),
                });
                has_pcb_buffer = true;
            }
        }

        let main_bind_group = context.device.create_bind_group(&BindGroupDescriptor {
            label: Some("main bind group"),
            layout: &self.graphics_pipeline.layout.main_bind_group_layout,
            entries: &main_heap_array,
        });

        let sampler_bind_group = context.device.create_bind_group(&BindGroupDescriptor {
            label: Some("sampler bind group"),
            layout: &self.graphics_pipeline.layout.sampler_bind_group_layout,
            entries: &sampler_heap_array,
        });

        let mut render_pass = self.graphics_pipeline.begin_rendering(output, cmd);

        render_pass.set_bind_group(0, &main_bind_group, &[]);

        render_pass.set_bind_group(1, &sampler_bind_group, &[]);

        if let Some(push) = &self
            .reflection
            .push_constant
            .as_ref()
            .filter(|_| !has_pcb_buffer)
        {
            let mut stage_mask = ShaderStages::empty();
            if push.stage_mask.contains(BindingStage::FRAGMENT) {
                stage_mask |= ShaderStages::FRAGMENT;
            }
            if push.stage_mask.contains(BindingStage::VERTEX) {
                stage_mask |= ShaderStages::VERTEX;
            }
            render_pass.set_push_constants(
                stage_mask,
                0,
                self.uniform_storage.push_slice(),
            )
        }

        parent.draw_quad.draw_quad(&mut render_pass, vbo_type);

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn build_semantics<'a>(
        &mut self,
        pass_index: usize,
        parent: &FilterCommon,
        mvp: &[f32; 16],
        frame_count: u32,
        options: &FrameOptionsWgpu,
        fb_size: Size<u32>,
        viewport_size: Size<u32>,
        original: &InputImage,
        source: &InputImage,
        main_heap: &'a mut FastHashMap<u32, WgpuArcBinding<wgpu::TextureView>>,
        sampler_heap: &'a mut FastHashMap<u32, WgpuArcBinding<wgpu::Sampler>>,
        context: &crate::context::Context,
    ) {
        Self::bind_semantics(
            &context.device,
            &parent.samplers,
            &mut self.uniform_storage,
            &mut (main_heap, sampler_heap),
            UniformInputs {
                mvp,
                frame_count,
                rotation: options.rotation,
                total_subframes: options.total_subframes,
                current_subframe: options.current_subframe,
                frame_direction: options.frame_direction,
                framebuffer_size: fb_size,
                aspect_ratio: options.aspect_ratio,
                frames_per_second: options.frames_per_second,
                frametime_delta: options.frametime_delta,
                viewport_size,
            },
            original,
            source,
            &self.uniform_bindings,
            &self.reflection.meta.texture_meta,
            parent.output_textures[0..pass_index]
                .iter()
                .map(|o| o.as_ref()),
            parent.feedback_textures.iter().map(|o| o.as_ref()),
            parent.history_textures.iter().map(|o| o.as_ref()),
            parent.luts.iter().map(|(u, i)| (*u, i.as_ref())),
            &self.source.parameters,
            &parent.config,
        );

        // flush to buffers
        self.uniform_storage.inner_ubo().flush(&context.queue);
        self.uniform_storage.inner_push().flush(&context.queue);
    }
}

impl FilterPassMeta for FilterPass {
    fn framebuffer_format(&self) -> ImageFormat {
        self.source.format
    }

    fn meta(&self) -> &PassMeta {
        &self.meta
    }
}
