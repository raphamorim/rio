// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use ash::vk;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::atlas::{AtlasSlot, GlyphKey, RasterizedGlyph};
use super::cell::{CellBg, CellText, GridUniforms};
use crate::context::vulkan::{
    allocate_host_visible_buffer_raw, VkShared, VulkanBuffer, VulkanContext, VulkanImage,
    FRAMES_IN_FLIGHT,
};
use crate::renderer::image_cache::atlas::AtlasAllocator;

// Compiled at build time by `sugarloaf/build.rs`. Source GLSL lives
// in `sugarloaf/src/grid/shaders/`; edit those, not the .spv.
const BG_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/grid_bg.vert.spv"));
const BG_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/grid_bg.frag.spv"));
const TEXT_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/grid_text.vert.spv"));
const TEXT_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/grid_text.frag.spv"));

/// Extra slots appended to `fg_rows` for cursor glyphs. Mirrors the
/// Metal layout so the CPU emit code is byte-identical.
const CURSOR_ROW_SLOTS: usize = 2;

/// Per-page atlas side. 2048² @ R8 = 4 MiB; @ RGBA8 = 16 MiB. Pages
/// are never resized at runtime; this is the side every page is
/// created at.
const ATLAS_PAGE_SIZE: u16 = 2048;

/// Cap on pages per kind. 16 pages × 2048² = 64 M pixels, matching the
/// effective area of an 8192² atlas while keeping each page small
/// enough to allocate on demand without ever touching an in-use image.
/// Memory is lazy — typical sessions use 1–2 pages.
const MAX_PAGES: usize = 16;

/// One pending glyph upload — `bytes` were copied at insert time, so
/// the rasterizer's buffer can be reused immediately. Drained by
/// `flush_uploads` on the next frame's command buffer.
struct PendingUpload {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    bytes: Vec<u8>,
}

/// One contiguous run of cells in the per-slot foreground vertex
/// buffer that all share the same `(atlas_kind, page)`. `render_text`
/// binds the matching page's descriptor set + a push constant for the
/// kind and issues one `cmd_draw` per bucket.
#[derive(Clone, Copy)]
struct TextBucket {
    kind: u8,
    page: u8,
    start: u32,
    count: u32,
}

/// One atlas page: backing image, slot allocator, per-page upload
/// queue, and a descriptor set bound to the image + the shared
/// sampler.
///
/// Pages are appended on demand by `VulkanGlyphAtlas::insert` and
/// never resized, copied, or freed at runtime — so the descriptor set
/// is written once at creation and never updated again, sidestepping
/// the "update a bound descriptor while in-flight frames sample it"
/// hazard that resizing a single atlas image had.
struct VulkanAtlasPage {
    image: VulkanImage,
    allocator: AtlasAllocator,
    pending: Vec<PendingUpload>,
    /// `true` once the image has been transitioned out of `UNDEFINED`.
    /// `make_page` does the initial `UNDEFINED → SHADER_READ_ONLY`
    /// transition synchronously, so this is `true` from page birth;
    /// kept around so `upload_to_page` can pick the right
    /// `src_stage`/`src_access` for its leading barrier — same shape
    /// the previous single-image atlas's flag had.
    initialized: bool,
    /// Descriptor set with the page's image + the shared sampler at
    /// binding 0. Allocated from the renderer's descriptor pool when
    /// the page is created; bound by `render_text` for each draw of
    /// cells that live in this page.
    descriptor_set: vk::DescriptorSet,
}

/// Per-kind glyph atlas as a list of fixed-size pages.
///
/// One instance per atlas kind (R8 grayscale, RGBA8 color). Owned by
/// either `VulkanGridRenderer` (per-panel terminal grids) or
/// `sugarloaf::text::Text`'s Vulkan state (UI overlay text).
///
/// On `insert`, the atlas tries each existing page in order; if none
/// has room it appends a new page (up to `MAX_PAGES`). Old pages and
/// their slot coordinates stay valid forever — nothing is ever
/// resized, copied, or freed at runtime. That removes the
/// use-after-free hazard of resizing an in-use image and the
/// `TRANSFER_SRC` usage requirement (no GPU-side copy).
///
/// The descriptor pool, set layout, and sampler are owned by the
/// caller (the renderer) so all pages across both atlases share one
/// pool and one layout; the atlas just allocates a fresh set from
/// that pool whenever it grows a page.
pub struct VulkanGlyphAtlas {
    pages: Vec<VulkanAtlasPage>,
    slots: FxHashMap<GlyphKey, AtlasSlot>,
    format: vk::Format,
    bytes_per_pixel: u32,
    /// One staging buffer per frame-in-flight slot, sized for the
    /// largest single-frame upload (sum across all pages). Reused
    /// across frames within a slot — the `acquire_frame` fence wait
    /// inside `VulkanContext` proves the previous use of slot N's
    /// staging is GPU-complete.
    staging: [Option<crate::context::vulkan::VulkanBuffer>; FRAMES_IN_FLIGHT],
    staging_capacity: [usize; FRAMES_IN_FLIGHT],
    /// Pieces needed to spin up additional pages on demand from
    /// inside `insert` (which has no `&VulkanContext` borrow).
    shared: Arc<VkShared>,
    sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    queue: vk::Queue,
    queue_family_index: u32,
}

impl VulkanGlyphAtlas {
    pub fn new_grayscale(
        ctx: &VulkanContext,
        descriptor_pool: vk::DescriptorPool,
        descriptor_set_layout: vk::DescriptorSetLayout,
        sampler: vk::Sampler,
    ) -> Self {
        Self::new(
            ctx,
            vk::Format::R8_UNORM,
            1,
            descriptor_pool,
            descriptor_set_layout,
            sampler,
        )
    }

    pub fn new_color(
        ctx: &VulkanContext,
        descriptor_pool: vk::DescriptorPool,
        descriptor_set_layout: vk::DescriptorSetLayout,
        sampler: vk::Sampler,
    ) -> Self {
        Self::new(
            ctx,
            vk::Format::R8G8B8A8_UNORM,
            4,
            descriptor_pool,
            descriptor_set_layout,
            sampler,
        )
    }

    fn new(
        ctx: &VulkanContext,
        format: vk::Format,
        bytes_per_pixel: u32,
        descriptor_pool: vk::DescriptorPool,
        descriptor_set_layout: vk::DescriptorSetLayout,
        sampler: vk::Sampler,
    ) -> Self {
        let shared = ctx.shared().clone();
        let queue = ctx.queue;
        let queue_family_index = ctx.queue_family_index;
        let initial_page = make_page(
            &shared,
            queue,
            queue_family_index,
            format,
            descriptor_pool,
            descriptor_set_layout,
            sampler,
        );
        Self {
            pages: vec![initial_page],
            slots: FxHashMap::default(),
            format,
            bytes_per_pixel,
            staging: std::array::from_fn(|_| None),
            staging_capacity: [0; FRAMES_IN_FLIGHT],
            shared,
            sampler,
            descriptor_pool,
            descriptor_set_layout,
            queue,
            queue_family_index,
        }
    }

    /// Drain each page's `pending` into the per-slot staging buffer
    /// (growing it if needed), then record one copy + barrier pair per
    /// non-empty page into `cmd`. Caller MUST be outside a
    /// dynamic-rendering pass — Vulkan 1.3 spec
    /// `VUID-vkCmdCopyBufferToImage-renderpass` forbids transfer
    /// commands inside one. No-op when no page has pending uploads.
    ///
    /// We take `&Arc<VkShared>` rather than `&VulkanContext` so the
    /// text overlay path can call this without holding an immutable
    /// borrow on the context (`Sugarloaf::render_vulkan` keeps
    /// `ctx: &mut VulkanContext` for the swapchain acquire/present
    /// cycle).
    pub fn flush_uploads(
        &mut self,
        shared: &Arc<VkShared>,
        cmd: vk::CommandBuffer,
        slot: usize,
    ) {
        let total_bytes: usize = self
            .pages
            .iter()
            .flat_map(|p| p.pending.iter())
            .map(|u| (u.w as usize) * (u.h as usize) * self.bytes_per_pixel as usize)
            .sum();
        if total_bytes == 0 {
            return;
        }

        // Grow per-slot staging if needed. The `max(256K)` floor keeps
        // us from churning allocations during the first-frame burst.
        if total_bytes > self.staging_capacity[slot] {
            let new_cap = total_bytes.next_power_of_two().max(256 * 1024);
            self.staging[slot] = Some(allocate_host_visible_buffer_raw(
                shared,
                new_cap as u64,
                vk::BufferUsageFlags::TRANSFER_SRC,
            ));
            self.staging_capacity[slot] = new_cap;
        }
        let staging = self.staging[slot].as_ref().unwrap();
        let staging_ptr = staging.as_mut_ptr();
        let staging_handle = staging.handle();
        let bpp = self.bytes_per_pixel as usize;

        let mut offset: u64 = 0;
        for page in &mut self.pages {
            if page.pending.is_empty() {
                continue;
            }
            let mut copies: Vec<vk::BufferImageCopy> =
                Vec::with_capacity(page.pending.len());
            unsafe {
                for upload in page.pending.drain(..) {
                    let bytes = (upload.w as usize) * (upload.h as usize) * bpp;
                    std::ptr::copy_nonoverlapping(
                        upload.bytes.as_ptr(),
                        staging_ptr.add(offset as usize),
                        bytes,
                    );
                    copies.push(image_copy_region(
                        offset, upload.x, upload.y, upload.w, upload.h,
                    ));
                    offset += bytes as u64;
                }
            }
            upload_to_page(&shared.raw, cmd, staging_handle, page, &copies);
        }
    }

    #[inline]
    pub fn lookup(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.slots.get(&key).copied()
    }

    /// Descriptor set for the given page index. Bound to set=1 by
    /// `render_text` for each `(kind, page)` bucket of cells.
    #[inline]
    pub fn page_descriptor_set(&self, page: u8) -> vk::DescriptorSet {
        self.pages[page as usize].descriptor_set
    }

    /// Number of pages currently allocated.
    #[inline]
    pub fn num_pages(&self) -> usize {
        self.pages.len()
    }

    /// Pack + queue a glyph for upload. Tries each existing page in
    /// order; if none have room, appends a new page (up to
    /// `MAX_PAGES`) and packs into that. Returns `None` only when the
    /// glyph won't fit anywhere even after appending the last allowed
    /// page — same contract as the previous single-image atlas
    /// hitting its hard cap.
    pub fn insert(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if glyph.width == 0 || glyph.height == 0 {
            // Whitespace / control glyphs — record an empty slot in
            // page 0 so lookups don't keep retrying. The slot has
            // zero area, so it doesn't actually consume page space.
            let slot = AtlasSlot {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                bearing_x: glyph.bearing_x,
                bearing_y: glyph.bearing_y,
                page: 0,
            };
            self.slots.insert(key, slot);
            return Some(slot);
        }

        // Try existing pages in order. Glyphs pack into the earliest
        // page that fits — keeps the active working set on page 0/1.
        for (i, page) in self.pages.iter_mut().enumerate() {
            if let Some((x, y)) =
                page.allocator.allocate(glyph.width, glyph.height)
            {
                let slot = AtlasSlot {
                    x,
                    y,
                    w: glyph.width,
                    h: glyph.height,
                    bearing_x: glyph.bearing_x,
                    bearing_y: glyph.bearing_y,
                    page: i as u8,
                };
                self.slots.insert(key, slot);
                page.pending.push(PendingUpload {
                    x,
                    y,
                    w: glyph.width,
                    h: glyph.height,
                    bytes: glyph.bytes.to_vec(),
                });
                return Some(slot);
            }
        }

        // No existing page fit — append a new one (within the cap).
        if self.pages.len() >= MAX_PAGES {
            return None;
        }
        let new_page = make_page(
            &self.shared,
            self.queue,
            self.queue_family_index,
            self.format,
            self.descriptor_pool,
            self.descriptor_set_layout,
            self.sampler,
        );
        let new_idx = self.pages.len();
        self.pages.push(new_page);
        let page = &mut self.pages[new_idx];
        let (x, y) = page.allocator.allocate(glyph.width, glyph.height)?;
        let slot = AtlasSlot {
            x,
            y,
            w: glyph.width,
            h: glyph.height,
            bearing_x: glyph.bearing_x,
            bearing_y: glyph.bearing_y,
            page: new_idx as u8,
        };
        self.slots.insert(key, slot);
        page.pending.push(PendingUpload {
            x,
            y,
            w: glyph.width,
            h: glyph.height,
            bytes: glyph.bytes.to_vec(),
        });
        Some(slot)
    }
}

/// Allocate a fresh atlas page: create the image + view, transition
/// it to `SHADER_READ_ONLY_OPTIMAL` via a oneshot submit, allocate a
/// descriptor set from `pool`, and point that set at the image +
/// sampler.
///
/// The transition + descriptor write are safe to do mid-frame because
/// the page is brand new — no submitted command buffer can reference
/// it yet, so there's no in-flight-frame hazard. Compare the previous
/// single-image atlas's `grow`, which had to either stall the device
/// or accept a use-after-free on every resize.
fn make_page(
    shared: &Arc<VkShared>,
    queue: vk::Queue,
    queue_family_index: u32,
    format: vk::Format,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    sampler: vk::Sampler,
) -> VulkanAtlasPage {
    let image = allocate_atlas_page_image(shared, format);
    let img_handle = image.handle();

    // Oneshot: UNDEFINED → SHADER_READ_ONLY so `upload_to_page`'s
    // `initialized` branch (SHADER_READ → TRANSFER_DST → SHADER_READ)
    // is correct from the very first upload, and so the layout matches
    // the `SHADER_READ_ONLY_OPTIMAL` declared in the descriptor write
    // below.
    submit_inline_oneshot(shared, queue, queue_family_index, |cmd| unsafe {
        let to_read = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(img_handle)
            .subresource_range(color_subresource_range());
        let barriers = [to_read];
        let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
        shared.raw.cmd_pipeline_barrier2(cmd, &dep);
    });

    let descriptor_set = allocate_one_descriptor_set(
        &shared.raw,
        descriptor_pool,
        descriptor_set_layout,
    );
    update_page_descriptor_set(&shared.raw, descriptor_set, &image, sampler);

    VulkanAtlasPage {
        image,
        allocator: AtlasAllocator::new(ATLAS_PAGE_SIZE, ATLAS_PAGE_SIZE),
        pending: Vec::new(),
        initialized: true,
        descriptor_set,
    }
}

/// Inline equivalent of `VulkanContext::allocate_sampled_image` for
/// callers that only hold an `Arc<VkShared>` — used by `make_page`
/// when growing the atlas mid-frame from inside `insert`.
fn allocate_atlas_page_image(shared: &Arc<VkShared>, format: vk::Format) -> VulkanImage {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width: ATLAS_PAGE_SIZE as u32,
            height: ATLAS_PAGE_SIZE as u32,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        // No `TRANSFER_SRC` — pages are never copied from; only the
        // glyph-byte staging buffer copies INTO them via TRANSFER_DST.
        .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let image = unsafe {
        shared
            .create_image(&image_info, None)
            .expect("atlas page: create_image")
    };

    let req = unsafe { shared.get_image_memory_requirements(image) };
    let mem_type = crate::context::vulkan::find_memory_type(
        &shared.instance,
        shared.physical_device,
        req.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )
    .expect("atlas page: no DEVICE_LOCAL memory type");

    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(mem_type);
    let memory = unsafe {
        shared
            .allocate_memory(&alloc_info, None)
            .expect("atlas page: allocate_memory")
    };
    unsafe {
        shared
            .bind_image_memory(image, memory, 0)
            .expect("atlas page: bind_image_memory");
    }

    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .components(vk::ComponentMapping::default())
        .subresource_range(color_subresource_range());
    let view = unsafe {
        shared
            .create_image_view(&view_info, None)
            .expect("atlas page: create_image_view")
    };

    VulkanImage {
        shared: shared.clone(),
        image,
        view,
        memory,
        width: ATLAS_PAGE_SIZE as u32,
        height: ATLAS_PAGE_SIZE as u32,
        format,
    }
}

/// Build, submit, and wait on a one-shot command buffer. Inline
/// equivalent of `VulkanContext::submit_oneshot` for callers that
/// only hold an `Arc<VkShared>` plus the queue handles — used by
/// `make_page` to do each new page's initial layout transition.
fn submit_inline_oneshot(
    shared: &Arc<VkShared>,
    queue: vk::Queue,
    queue_family_index: u32,
    record: impl FnOnce(vk::CommandBuffer),
) {
    unsafe {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let pool = shared
            .create_command_pool(&pool_info, None)
            .expect("oneshot: create_command_pool");

        let alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let cmd = shared
            .allocate_command_buffers(&alloc)
            .expect("oneshot: allocate_command_buffers")[0];

        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        shared.begin_command_buffer(cmd, &begin).expect("oneshot: begin");
        record(cmd);
        shared.end_command_buffer(cmd).expect("oneshot: end");

        let fence = shared
            .create_fence(&vk::FenceCreateInfo::default(), None)
            .expect("oneshot: create_fence");
        let cmds = [cmd];
        let submit = vk::SubmitInfo::default().command_buffers(&cmds);
        shared
            .queue_submit(queue, &[submit], fence)
            .expect("oneshot: queue_submit");
        shared
            .wait_for_fences(&[fence], true, u64::MAX)
            .expect("oneshot: wait_for_fences");

        shared.destroy_fence(fence, None);
        shared.destroy_command_pool(pool, None);
    }
}

/// Write a page's combined-image-sampler binding (=0) to point at
/// `image` + `sampler`. Called exactly once when the page is created;
/// the set is brand new at that point, so the write has no in-flight
/// hazard.
fn update_page_descriptor_set(
    device: &ash::Device,
    set: vk::DescriptorSet,
    image: &VulkanImage,
    sampler: vk::Sampler,
) {
    let image_info = vk::DescriptorImageInfo::default()
        .sampler(sampler)
        .image_view(image.view())
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    let image_infos = [image_info];
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(0)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&image_infos);
    unsafe { device.update_descriptor_sets(&[write], &[]) };
}

pub struct VulkanGridRenderer {
    /// Shared device handle. Cloned from `VulkanContext` at
    /// construction so this renderer's `Drop` can call `destroy_*` on
    /// pipelines/descriptor pools/etc. without depending on
    /// `VulkanContext` still being alive — `vkDestroyDevice` runs
    /// only when the last `Arc<VkShared>` clone (across all
    /// renderers, buffers, images, atlases) is dropped. See
    /// `VkShared`. Also lets `resize` (which only has `&mut self`)
    /// allocate bg buffers via `allocate_host_visible_buffer_raw`
    /// without needing a `&VulkanContext` borrow.
    shared: Arc<VkShared>,

    cols: u32,
    rows: u32,

    bg_buffers: [VulkanBuffer; FRAMES_IN_FLIGHT],
    bg_dirty: [bool; FRAMES_IN_FLIGHT],
    bg_cpu: Vec<CellBg>,

    uniform_buffers: [VulkanBuffer; FRAMES_IN_FLIGHT],

    bg_descriptor_pool: vk::DescriptorPool,
    bg_descriptor_set_layout: vk::DescriptorSetLayout,
    bg_descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    bg_pipeline_layout: vk::PipelineLayout,
    bg_pipeline: vk::Pipeline,

    fg_rows: Vec<Vec<CellText>>,
    fg_staging: Vec<CellText>,
    fg_buffers: [Option<VulkanBuffer>; FRAMES_IN_FLIGHT],
    fg_capacity: [usize; FRAMES_IN_FLIGHT],
    fg_live_count: [u32; FRAMES_IN_FLIGHT],
    fg_dirty: [bool; FRAMES_IN_FLIGHT],
    /// Per-slot bucket layout describing the ranges of cells in
    /// `fg_buffers[slot]` that share an `(atlas_kind, page)` —
    /// rebuilt whenever `fg_dirty[slot]` triggers a re-upload, walked
    /// by `render_text` to emit one draw per bucket.
    fg_buckets: [Vec<TextBucket>; FRAMES_IN_FLIGHT],

    text_uniform_descriptor_set_layout: vk::DescriptorSetLayout,
    /// Layout for one atlas page's descriptor set (a single combined
    /// image+sampler at binding 0). Shared across all pages of both
    /// atlases — pages have identical descriptor shape.
    text_atlas_descriptor_set_layout: vk::DescriptorSetLayout,
    /// Pool sized for `FRAMES_IN_FLIGHT` uniform sets plus
    /// `MAX_PAGES × 2` atlas page sets (grayscale + color atlases).
    text_descriptor_pool: vk::DescriptorPool,
    text_uniform_descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    text_pipeline_layout: vk::PipelineLayout,
    text_pipeline: vk::Pipeline,
    sampler: vk::Sampler,

    pub atlas_grayscale: VulkanGlyphAtlas,
    pub atlas_color: VulkanGlyphAtlas,

    needs_full_rebuild: bool,
}

impl VulkanGridRenderer {
    pub fn new(ctx: &VulkanContext, cols: u32, rows: u32) -> Self {
        let shared = ctx.shared().clone();
        let device = &shared.raw;

        let bg_buffers = std::array::from_fn(|_| alloc_bg_buffer(ctx, cols, rows));
        let uniform_buffers = std::array::from_fn(|_| {
            ctx.allocate_host_visible_buffer(
                std::mem::size_of::<GridUniforms>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )
        });

        let bg_descriptor_set_layout = create_bg_descriptor_set_layout(device);
        let bg_descriptor_pool = create_bg_descriptor_pool(device);
        let bg_descriptor_sets = allocate_descriptor_sets(
            device,
            bg_descriptor_pool,
            bg_descriptor_set_layout,
        );
        for slot in 0..FRAMES_IN_FLIGHT {
            update_bg_descriptor_set(
                device,
                bg_descriptor_sets[slot],
                &uniform_buffers[slot],
                &bg_buffers[slot],
            );
        }
        let bg_pipeline_layout =
            create_pipeline_layout(device, &[bg_descriptor_set_layout], &[]);
        let pipeline_cache = ctx.pipeline_cache();
        let bg_pipeline = create_bg_pipeline(
            device,
            pipeline_cache,
            bg_pipeline_layout,
            ctx.swapchain_format(),
        );

        // Text pipeline plumbing — built BEFORE the atlases so each
        // atlas can allocate its initial page's descriptor set from
        // the pool we hand it.
        let sampler = create_sampler(device);
        let text_uniform_descriptor_set_layout =
            create_text_uniform_descriptor_set_layout(device);
        let text_atlas_descriptor_set_layout =
            create_text_atlas_descriptor_set_layout(device);
        let text_descriptor_pool = create_text_descriptor_pool(device);

        let text_uniform_descriptor_sets = allocate_descriptor_sets(
            device,
            text_descriptor_pool,
            text_uniform_descriptor_set_layout,
        );
        for slot in 0..FRAMES_IN_FLIGHT {
            update_text_uniform_descriptor_set(
                device,
                text_uniform_descriptor_sets[slot],
                &uniform_buffers[slot],
            );
        }

        let atlas_grayscale = VulkanGlyphAtlas::new_grayscale(
            ctx,
            text_descriptor_pool,
            text_atlas_descriptor_set_layout,
            sampler,
        );
        let atlas_color = VulkanGlyphAtlas::new_color(
            ctx,
            text_descriptor_pool,
            text_atlas_descriptor_set_layout,
            sampler,
        );

        // Push constant `is_color: u32` switches sampling mode per
        // draw — see `grid_text.frag.glsl`. Fragment stage only;
        // 4 bytes at offset 0.
        let text_push_constants = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(4)];
        let text_pipeline_layout = create_pipeline_layout(
            device,
            &[
                text_uniform_descriptor_set_layout,
                text_atlas_descriptor_set_layout,
            ],
            &text_push_constants,
        );
        let text_pipeline = create_text_pipeline(
            device,
            pipeline_cache,
            text_pipeline_layout,
            ctx.swapchain_format(),
        );

        let bg_len = (cols as usize) * (rows as usize);
        Self {
            shared,
            cols,
            rows,
            bg_buffers,
            bg_dirty: [true; FRAMES_IN_FLIGHT],
            bg_cpu: vec![CellBg::TRANSPARENT; bg_len],
            uniform_buffers,
            bg_descriptor_pool,
            bg_descriptor_set_layout,
            bg_descriptor_sets,
            bg_pipeline_layout,
            bg_pipeline,
            fg_rows: init_fg_rows(rows),
            fg_staging: Vec::new(),
            fg_buffers: std::array::from_fn(|_| None),
            fg_capacity: [0; FRAMES_IN_FLIGHT],
            fg_live_count: [0; FRAMES_IN_FLIGHT],
            fg_dirty: [true; FRAMES_IN_FLIGHT],
            fg_buckets: std::array::from_fn(|_| Vec::new()),
            text_uniform_descriptor_set_layout,
            text_atlas_descriptor_set_layout,
            text_descriptor_pool,
            text_uniform_descriptor_sets,
            text_pipeline_layout,
            text_pipeline,
            sampler,
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
        unsafe {
            let _ = self.shared.device_wait_idle();
        }

        self.cols = cols;
        self.rows = rows;
        let bg_len = (cols as usize) * (rows as usize);
        self.bg_cpu = vec![CellBg::TRANSPARENT; bg_len];

        // Reallocate bg buffers via the cached `Arc<VkShared>` and
        // re-wire descriptor sets to the new buffer handles.
        let bg_byte_size = (bg_len * std::mem::size_of::<CellBg>())
            .max(std::mem::size_of::<CellBg>()) as u64;
        self.bg_buffers = std::array::from_fn(|_| {
            allocate_host_visible_buffer_raw(
                &self.shared,
                bg_byte_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )
        });
        for slot in 0..FRAMES_IN_FLIGHT {
            update_bg_descriptor_set(
                &self.shared.raw,
                self.bg_descriptor_sets[slot],
                &self.uniform_buffers[slot],
                &self.bg_buffers[slot],
            );
        }
        self.bg_dirty = [true; FRAMES_IN_FLIGHT];

        // Reset fg state — emit loop will re-populate after resize.
        self.fg_rows = init_fg_rows(rows);
        self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        self.fg_live_count = [0; FRAMES_IN_FLIGHT];
        self.needs_full_rebuild = true;
    }

    pub fn write_row(&mut self, row: u32, bg: &[CellBg], fg: &[CellText]) {
        // FG: stash in CPU per-row vec, mark all slots dirty.
        let idx = (row as usize) + 1;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            slot.clear();
            slot.extend_from_slice(fg);
            self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        }

        if row >= self.rows {
            return;
        }
        let row_start = (row as usize) * (self.cols as usize);
        let row_len = (self.cols as usize).min(bg.len());
        self.bg_cpu[row_start..row_start + row_len].copy_from_slice(&bg[..row_len]);
        for slot in &mut self.bg_cpu[row_start + row_len..row_start + self.cols as usize]
        {
            *slot = CellBg::TRANSPARENT;
        }
        self.bg_dirty = [true; FRAMES_IN_FLIGHT];
    }

    pub fn clear_row(&mut self, row: u32) {
        let idx = (row as usize) + 1;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            if !slot.is_empty() {
                self.fg_dirty = [true; FRAMES_IN_FLIGHT];
            }
            slot.clear();
        }
        if row >= self.rows {
            return;
        }
        let row_start = (row as usize) * (self.cols as usize);
        for slot in &mut self.bg_cpu[row_start..row_start + self.cols as usize] {
            *slot = CellBg::TRANSPARENT;
        }
        self.bg_dirty = [true; FRAMES_IN_FLIGHT];
    }

    pub fn set_block_cursor(&mut self, cells: &[CellText]) {
        if let Some(slot) = self.fg_rows.first_mut() {
            if slot.is_empty() && cells.is_empty() {
                return;
            }
            slot.clear();
            slot.extend_from_slice(cells);
            self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        }
    }

    pub fn set_non_block_cursor(&mut self, cells: &[CellText]) {
        let idx = self.fg_rows.len().saturating_sub(1);
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            if slot.is_empty() && cells.is_empty() {
                return;
            }
            slot.clear();
            slot.extend_from_slice(cells);
            self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        }
    }

    pub fn clear_cursor(&mut self) {
        let mut changed = false;
        if let Some(slot) = self.fg_rows.first_mut() {
            if !slot.is_empty() {
                slot.clear();
                changed = true;
            }
        }
        let last = self.fg_rows.len().saturating_sub(1);
        if last > 0 {
            if let Some(slot) = self.fg_rows.get_mut(last) {
                if !slot.is_empty() {
                    slot.clear();
                    changed = true;
                }
            }
        }
        if changed {
            self.fg_dirty = [true; FRAMES_IN_FLIGHT];
        }
    }

    #[inline]
    pub fn lookup_glyph(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_grayscale.lookup(key)
    }

    #[inline]
    pub fn lookup_glyph_color(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_color.lookup(key)
    }

    #[inline]
    pub fn insert_glyph(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        // The atlas's own `insert` walks existing pages and appends a
        // new one when none fit, so the renderer doesn't need to retry.
        self.atlas_grayscale.insert(key, glyph)
    }

    #[inline]
    pub fn insert_glyph_color(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        self.atlas_color.insert(key, glyph)
    }

    /// Drain pending atlas uploads into `cmd`. MUST be called BEFORE
    /// `Sugarloaf::render_vulkan` opens its dynamic-rendering pass —
    /// `vkCmdCopyBufferToImage` is forbidden inside a render pass.
    /// No-op when both atlases have no pending entries.
    pub fn prepare(
        &mut self,
        ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        frame_slot: usize,
    ) {
        debug_assert!(frame_slot < FRAMES_IN_FLIGHT);
        // Each atlas's `flush_uploads` early-outs when no page has
        // anything pending, so we can call it unconditionally.
        self.flush_pending_uploads(ctx, cmd, frame_slot);
    }

    /// Record the cell-bg pass into `cmd`. Uploads bg cells +
    /// uniforms for `frame_slot` first, then issues the fullscreen
    /// triangle. Caller must have opened the dynamic-rendering pass +
    /// set viewport/scissor + flushed atlas uploads via `prepare()`.
    /// Pair with `render_text`, with any `kitty_below_text` images
    /// composited in between.
    pub fn render_bg(
        &mut self,
        _ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        frame_slot: usize,
        uniforms: &GridUniforms,
    ) {
        debug_assert!(frame_slot < FRAMES_IN_FLIGHT);
        let slot = frame_slot;

        if self.bg_dirty[slot] {
            unsafe {
                let dst = self.bg_buffers[slot].as_mut_ptr() as *mut CellBg;
                std::ptr::copy_nonoverlapping(
                    self.bg_cpu.as_ptr(),
                    dst,
                    self.bg_cpu.len(),
                );
            }
            self.bg_dirty[slot] = false;
        }
        unsafe {
            let dst = self.uniform_buffers[slot].as_mut_ptr() as *mut GridUniforms;
            std::ptr::write(dst, *uniforms);
        }

        unsafe {
            self.shared.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.bg_pipeline,
            );
            self.shared.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.bg_pipeline_layout,
                0,
                &[self.bg_descriptor_sets[slot]],
                &[],
            );
            self.shared.cmd_draw(cmd, 3, 1, 0, 0);
        }
    }

    /// Record the cell-text pass into `cmd`. One instanced quad per
    /// `CellText`. Lazily flushes per-row CPU vecs into the per-slot
    /// fg buffer only when `fg_dirty[slot]` — no concat + memcpy on
    /// Noop/CursorOnly damage frames.
    pub fn render_text(
        &mut self,
        ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        frame_slot: usize,
        _uniforms: &GridUniforms,
    ) {
        debug_assert!(frame_slot < FRAMES_IN_FLIGHT);
        let slot = frame_slot;

        if self.fg_dirty[slot] {
            // Bucket cells by (atlas_kind, page) before uploading.
            // Each `cmd_draw` binds exactly one page's descriptor set
            // + one kind, so cells with different `(kind, page)` go
            // into separate ranges of the same vertex buffer.
            // Typical session: ≤ 4 buckets (2 kinds × 1–2 pages), so
            // the linear-search bucketing here is cheap.
            self.fg_staging.clear();
            self.fg_buckets[slot].clear();
            let mut scratch: Vec<((u8, u8), Vec<CellText>)> = Vec::with_capacity(4);
            for row in &self.fg_rows {
                for cell in row {
                    let key = (cell.atlas, cell.page);
                    match scratch.iter_mut().find(|(k, _)| *k == key) {
                        Some(b) => b.1.push(*cell),
                        None => scratch.push((key, vec![*cell])),
                    }
                }
            }
            for ((kind, page), cells) in scratch.into_iter() {
                let start = self.fg_staging.len() as u32;
                let count = cells.len() as u32;
                self.fg_staging.extend_from_slice(&cells);
                self.fg_buckets[slot].push(TextBucket {
                    kind,
                    page,
                    start,
                    count,
                });
            }
            let needed = self.fg_staging.len();

            if needed > self.fg_capacity[slot] {
                let new_cap = needed.next_power_of_two().max(64);
                self.fg_buffers[slot] = Some(ctx.allocate_host_visible_buffer(
                    (new_cap * std::mem::size_of::<CellText>()) as u64,
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                ));
                self.fg_capacity[slot] = new_cap;
            }

            if needed > 0 {
                let buf = self.fg_buffers[slot].as_ref().unwrap();
                unsafe {
                    let dst = buf.as_mut_ptr() as *mut CellText;
                    std::ptr::copy_nonoverlapping(self.fg_staging.as_ptr(), dst, needed);
                }
            }
            self.fg_live_count[slot] = needed as u32;
            self.fg_dirty[slot] = false;
        }

        if self.fg_live_count[slot] == 0 {
            return;
        }

        unsafe {
            self.shared.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.text_pipeline,
            );
            // Uniform set at slot 0 stays the same across every bucket.
            self.shared.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.text_pipeline_layout,
                0,
                &[self.text_uniform_descriptor_sets[slot]],
                &[],
            );
            let buf = self.fg_buffers[slot].as_ref().unwrap();
            self.shared
                .cmd_bind_vertex_buffers(cmd, 0, &[buf.handle()], &[0]);

            // One draw per `(kind, page)` bucket. Each draw binds
            // that page's descriptor set at slot 1 and pushes the
            // sampling-mode flag for the fragment shader.
            for bucket in &self.fg_buckets[slot] {
                let set = if bucket.kind == CellText::ATLAS_COLOR {
                    self.atlas_color.page_descriptor_set(bucket.page)
                } else {
                    self.atlas_grayscale.page_descriptor_set(bucket.page)
                };
                self.shared.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.text_pipeline_layout,
                    1,
                    &[set],
                    &[],
                );
                let is_color: u32 = if bucket.kind == CellText::ATLAS_COLOR {
                    1
                } else {
                    0
                };
                self.shared.cmd_push_constants(
                    cmd,
                    self.text_pipeline_layout,
                    vk::ShaderStageFlags::FRAGMENT,
                    0,
                    &is_color.to_ne_bytes(),
                );
                self.shared.cmd_draw(cmd, 4, bucket.count, 0, bucket.start);
            }
        }
    }

    /// Delegate to each atlas's own `flush_uploads`. Each atlas owns
    /// its own per-slot staging buffer ring now — see
    /// `VulkanGlyphAtlas::flush_uploads`.
    fn flush_pending_uploads(
        &mut self,
        _ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        slot: usize,
    ) {
        self.atlas_grayscale.flush_uploads(&self.shared, cmd, slot);
        self.atlas_color.flush_uploads(&self.shared, cmd, slot);
    }
}

/// Record an atlas upload: barrier image → `TRANSFER_DST_OPTIMAL`,
/// `cmd_copy_buffer_to_image`, barrier image → `SHADER_READ_ONLY_OPTIMAL`.
///
/// Both barriers are required: the first synchronizes any prior
/// fragment-shader read of the atlas (steady state) against the
/// upcoming transfer write; the second synchronizes the transfer
/// write against the *next* fragment-shader read (which happens in
/// the same command buffer, in the text pipeline draw a few hundred
/// instructions later). Without the trailing barrier the GPU is free
/// to start the fragment work before the copy completes, producing
/// transient garbage glyphs.
///
/// Caller (`flush_pending_uploads`) must ensure this is invoked
/// *outside* a dynamic-rendering pass — Vulkan 1.3 spec
/// VUID-vkCmdCopyBufferToImage-renderpass forbids transfer commands
/// inside a render pass. `Sugarloaf::render_vulkan` honours this by
/// calling `prepare_vulkan` before `cmd_begin_rendering`.
fn upload_to_page(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    staging: vk::Buffer,
    page: &mut VulkanAtlasPage,
    copies: &[vk::BufferImageCopy],
) {
    let old_layout = if page.initialized {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    } else {
        vk::ImageLayout::UNDEFINED
    };
    unsafe {
        // → TRANSFER_DST
        let to_transfer = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(if page.initialized {
                vk::PipelineStageFlags2::FRAGMENT_SHADER
            } else {
                vk::PipelineStageFlags2::TOP_OF_PIPE
            })
            .src_access_mask(if page.initialized {
                vk::AccessFlags2::SHADER_READ
            } else {
                vk::AccessFlags2::empty()
            })
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(page.image.handle())
            .subresource_range(color_subresource_range());
        let barriers = [to_transfer];
        let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
        device.cmd_pipeline_barrier2(cmd, &dep);

        // copy
        device.cmd_copy_buffer_to_image(
            cmd,
            staging,
            page.image.handle(),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            copies,
        );

        // → SHADER_READ
        let to_shader_read = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(page.image.handle())
            .subresource_range(color_subresource_range());
        let barriers = [to_shader_read];
        let dep = vk::DependencyInfo::default().image_memory_barriers(&barriers);
        device.cmd_pipeline_barrier2(cmd, &dep);
    }

    page.initialized = true;
}

impl Drop for VulkanGridRenderer {
    fn drop(&mut self) {
        unsafe {
            // Idle the queue before destroying pipelines / descriptor
            // resources. The shared `Arc<VkShared>` keeps the
            // underlying device alive across this whole Drop —
            // `vkDestroyDevice` runs only after every clone is gone.
            let _ = self.shared.device_wait_idle();
            self.shared.destroy_pipeline(self.text_pipeline, None);
            self.shared
                .destroy_pipeline_layout(self.text_pipeline_layout, None);
            self.shared
                .destroy_descriptor_pool(self.text_descriptor_pool, None);
            self.shared.destroy_descriptor_set_layout(
                self.text_atlas_descriptor_set_layout,
                None,
            );
            self.shared.destroy_descriptor_set_layout(
                self.text_uniform_descriptor_set_layout,
                None,
            );
            self.shared.destroy_sampler(self.sampler, None);

            self.shared.destroy_pipeline(self.bg_pipeline, None);
            self.shared
                .destroy_pipeline_layout(self.bg_pipeline_layout, None);
            self.shared
                .destroy_descriptor_pool(self.bg_descriptor_pool, None);
            self.shared
                .destroy_descriptor_set_layout(self.bg_descriptor_set_layout, None);
            // Buffers + atlas images drop themselves.
        }
    }
}

#[inline]
fn alloc_bg_buffer(ctx: &VulkanContext, cols: u32, rows: u32) -> VulkanBuffer {
    let size = (cols as u64)
        .saturating_mul(rows as u64)
        .saturating_mul(std::mem::size_of::<CellBg>() as u64)
        .max(std::mem::size_of::<CellBg>() as u64);
    ctx.allocate_host_visible_buffer(size, vk::BufferUsageFlags::STORAGE_BUFFER)
}

#[inline]
fn init_fg_rows(rows: u32) -> Vec<Vec<CellText>> {
    (0..(rows as usize + CURSOR_ROW_SLOTS))
        .map(|_| Vec::new())
        .collect()
}

#[inline]
fn color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
}

#[inline]
fn image_copy_region(
    buffer_offset: u64,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .buffer_offset(buffer_offset)
        .buffer_row_length(0) // tightly packed — same as bytes_per_row = w * bpp
        .buffer_image_height(0) // tightly packed
        .image_subresource(
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1),
        )
        .image_offset(vk::Offset3D {
            x: x as i32,
            y: y as i32,
            z: 0,
        })
        .image_extent(vk::Extent3D {
            width: w as u32,
            height: h as u32,
            depth: 1,
        })
}

fn create_bg_descriptor_set_layout(device: &ash::Device) -> vk::DescriptorSetLayout {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(grid.bg)")
    }
}

fn create_bg_descriptor_pool(device: &ash::Device) -> vk::DescriptorPool {
    let sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: FRAMES_IN_FLIGHT as u32,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: FRAMES_IN_FLIGHT as u32,
        },
    ];
    let info = vk::DescriptorPoolCreateInfo::default()
        .max_sets(FRAMES_IN_FLIGHT as u32)
        .pool_sizes(&sizes);
    unsafe {
        device
            .create_descriptor_pool(&info, None)
            .expect("create_descriptor_pool(grid.bg)")
    }
}

fn create_text_uniform_descriptor_set_layout(
    device: &ash::Device,
) -> vk::DescriptorSetLayout {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(grid.text uniform)")
    }
}

/// One combined-image-sampler binding — the single atlas page that
/// each draw binds. With page-list atlases there's no longer a
/// grayscale-vs-color binding split; the bound *page* implies the
/// kind, and the text pipeline picks sampling mode from a push
/// constant.
fn create_text_atlas_descriptor_set_layout(
    device: &ash::Device,
) -> vk::DescriptorSetLayout {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe {
        device
            .create_descriptor_set_layout(&info, None)
            .expect("create_descriptor_set_layout(grid.text atlas)")
    }
}

/// Pool sized for the uniform sets + every page either atlas can ever
/// allocate (`MAX_PAGES × 2`). Pages are never freed during the
/// renderer's lifetime, so the pool is sized once and never grown.
fn create_text_descriptor_pool(device: &ash::Device) -> vk::DescriptorPool {
    let max_atlas_sets = (MAX_PAGES * 2) as u32;
    let sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: FRAMES_IN_FLIGHT as u32,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: max_atlas_sets,
        },
    ];
    let info = vk::DescriptorPoolCreateInfo::default()
        .max_sets(FRAMES_IN_FLIGHT as u32 + max_atlas_sets)
        .pool_sizes(&sizes);
    unsafe {
        device
            .create_descriptor_pool(&info, None)
            .expect("create_descriptor_pool(grid.text)")
    }
}

fn allocate_descriptor_sets(
    device: &ash::Device,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> [vk::DescriptorSet; FRAMES_IN_FLIGHT] {
    let layouts = [layout; FRAMES_IN_FLIGHT];
    let info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(pool)
        .set_layouts(&layouts);
    let sets = unsafe {
        device
            .allocate_descriptor_sets(&info)
            .expect("allocate_descriptor_sets")
    };
    let mut out = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
    out.copy_from_slice(&sets);
    out
}

fn allocate_one_descriptor_set(
    device: &ash::Device,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> vk::DescriptorSet {
    let layouts = [layout];
    let info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(pool)
        .set_layouts(&layouts);
    unsafe {
        device
            .allocate_descriptor_sets(&info)
            .expect("allocate_descriptor_sets(one)")[0]
    }
}

fn update_bg_descriptor_set(
    device: &ash::Device,
    set: vk::DescriptorSet,
    uniform: &VulkanBuffer,
    cells: &VulkanBuffer,
) {
    let uniform_info = vk::DescriptorBufferInfo::default()
        .buffer(uniform.handle())
        .offset(0)
        .range(uniform.size());
    let uniform_infos = [uniform_info];
    let cells_info = vk::DescriptorBufferInfo::default()
        .buffer(cells.handle())
        .offset(0)
        .range(cells.size());
    let cells_infos = [cells_info];

    let writes = [
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&uniform_infos),
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&cells_infos),
    ];
    unsafe {
        device.update_descriptor_sets(&writes, &[]);
    }
}

fn update_text_uniform_descriptor_set(
    device: &ash::Device,
    set: vk::DescriptorSet,
    uniform: &VulkanBuffer,
) {
    let uniform_info = vk::DescriptorBufferInfo::default()
        .buffer(uniform.handle())
        .offset(0)
        .range(uniform.size());
    let uniform_infos = [uniform_info];
    let writes = [vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(&uniform_infos)];
    unsafe {
        device.update_descriptor_sets(&writes, &[]);
    }
}

fn create_pipeline_layout(
    device: &ash::Device,
    set_layouts: &[vk::DescriptorSetLayout],
    push_constant_ranges: &[vk::PushConstantRange],
) -> vk::PipelineLayout {
    let info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(set_layouts)
        .push_constant_ranges(push_constant_ranges);
    unsafe {
        device
            .create_pipeline_layout(&info, None)
            .expect("create_pipeline_layout(grid)")
    }
}

fn create_sampler(device: &ash::Device) -> vk::Sampler {
    // Nearest filter + clamp-to-edge — matches Metal's
    // `filter::nearest, address::clamp_to_edge`. Not used for
    // sampling per se (we use `texelFetch` in the fragment shader),
    // but the COMBINED_IMAGE_SAMPLER descriptor still requires a
    // sampler object.
    let info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::NEAREST)
        .min_filter(vk::Filter::NEAREST)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
    unsafe {
        device
            .create_sampler(&info, None)
            .expect("create_sampler(grid.text)")
    }
}

fn create_bg_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> vk::Pipeline {
    build_pipeline(
        device,
        pipeline_cache,
        layout,
        color_format,
        BG_VERT_SPV,
        BG_FRAG_SPV,
        &[], // no vertex bindings
        &[],
        vk::PrimitiveTopology::TRIANGLE_LIST,
        BlendMode::Premultiplied, // bg uses src=SRC_ALPHA
    )
}

fn create_text_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> vk::Pipeline {
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<CellText>() as u32)
        .input_rate(vk::VertexInputRate::INSTANCE)];
    let attrs = [
        // 0: glyph_pos uvec2 @ 0
        vk::VertexInputAttributeDescription::default()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32_UINT)
            .offset(0),
        // 1: glyph_size uvec2 @ 8
        vk::VertexInputAttributeDescription::default()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32_UINT)
            .offset(8),
        // 2: bearings ivec2 @ 16 (stored as i16x2)
        vk::VertexInputAttributeDescription::default()
            .location(2)
            .binding(0)
            .format(vk::Format::R16G16_SINT)
            .offset(16),
        // 3: grid_pos uvec2 @ 20 (stored as u16x2)
        vk::VertexInputAttributeDescription::default()
            .location(3)
            .binding(0)
            .format(vk::Format::R16G16_UINT)
            .offset(20),
        // 4: color vec4 @ 24 (UNORM8)
        vk::VertexInputAttributeDescription::default()
            .location(4)
            .binding(0)
            .format(vk::Format::R8G8B8A8_UNORM)
            .offset(24),
        // `atlas` (offset 28) and `page` (offset 30) live in the
        // host-side struct so `render_text` can bucket cells, but the
        // shader doesn't read them — the bound descriptor set already
        // implies which page is sampled, and a push constant carries
        // the kind. No vertex attribute for either.
        // 5: bools u8 @ 29 → uint
        vk::VertexInputAttributeDescription::default()
            .location(5)
            .binding(0)
            .format(vk::Format::R8_UINT)
            .offset(29),
    ];
    build_pipeline(
        device,
        pipeline_cache,
        layout,
        color_format,
        TEXT_VERT_SPV,
        TEXT_FRAG_SPV,
        &bindings,
        &attrs,
        vk::PrimitiveTopology::TRIANGLE_STRIP,
        BlendMode::PremultipliedOverFromOne, // text fragment returns premultiplied
    )
}

#[derive(Copy, Clone)]
enum BlendMode {
    /// Source RGB factor = `SRC_ALPHA`. For shaders that return
    /// non-premultiplied RGBA + alpha (the bg pass).
    Premultiplied,
    /// Source RGB factor = `ONE`. For shaders that return
    /// already-premultiplied RGBA (the text pass — `in.color * mask_a`
    /// and the color atlas sample are both premultiplied).
    PremultipliedOverFromOne,
}

#[allow(clippy::too_many_arguments)]
fn build_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    vert_spv: &[u8],
    frag_spv: &[u8],
    vertex_bindings: &[vk::VertexInputBindingDescription],
    vertex_attrs: &[vk::VertexInputAttributeDescription],
    topology: vk::PrimitiveTopology,
    blend: BlendMode,
) -> vk::Pipeline {
    let vert = load_shader_module(device, vert_spv);
    let frag = load_shader_module(device, frag_spv);

    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(entry),
    ];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(vertex_bindings)
        .vertex_attribute_descriptions(vertex_attrs);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(topology)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let (src_rgb, dst_rgb) = match blend {
        BlendMode::Premultiplied => (
            vk::BlendFactor::SRC_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        ),
        BlendMode::PremultipliedOverFromOne => {
            (vk::BlendFactor::ONE, vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        }
    };
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(src_rgb)
        .dst_color_blend_factor(dst_rgb)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let blend_attachments = [blend_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let color_attachment_formats = [color_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_attachment_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .push_next(&mut rendering);

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(pipeline_cache, &[pipeline_info], None)
            .map_err(|(_, e)| e)
            .expect("create_graphics_pipelines(grid)")[0]
    };

    unsafe {
        device.destroy_shader_module(vert, None);
        device.destroy_shader_module(frag, None);
    }
    pipeline
}

fn load_shader_module(device: &ash::Device, bytes: &[u8]) -> vk::ShaderModule {
    let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
        .expect("read_spv (embedded grid shader is valid)");
    let info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe {
        device
            .create_shader_module(&info, None)
            .expect("create_shader_module(grid)")
    }
}
