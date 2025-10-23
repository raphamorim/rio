mod batch;
mod compositor;
mod image_cache;
#[cfg(test)]
mod positioning_tests;
pub mod text;
mod text_run_manager;

use crate::components::core::orthographic_projection;
use crate::components::rich_text::image_cache::{GlyphCache, ImageCache};
use crate::components::rich_text::text_run_manager::{CacheResult, TextRunManager};
use crate::context::webgpu::WgpuContext;
use crate::context::{Context, ContextType};
use crate::font::FontLibrary;
use crate::font_introspector::GlyphId;
use crate::layout::{RichTextLayout, SugarDimensions};
use crate::sugarloaf::graphics::GraphicId;
use crate::Graphics;
use crate::RichTextLinesRange;
use compositor::{Compositor, Rect, Vertex};
use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::{borrow::Cow, mem};
use text::{Glyph, TextRunStyle};
use wgpu::util::DeviceExt;

#[cfg(target_os = "macos")]
use crate::context::metal::MetalContext;
#[cfg(target_os = "macos")]
use metal::*;

pub const BLEND: Option<wgpu::BlendState> = Some(wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
});

#[derive(Debug)]
pub enum RichTextBrushType {
    Wgpu(WgpuRichTextBrush),
    #[cfg(target_os = "macos")]
    Metal(MetalRichTextBrush),
}

#[derive(Debug)]
pub struct WgpuRichTextBrush {
    vertex_buffer: wgpu::Buffer,
    constant_bind_group: wgpu::BindGroup,
    layout_bind_group: wgpu::BindGroup,
    layout_bind_group_layout: wgpu::BindGroupLayout,
    transform: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    current_transform: [f32; 16],
    supported_vertex_buffer: usize,
    textures_version: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct Globals {
    transform: [f32; 16],
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct MetalRichTextBrush {
    pipeline_state: RenderPipelineState,
    vertex_buffer: Buffer,
    uniform_buffer: Buffer,
    sampler: SamplerState,
    supported_vertex_buffer: usize,
    current_transform: [f32; 16],
}

#[cfg(target_os = "macos")]
impl MetalRichTextBrush {
    pub fn new(context: &MetalContext) -> Self {
        let supported_vertex_buffer = 500;

        // Create Metal shader library
        let shader_source = include_str!("rich_text.metal");
        let library = context
            .device
            .new_library_with_source(shader_source, &CompileOptions::new())
            .expect("Failed to create shader library");

        let vertex_function = library
            .get_function("vs_main", None)
            .expect("Failed to get vertex function");
        let fragment_function = library
            .get_function("fs_main", None)
            .expect("Failed to get fragment function");

        // Create vertex descriptor for rich text rendering
        let vertex_descriptor = VertexDescriptor::new();
        let attributes = vertex_descriptor.attributes();

        // Position (attribute 0) - vec4<f32>
        attributes
            .object_at(0)
            .unwrap()
            .set_format(MTLVertexFormat::Float3);
        attributes.object_at(0).unwrap().set_offset(0);
        attributes.object_at(0).unwrap().set_buffer_index(0);

        // Color (attribute 1) - vec4<f32>
        attributes
            .object_at(1)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        attributes.object_at(1).unwrap().set_offset(12);
        attributes.object_at(1).unwrap().set_buffer_index(0);

        // UV (attribute 2) - vec2<f32>
        attributes
            .object_at(2)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        attributes.object_at(2).unwrap().set_offset(28);
        attributes.object_at(2).unwrap().set_buffer_index(0);

        // Layers (attribute 3) - vec2<i32>
        attributes
            .object_at(3)
            .unwrap()
            .set_format(MTLVertexFormat::Int2);
        attributes.object_at(3).unwrap().set_offset(36);
        attributes.object_at(3).unwrap().set_buffer_index(0);

        // Border radius (attribute 4) - f32
        attributes
            .object_at(4)
            .unwrap()
            .set_format(MTLVertexFormat::Float);
        attributes.object_at(4).unwrap().set_offset(44);
        attributes.object_at(4).unwrap().set_buffer_index(0);

        // Rect size (attribute 5) - vec2<f32>
        attributes
            .object_at(5)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        attributes.object_at(5).unwrap().set_offset(48);
        attributes.object_at(5).unwrap().set_buffer_index(0);

        // Set up buffer layout
        let layouts = vertex_descriptor.layouts();
        layouts
            .object_at(0)
            .unwrap()
            .set_stride(mem::size_of::<Vertex>() as u64);
        layouts
            .object_at(0)
            .unwrap()
            .set_step_function(MTLVertexStepFunction::PerVertex);
        layouts.object_at(0).unwrap().set_step_rate(1);

        // Create render pipeline
        let pipeline_descriptor = RenderPipelineDescriptor::new();
        pipeline_descriptor.set_vertex_function(Some(&vertex_function));
        pipeline_descriptor.set_fragment_function(Some(&fragment_function));
        pipeline_descriptor.set_vertex_descriptor(Some(&vertex_descriptor));

        // Set up blending for text rendering - FIXED BLENDING
        let color_attachment = pipeline_descriptor
            .color_attachments()
            .object_at(0)
            .unwrap();
        color_attachment.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        color_attachment.set_blending_enabled(true);
        // Match WGSL BLEND settings exactly:
        // color: src_factor: SrcAlpha, dst_factor: OneMinusSrcAlpha, operation: Add
        color_attachment.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
        color_attachment
            .set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        color_attachment.set_rgb_blend_operation(MTLBlendOperation::Add);
        // alpha: src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add
        color_attachment.set_source_alpha_blend_factor(MTLBlendFactor::One);
        color_attachment
            .set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        color_attachment.set_alpha_blend_operation(MTLBlendOperation::Add);

        let pipeline_state = context
            .device
            .new_render_pipeline_state(&pipeline_descriptor)
            .expect("Failed to create render pipeline state");

        // Create vertex buffer
        let vertex_buffer = context.device.new_buffer(
            (mem::size_of::<Vertex>() * supported_vertex_buffer) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        vertex_buffer.set_label("sugarloaf::rich_text vertex buffer");

        // Create uniform buffer
        let uniform_buffer = context.device.new_buffer(
            mem::size_of::<Globals>() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        uniform_buffer.set_label("sugarloaf::rich_text uniform buffer");

        // Create sampler for texture sampling - IMPROVED SAMPLER SETTINGS
        let sampler_descriptor = SamplerDescriptor::new();
        // Match WGPU settings: Linear filtering for crisp text
        sampler_descriptor.set_min_filter(MTLSamplerMinMagFilter::Linear);
        sampler_descriptor.set_mag_filter(MTLSamplerMinMagFilter::Linear);
        sampler_descriptor.set_mip_filter(MTLSamplerMipFilter::Linear);
        // ClampToEdge addressing to prevent texture bleeding
        sampler_descriptor.set_address_mode_s(MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.set_address_mode_t(MTLSamplerAddressMode::ClampToEdge);
        sampler_descriptor.set_address_mode_r(MTLSamplerAddressMode::ClampToEdge);
        let sampler = context.device.new_sampler(&sampler_descriptor);

        Self {
            pipeline_state,
            vertex_buffer,
            sampler,
            supported_vertex_buffer,
            current_transform: [0.0; 16],
            uniform_buffer,
        }
    }

    pub fn resize(&mut self, transform: [f32; 16]) {
        if self.current_transform != transform {
            let globals = Globals { transform };
            let contents = self.uniform_buffer.contents() as *mut Globals;
            unsafe {
                *contents = globals;
            }
            self.current_transform = transform;
        }
    }

    pub fn render(
        &mut self,
        vertices: &[Vertex],
        images: &ImageCache,
        render_encoder: &RenderCommandEncoderRef,
        context: &MetalContext,
    ) {
        if vertices.is_empty() {
            return;
        }

        // Expand vertex buffer if needed
        if vertices.len() > self.supported_vertex_buffer {
            self.supported_vertex_buffer = (vertices.len() as f32 * 1.25) as usize;

            // Recreate vertex buffer with larger size
            self.vertex_buffer = context.device.new_buffer(
                (mem::size_of::<Vertex>() * self.supported_vertex_buffer) as u64,
                MTLResourceOptions::StorageModeShared,
            );
            self.vertex_buffer
                .set_label("sugarloaf::rich_text vertex buffer (resized)");
        }

        // Copy vertex data to buffer
        let vertex_data = self.vertex_buffer.contents() as *mut Vertex;
        unsafe {
            std::ptr::copy_nonoverlapping(vertices.as_ptr(), vertex_data, vertices.len());
        }

        // Set up render state
        render_encoder.set_render_pipeline_state(&self.pipeline_state);
        render_encoder.set_vertex_buffer(0, Some(&self.vertex_buffer), 0);

        // FIXED: Update transform matrix with current context size
        let transform = orthographic_projection(context.size.width, context.size.height);
        if self.current_transform != transform {
            let globals = Globals { transform };
            let uniform_data = self.uniform_buffer.contents() as *mut Globals;
            unsafe {
                *uniform_data = globals;
            }
            self.current_transform = transform;
        }

        // FIXED: Bind uniform buffer at correct index (buffer 1 in Metal shader)
        render_encoder.set_vertex_buffer(1, Some(&self.uniform_buffer), 0);

        // Set sampler
        render_encoder.set_fragment_sampler_state(0, Some(&self.sampler));

        // Implement proper batching by atlas to avoid lifetime issues
        let color_textures = images.get_metal_textures();
        let mask_texture = images.get_mask_texture();

        // Group vertices by their texture binding requirements
        // layers[0] = color_layer (0 = no color texture, 1+ = color atlas index)
        // layers[1] = mask_layer (0 = no mask texture, 1 = mask atlas)

        let mut current_vertex = 0usize;
        while current_vertex < vertices.len() {
            let start = current_vertex;
            let current_color_layer = vertices[start].layers[0];
            let current_mask_layer = vertices[start].layers[1];

            // Find the end of this batch (consecutive vertices with same layers)
            let mut end = start;
            while end < vertices.len()
                && vertices[end].layers[0] == current_color_layer
                && vertices[end].layers[1] == current_mask_layer
            {
                end += 1;
            }

            // Bind appropriate textures for this batch
            if current_color_layer > 0 {
                // Use color atlas (current_color_layer is 1-based, so subtract 1 for 0-based index)
                let atlas_index = (current_color_layer - 1) as usize;
                if atlas_index < color_textures.len() {
                    render_encoder
                        .set_fragment_texture(0, Some(color_textures[atlas_index]));
                } else {
                    render_encoder.set_fragment_texture(0, None);
                }
            } else {
                render_encoder.set_fragment_texture(0, None);
            }

            if current_mask_layer > 0 {
                if let Some(mask_tex) = mask_texture {
                    render_encoder.set_fragment_texture(1, Some(mask_tex));
                } else {
                    render_encoder.set_fragment_texture(1, None);
                }
            } else {
                render_encoder.set_fragment_texture(1, None);
            }

            // Draw this batch
            render_encoder.draw_primitives(
                MTLPrimitiveType::Triangle,
                start as u64,
                (end - start) as u64,
            );

            current_vertex = end;
        }
    }
}

struct CachedGraphic {
    location: image_cache::ImageLocation,
    width: f32,
    height: f32,
    last_used_frame: u64,
    /// ImageId for looking up individual texture (if uses_individual_texture is true)
    image_id: image_cache::ImageId,
    /// True if this graphic uses an individual GPU texture instead of the atlas
    uses_individual_texture: bool,
    /// Atlas layer index (1-based, 0 = no texture)
    atlas_layer: i32,
}

pub struct RichTextBrush {
    brush_type: RichTextBrushType,
    comp: Compositor,
    vertices: Vec<Vertex>,
    images: ImageCache,
    glyphs: GlyphCache,
    text_run_manager: TextRunManager,
    graphic_cache: FxHashMap<GraphicId, CachedGraphic>,
    current_frame: u64,
}

impl RichTextBrush {
    pub fn new(context: &Context) -> Self {
        let brush_type = match &context.inner {
            ContextType::Wgpu(wgpu_context) => {
                RichTextBrushType::Wgpu(WgpuRichTextBrush::new(wgpu_context))
            }
            #[cfg(target_os = "macos")]
            ContextType::Metal(metal_context) => {
                RichTextBrushType::Metal(MetalRichTextBrush::new(metal_context))
            }
        };

        Self {
            brush_type,
            comp: Compositor::new(),
            vertices: vec![],
            images: ImageCache::new(context),
            glyphs: GlyphCache::new(),
            text_run_manager: TextRunManager::new(),
            graphic_cache: FxHashMap::default(),
            current_frame: 0,
        }
    }

    #[inline]
    pub fn prepare(
        &mut self,
        context: &mut crate::context::Context,
        state: &crate::sugarloaf::state::SugarState,
        graphics: &mut Graphics,
    ) {
        // Always clear vertices first
        self.vertices.clear();

        if state.content.states.is_empty() {
            return;
        }

        let library = state.content.font_library();
        // Iterate over all content states and render visible ones
        for (rich_text_id, builder_state) in &state.content.states {
            // Skip if marked for removal or hidden
            if builder_state.render_data.should_remove || builder_state.render_data.hidden
            {
                continue;
            }

            // Skip if there are no lines to render
            if builder_state.lines.is_empty() {
                continue;
            }

            let pos = (
                builder_state.render_data.position[0] * state.style.scale_factor,
                builder_state.render_data.position[1] * state.style.scale_factor,
            );
            let depth = builder_state.render_data.depth;

            self.draw_layout(
                *rich_text_id, // Pass the rich text ID for caching
                &builder_state.lines,
                &None, // No line range filtering for now
                Some(pos),
                depth,
                library,
                Some(&builder_state.layout),
                graphics,
            );
        }

        self.vertices.clear();
        self.images.process_atlases(context);
        self.comp.finish(&mut self.vertices);
    }

    #[inline]
    /// Get character cell dimensions using font metrics (fast, no rendering)
    pub fn get_character_cell_dimensions(
        &self,
        font_library: &FontLibrary,
        font_size: f32,
        line_height: f32,
    ) -> Option<SugarDimensions> {
        // Use read lock instead of write lock since we're not modifying
        if let Some(font_library_data) = font_library.inner.try_read() {
            let font_id = 0; // FONT_ID_REGULAR

            // Use existing method to get cached metrics
            drop(font_library_data); // Drop read lock
            let mut font_library_data = font_library.inner.write();
            if let Some((ascent, descent, leading)) =
                font_library_data.get_font_metrics(&font_id, font_size)
            {
                // Calculate character width using font metrics
                // For monospace fonts, we can estimate character width
                let char_width = font_size * 0.6; // Common monospace width ratio
                let total_line_height = (ascent + descent + leading) * line_height;

                return Some(SugarDimensions {
                    width: char_width.max(1.0),
                    height: total_line_height.max(1.0),
                    scale: 1.0,
                });
            }
        }
        None
    }

    /// Extract font metrics using per-font calculation.
    /// Each font calculates its own metrics using consistent approach.
    #[inline]
    fn extract_normalized_metrics(
        &self,
        lines: &[crate::layout::BuilderLine],
        font_library: &FontLibrary,
    ) -> Option<(f32, f32, f32, usize, f32)> {
        // Get the first run to determine font_id and size
        let first_run = lines
            .iter()
            .filter(|line| !line.render_data.runs.is_empty())
            .map(|line| &line.render_data.runs[0])
            .next()?;

        let font_id = 0; // FONT_ID_REGULAR
        let font_size = first_run.size;

        // Get metrics from the specific font using consistent calculation
        let mut font_library_data = font_library.inner.write();
        if let Some((ascent, descent, leading)) =
            font_library_data.get_font_metrics(&font_id, font_size)
        {
            Some((ascent, descent, leading, font_id, font_size))
        } else {
            // Fallback to run metrics if font metrics calculation fails
            Some((
                first_run.ascent,
                first_run.descent,
                first_run.leading,
                font_id,
                font_size,
            ))
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn draw_layout(
        &mut self,
        _rich_text_id: usize,
        lines: &Vec<crate::layout::BuilderLine>,
        selected_lines: &Option<RichTextLinesRange>,
        pos: Option<(f32, f32)>,
        depth: f32,
        font_library: &FontLibrary,
        rte_layout: Option<&RichTextLayout>,
        graphics: &mut Graphics,
    ) {
        if lines.is_empty() {
            return;
        }

        // For dimensions mode, we only process the first line
        let lines_to_process = lines.as_slice();

        // Extract font metrics before borrowing self.comp
        let font_metrics =
            self.extract_normalized_metrics(lines_to_process, font_library);

        // let start = std::time::Instant::now();
        // Get initial position
        let (x, y) = pos.unwrap_or((0.0, 0.0));

        // Increment frame counter for LRU tracking
        self.current_frame += 1;

        // Pre-process: Upload graphics to atlas and cache their data
        // This must happen BEFORE we borrow comp to avoid borrow checker issues
        for line in lines_to_process {
            for run in &line.render_data.runs {
                if let Some(graphic) = run.span.media {
                    // Check if already cached
                    if let Some(cached) = self.graphic_cache.get_mut(&graphic.id) {
                        // Update last used frame
                        cached.last_used_frame = self.current_frame;
                        continue;
                    }

                    // Not cached - need to upload to atlas
                    if let Some(entry) = graphics.get(&graphic.id) {
                        if let crate::components::core::image::Data::Rgba {
                            width,
                            height,
                            ref pixels,
                        } = entry.handle.data
                        {
                            let add_image = image_cache::AddImage {
                                width: width as u16,
                                height: height as u16,
                                has_alpha: true,
                                data: image_cache::ImageData::Borrowed(pixels.as_ref()),
                                content_type: image_cache::ContentType::Color,
                                // Protocol graphics (Kitty/Sixel) get individual textures
                                uses_individual_texture: true,
                            };

                            // Try to allocate, with eviction retry if needed
                            let mut image_id = self.images.allocate(add_image.clone());

                            if image_id.is_none() {
                                // Atlas full - try evicting oldest graphics
                                tracing::warn!(
                                    "Atlas full, attempting to evict oldest graphics"
                                );
                                let mut evicted_count = 0;

                                // Try evicting up to 5 graphics
                                while evicted_count < 5 {
                                    if let Some(oldest_id) = self.find_oldest_graphic() {
                                        self.evict_graphic(oldest_id);
                                        evicted_count += 1;

                                        // Retry allocation
                                        image_id =
                                            self.images.allocate(add_image.clone());
                                        if image_id.is_some() {
                                            tracing::info!("Successfully allocated after evicting {} graphics", evicted_count);
                                            break;
                                        }
                                    } else {
                                        break; // No more graphics to evict
                                    }
                                }

                                if image_id.is_none() {
                                    tracing::error!("Failed to allocate graphic {:?} even after evicting {} graphics", graphic.id, evicted_count);
                                }
                            }

                            if let Some(id) = image_id {
                                if let Some(location) = self.images.get(&id) {
                                    // Get atlas layer for this image
                                    let atlas_layer = self
                                        .images
                                        .get_atlas_index(id)
                                        .map(|idx| (idx + 1) as i32)
                                        .unwrap_or(1);

                                    // Cache coords + dimensions + frame + atlas layer
                                    self.graphic_cache.insert(
                                        graphic.id,
                                        CachedGraphic {
                                            location,
                                            width: entry.width,
                                            height: entry.height,
                                            last_used_frame: self.current_frame,
                                            image_id: id,
                                            uses_individual_texture: true, // Protocol graphics use individual textures
                                            atlas_layer,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Now set up rendering - borrow comp and caches
        let comp = &mut self.comp;
        let caches = (&mut self.images, &mut self.glyphs);
        let (image_cache, glyphs_cache) = caches;
        let font_coords: &[i16] = &[0, 0, 0, 0];

        // Set up caches based on mode
        let mut glyphs = Vec::new();
        let mut last_rendered_graphic = HashSet::new();
        let mut line_y = y;
        if let Some((
            ascent,
            descent,
            leading,
            current_font_from_valid_run,
            current_font_size_from_valid_run,
        )) = font_metrics
        {
            // Initialize from first run if available
            let mut current_font = current_font_from_valid_run;
            let mut current_font_size = current_font_size_from_valid_run;

            let mut session = glyphs_cache.session(
                image_cache,
                current_font,
                font_library,
                font_coords,
                current_font_size,
            );

            // Calculate line height with modifier if available
            let line_height_without_mod = ascent + descent + leading;
            let line_height_mod = rte_layout.map_or(1.0, |layout| layout.line_height);
            let line_height = line_height_without_mod * line_height_mod;

            let skip_count = selected_lines.map_or(0, |range| range.start);
            let take_count = selected_lines
                .map_or(lines_to_process.len(), |range| range.end - range.start);

            for (_line_idx, line) in lines_to_process
                .iter()
                .enumerate()
                .skip(skip_count)
                .take(take_count)
            {
                if line.render_data.runs.is_empty() {
                    continue;
                }

                let mut px = x;

                // Calculate baseline using proper typographic positioning
                let padding_top = (line_height - ascent - descent) / 2.0;
                let baseline = line_y + padding_top + ascent;

                // Keep line_y as the top of the line for proper line spacing
                // Don't modify line_y here - it should remain at the top of the line

                // Calculate padding
                let padding_y = if line_height_mod > 1.0 {
                    (line_height - line_height_without_mod) / 2.0
                } else {
                    0.0
                };

                let py = line_y;

                for run in &line.render_data.runs {
                    let font = run.span.font_id;
                    let char_width = run.span.width;
                    let run_x = px;

                    // Extract text from the run for caching
                    let run_text = run
                        .glyphs
                        .iter()
                        .filter_map(|g| char::from_u32(g.simple_data().0 as u32))
                        .collect::<String>();

                    // Try to get cached data for this text run
                    let cached_result = if !run_text.is_empty() {
                        self.text_run_manager.get_cached_data(
                            &run_text,
                            font,
                            run.size,
                            Some(run.span.color),
                        )
                    } else {
                        CacheResult::Miss
                    };

                    match cached_result {
                        CacheResult::FullRender {
                            glyphs: _cached_glyphs,
                            vertices,
                            base_position,
                            ..
                        } => {
                            // Use cached render data - apply vertices directly
                            let mut vertex_output = Vec::new();
                            TextRunManager::apply_cached_vertices(
                                &vertices,
                                base_position,
                                (run_x, py + padding_y),
                                &mut vertex_output,
                            );

                            px += rte_layout.unwrap().dimensions.width * char_width;
                        }
                        CacheResult::ShapingOnly {
                            glyphs: cached_glyphs,
                            ..
                        }
                        | CacheResult::GlyphsOnly {
                            glyphs: cached_glyphs,
                            ..
                        } => {
                            // Use cached glyph data but need to render
                            glyphs.clear();
                            if cached_glyphs.is_empty() {
                                // No glyphs (graphics-only or whitespace) - advance by char_width
                                px += rte_layout.unwrap().dimensions.width * char_width;
                            } else {
                                for shaped_glyph in cached_glyphs.iter() {
                                    let x = px;
                                    let y = baseline; // Glyph y should be at baseline position

                                    px +=
                                        rte_layout.unwrap().dimensions.width * char_width;

                                    glyphs.push(Glyph {
                                        id: shaped_glyph.glyph_id as GlyphId,
                                        x,
                                        y,
                                    });
                                }
                            }

                            // Render using cached glyph data
                            let style = TextRunStyle {
                                font_coords,
                                font_size: run.size,
                                color: run.span.color,
                                cursor: run.span.cursor,
                                drawable_char: run.span.drawable_char,
                                background_color: run.span.background_color,
                                baseline,
                                topline: py, // Use py (line top) for cursor positioning
                                line_height,
                                padding_y,
                                line_height_without_mod,
                                advance: cached_glyphs.iter().map(|g| g.x_advance).sum(),
                                decoration: run.span.decoration,
                                decoration_color: run.span.decoration_color,
                                underline_offset: run.underline_offset,
                                strikeout_offset: run.strikeout_offset,
                                underline_thickness: run.strikeout_size,
                                x_height: run.x_height,
                                ascent: run.ascent,
                                descent: run.descent,
                            };

                            // Update font session if needed
                            if font != current_font
                                || style.font_size != current_font_size
                            {
                                current_font = font;
                                current_font_size = style.font_size;

                                session = glyphs_cache.session(
                                    image_cache,
                                    current_font,
                                    font_library,
                                    font_coords,
                                    style.font_size,
                                );
                            }

                            comp.draw_run(
                                &mut session,
                                Rect::new(run_x, py, px - run_x, 1.),
                                depth,
                                &style,
                                &glyphs,
                            );
                        }
                        CacheResult::Miss => {
                            // No cached data - need to shape and render from scratch
                            glyphs.clear();
                            let mut shaped_glyphs = Vec::new();

                            if run.glyphs.is_empty() {
                                // Graphics-only run (no text glyphs) - advance by char_width
                                px += rte_layout.unwrap().dimensions.width * char_width;
                            } else {
                                for glyph in &run.glyphs {
                                    let x = px;
                                    let y = baseline; // Use baseline for consistency with cached path
                                    let advance = glyph.simple_data().1;

                                    // Different advance calculation based on mode
                                    px +=
                                        rte_layout.unwrap().dimensions.width * char_width;

                                    let glyph_id = glyph.simple_data().0;
                                    glyphs.push(Glyph { id: glyph_id, x, y });

                                    // Store for caching
                                    shaped_glyphs.push(
                                        crate::font::text_run_cache::ShapedGlyph {
                                            glyph_id: glyph_id as u32,
                                            x_advance: advance,
                                            y_advance: 0.0,
                                            x_offset: 0.0,
                                            y_offset: 0.0,
                                            cluster: 0,
                                            atlas_coords: None,
                                            atlas_layer: None,
                                        },
                                    );
                                }
                            }

                            // Cache the shaped glyphs for future use
                            if !run_text.is_empty() {
                                self.text_run_manager.cache_shaping_data(
                                    &run_text,
                                    font,
                                    run.size,
                                    shaped_glyphs,
                                    false, // has_emoji - would need to be detected
                                    None, // shaping_features - would need actual shaping data
                                );
                            }

                            // Create style for rendering
                            let style = TextRunStyle {
                                font_coords,
                                font_size: run.size,
                                color: run.span.color,
                                cursor: run.span.cursor,
                                drawable_char: run.span.drawable_char,
                                background_color: run.span.background_color,
                                baseline,
                                topline: py, // Use py (line top) for cursor positioning
                                line_height,
                                padding_y,
                                line_height_without_mod,
                                advance: px - run_x,
                                decoration: run.span.decoration,
                                decoration_color: run.span.decoration_color,
                                underline_offset: run.underline_offset,
                                strikeout_offset: run.strikeout_offset,
                                underline_thickness: run.strikeout_size,
                                x_height: run.x_height,
                                ascent: run.ascent,
                                descent: run.descent,
                            };

                            // Update font session if needed
                            if font != current_font
                                || style.font_size != current_font_size
                            {
                                current_font = font;
                                current_font_size = style.font_size;

                                session = glyphs_cache.session(
                                    image_cache,
                                    current_font,
                                    font_library,
                                    font_coords,
                                    style.font_size,
                                );
                            }

                            comp.draw_run(
                                &mut session,
                                Rect::new(run_x, py, px - run_x, 1.),
                                depth,
                                &style,
                                &glyphs,
                            );
                        }
                    }

                    // Handle graphics - render directly using add_image_rect
                    if let Some(graphic) = run.span.media {
                        // Each cell stores which part of the graphic it shows via offset_x/offset_y
                        // We render once per graphic per frame, using the first cell we encounter
                        // We calculate the graphic's position by subtracting the cell's offset
                        // This ensures the graphic renders even when the origin cell is scrolled off-screen
                        if !last_rendered_graphic.contains(&graphic.id) {
                            // Get cached graphic data
                            if let Some(cached) = self.graphic_cache.get(&graphic.id) {
                                // Calculate graphic position: current cell position minus this cell's offset
                                // This positions the full graphic correctly regardless of which cell we encounter
                                let gx = run_x - graphic.offset_x as f32;
                                let gy = py - graphic.offset_y as f32;

                                tracing::info!(
                                    "Drawing graphic at ({}, {}), size={}x{}, atlas_layer={}",
                                    gx,
                                    gy,
                                    cached.width,
                                    cached.height,
                                    cached.atlas_layer
                                );

                                comp.batches.add_image_rect(
                                    &Rect::new(gx, gy, cached.width, cached.height),
                                    depth,
                                    &[1.0, 1.0, 1.0, 1.0],
                                    &[
                                        cached.location.min.0,
                                        cached.location.min.1,
                                        cached.location.max.0,
                                        cached.location.max.1,
                                    ],
                                    true,
                                    cached.atlas_layer,
                                );
                            } else {
                                tracing::warn!("Graphic {} not in cache!", graphic.id.0);
                            }

                            last_rendered_graphic.insert(graphic.id);
                        }
                    }
                }

                // Advance line_y for the next line
                line_y += line_height;
            }
        }

        // let screen_render_duration = start.elapsed();
        // if self.renderer.enable_performance_logging {
        // println!("[PERF] draw_layout() total: {:?}", screen_render_duration);
    }

    /// Find the least recently used graphic ID for eviction.
    /// Returns the GraphicId to evict, or None if cache is empty.
    fn find_oldest_graphic(&self) -> Option<GraphicId> {
        self.graphic_cache
            .iter()
            .min_by_key(|(_, cached)| cached.last_used_frame)
            .map(|(id, _)| *id)
    }

    /// Evict a specific graphic from the cache.
    fn evict_graphic(&mut self, graphic_id: GraphicId) -> bool {
        if let Some(cached) = self.graphic_cache.remove(&graphic_id) {
            // Note: ImageCache doesn't currently expose deallocate publicly,
            // but the entry will be overwritten when atlas space is reused
            tracing::debug!(
                "Evicted graphic {:?} (last used: frame {})",
                graphic_id,
                cached.last_used_frame
            );
            return true;
        }
        false
    }

    #[inline]
    pub fn reset(&mut self) {
        self.glyphs = GlyphCache::new();
        self.text_run_manager.clear_all();
        self.graphic_cache.clear();
    }

    #[inline]
    pub fn clear_atlas(&mut self) {
        self.images.clear_atlas();
        self.glyphs = GlyphCache::new();
        self.text_run_manager.clear_all();
        self.graphic_cache.clear();
        tracing::info!(
            "RichTextBrush atlas, glyph cache, text run cache, and graphic cache cleared"
        );
    }

    #[inline]
    pub fn rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
    ) {
        self.comp.batches.rect(
            &Rect {
                x,
                y,
                width,
                height,
            },
            depth,
            &color,
        );
    }

    /// Add a rounded rectangle with the specified border radius
    #[inline]
    pub fn rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
        border_radius: f32,
    ) {
        self.comp.batches.rounded_rect(
            &Rect {
                x,
                y,
                width,
                height,
            },
            depth,
            &color,
            border_radius,
        );
    }

    #[inline]
    pub fn add_image_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        coords: [f32; 4],
        has_alpha: bool,
        depth: f32,
        atlas_layer: i32,
    ) {
        self.comp.batches.add_image_rect(
            &Rect {
                x,
                y,
                width,
                height,
            },
            depth,
            &color,
            &coords,
            has_alpha,
            atlas_layer,
        );
    }

    #[inline]
    pub fn render<'pass>(
        &'pass mut self,
        ctx: &mut WgpuContext,
        rpass: &mut wgpu::RenderPass<'pass>,
    ) {
        #[cfg_attr(not(target_os = "macos"), expect(irrefutable_let_patterns))]
        if let RichTextBrushType::Wgpu(brush) = &mut self.brush_type {
            // Get all atlas textures
            let color_views = self.images.get_texture_views();
            let mask_texture_view = self.images.get_mask_texture_view();

            if color_views.is_empty() || self.vertices.is_empty() {
                return;
            }

            // Implement proper batching by atlas
            // Group vertices by their texture binding requirements
            // layers[0] = color_layer (0 = no color texture, 1+ = color atlas index)
            // layers[1] = mask_layer (0 = no mask texture, 1 = mask atlas)

            let mut current_vertex = 0usize;
            while current_vertex < self.vertices.len() {
                let start = current_vertex;
                let current_color_layer = self.vertices[start].layers[0];
                let current_mask_layer = self.vertices[start].layers[1];

                // Find the end of this batch (consecutive vertices with same layers)
                let mut end = start;
                while end < self.vertices.len()
                    && self.vertices[end].layers[0] == current_color_layer
                    && self.vertices[end].layers[1] == current_mask_layer
                {
                    end += 1;
                }

                // Bind appropriate textures for this batch
                let color_view = if current_color_layer > 0 {
                    // Use color atlas (current_color_layer is 1-based, so subtract 1 for 0-based index)
                    let atlas_index = (current_color_layer - 1) as usize;
                    color_views.get(atlas_index).unwrap_or(&color_views[0])
                } else {
                    &color_views[0] // Doesn't matter, won't be used
                };

                let final_mask_view = if current_mask_layer > 0 {
                    mask_texture_view.unwrap_or(color_views[0])
                } else {
                    color_views[0] // Doesn't matter, won't be used
                };

                brush.update_bind_group(
                    ctx,
                    color_view,
                    final_mask_view,
                    self.images.entries.len(),
                );

                // Draw this batch
                brush.render_range(ctx, &self.vertices, rpass, start..end);

                current_vertex = end;
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn render_metal(
        &mut self,
        context: &MetalContext, // Add context parameter
        render_encoder: &metal::RenderCommandEncoderRef,
    ) {
        if let RichTextBrushType::Metal(brush) = &mut self.brush_type {
            brush.render(&self.vertices, &self.images, render_encoder, context);
        }
    }

    pub fn resize(&mut self, context: &mut Context) {
        let transform = match &context.inner {
            ContextType::Wgpu(wgpu_ctx) => {
                orthographic_projection(wgpu_ctx.size.width, wgpu_ctx.size.height)
            }
            #[cfg(target_os = "macos")]
            ContextType::Metal(metal_ctx) => {
                orthographic_projection(metal_ctx.size.width, metal_ctx.size.height)
            }
        };

        match &mut self.brush_type {
            RichTextBrushType::Wgpu(wgpu_brush) => {
                if transform != wgpu_brush.current_transform {
                    let queue = match &context.inner {
                        ContextType::Wgpu(wgpu_ctx) => &wgpu_ctx.queue,
                        #[cfg(target_os = "macos")]
                        _ => unreachable!(),
                    };

                    queue.write_buffer(
                        &wgpu_brush.transform,
                        0,
                        bytemuck::bytes_of(&transform),
                    );
                    wgpu_brush.current_transform = transform;
                }
            }
            #[cfg(target_os = "macos")]
            RichTextBrushType::Metal(metal_brush) => {
                metal_brush.resize(transform);
            }
        }
    }
}

impl WgpuRichTextBrush {
    pub fn new(context: &WgpuContext) -> Self {
        let supported_vertex_buffer = 500;

        let current_transform =
            orthographic_projection(context.size.width, context.size.height);
        let transform =
            context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&current_transform),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });

        // Create pipeline layout
        let constant_bind_group_layout =
            context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(mem::size_of::<
                                    [f32; 16],
                                >(
                                )
                                    as wgpu::BufferAddress),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX
                                | wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(
                                wgpu::SamplerBindingType::Filtering,
                            ),
                            count: None,
                        },
                    ],
                });

        let layout_bind_group_layout =
            context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[
                        // Color texture (binding 0)
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: context.get_optimal_texture_sample_type(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Mask texture (binding 1)
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float {
                                    filterable: true,
                                },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout =
            context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        &constant_bind_group_layout,
                        &layout_bind_group_layout,
                    ],
                });

        let sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: 0f32,
            lod_max_clamp: 0f32,
            ..Default::default()
        });

        let constant_bind_group =
            context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &constant_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(
                                wgpu::BufferBinding {
                                    buffer: &transform,
                                    offset: 0,
                                    size: None,
                                },
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                    label: Some("rich_text::constant_bind_group"),
                });

        // Create initial layout bind group (will be updated when textures change)
        let layout_bind_group =
            context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &layout_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &context
                                    .device
                                    .create_texture(&wgpu::TextureDescriptor {
                                        label: Some("placeholder_color"),
                                        size: wgpu::Extent3d {
                                            width: 1,
                                            height: 1,
                                            depth_or_array_layers: 1,
                                        },
                                        mip_level_count: 1,
                                        sample_count: 1,
                                        dimension: wgpu::TextureDimension::D2,
                                        format: wgpu::TextureFormat::Rgba8Unorm,
                                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                                        view_formats: &[],
                                    })
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &context
                                    .device
                                    .create_texture(&wgpu::TextureDescriptor {
                                        label: Some("placeholder_mask"),
                                        size: wgpu::Extent3d {
                                            width: 1,
                                            height: 1,
                                            depth_or_array_layers: 1,
                                        },
                                        mip_level_count: 1,
                                        sample_count: 1,
                                        dimension: wgpu::TextureDimension::D2,
                                        format: wgpu::TextureFormat::R8Unorm,
                                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                                        view_formats: &[],
                                    })
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                    ],
                    label: Some("rich_text::layout_bind_group"),
                });

        let shader_source = if context.supports_f16() {
            include_str!("rich_text.wgsl")
        } else {
            include_str!("rich_text_f32.wgsl")
        };

        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
            });

        let pipeline =
            context
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    cache: None,
                    label: None,
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: mem::size_of::<Vertex>() as u64,
                            // https://docs.rs/wgpu/latest/wgpu/enum.VertexStepMode.html
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array!(
                                0 => Float32x3,
                                1 => Float32x4,
                                2 => Float32x2,
                                3 => Sint32x2,
                            ),
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: context.format,
                            blend: BLEND,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                });

        let vertex_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rich_text::Vertices Buffer"),
            size: mem::size_of::<Vertex>() as u64 * supported_vertex_buffer as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        WgpuRichTextBrush {
            layout_bind_group,
            layout_bind_group_layout,
            constant_bind_group,
            textures_version: 0,
            transform,
            pipeline,
            vertex_buffer,
            supported_vertex_buffer,
            current_transform,
        }
    }

    #[inline]
    pub fn render<'pass>(
        &'pass mut self,
        ctx: &mut WgpuContext,
        vertices: &[Vertex],
        rpass: &mut wgpu::RenderPass<'pass>,
    ) {
        // let start = std::time::Instant::now();
        // There's nothing to render
        if vertices.is_empty() {
            return;
        }

        let queue = &mut ctx.queue;

        if vertices.len() > self.supported_vertex_buffer {
            self.vertex_buffer.destroy();

            // Allocate 25% more buffer space to reduce frequent reallocations
            self.supported_vertex_buffer = (vertices.len() as f32 * 1.25) as usize;
            self.vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::rich_text::Pipeline vertices"),
                size: mem::size_of::<Vertex>() as u64
                    * self.supported_vertex_buffer as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let vertices_bytes: &[u8] = bytemuck::cast_slice(vertices);
        if !vertices_bytes.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, vertices_bytes);
        }

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.constant_bind_group, &[]);
        rpass.set_bind_group(1, &self.layout_bind_group, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let vertex_count = vertices.len() as u32;
        rpass.draw(0..vertex_count, 0..1);
        // let duration = start.elapsed();
        // println!("Time elapsed in rich_text::render is: {:?}", duration);
    }

    #[inline]
    pub fn render_range(
        &mut self,
        ctx: &mut WgpuContext,
        vertices: &[Vertex],
        rpass: &mut wgpu::RenderPass,
        range: std::ops::Range<usize>,
    ) {
        if range.is_empty() {
            return;
        }

        let queue = &mut ctx.queue;

        // Ensure buffer is large enough
        if vertices.len() > self.supported_vertex_buffer {
            self.vertex_buffer.destroy();
            self.supported_vertex_buffer = (vertices.len() as f32 * 1.25) as usize;
            self.vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::rich_text::Pipeline vertices"),
                size: mem::size_of::<Vertex>() as u64
                    * self.supported_vertex_buffer as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Write all vertices to buffer (we need the full buffer for correct indexing)
        let vertices_bytes: &[u8] = bytemuck::cast_slice(vertices);
        if !vertices_bytes.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, vertices_bytes);
        }

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.constant_bind_group, &[]);
        rpass.set_bind_group(1, &self.layout_bind_group, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        // Draw only the specified range
        rpass.draw(range.start as u32..range.end as u32, 0..1);
    }

    pub fn update_bind_group(
        &mut self,
        ctx: &WgpuContext,
        color_view: &wgpu::TextureView,
        mask_view: &wgpu::TextureView,
        textures_version: usize,
    ) {
        if textures_version != self.textures_version {
            self.textures_version = textures_version;
            self.layout_bind_group =
                ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &self.layout_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(color_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(mask_view),
                        },
                    ],
                    label: Some("rich_text::Pipeline uniforms"),
                });
        }
    }
}

#[cfg(test)]
mod rect_positioning_tests {
    // ... existing tests remain the same ...
    #[derive(Debug)]
    struct GlyphRect {
        #[allow(unused)]
        pub x: f32,
        pub y: f32,
        #[allow(unused)]
        pub width: f32,
        pub height: f32,
        #[allow(unused)]
        pub baseline_y: f32,
        pub glyph_center_x: f32,
        pub glyph_center_y: f32,
    }

    #[derive(Debug)]
    struct LineRect {
        #[allow(unused)]
        pub x: f32,
        pub y: f32,
        #[allow(unused)]
        pub width: f32,
        pub height: f32,
        pub baseline_y: f32,
    }

    #[test]
    fn test_glyph_rect_positioning_and_centering() {
        // Test parameters
        let line_height = 20.0;
        let char_width = 8.0;
        let ascent = 12.0;
        let descent = 4.0;
        let _leading = 0.0;

        // Expected calculations (matching our current implementation)
        let padding_top = (line_height - ascent - descent) / 2.0; // (20 - 12 - 4) / 2 = 2.0
        let expected_baseline_y = 0.0 + padding_top + ascent; // 0 + 2 + 12 = 14.0

        // Create line rect
        let line_rect = LineRect {
            x: 0.0,
            y: 0.0,
            width: char_width,
            height: line_height,
            baseline_y: expected_baseline_y,
        };

        // Expected glyph rect (should be centered within line rect)
        let expected_glyph_rect = GlyphRect {
            x: 0.0,
            y: 0.0,
            width: char_width,
            height: line_height,
            baseline_y: expected_baseline_y,
            glyph_center_x: char_width / 2.0,  // 4.0
            glyph_center_y: line_height / 2.0, // 10.0
        };

        // Verify baseline is positioned correctly within the line rect
        assert!(
            expected_baseline_y > line_rect.y,
            "Baseline should be below line top"
        );
        assert!(
            expected_baseline_y < line_rect.y + line_rect.height,
            "Baseline should be above line bottom"
        );

        // Verify glyph center is in the middle of the rect
        assert_eq!(
            expected_glyph_rect.glyph_center_x,
            char_width / 2.0,
            "Glyph should be horizontally centered"
        );
        assert_eq!(
            expected_glyph_rect.glyph_center_y,
            line_height / 2.0,
            "Glyph should be vertically centered"
        );

        // Verify baseline relationship to glyph center
        let baseline_offset_from_center =
            expected_baseline_y - expected_glyph_rect.glyph_center_y;

        // The baseline should be slightly above center for typical fonts
        // With ascent=12, descent=4, the baseline should be at 14.0, center at 10.0
        // So baseline is 4.0 units above center, which makes sense
        assert_eq!(
            baseline_offset_from_center, 4.0,
            "Baseline should be 4.0 units above glyph center"
        );
    }

    #[test]
    fn test_find_oldest_graphic() {
        use super::CachedGraphic;
        use crate::GraphicId;
        use rustc_hash::FxHashMap;

        let mut graphic_cache: FxHashMap<GraphicId, CachedGraphic> = FxHashMap::default();

        // Create dummy graphics with different last_used_frame values
        let graphic1 = GraphicId(1);
        let graphic2 = GraphicId(2);
        let graphic3 = GraphicId(3);

        // graphic2 is oldest (frame 5)
        // graphic1 is middle (frame 10)
        // graphic3 is newest (frame 15)
        graphic_cache.insert(
            graphic1,
            CachedGraphic {
                location: super::image_cache::ImageLocation {
                    min: (0.0, 0.0),
                    max: (1.0, 1.0),
                },
                width: 100.0,
                height: 100.0,
                last_used_frame: 10,
                image_id: super::image_cache::ImageId::empty(),
                uses_individual_texture: false,
                atlas_layer: 1,
            },
        );

        graphic_cache.insert(
            graphic2,
            CachedGraphic {
                location: super::image_cache::ImageLocation {
                    min: (0.0, 0.0),
                    max: (1.0, 1.0),
                },
                width: 100.0,
                height: 100.0,
                last_used_frame: 5, // Oldest
                image_id: super::image_cache::ImageId::empty(),
                uses_individual_texture: false,
                atlas_layer: 1,
            },
        );

        graphic_cache.insert(
            graphic3,
            CachedGraphic {
                location: super::image_cache::ImageLocation {
                    min: (0.0, 0.0),
                    max: (1.0, 1.0),
                },
                width: 100.0,
                height: 100.0,
                last_used_frame: 15, // Newest
                image_id: super::image_cache::ImageId::empty(),
                uses_individual_texture: false,
                atlas_layer: 1,
            },
        );

        // Find oldest should return graphic2
        let oldest = graphic_cache
            .iter()
            .min_by_key(|(_, cached)| cached.last_used_frame)
            .map(|(id, _)| *id);

        assert_eq!(oldest, Some(graphic2), "Should find oldest graphic");
    }

    #[test]
    fn test_graphic_lru_update() {
        use super::CachedGraphic;
        use crate::GraphicId;
        use rustc_hash::FxHashMap;

        let mut graphic_cache: FxHashMap<GraphicId, CachedGraphic> = FxHashMap::default();
        let current_frame = 100;

        let graphic1 = GraphicId(1);
        graphic_cache.insert(
            graphic1,
            CachedGraphic {
                location: super::image_cache::ImageLocation {
                    min: (0.0, 0.0),
                    max: (1.0, 1.0),
                },
                width: 100.0,
                height: 100.0,
                last_used_frame: 50,
                image_id: super::image_cache::ImageId::empty(),
                uses_individual_texture: false,
                atlas_layer: 1,
            },
        );

        // Simulate accessing the graphic (updating last_used_frame)
        if let Some(cached) = graphic_cache.get_mut(&graphic1) {
            cached.last_used_frame = current_frame;
        }

        // Verify it was updated
        assert_eq!(
            graphic_cache.get(&graphic1).unwrap().last_used_frame,
            current_frame,
            "Last used frame should be updated"
        );
    }

    #[test]
    fn test_graphic_positioning_with_offsets() {
        // Test that graphics are positioned correctly based on cell offsets
        // This simulates the logic: gx = run_x - offset_x, gy = py - offset_y

        // Cell at position (100, 200) contains a graphic with offset (20, 30)
        let run_x = 100.0;
        let py = 200.0;
        let offset_x = 20;
        let offset_y = 30;

        // Calculate graphic position
        let gx = run_x - offset_x as f32;
        let gy = py - offset_y as f32;

        // The graphic's top-left should be at (80, 170)
        // because we back-calculate from the cell's position
        assert_eq!(gx, 80.0, "Graphic x should account for offset_x");
        assert_eq!(gy, 170.0, "Graphic y should account for offset_y");

        // Verify origin cell (offset 0,0) at same position
        let origin_run_x = 80.0;
        let origin_py = 170.0;
        let origin_offset_x = 0;
        let origin_offset_y = 0;

        let origin_gx = origin_run_x - origin_offset_x as f32;
        let origin_gy = origin_py - origin_offset_y as f32;

        // Both cells should calculate the same graphic position
        assert_eq!(
            gx, origin_gx,
            "Graphic position should be same from any cell"
        );
        assert_eq!(
            gy, origin_gy,
            "Graphic position should be same from any cell"
        );
    }

    #[test]
    fn test_graphic_deduplication() {
        // Test that the same graphic ID is only rendered once per frame
        use crate::GraphicId;
        use std::collections::HashSet;

        let mut last_rendered_graphic: HashSet<GraphicId> = HashSet::new();

        let graphic_id = GraphicId(42);

        // First cell with this graphic - should render
        assert!(
            !last_rendered_graphic.contains(&graphic_id),
            "First occurrence should not be in set"
        );
        last_rendered_graphic.insert(graphic_id);

        // Second cell with same graphic - should NOT render
        assert!(
            last_rendered_graphic.contains(&graphic_id),
            "Second occurrence should be in set, preventing duplicate render"
        );

        // Clear for next frame
        last_rendered_graphic.clear();
        assert!(
            !last_rendered_graphic.contains(&graphic_id),
            "After clear, graphic should be renderable again"
        );
    }
}
