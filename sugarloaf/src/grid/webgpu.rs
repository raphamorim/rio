// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! wgpu backend for the grid renderer.
//!
//! Data-side mirror of `super::metal`. Phase 1b added the bg pass;
//! Phase 1d here adds the text pass — per-instance vertex buffer of
//! `CellText`, grayscale glyph atlas, instanced quad draws.

use rustc_hash::FxHashMap;

use super::atlas::{AtlasSlot, GlyphKey, RasterizedGlyph};
use super::cell::{CellBg, CellText, GridUniforms};
use crate::context::webgpu::WgpuContext;
use crate::renderer::image_cache::atlas::AtlasAllocator;

const FRAMES_IN_FLIGHT: usize = 3;
const CURSOR_ROW_SLOTS: usize = 2;
const ATLAS_SIZE: u32 = 2048;

pub struct WgpuGlyphAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    allocator: AtlasAllocator,
    slots: FxHashMap<GlyphKey, AtlasSlot>,
    queue: wgpu::Queue,
    bytes_per_pixel: u32,
}

impl WgpuGlyphAtlas {
    pub fn new_grayscale(device: &wgpu::Device, queue: wgpu::Queue) -> Self {
        Self::new_with_format(
            device,
            queue,
            wgpu::TextureFormat::R8Unorm,
            1,
            "grid.atlas_grayscale",
        )
    }

    pub fn new_color(device: &wgpu::Device, queue: wgpu::Queue) -> Self {
        Self::new_with_format(
            device,
            queue,
            wgpu::TextureFormat::Rgba8Unorm,
            4,
            "grid.atlas_color",
        )
    }

    fn new_with_format(
        device: &wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        bytes_per_pixel: u32,
        label: &'static str,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            allocator: AtlasAllocator::new(ATLAS_SIZE as u16, ATLAS_SIZE as u16),
            slots: FxHashMap::default(),
            queue,
            bytes_per_pixel,
        }
    }

    #[inline]
    pub fn lookup(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.slots.get(&key).copied()
    }

    pub fn insert(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if glyph.width == 0 || glyph.height == 0 {
            let slot = AtlasSlot {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                bearing_x: glyph.bearing_x,
                bearing_y: glyph.bearing_y,
            };
            self.slots.insert(key, slot);
            return Some(slot);
        }

        let (x, y) = self.allocator.allocate(glyph.width, glyph.height)?;
        let slot = AtlasSlot {
            x,
            y,
            w: glyph.width,
            h: glyph.height,
            bearing_x: glyph.bearing_x,
            bearing_y: glyph.bearing_y,
        };
        self.slots.insert(key, slot);

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: x as u32,
                    y: y as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            glyph.bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(glyph.width as u32 * self.bytes_per_pixel),
                rows_per_image: Some(glyph.height as u32),
            },
            wgpu::Extent3d {
                width: glyph.width as u32,
                height: glyph.height as u32,
                depth_or_array_layers: 1,
            },
        );
        Some(slot)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.allocator.clear();
        self.slots.clear();
    }

    #[inline]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

pub struct WgpuGridRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,

    cols: u32,
    rows: u32,

    bg_cpu: [Vec<CellBg>; FRAMES_IN_FLIGHT],
    bg_buffers: [wgpu::Buffer; FRAMES_IN_FLIGHT],

    fg_rows: Vec<Vec<CellText>>,
    fg_buffers: [wgpu::Buffer; FRAMES_IN_FLIGHT],
    fg_capacity: [usize; FRAMES_IN_FLIGHT],
    fg_staging: Vec<CellText>,

    /// GPU-resident instance count in `fg_buffers[0]` from the last
    /// flush. Reused on Noop/CursorOnly frames to skip the concat
    /// and `write_buffer` call. Same pattern as `MetalGridRenderer`.
    fg_live_count: u32,
    /// Any row-level write since the last flush.
    fg_dirty: bool,
    /// `bg_cpu` changed since the last `write_buffer`.
    bg_dirty: bool,

    #[allow(dead_code)]
    frame: usize,

    uniform_buffer: wgpu::Buffer,

    // bg pipeline
    bg_bind_group_layout: wgpu::BindGroupLayout,
    bg_bind_group: wgpu::BindGroup,
    bg_pipeline: wgpu::RenderPipeline,

    // text pipeline. The bind-group-layout fields are retained for
    // Phase 2 (recreating the atlas bind group when we grow/rotate
    // the atlas texture); hence `#[allow(dead_code)]` for now.
    #[allow(dead_code)]
    text_uniform_bgl: wgpu::BindGroupLayout,
    text_uniform_bg: wgpu::BindGroup,
    #[allow(dead_code)]
    text_atlas_bgl: wgpu::BindGroupLayout,
    text_atlas_bg: wgpu::BindGroup,
    text_pipeline: wgpu::RenderPipeline,

    atlas_grayscale: WgpuGlyphAtlas,
    atlas_color: WgpuGlyphAtlas,

    /// Mirror of `MetalGridRenderer::needs_full_rebuild`. Set on
    /// `new` / `resize`, cleared via `mark_full_rebuild_done`.
    needs_full_rebuild: bool,
}

impl WgpuGridRenderer {
    pub fn new(ctx: &WgpuContext<'_>, cols: u32, rows: u32) -> Self {
        let device = ctx.device.clone();
        let queue = ctx.queue.clone();

        let bg_len = (cols as usize) * (rows as usize);
        let bg_cpu = std::array::from_fn(|_| vec![CellBg::TRANSPARENT; bg_len]);
        let bg_buffers = std::array::from_fn(|_| alloc_bg_buffer(&device, cols, rows));

        let initial_fg_capacity = bg_len.max(1);
        let fg_buffers =
            std::array::from_fn(|_| alloc_fg_buffer(&device, initial_fg_capacity));

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid.uniforms"),
            size: std::mem::size_of::<GridUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // bg pipeline — uniforms + storage buffer.
        let bg_bind_group_layout = create_bg_bind_group_layout(&device);
        let bg_bind_group = create_bg_bind_group(
            &device,
            &bg_bind_group_layout,
            &uniform_buffer,
            &bg_buffers[0],
        );

        // text pipeline — uniforms in group(0), atlas textures in group(1).
        let atlas_grayscale = WgpuGlyphAtlas::new_grayscale(&device, queue.clone());
        let atlas_color = WgpuGlyphAtlas::new_color(&device, queue.clone());
        let text_uniform_bgl = create_text_uniform_bgl(&device);
        let text_uniform_bg =
            create_text_uniform_bg(&device, &text_uniform_bgl, &uniform_buffer);
        let text_atlas_bgl = create_text_atlas_bgl(&device);
        let text_atlas_bg = create_text_atlas_bg(
            &device,
            &text_atlas_bgl,
            atlas_grayscale.view(),
            atlas_color.view(),
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/grid.wgsl").into()),
        });

        let bg_pipeline =
            build_bg_pipeline(&device, ctx.format, &bg_bind_group_layout, &shader);
        let text_pipeline = build_text_pipeline(
            &device,
            ctx.format,
            &[&text_uniform_bgl, &text_atlas_bgl],
            &shader,
        );

        Self {
            device,
            queue,
            cols,
            rows,
            bg_cpu,
            bg_buffers,
            fg_rows: init_fg_rows(rows),
            fg_buffers,
            fg_capacity: [initial_fg_capacity; FRAMES_IN_FLIGHT],
            fg_staging: Vec::new(),
            fg_live_count: 0,
            fg_dirty: true,
            bg_dirty: true,
            frame: 0,
            uniform_buffer,
            bg_bind_group_layout,
            bg_bind_group,
            bg_pipeline,
            text_uniform_bgl,
            text_uniform_bg,
            text_atlas_bgl,
            text_atlas_bg,
            text_pipeline,
            atlas_grayscale,
            atlas_color,
            needs_full_rebuild: true,
        }
    }

    #[inline]
    pub fn needs_full_rebuild(&self) -> bool {
        self.needs_full_rebuild
    }

    #[inline]
    pub fn mark_full_rebuild_done(&mut self) {
        self.needs_full_rebuild = false;
    }

    pub fn resize(&mut self, cols: u32, rows: u32) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        let bg_len = (cols as usize) * (rows as usize);
        self.bg_cpu = std::array::from_fn(|_| vec![CellBg::TRANSPARENT; bg_len]);
        self.bg_buffers =
            std::array::from_fn(|_| alloc_bg_buffer(&self.device, cols, rows));
        self.fg_rows = init_fg_rows(rows);
        let initial_fg_capacity = bg_len.max(1);
        self.fg_buffers =
            std::array::from_fn(|_| alloc_fg_buffer(&self.device, initial_fg_capacity));
        self.fg_capacity = [initial_fg_capacity; FRAMES_IN_FLIGHT];
        self.needs_full_rebuild = true;
        self.fg_dirty = true;
        self.bg_dirty = true;
        self.fg_live_count = 0;
        self.bg_bind_group = create_bg_bind_group(
            &self.device,
            &self.bg_bind_group_layout,
            &self.uniform_buffer,
            &self.bg_buffers[0],
        );
    }

    pub fn write_row(&mut self, row: u32, bg: &[CellBg], fg: &[CellText]) {
        let idx = (row as usize) + 1;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            slot.clear();
            slot.extend_from_slice(fg);
            self.fg_dirty = true;
        }

        if row >= self.rows {
            return;
        }
        let row_start = (row as usize) * (self.cols as usize);
        let row_len = (self.cols as usize).min(bg.len());
        let cpu = &mut self.bg_cpu[0];
        cpu[row_start..row_start + row_len].copy_from_slice(&bg[..row_len]);
        for slot in &mut cpu[row_start + row_len..row_start + self.cols as usize] {
            *slot = CellBg::TRANSPARENT;
        }
        self.bg_dirty = true;
    }

    pub fn clear_row(&mut self, row: u32) {
        let idx = (row as usize) + 1;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            if !slot.is_empty() {
                self.fg_dirty = true;
            }
            slot.clear();
        }
        if row >= self.rows {
            return;
        }
        let row_start = (row as usize) * (self.cols as usize);
        let cpu = &mut self.bg_cpu[0];
        for slot in &mut cpu[row_start..row_start + self.cols as usize] {
            *slot = CellBg::TRANSPARENT;
        }
        self.bg_dirty = true;
    }

    pub fn lookup_glyph(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_grayscale.lookup(key)
    }

    pub fn lookup_glyph_color(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_color.lookup(key)
    }

    pub fn insert_glyph(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        self.atlas_grayscale.insert(key, glyph)
    }

    pub fn insert_glyph_color(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        self.atlas_color.insert(key, glyph)
    }

    /// Record bg pass + text pass against the caller's `render_pass`.
    pub fn render<'pass>(
        &'pass mut self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        uniforms: &GridUniforms,
    ) {
        // Uniforms always upload (cheap, and cursor/min_contrast can
        // change without a row write).
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));

        // Skip re-uploading bg cells when no row changed — the GPU
        // copy is already correct from the previous frame.
        if self.bg_dirty {
            self.queue.write_buffer(
                &self.bg_buffers[0],
                0,
                bytemuck::cast_slice(&self.bg_cpu[0]),
            );
            self.bg_dirty = false;
        }

        // ---------- bg pass ----------
        render_pass.set_pipeline(&self.bg_pipeline);
        render_pass.set_bind_group(0, &self.bg_bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        // ---------- text pass ----------
        if self.fg_dirty {
            self.fg_staging.clear();
            for row in &self.fg_rows {
                self.fg_staging.extend_from_slice(row);
            }

            if self.fg_staging.len() > self.fg_capacity[0] {
                let new_cap = self.fg_staging.len().next_power_of_two();
                self.fg_buffers[0] = alloc_fg_buffer(&self.device, new_cap);
                self.fg_capacity[0] = new_cap;
            }
            self.queue.write_buffer(
                &self.fg_buffers[0],
                0,
                bytemuck::cast_slice(&self.fg_staging),
            );
            self.fg_live_count = self.fg_staging.len() as u32;
            self.fg_dirty = false;
        }

        let instance_count = self.fg_live_count as usize;
        if instance_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.text_pipeline);
        render_pass.set_bind_group(0, &self.text_uniform_bg, &[]);
        render_pass.set_bind_group(1, &self.text_atlas_bg, &[]);
        render_pass.set_vertex_buffer(0, self.fg_buffers[0].slice(..));
        // 4 vertices per instance → triangle strip quad.
        render_pass.draw(0..4, 0..instance_count as u32);
    }
}

// ---------- buffer / layout / pipeline helpers ----------

fn alloc_bg_buffer(device: &wgpu::Device, cols: u32, rows: u32) -> wgpu::Buffer {
    let size = (cols as u64)
        .saturating_mul(rows as u64)
        .saturating_mul(std::mem::size_of::<CellBg>() as u64)
        .max(std::mem::size_of::<CellBg>() as u64);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grid.bg_cells"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn alloc_fg_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let size = (capacity as u64)
        .saturating_mul(std::mem::size_of::<CellText>() as u64)
        .max(std::mem::size_of::<CellText>() as u64);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grid.fg_cells"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn init_fg_rows(rows: u32) -> Vec<Vec<CellText>> {
    (0..(rows as usize + CURSOR_ROW_SLOTS))
        .map(|_| Vec::new())
        .collect()
}

fn create_bg_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("grid.bg_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<
                        GridUniforms,
                    >()
                        as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn create_bg_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    bg_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("grid.bg_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: bg_buffer.as_entire_binding(),
            },
        ],
    })
}

fn create_text_uniform_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("grid.text_uniform_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<
                    GridUniforms,
                >() as u64),
            },
            count: None,
        }],
    })
}

fn create_text_uniform_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("grid.text_uniform_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    })
}

fn create_text_atlas_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("grid.text_atlas_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

fn create_text_atlas_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    grayscale: &wgpu::TextureView,
    color: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("grid.text_atlas_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(grayscale),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(color),
            },
        ],
    })
}

fn premultiplied_blend() -> wgpu::BlendState {
    // Premultiplied-over, matching Ghostty. Text fragment returns
    // premultiplied RGBA (`in.color * mask_a` for grayscale, atlas
    // sample for color), so source RGB must be `One`.
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

fn build_bg_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
    bg_bgl: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("grid.bg_pl"),
        bind_group_layouts: &[bg_bgl],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("grid.bg"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("grid_bg_vertex"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("grid_bg_fragment"),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(premultiplied_blend()),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn build_text_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
    bgls: &[&wgpu::BindGroupLayout],
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("grid.text_pl"),
        bind_group_layouts: bgls,
        immediate_size: 0,
    });

    // Per-instance vertex buffer layout — mirrors `CellText`.
    let attrs = &[
        // @location(0) glyph_pos: vec2<u32> @ offset 0
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 0,
            shader_location: 0,
        },
        // @location(1) glyph_size: vec2<u32> @ offset 8
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 8,
            shader_location: 1,
        },
        // @location(2) bearings: vec2<i32> @ offset 16 — stored as i16x2,
        // widened to i32 in the shader via `Sint16x2`.
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Sint16x2,
            offset: 16,
            shader_location: 2,
        },
        // @location(3) grid_pos: vec2<u32> @ offset 20 — stored as u16x2.
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint16x2,
            offset: 20,
            shader_location: 3,
        },
        // @location(4) color: vec4<f32> @ offset 24 — UNorm8x4 → 0..1.
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Unorm8x4,
            offset: 24,
            shader_location: 4,
        },
        // @location(5) atlas: u32 @ offset 28 — u8 widened.
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint8,
            offset: 28,
            shader_location: 5,
        },
        // @location(6) bools: u32 @ offset 29 — u8 widened.
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint8,
            offset: 29,
            shader_location: 6,
        },
    ];
    let vbuf_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<CellText>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: attrs,
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("grid.text"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("grid_text_vertex"),
            buffers: &[vbuf_layout],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("grid_text_fragment"),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(premultiplied_blend()),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
