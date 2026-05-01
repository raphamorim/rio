//! Vulkan backend built directly on `ash`.
//!
//! Mirrors `context::metal::MetalContext` in shape and intent: one struct
//! owning everything needed to present a swapchain image, plus the
//! per-frame synchronisation primitives. No wgpu involvement.
//!
//! Targets Vulkan 1.3 so we can reach for `VK_KHR_dynamic_rendering` (core
//! in 1.3) later without changing device creation. Anyone on a driver
//! older than early-2022 will fail at `create_instance` with
//! `ERROR_INCOMPATIBLE_DRIVER` — same class of failure as an ancient GPU
//! on the Metal path.
//!
//! Surface creation dispatches on `raw-window-handle` variants inline
//! rather than pulling in `ash-window` — that crate is not in Debian and
//! `ash-window` buys us ~30 lines of glue per platform that we'd rather
//! own.

use crate::sugarloaf::{Colorspace, SugarloafWindow, SugarloafWindowSize};
use ash::khr;
use ash::vk;
use ash::{Device, Entry, Instance};
use raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use std::ffi::{c_char, CStr};
use std::sync::Arc;

/// Reference-counted owner of the raw Vulkan handles whose destruction
/// must be sequenced last. Held as `Arc<VkShared>` by every struct
/// (VulkanContext, VulkanGridRenderer, VulkanRenderer, VulkanBuffer,
/// VulkanImage, VulkanImageTexture, the text overlay's Vulkan state)
/// that would otherwise call into the device from its own `Drop`.
///
/// `vkDestroyDevice` runs only when the last `Arc` clone is dropped,
/// so per-resource Drop order across `Sugarloaf` and `Screen.grids`
/// stops being load-bearing — the field-declaration trick that worked
/// for `VulkanRenderer`-inside-`Sugarloaf` (single parent) cannot
/// extend to `VulkanGridRenderer`-inside-`Screen.grids` (a sibling
/// of the parent that owns the device), which is the bug behind
/// raphamorim/rio#1568.
///
/// Mirrors `wgpu_hal::vulkan::DeviceShared`. `raw` is named for the
/// underlying `ash::Device` to match wgpu-hal's convention; `Deref`
/// dispatches `shared.method(...)` calls straight to the device's
/// dispatch table, so consumers don't have to write `shared.raw.method()`.
pub struct VkShared {
    // Declaration order = drop order. Vulkan rules:
    //   * `vkDestroyDevice` requires the parent `Instance` to still be
    //     alive (we look up the destroy entry point through it).
    //   * `vkDestroyInstance` requires the loaded `libvulkan` symbols
    //     (the `Entry`) to still be loaded.
    pub raw: Device,
    pub instance: Instance,
    pub physical_device: vk::PhysicalDevice,
    _entry: Entry,
}

// `ash::Device` / `ash::Instance` / `ash::Entry` are dispatch tables
// with no interior mutability; the underlying Vulkan handles are
// thread-safe per spec (external synchronisation is per-object, not
// per-device). `vk::PhysicalDevice` is a plain handle. So `VkShared`
// is safe to share across threads via `Arc`.
unsafe impl Send for VkShared {}
unsafe impl Sync for VkShared {}

impl Drop for VkShared {
    fn drop(&mut self) {
        unsafe {
            // Defensive: the last clone of `VkShared` should normally
            // be `VulkanContext`, which already idled in its own
            // `Drop`. If a leaf resource (buffer, image, grid
            // renderer) outlives the context — possible because the
            // grid renderers live in `Screen.grids` while
            // `VulkanContext` lives in `Screen.sugarloaf` — this is
            // the only `device_wait_idle` we get. Cheap on an idle
            // queue.
            let _ = self.raw.device_wait_idle();
            self.raw.destroy_device(None);
            self.instance.destroy_instance(None);
            // `_entry` drops here, unloading `libvulkan`.
        }
    }
}

impl std::ops::Deref for VkShared {
    type Target = Device;
    #[inline]
    fn deref(&self) -> &Device {
        &self.raw
    }
}

/// How many frames the CPU is allowed to pipeline ahead of the GPU.
/// Three matches the Metal backend (`MetalLayer::set_maximum_drawable_count(3)`
/// in `context::metal::MetalContext::new`) and Apple's standard sample
/// pattern — CPU / GPU / compositor each work on their own frame in
/// parallel. The cost is two extra `FrameSync` slots and one extra
/// swapchain image's worth of memory.
pub const FRAMES_IN_FLIGHT: usize = 3;

/// One set of synchronisation objects + a command pool & pre-allocated
/// primary buffer, reused each time the same slot comes around. The
/// `in_flight` fence is signalled by the submit that uses this slot so
/// the *next* owner knows the GPU is done with this slot's resources.
struct FrameSync {
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    in_flight: vk::Fence,
    cmd_pool: vk::CommandPool,
    cmd_buffer: vk::CommandBuffer,
}

pub struct VulkanContext {
    // Logical fields for the public surface.
    pub size: SugarloafWindowSize,
    pub scale: f32,
    pub supports_f16: bool,
    pub colorspace: Colorspace,
    /// Updated on every `acquire_frame()` — `true` if the driver hinted
    /// that the swapchain is out of date and we should recreate at our
    /// earliest convenience (we already did if ERROR_OUT_OF_DATE_KHR, but
    /// SUBOPTIMAL_KHR says "still usable this frame").
    pub needs_recreate: bool,

    // Per-frame state.
    frame_index: usize,
    frames: [FrameSync; FRAMES_IN_FLIGHT],

    // Swapchain state. Rebuilt by `resize()`.
    swapchain_extent: vk::Extent2D,
    swapchain_color_space: vk::ColorSpaceKHR,
    swapchain_format: vk::Format,
    swapchain_images: Vec<vk::Image>,
    swapchain_views: Vec<vk::ImageView>,
    swapchain: vk::SwapchainKHR,
    swapchain_loader: khr::swapchain::Device,

    // Core device.
    queue: vk::Queue,
    // Kept around so future phases (atlas uploads, a dedicated transfer
    // pool, pipeline creation) don't have to re-probe the family.
    #[allow(dead_code)]
    queue_family_index: u32,

    /// Reference-counted owner of `device`, `instance`, `physical_device`,
    /// and the loader (`Entry`). Cloned into every per-resource struct
    /// (`VulkanBuffer`, `VulkanImage`, `VulkanGridRenderer`,
    /// `VulkanRenderer`, `VulkanImageTexture`, the text overlay's
    /// Vulkan state) so `vkDestroyDevice` runs only after the last
    /// dependent resource is dropped. Replaces the previous bare
    /// `device.clone()` cloning, which was unsafe across struct
    /// boundaries (raphamorim/rio#1568). `Deref` to `ash::Device`
    /// keeps call sites unchanged: `self.shared.cmd_bind_pipeline(...)`.
    shared: Arc<VkShared>,

    /// Pipeline cache shared by every `create_graphics_pipelines`
    /// call. Loaded from `~/.cache/rio/sugarloaf-vulkan.cache` (best
    /// effort) at startup and serialised back on `Drop`. Saves
    /// ~10–50ms of pipeline build time on subsequent launches.
    pipeline_cache: vk::PipelineCache,

    // Instance-level state — held last so it outlives everything above in
    // the Drop impl (drop order = declaration order).
    surface: vk::SurfaceKHR,
    surface_loader: khr::surface::Instance,
    /// Debug-utils messenger, present only when validation layers
    /// were requested via `RIO_VULKAN_VALIDATION=1`. Drops before
    /// `instance` (declaration order) so the messenger handle is
    /// destroyed while the instance is still alive.
    _debug_messenger: Option<DebugMessenger>,
}

/// Owns one `vk::DebugUtilsMessengerEXT` and its loader. Destroyed
/// in `Drop` — the loader needs the parent `Instance` to still be
/// valid, which the field-order convention ensures.
struct DebugMessenger {
    loader: ash::ext::debug_utils::Instance,
    handle: vk::DebugUtilsMessengerEXT,
}

impl Drop for DebugMessenger {
    fn drop(&mut self) {
        unsafe {
            self.loader.destroy_debug_utils_messenger(self.handle, None);
        }
    }
}

/// In-flight handle returned by `acquire_frame()`. The caller records
/// commands into `cmd_buffer` targeting `image` / `image_view`, then
/// hands it back to `present_frame()`.
pub struct VulkanFrame {
    pub image_index: u32,
    pub image: vk::Image,
    pub image_view: vk::ImageView,
    pub cmd_buffer: vk::CommandBuffer,
    pub extent: vk::Extent2D,
    pub format: vk::Format,
    /// Frame-in-flight slot for this frame. Renderers (grid, text,
    /// images) ring their per-frame GPU resources by this index; the
    /// `in_flight` fence wait inside `acquire_frame` proved this slot
    /// is GPU-idle, so writing into slot `N`'s buffers from the CPU
    /// is safe.
    pub slot: usize,
}

impl VulkanContext {
    pub fn new(sugarloaf_window: SugarloafWindow) -> Self {
        let size = sugarloaf_window.size;
        let scale = sugarloaf_window.scale;

        // Loading the loader itself can fail if libvulkan.so is missing —
        // which is the expected failure on a machine without a Vulkan
        // driver installed. We let the panic propagate: the caller is
        // `Context::new` and the backend selection happened upstream, so
        // there's no graceful degradation path here (the WGPU/CPU
        // backends live behind different enum variants).
        let entry =
            unsafe { Entry::load() }.expect("failed to load Vulkan loader (libvulkan)");

        let validation_requested = validation_requested();
        let instance = create_instance(&entry, &sugarloaf_window, validation_requested);
        let _debug_messenger = if validation_requested {
            create_debug_messenger(&entry, &instance)
        } else {
            None
        };
        let surface_loader = khr::surface::Instance::new(&entry, &instance);
        let surface = create_surface(&entry, &instance, &sugarloaf_window);

        let (physical_device, queue_family_index) =
            pick_physical_device(&instance, &surface_loader, surface);

        let device = create_device(&instance, physical_device, queue_family_index);
        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
        let pipeline_cache = create_pipeline_cache(&device);

        let swapchain_loader = khr::swapchain::Device::new(&instance, &device);

        let (
            swapchain,
            swapchain_format,
            swapchain_color_space,
            swapchain_extent,
            swapchain_images,
            swapchain_views,
        ) = create_swapchain(
            &device,
            &surface_loader,
            &swapchain_loader,
            physical_device,
            surface,
            size.width as u32,
            size.height as u32,
            vk::SwapchainKHR::null(),
        );

        let frames = create_frames(&device, queue_family_index);

        // f16 = Vulkan's VK_KHR_shader_float16_int8 feature. Probe at
        // device creation time in a follow-up; for the MVP we report
        // false, matching the conservative default.
        let supports_f16 = false;

        tracing::info!(
            "Vulkan device created: {}",
            physical_device_name(&instance, physical_device)
        );
        tracing::info!(
            "Swapchain: {:?} {}x{} ({} images)",
            swapchain_format,
            swapchain_extent.width,
            swapchain_extent.height,
            swapchain_images.len()
        );
        log_memory_heap_choice(&instance, physical_device);

        let shared = Arc::new(VkShared {
            raw: device,
            instance,
            physical_device,
            _entry: entry,
        });

        VulkanContext {
            size,
            scale,
            supports_f16,
            colorspace: Colorspace::Srgb,
            needs_recreate: false,
            frame_index: 0,
            frames,
            swapchain_extent,
            swapchain_color_space,
            swapchain_format,
            swapchain_images,
            swapchain_views,
            swapchain,
            swapchain_loader,
            queue,
            queue_family_index,
            shared,
            pipeline_cache,
            surface,
            surface_loader,
            _debug_messenger,
        }
    }

    #[inline]
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    #[inline]
    pub fn supports_f16(&self) -> bool {
        self.supports_f16
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.size.width = width as f32;
        self.size.height = height as f32;
        self.recreate_swapchain(width, height);
    }

    fn recreate_swapchain(&mut self, width: u32, height: u32) {
        // The spec requires no resources tied to the old swapchain be
        // in use. Easiest safe path: wait for the device to go idle.
        // This is a resize, not a per-frame operation, so the stall is
        // acceptable (wgpu does the same thing).
        unsafe {
            let _ = self.shared.device_wait_idle();
        }

        for &view in &self.swapchain_views {
            unsafe { self.shared.destroy_image_view(view, None) };
        }
        self.swapchain_views.clear();
        self.swapchain_images.clear();

        let old = self.swapchain;
        let (swapchain, format, color_space, extent, images, views) = create_swapchain(
            &self.shared.raw,
            &self.surface_loader,
            &self.swapchain_loader,
            self.shared.physical_device,
            self.surface,
            width,
            height,
            old,
        );

        unsafe { self.swapchain_loader.destroy_swapchain(old, None) };

        self.swapchain = swapchain;
        self.swapchain_format = format;
        self.swapchain_color_space = color_space;
        self.swapchain_extent = extent;
        self.swapchain_images = images;
        self.swapchain_views = views;
        self.needs_recreate = false;
    }

    /// Acquire the next swapchain image and begin the per-frame command
    /// buffer. Returns `None` if the swapchain needed recreation (caller
    /// should skip this frame).
    pub fn acquire_frame(&mut self) -> Option<VulkanFrame> {
        let slot = self.frame_index;
        let sync = &self.frames[slot];

        unsafe {
            self.shared
                .wait_for_fences(&[sync.in_flight], true, u64::MAX)
                .expect("wait_for_fences");
        }

        let (image_index, suboptimal) = unsafe {
            match self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                sync.image_available,
                vk::Fence::null(),
            ) {
                Ok(pair) => pair,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.recreate_swapchain(
                        self.size.width as u32,
                        self.size.height as u32,
                    );
                    return None;
                }
                Err(e) => panic!("acquire_next_image failed: {e:?}"),
            }
        };
        if suboptimal {
            self.needs_recreate = true;
        }

        // Only reset *after* we've committed to submitting — resetting
        // before acquire_next_image would leave us deadlocked if the
        // acquire returned OUT_OF_DATE and we bailed out.
        unsafe {
            self.shared
                .reset_fences(&[sync.in_flight])
                .expect("reset_fences");
            self.shared
                .reset_command_pool(sync.cmd_pool, vk::CommandPoolResetFlags::empty())
                .expect("reset_command_pool");
            self.shared
                .begin_command_buffer(
                    sync.cmd_buffer,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .expect("begin_command_buffer");
        }

        Some(VulkanFrame {
            image_index,
            image: self.swapchain_images[image_index as usize],
            image_view: self.swapchain_views[image_index as usize],
            cmd_buffer: sync.cmd_buffer,
            extent: self.swapchain_extent,
            format: self.swapchain_format,
            slot,
        })
    }

    /// End the command buffer, submit, present, advance frame index.
    pub fn present_frame(&mut self, frame: VulkanFrame) {
        let sync = &self.frames[frame.slot];
        unsafe {
            self.shared
                .end_command_buffer(sync.cmd_buffer)
                .expect("end_command_buffer");

            let wait_semaphores = [sync.image_available];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signal_semaphores = [sync.render_finished];
            let cmd_buffers = [sync.cmd_buffer];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&cmd_buffers)
                .signal_semaphores(&signal_semaphores);
            self.shared
                .queue_submit(self.queue, &[submit], sync.in_flight)
                .expect("queue_submit");

            let swapchains = [self.swapchain];
            let image_indices = [frame.image_index];
            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            match self
                .swapchain_loader
                .queue_present(self.queue, &present_info)
            {
                Ok(suboptimal) => {
                    if suboptimal {
                        self.needs_recreate = true;
                    }
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.needs_recreate = true;
                }
                Err(e) => panic!("queue_present failed: {e:?}"),
            }
        }

        self.frame_index = (self.frame_index + 1) % FRAMES_IN_FLIGHT;
    }

    /// Expose the underlying device so the renderer can record commands.
    #[inline]
    pub fn device(&self) -> &Device {
        &self.shared.raw
    }

    /// Reference-counted handle to the device + instance + entry. Each
    /// per-resource type (`VulkanBuffer`, `VulkanImage`,
    /// `VulkanGridRenderer`, `VulkanRenderer`, `VulkanImageTexture`,
    /// `TextVulkanState`) clones this and stores it directly so its
    /// own `Drop` can call `destroy_*` on a device that is guaranteed
    /// to still be alive — `vkDestroyDevice` runs only when the last
    /// `Arc` is dropped. The previous design (each leaf cloning the
    /// raw `ash::Device` dispatch table) crashed when the parent
    /// `VulkanContext` happened to drop first; see
    /// raphamorim/rio#1568.
    #[inline]
    pub fn shared(&self) -> &Arc<VkShared> {
        &self.shared
    }

    /// Color attachment format the swapchain was created with. Real
    /// pipelines need this at construction time so `VkPipelineRenderingCreateInfo`
    /// can declare a matching color attachment format. Stable across
    /// resize (only `extent` changes there).
    #[inline]
    pub fn swapchain_format(&self) -> vk::Format {
        self.swapchain_format
    }

    /// The instance + physical device that own this context's logical
    /// device. Renderers cache these so they can allocate buffers /
    /// images via the free `allocate_host_visible_buffer_raw` /
    /// `allocate_sampled_image_raw` helpers without needing a live
    /// `&VulkanContext` borrow at every allocation site (chiefly,
    /// `resize` which only has `&mut self`).
    #[inline]
    pub fn instance(&self) -> &Instance {
        &self.shared.instance
    }

    #[inline]
    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.shared.physical_device
    }

    /// Pipeline cache shared by every renderer's
    /// `create_graphics_pipelines` call. Pass this instead of
    /// `vk::PipelineCache::null()` so cached binaries land on disk
    /// at shutdown and short-circuit subsequent compiles.
    #[inline]
    pub fn pipeline_cache(&self) -> vk::PipelineCache {
        self.pipeline_cache
    }

    /// Run `record` against a transient command buffer, submit it,
    /// wait for completion. Used for one-shot transfer work that
    /// can't piggy-back on the per-frame command buffer (atlas /
    /// image / texture uploads triggered from outside the render
    /// loop, where there's no live `cmd` to append to).
    ///
    /// Allocates a fresh `vk::CommandPool` + `vk::Fence` per call
    /// and tears them down at the end. Cheap (microseconds) compared
    /// to the actual GPU transfer; not a hot path.
    pub fn submit_oneshot<F: FnOnce(vk::CommandBuffer)>(&self, record: F) {
        unsafe {
            let pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(self.queue_family_index)
                .flags(vk::CommandPoolCreateFlags::TRANSIENT);
            let pool = self
                .shared
                .create_command_pool(&pool_info, None)
                .expect("create_command_pool(oneshot)");

            let alloc = vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd = self
                .shared
                .allocate_command_buffers(&alloc)
                .expect("allocate_command_buffers(oneshot)")[0];

            let begin = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.shared
                .begin_command_buffer(cmd, &begin)
                .expect("begin_command_buffer(oneshot)");

            record(cmd);

            self.shared
                .end_command_buffer(cmd)
                .expect("end_command_buffer(oneshot)");

            let fence = self
                .shared
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .expect("create_fence(oneshot)");
            let cmds = [cmd];
            let submit = vk::SubmitInfo::default().command_buffers(&cmds);
            self.shared
                .queue_submit(self.queue, &[submit], fence)
                .expect("queue_submit(oneshot)");
            self.shared
                .wait_for_fences(&[fence], true, u64::MAX)
                .expect("wait_for_fences(oneshot)");

            self.shared.destroy_fence(fence, None);
            self.shared.destroy_command_pool(pool, None);
        }
    }

    /// Index of the slot the *next* `acquire_frame` will use. Renderers
    /// (grid, text, image overlay) ring their per-frame GPU resources by
    /// this index so that a write into slot N can't race the GPU still
    /// reading from slot N. Stable for the lifetime of `VulkanContext`.
    #[inline]
    pub fn current_frame_slot(&self) -> usize {
        self.frame_index
    }

    /// Allocate a host-visible, host-coherent, persistently-mapped buffer
    /// suitable for per-frame uploads (uniform buffers, vertex/instance
    /// buffers, storage buffers that the CPU writes into and the GPU
    /// reads from this frame). On UMA/integrated GPUs the underlying
    /// memory will also be `DEVICE_LOCAL` (BAR memory) — the driver
    /// picks the best matching type via `memory_type_bits` filtering.
    ///
    /// We do not run a suballocator: each call burns one device memory
    /// allocation. Vulkan guarantees ≥4096 active allocations per
    /// device, and sugarloaf's working set is well under that ceiling
    /// (a couple of atlases + per-frame ring buffers per terminal).
    /// Switch to a slab allocator only if profiling ever shows
    /// allocation churn — current call sites construct once, reuse
    /// thereafter, and only reallocate on grow.
    pub fn allocate_host_visible_buffer(
        &self,
        size: u64,
        usage: vk::BufferUsageFlags,
    ) -> VulkanBuffer {
        // `vkCreateBuffer` rejects zero-sized buffers; bump up to a
        // single byte so callers don't have to special-case empty rings.
        let size = size.max(1);

        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe {
            self.shared
                .create_buffer(&buffer_info, None)
                .expect("create_buffer")
        };

        let req = unsafe { self.shared.get_buffer_memory_requirements(buffer) };
        let mem_type = find_memory_type(
            &self.shared.instance,
            self.shared.physical_device,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("no HOST_VISIBLE | HOST_COHERENT memory type — driver bug?");

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mem_type);
        let memory = unsafe {
            self.shared
                .allocate_memory(&alloc_info, None)
                .expect("allocate_memory")
        };
        unsafe {
            self.shared
                .bind_buffer_memory(buffer, memory, 0)
                .expect("bind_buffer_memory");
        }

        // HOST_COHERENT means we never have to flush; mapping stays
        // valid until `vkUnmapMemory`, which we only do at Drop.
        let mapped = unsafe {
            self.shared
                .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                .expect("map_memory") as *mut u8
        };

        VulkanBuffer {
            shared: self.shared.clone(),
            buffer,
            memory,
            mapped,
            size,
        }
    }
}

/// Host-visible, persistently-mapped buffer. Written to via [`as_mut_ptr`]
/// (raw pointer; caller owns the layout / bounds checks). The buffer
/// destroys itself + frees its backing memory on drop.
pub struct VulkanBuffer {
    /// Shared device handle. The Arc keeps the underlying
    /// `vkDestroyDevice` from running until *every* `VulkanBuffer`
    /// (and other resource) is dropped, regardless of the order in
    /// which their parents drop. See `VkShared`.
    shared: Arc<VkShared>,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *mut u8,
    size: u64,
}

// `vk::Buffer`, `vk::DeviceMemory`, and the mapped pointer are all
// values the driver hands out per-allocation; `Arc<VkShared>` is
// Send+Sync (see the `unsafe impl` on `VkShared`). Buffers are never
// shared across threads in sugarloaf, but `Send` lets them sit inside
// `Sugarloaf` (which is not `!Send`).
unsafe impl Send for VulkanBuffer {}
unsafe impl Sync for VulkanBuffer {}

impl VulkanBuffer {
    #[inline]
    pub fn handle(&self) -> vk::Buffer {
        self.buffer
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Raw pointer to the start of the mapping. Valid for the lifetime
    /// of this `VulkanBuffer`. Writes through this pointer are visible
    /// to the GPU at submit time — `HOST_COHERENT` removes the need for
    /// `vkFlushMappedMemoryRanges`.
    #[inline]
    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.mapped
    }
}

impl Drop for VulkanBuffer {
    fn drop(&mut self) {
        unsafe {
            // Order: unmap, free memory, destroy buffer.
            // `vkFreeMemory` on a non-mapped allocation is safe; we
            // unmap first only because some validation layers warn
            // about freeing memory that's still mapped.
            self.shared.unmap_memory(self.memory);
            self.shared.destroy_buffer(self.buffer, None);
            self.shared.free_memory(self.memory, None);
        }
    }
}

/// Free-function variant of [`VulkanContext::allocate_host_visible_buffer`]
/// for callers that hold a cached `Arc<VkShared>` rather than a live
/// `&VulkanContext` borrow. The grid / text / image renderers stash
/// the shared handle at construction time so they can allocate from
/// inside their own `resize` paths (which only have `&mut self`, not
/// the parent context).
pub fn allocate_host_visible_buffer_raw(
    shared: &Arc<VkShared>,
    size: u64,
    usage: vk::BufferUsageFlags,
) -> VulkanBuffer {
    let size = size.max(1);
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe {
        shared
            .create_buffer(&buffer_info, None)
            .expect("create_buffer")
    };
    let req = unsafe { shared.get_buffer_memory_requirements(buffer) };
    let mem_type = find_memory_type(
        &shared.instance,
        shared.physical_device,
        req.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )
    .expect("no HOST_VISIBLE | HOST_COHERENT memory type");
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(mem_type);
    let memory = unsafe {
        shared
            .allocate_memory(&alloc_info, None)
            .expect("allocate_memory")
    };
    unsafe {
        shared
            .bind_buffer_memory(buffer, memory, 0)
            .expect("bind_buffer_memory");
    }
    let mapped = unsafe {
        shared
            .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
            .expect("map_memory") as *mut u8
    };
    VulkanBuffer {
        shared: shared.clone(),
        buffer,
        memory,
        mapped,
        size,
    }
}

/// Walks the device's memory types looking for one that matches both
/// `type_filter` (the bitmask returned by `vkGetBufferMemoryRequirements`)
/// and the requested `flags`. Returns `None` if no matching type
/// exists — that's a Vulkan-spec violation the driver should never
/// produce for the standard `HOST_VISIBLE | HOST_COHERENT` and
/// `DEVICE_LOCAL` combinations, but callers should still treat it as
/// fatal rather than ignore it.
fn find_memory_type(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    type_filter: u32,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let props =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };
    for i in 0..props.memory_type_count {
        let supported = (type_filter & (1 << i)) != 0;
        let matches_flags = props.memory_types[i as usize]
            .property_flags
            .contains(flags);
        if supported && matches_flags {
            return Some(i);
        }
    }
    None
}

/// One-off boot-time log of the memory types we'd pick for our two
/// hot allocation patterns. On UMA / integrated GPUs (Intel iGPU,
/// AMD APU, common Debian-laptop hardware) we expect the
/// host-visible heap to also report `DEVICE_LOCAL` — that's BAR
/// memory and our persistently-mapped per-frame buffers land in fast
/// GPU-accessible memory with no staging copy. On discrete GPUs the
/// host-visible heap is plain system RAM, slower for the GPU to
/// read; we'd want to switch to staging-buffer uploads for hot
/// per-frame data if profiling shows it matters.
fn log_memory_heap_choice(instance: &Instance, physical_device: vk::PhysicalDevice) {
    // Pretend `type_filter = !0` to ignore per-resource alignment
    // filtering — we just want the canonical pick for each pattern.
    let host_visible = find_memory_type(
        instance,
        physical_device,
        !0,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    );
    let device_local = find_memory_type(
        instance,
        physical_device,
        !0,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    );
    let props =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };
    if let Some(idx) = host_visible {
        let flags = props.memory_types[idx as usize].property_flags;
        let bar = flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
        tracing::info!(
            "Vulkan host-visible memory: type {} flags={:?} ({})",
            idx,
            flags,
            if bar {
                "BAR / unified — fast GPU reads"
            } else {
                "system RAM — slower GPU reads, consider staging for hot data"
            }
        );
    }
    if let Some(idx) = device_local {
        tracing::info!(
            "Vulkan device-local memory: type {} flags={:?}",
            idx,
            props.memory_types[idx as usize].property_flags
        );
    }
}

// -----------------------------------------------------------------------
// Image helper (device-local 2D image + view + memory)
// -----------------------------------------------------------------------

impl VulkanContext {
    /// Allocate a device-local 2D image suitable for sampling from a
    /// shader (atlas, kitty graphic, background image). Created in
    /// `UNDEFINED` layout — the caller's first transfer command must
    /// include a barrier transitioning to `TRANSFER_DST_OPTIMAL`
    /// before any `vkCmdCopyBufferToImage`.
    pub fn allocate_sampled_image(
        &self,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> VulkanImage {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe {
            self.shared
                .create_image(&image_info, None)
                .expect("create_image")
        };

        let req = unsafe { self.shared.get_image_memory_requirements(image) };
        let mem_type = find_memory_type(
            &self.shared.instance,
            self.shared.physical_device,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .expect("no DEVICE_LOCAL memory type — driver bug?");

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mem_type);
        let memory = unsafe {
            self.shared
                .allocate_memory(&alloc_info, None)
                .expect("allocate_memory(image)")
        };
        unsafe {
            self.shared
                .bind_image_memory(image, memory, 0)
                .expect("bind_image_memory");
        }

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping::default())
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );
        let view = unsafe {
            self.shared
                .create_image_view(&view_info, None)
                .expect("create_image_view")
        };

        VulkanImage {
            shared: self.shared.clone(),
            image,
            view,
            memory,
            width,
            height,
            format,
        }
    }
}

/// Device-local 2D image with view + backing memory. Drops the view,
/// image, and memory on `Drop`. The image starts in `UNDEFINED` layout
/// — the first command that uses it must barrier-transition to a
/// usable layout (`TRANSFER_DST_OPTIMAL` for the initial upload).
pub struct VulkanImage {
    /// Shared device handle. See `VkShared`.
    shared: Arc<VkShared>,
    image: vk::Image,
    view: vk::ImageView,
    memory: vk::DeviceMemory,
    pub width: u32,
    pub height: u32,
    pub format: vk::Format,
}

unsafe impl Send for VulkanImage {}
unsafe impl Sync for VulkanImage {}

impl VulkanImage {
    #[inline]
    pub fn handle(&self) -> vk::Image {
        self.image
    }

    #[inline]
    pub fn view(&self) -> vk::ImageView {
        self.view
    }
}

impl Drop for VulkanImage {
    fn drop(&mut self) {
        unsafe {
            self.shared.destroy_image_view(self.view, None);
            self.shared.destroy_image(self.image, None);
            self.shared.free_memory(self.memory, None);
        }
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            // Idle the queue before tearing down anything that might
            // still be in flight (swapchain image views, sync prims).
            // `vkDestroyDevice` itself happens later, when the last
            // `Arc<VkShared>` clone drops — see `VkShared::drop`.
            let _ = self.shared.device_wait_idle();

            // Best-effort: serialize the pipeline cache to disk
            // before destroying it. Failure (no XDG_CACHE_HOME, no
            // write perms, etc) is logged but not fatal.
            save_pipeline_cache(&self.shared.raw, self.pipeline_cache);
            self.shared
                .destroy_pipeline_cache(self.pipeline_cache, None);

            for frame in &self.frames {
                self.shared.destroy_semaphore(frame.image_available, None);
                self.shared.destroy_semaphore(frame.render_finished, None);
                self.shared.destroy_fence(frame.in_flight, None);
                self.shared.destroy_command_pool(frame.cmd_pool, None);
            }

            for &view in &self.swapchain_views {
                self.shared.destroy_image_view(view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            self.surface_loader.destroy_surface(self.surface, None);
            // `_debug_messenger` (declared after this Drop's body
            // unwind path completes) drops before `shared` (declared
            // before it), so the messenger handle is destroyed while
            // the instance is still alive. `vkDestroyDevice` and
            // `vkDestroyInstance` run in `VkShared::drop` once the
            // last `Arc<VkShared>` clone is gone.
        }
    }
}

// =======================================================================
// Pipeline cache (load on `new`, save on `Drop`)
// =======================================================================

/// Path to the on-disk pipeline cache. Returns `None` if neither
/// `XDG_CACHE_HOME` nor `HOME` is set.
fn pipeline_cache_path() -> Option<std::path::PathBuf> {
    let dir = if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        std::path::PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        let mut p = std::path::PathBuf::from(home);
        p.push(".cache");
        p
    } else {
        return None;
    };
    Some(dir.join("rio").join("sugarloaf-vulkan.cache"))
}

fn create_pipeline_cache(device: &Device) -> vk::PipelineCache {
    let initial_data: Vec<u8> = pipeline_cache_path()
        .and_then(|p| std::fs::read(&p).ok())
        .unwrap_or_default();
    if !initial_data.is_empty() {
        tracing::info!("loaded Vulkan pipeline cache: {} bytes", initial_data.len());
    }
    let info = vk::PipelineCacheCreateInfo::default().initial_data(&initial_data);
    unsafe {
        device
            .create_pipeline_cache(&info, None)
            .expect("create_pipeline_cache")
    }
}

fn save_pipeline_cache(device: &Device, cache: vk::PipelineCache) {
    let Some(path) = pipeline_cache_path() else {
        return;
    };
    let data = match unsafe { device.get_pipeline_cache_data(cache) } {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("get_pipeline_cache_data failed: {:?}", e);
            return;
        }
    };
    if data.is_empty() {
        return;
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("pipeline cache mkdir {:?} failed: {}", parent, e);
            return;
        }
    }
    if let Err(e) = std::fs::write(&path, &data) {
        tracing::warn!("pipeline cache write {:?} failed: {}", path, e);
    } else {
        tracing::info!(
            "saved Vulkan pipeline cache: {} bytes → {:?}",
            data.len(),
            path
        );
    }
}

// -------------------------------------------------------------------------
// Internal helpers (free functions so `new()` stays readable).
// -------------------------------------------------------------------------

fn create_instance(
    entry: &Entry,
    window: &SugarloafWindow,
    enable_validation: bool,
) -> Instance {
    let app_name = c"sugarloaf";
    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name)
        .application_version(0)
        .engine_name(app_name)
        .engine_version(0)
        .api_version(vk::API_VERSION_1_3);

    // KHR_surface + the right platform surface extension for the window
    // handle we were given. Adding extensions the driver doesn't
    // advertise makes `create_instance` fail, so we match the window
    // type exactly instead of asking for all three.
    let mut extensions: Vec<*const c_char> = vec![khr::surface::NAME.as_ptr()];
    match window.display_handle().unwrap().as_raw() {
        RawDisplayHandle::Xlib(_) => extensions.push(khr::xlib_surface::NAME.as_ptr()),
        RawDisplayHandle::Xcb(_) => extensions.push(khr::xcb_surface::NAME.as_ptr()),
        RawDisplayHandle::Wayland(_) => {
            extensions.push(khr::wayland_surface::NAME.as_ptr())
        }
        other => panic!("Vulkan backend: unsupported display handle {:?}", other),
    }

    // Validation: append `VK_EXT_debug_utils` so we can install a
    // messenger callback after instance creation. The layer
    // (`VK_LAYER_KHRONOS_validation`) is enabled separately via
    // `enabled_layer_names` below.
    let validation_layer_name = c"VK_LAYER_KHRONOS_validation";
    let layer_ptrs: Vec<*const c_char> = if enable_validation {
        if validation_layer_available(entry, validation_layer_name) {
            extensions.push(ash::ext::debug_utils::NAME.as_ptr());
            vec![validation_layer_name.as_ptr()]
        } else {
            tracing::warn!(
                "RIO_VULKAN_VALIDATION set but VK_LAYER_KHRONOS_validation \
                 not available — install `vulkan-validationlayers` (Debian) \
                 / `vulkan-validation-layers` (Arch) to enable it"
            );
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extensions)
        .enabled_layer_names(&layer_ptrs);

    unsafe { entry.create_instance(&create_info, None) }
        .expect("vkCreateInstance failed — is a Vulkan 1.3 driver installed?")
}

/// True if the user opted into validation via `RIO_VULKAN_VALIDATION=1`.
/// We always read the env var (debug + release) so users can flip it
/// on for one run without recompiling.
fn validation_requested() -> bool {
    std::env::var_os("RIO_VULKAN_VALIDATION")
        .map(|v| v != "0" && !v.is_empty())
        .unwrap_or(false)
}

fn validation_layer_available(entry: &Entry, target: &CStr) -> bool {
    match unsafe { entry.enumerate_instance_layer_properties() } {
        Ok(layers) => layers.iter().any(|l| {
            let name = unsafe { CStr::from_ptr(l.layer_name.as_ptr()) };
            name == target
        }),
        Err(_) => false,
    }
}

fn create_debug_messenger(entry: &Entry, instance: &Instance) -> Option<DebugMessenger> {
    let loader = ash::ext::debug_utils::Instance::new(entry, instance);

    let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(debug_callback));

    let handle = unsafe { loader.create_debug_utils_messenger(&info, None) }
        .expect("create_debug_utils_messenger");
    tracing::info!("Vulkan validation layers active");
    Some(DebugMessenger { loader, handle })
}

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    msg_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let data = unsafe { &*callback_data };
    let message = if data.p_message.is_null() {
        std::borrow::Cow::Borrowed("<null>")
    } else {
        unsafe { CStr::from_ptr(data.p_message) }.to_string_lossy()
    };
    let kind = if msg_type.contains(vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION) {
        "validation"
    } else if msg_type.contains(vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE) {
        "perf"
    } else {
        "general"
    };
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        tracing::error!("vk[{}] {}", kind, message);
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        tracing::warn!("vk[{}] {}", kind, message);
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        tracing::info!("vk[{}] {}", kind, message);
    } else {
        tracing::debug!("vk[{}] {}", kind, message);
    }
    vk::FALSE
}

fn create_surface(
    entry: &Entry,
    instance: &Instance,
    window: &SugarloafWindow,
) -> vk::SurfaceKHR {
    let display = window.display_handle().unwrap().as_raw();
    let window_handle = window.window_handle().unwrap().as_raw();

    unsafe {
        match (display, window_handle) {
            (RawDisplayHandle::Xlib(d), RawWindowHandle::Xlib(w)) => {
                let loader = khr::xlib_surface::Instance::new(entry, instance);
                let info = vk::XlibSurfaceCreateInfoKHR::default()
                    .dpy(
                        d.display
                            .expect("Xlib display pointer missing")
                            .as_ptr()
                            .cast(),
                    )
                    .window(w.window);
                loader
                    .create_xlib_surface(&info, None)
                    .expect("create_xlib_surface")
            }
            (RawDisplayHandle::Xcb(d), RawWindowHandle::Xcb(w)) => {
                let loader = khr::xcb_surface::Instance::new(entry, instance);
                let info = vk::XcbSurfaceCreateInfoKHR::default()
                    .connection(
                        d.connection
                            .expect("Xcb connection pointer missing")
                            .as_ptr()
                            .cast(),
                    )
                    .window(w.window.get());
                loader
                    .create_xcb_surface(&info, None)
                    .expect("create_xcb_surface")
            }
            (RawDisplayHandle::Wayland(d), RawWindowHandle::Wayland(w)) => {
                let loader = khr::wayland_surface::Instance::new(entry, instance);
                let info = vk::WaylandSurfaceCreateInfoKHR::default()
                    .display(d.display.as_ptr().cast())
                    .surface(w.surface.as_ptr().cast());
                loader
                    .create_wayland_surface(&info, None)
                    .expect("create_wayland_surface")
            }
            (d, w) => panic!(
                "Vulkan backend: mismatched or unsupported handles: display={d:?} window={w:?}"
            ),
        }
    }
}

/// Pick a physical device + queue family. Prefer discrete GPU, require
/// a queue family that supports both graphics and present on our surface.
fn pick_physical_device(
    instance: &Instance,
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> (vk::PhysicalDevice, u32) {
    let devices = unsafe { instance.enumerate_physical_devices() }
        .expect("enumerate_physical_devices");

    let mut best: Option<(vk::PhysicalDevice, u32, i32)> = None;
    for device in devices {
        let props = unsafe { instance.get_physical_device_properties(device) };
        let qf_props =
            unsafe { instance.get_physical_device_queue_family_properties(device) };

        for (index, qf) in qf_props.iter().enumerate() {
            let index = index as u32;
            if !qf.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                continue;
            }
            let present_ok = unsafe {
                surface_loader
                    .get_physical_device_surface_support(device, index, surface)
                    .unwrap_or(false)
            };
            if !present_ok {
                continue;
            }
            let score = match props.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 1000,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 500,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 100,
                vk::PhysicalDeviceType::CPU => 10,
                _ => 1,
            };
            if best.map(|(_, _, s)| score > s).unwrap_or(true) {
                best = Some((device, index, score));
            }
        }
    }

    let (device, queue_family, _) =
        best.expect("no Vulkan device with graphics + present support on this surface");
    (device, queue_family)
}

fn physical_device_name(instance: &Instance, device: vk::PhysicalDevice) -> String {
    let props = unsafe { instance.get_physical_device_properties(device) };
    // `device_name` is a C string embedded in a fixed-size array.
    let raw = props.device_name.as_ptr();
    unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned()
}

fn create_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    queue_family_index: u32,
) -> Device {
    let queue_priorities = [1.0f32];
    let queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family_index)
        .queue_priorities(&queue_priorities);

    let device_extensions = [khr::swapchain::NAME.as_ptr()];

    // Enable dynamic rendering up front — it's Vulkan 1.3 core. We're
    // not using it yet in the clear-only path, but enabling it here
    // avoids having to recreate the device when pipelines land.
    let mut vk13_features =
        vk::PhysicalDeviceVulkan13Features::default().dynamic_rendering(true);

    let queue_infos = [queue_info];
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&device_extensions)
        .push_next(&mut vk13_features);

    unsafe { instance.create_device(physical_device, &create_info, None) }
        .expect("vkCreateDevice")
}

/// Build a swapchain and its image views. `old` is passed as
/// `old_swapchain` so the driver can recycle images during resize.
#[allow(clippy::too_many_arguments)]
fn create_swapchain(
    device: &Device,
    surface_loader: &khr::surface::Instance,
    swapchain_loader: &khr::swapchain::Device,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    requested_width: u32,
    requested_height: u32,
    old: vk::SwapchainKHR,
) -> (
    vk::SwapchainKHR,
    vk::Format,
    vk::ColorSpaceKHR,
    vk::Extent2D,
    Vec<vk::Image>,
    Vec<vk::ImageView>,
) {
    let caps = unsafe {
        surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface)
            .expect("get_physical_device_surface_capabilities")
    };
    let formats = unsafe {
        surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .expect("get_physical_device_surface_formats")
    };
    let present_modes = unsafe {
        surface_loader
            .get_physical_device_surface_present_modes(physical_device, surface)
            .expect("get_physical_device_surface_present_modes")
    };

    // Prefer BGRA8_UNORM (linear) so blending stays in gamma space — the
    // same choice Metal makes (`MTLPixelFormat::BGRA8Unorm` + DisplayP3
    // tag). Fragment shaders will emit sRGB-encoded output. If the
    // driver doesn't offer BGRA8_UNORM, fall back to whatever it gives
    // us — formats[0] is guaranteed present per the spec.
    let chosen_format = formats
        .iter()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_UNORM
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .copied()
        .unwrap_or(formats[0]);

    // Present mode: Mailbox for low-latency "triple buffered", FIFO as a
    // guaranteed fallback. We don't expose a config knob yet — same
    // story as Metal's hard-coded `maximumDrawableCount = 3`.
    let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
        vk::PresentModeKHR::MAILBOX
    } else {
        vk::PresentModeKHR::FIFO
    };

    let extent = if caps.current_extent.width != u32::MAX {
        caps.current_extent
    } else {
        vk::Extent2D {
            width: requested_width
                .clamp(caps.min_image_extent.width, caps.max_image_extent.width),
            height: requested_height
                .clamp(caps.min_image_extent.height, caps.max_image_extent.height),
        }
    };

    // Aim for 3 images where the driver allows it (triple buffering),
    // clamped to the advertised range. `max_image_count == 0` means "no
    // upper limit".
    let mut image_count = caps.min_image_count.max(3);
    if caps.max_image_count != 0 && image_count > caps.max_image_count {
        image_count = caps.max_image_count;
    }

    let create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(chosen_format.format)
        .image_color_space(chosen_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST,
        )
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(caps.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .old_swapchain(old);

    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }
        .expect("create_swapchain");

    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }
        .expect("get_swapchain_images");

    let views = images
        .iter()
        .map(|&image| {
            let info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(chosen_format.format)
                .components(vk::ComponentMapping::default())
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            unsafe { device.create_image_view(&info, None) }.expect("create_image_view")
        })
        .collect();

    (
        swapchain,
        chosen_format.format,
        chosen_format.color_space,
        extent,
        images,
        views,
    )
}

fn create_frames(
    device: &Device,
    queue_family_index: u32,
) -> [FrameSync; FRAMES_IN_FLIGHT] {
    std::array::from_fn(|_| {
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info =
            vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family_index)
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);

        unsafe {
            let image_available = device
                .create_semaphore(&semaphore_info, None)
                .expect("create_semaphore");
            let render_finished = device
                .create_semaphore(&semaphore_info, None)
                .expect("create_semaphore");
            let in_flight = device
                .create_fence(&fence_info, None)
                .expect("create_fence");
            let cmd_pool = device
                .create_command_pool(&pool_info, None)
                .expect("create_command_pool");
            let alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(cmd_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd_buffer = device
                .allocate_command_buffers(&alloc_info)
                .expect("allocate_command_buffers")[0];

            FrameSync {
                image_available,
                render_finished,
                in_flight,
                cmd_pool,
                cmd_buffer,
            }
        }
    })
}
