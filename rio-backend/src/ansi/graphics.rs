// graphics.rs was retired from a alacritty PR made by ayosec
// Alacritty is licensed under Apache 2.0 license.
// https://github.com/alacritty/alacritty/pull/4763/files

use crate::ansi::sixel;
use crate::config::colors::ColorRgb;
use crate::crosswords::grid::Dimensions;
use crate::sugarloaf::{GraphicData, GraphicId};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::mem;
use std::sync::Arc;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct UpdateQueues {
    /// Atlas graphics (sixel/iTerm2) read from the PTY.
    pub pending: Vec<GraphicData>,

    /// Image textures (kitty) keyed by image_id.
    pub pending_images: Vec<(u32, GraphicData)>,

    /// Image keys removed from the grid or evicted
    /// (`sugarloaf::graphics::kitty_image_key` / `atlas_image_key`).
    pub remove_queue: Vec<u64>,
}

/// Kitty graphics Unicode placeholder character
pub const KITTY_PLACEHOLDER: char = '\u{10EEEE}';

/// Stored image data for Kitty graphics protocol
#[derive(Debug, Clone, PartialEq)]
pub struct StoredImage {
    pub data: GraphicData,
    pub transmission_time: std::time::Instant,
}

/// Overlay placement for a kitty graphics image.
/// Stored separately from grid cells — rendered as an overlay layer.
///
/// Kitty images use the protocol's `image_id: u32` directly, not `GraphicId`.
/// `GraphicId` is for atlas-based graphics (sixel/iTerm2) which share a
/// sequential ID space. Kitty image_ids come from the protocol and would
/// collide with atlas IDs. They also use a completely separate rendering
/// pipeline (per-image GPU textures, not atlas), so there's no reason to
/// wrap them in `GraphicId`.
#[derive(Debug, Clone, PartialEq)]
pub struct KittyPlacement {
    /// Kitty protocol image ID (i= parameter).
    pub image_id: u32,
    /// Kitty protocol placement ID (p= parameter).
    pub placement_id: u32,
    /// Source rectangle within the image, exactly as requested
    /// (`x=`/`y=`/`w=`/`h=`; zero width/height means "to the image
    /// edge"). Stored raw and resolved against the image's current
    /// dimensions at read time, so a retransmit that changes the
    /// image size re-clamps instead of showing a stale crop.
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
    /// Grid column of the top-left corner.
    pub dest_col: usize,
    /// Absolute row (scrollback-aware) of the top-left corner.
    pub dest_row: i64,
    /// Display size in cells.
    pub columns: u32,
    pub rows: u32,
    /// The `c=`/`r=` span the client requested (0 = derived). Kept
    /// separate from `columns`/`rows` so a cell size change can tell
    /// cell-sized placements (which track the grid) apart from
    /// native-size ones (which keep their pixel size).
    pub requested_columns: u32,
    pub requested_rows: u32,
    /// Cached display pixel size for grid footprint bookkeeping. The
    /// render path resolves size per frame and never reads these.
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// Sub-cell pixel offset.
    pub cell_x_offset: u32,
    pub cell_y_offset: u32,
    /// Z-index layer for rendering order.
    pub z_index: i32,
    /// Transmission timestamp for cache invalidation.
    pub transmit_time: std::time::Instant,
}

/// Resolve a raw kitty source rectangle against the image's current
/// dimensions: origin clamped to the edges, zero width/height meaning
/// "to the edge". Returns `None` when nothing of the crop lies inside
/// the image (or the image is empty).
pub fn resolve_source_rect(
    source_x: u32,
    source_y: u32,
    source_width: u32,
    source_height: u32,
    image_width: usize,
    image_height: usize,
) -> Option<(usize, usize, usize, usize)> {
    if image_width == 0 || image_height == 0 {
        return None;
    }
    let x = (source_x as usize).min(image_width);
    let y = (source_y as usize).min(image_height);
    let width = if source_width > 0 {
        (source_width as usize).min(image_width - x)
    } else {
        image_width - x
    };
    let height = if source_height > 0 {
        (source_height as usize).min(image_height - y)
    } else {
        image_height - y
    };
    if width == 0 || height == 0 {
        return None;
    }
    Some((x, y, width, height))
}

/// Display pixel size for a kitty placement: the source rectangle
/// scaled to the requested cell span, keeping aspect when only one
/// axis is given, or shown at native size when no span is requested.
pub fn kitty_display_size(
    source_width: usize,
    source_height: usize,
    requested_columns: u32,
    requested_rows: u32,
    cell_width: usize,
    cell_height: usize,
) -> (usize, usize) {
    if source_width == 0 || source_height == 0 {
        return (0, 0);
    }
    match (requested_columns, requested_rows) {
        (0, 0) => (source_width, source_height),
        (c, 0) => {
            let w = c as usize * cell_width;
            let h =
                (source_height as f64 * w as f64 / source_width as f64).round() as usize;
            (w, h)
        }
        (0, r) => {
            let h = r as usize * cell_height;
            let w =
                (source_width as f64 * h as f64 / source_height as f64).round() as usize;
            (w, h)
        }
        (c, r) => (c as usize * cell_width, r as usize * cell_height),
    }
}

impl KittyPlacement {
    /// Recompute display size and cell span against the image's
    /// current dimensions and a cell size. Cell-sized placements
    /// track the grid; native-size ones keep their pixel dimensions
    /// but re-derive how many cells they cover. Called on resize and
    /// retransmission.
    pub fn rescale(
        &mut self,
        image_width: usize,
        image_height: usize,
        cell_width: usize,
        cell_height: usize,
    ) {
        if cell_width == 0 || cell_height == 0 {
            return;
        }
        let Some((_, _, source_width, source_height)) = resolve_source_rect(
            self.source_x,
            self.source_y,
            self.source_width,
            self.source_height,
            image_width,
            image_height,
        ) else {
            // Nothing of the crop is inside the image: invisible, no
            // cell footprint.
            self.pixel_width = 0;
            self.pixel_height = 0;
            self.columns = 0;
            self.rows = 0;
            return;
        };
        let (w, h) = kitty_display_size(
            source_width,
            source_height,
            self.requested_columns,
            self.requested_rows,
            cell_width,
            cell_height,
        );
        if w == 0 || h == 0 {
            return;
        }
        self.pixel_width = w as u32;
        self.pixel_height = h as u32;
        // Offsets are stored raw and clamped where they're read, so a
        // shrink-then-grow of the cell size can't lose the original.
        let x_offset = (self.cell_x_offset as usize).min(cell_width - 1);
        let y_offset = (self.cell_y_offset as usize).min(cell_height - 1);
        self.columns = if self.requested_columns > 0 {
            self.requested_columns
        } else {
            (w + x_offset).div_ceil(cell_width) as u32
        };
        self.rows = if self.requested_rows > 0 {
            self.requested_rows
        } else {
            (h + y_offset).div_ceil(cell_height) as u32
        };
    }
}

/// On-screen quad for a direct kitty placement, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KittyOverlayGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Normalized source rectangle within the image texture.
    pub source_rect: [f32; 4],
}

/// Clip an overlay quad to a panel's content rectangle, shrinking the
/// normalized source rect proportionally so the visible slice shows
/// exactly the covered part of the image. Returns `false` when nothing
/// of the quad lies inside the rect (drop the overlay).
///
/// Without this, an image wider or taller than its panel paints across
/// split dividers onto neighbor panels — the image pipeline draws
/// global quads with no per-panel scissor.
pub fn clip_overlay_to_rect(
    overlay: &mut crate::sugarloaf::GraphicOverlay,
    clip_x0: f32,
    clip_y0: f32,
    clip_x1: f32,
    clip_y1: f32,
) -> bool {
    if overlay.width <= 0.0 || overlay.height <= 0.0 {
        return false;
    }
    let x0 = overlay.x;
    let y0 = overlay.y;
    let x1 = overlay.x + overlay.width;
    let y1 = overlay.y + overlay.height;

    let nx0 = x0.max(clip_x0);
    let ny0 = y0.max(clip_y0);
    let nx1 = x1.min(clip_x1);
    let ny1 = y1.min(clip_y1);
    if nx1 <= nx0 || ny1 <= ny0 {
        return false;
    }

    let [u0, v0, u1, v1] = overlay.source_rect;
    let fx0 = (nx0 - x0) / overlay.width;
    let fx1 = (nx1 - x0) / overlay.width;
    let fy0 = (ny0 - y0) / overlay.height;
    let fy1 = (ny1 - y0) / overlay.height;
    overlay.source_rect = [
        u0 + (u1 - u0) * fx0,
        v0 + (v1 - v0) * fy0,
        u0 + (u1 - u0) * fx1,
        v0 + (v1 - v0) * fy1,
    ];
    overlay.x = nx0;
    overlay.y = ny0;
    overlay.width = nx1 - nx0;
    overlay.height = ny1 - ny0;
    true
}

/// Viewport parameters for placement geometry: the panel content
/// origin in physical pixels, the canonical cell stride the grid
/// paints with, and the scroll state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayViewport {
    pub cell_width: f32,
    pub cell_height: f32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub history_size: i64,
    pub display_offset: i64,
    pub screen_lines: i64,
}

/// Compute where a direct kitty placement lands on screen.
///
/// The crop and display size are resolved here, per frame, against
/// the image's current dimensions and the viewport's cell stride —
/// nothing baked at place time can go stale when the image is
/// retransmitted or the cell size changes. The viewport cell stride
/// must be the same one the grid paints with, so image position and
/// text stay in lockstep. Returns `None` when the placement is fully
/// outside the viewport or resolves to nothing visible; a partially
/// visible placement keeps its full quad and the GPU clips it.
pub fn kitty_overlay_geometry(
    placement: &KittyPlacement,
    image_width: usize,
    image_height: usize,
    viewport: &OverlayViewport,
) -> Option<KittyOverlayGeometry> {
    // dest_row is absolute (scrollback aware); the viewport top sits
    // at history_size - display_offset, so scrolling up moves the
    // placement down relative to the viewport.
    let screen_row =
        placement.dest_row - (viewport.history_size - viewport.display_offset);
    let bottom_row = screen_row + placement.rows as i64;
    if bottom_row <= 0 || screen_row >= viewport.screen_lines {
        return None;
    }

    let (source_x, source_y, source_width, source_height) = resolve_source_rect(
        placement.source_x,
        placement.source_y,
        placement.source_width,
        placement.source_height,
        image_width,
        image_height,
    )?;

    let cell_width_px = viewport.cell_width.round() as usize;
    let cell_height_px = viewport.cell_height.round() as usize;
    if cell_width_px == 0 || cell_height_px == 0 {
        return None;
    }
    let (display_width, display_height) = kitty_display_size(
        source_width,
        source_height,
        placement.requested_columns,
        placement.requested_rows,
        cell_width_px,
        cell_height_px,
    );
    if display_width == 0 || display_height == 0 {
        return None;
    }

    // Normalized `[u0, v0, u1, v1]` (origin, end), the convention all
    // three image shaders share.
    let image_width = image_width as f32;
    let image_height = image_height as f32;
    let source_rect = [
        source_x as f32 / image_width,
        source_y as f32 / image_height,
        (source_x + source_width) as f32 / image_width,
        (source_y + source_height) as f32 / image_height,
    ];

    // Per the kitty spec the sub-cell offset stays inside the cell
    // box; stored values are raw, so clamp against the current cell
    // size here.
    let x_offset = (placement.cell_x_offset as f32).min(viewport.cell_width - 1.0);
    let y_offset = (placement.cell_y_offset as f32).min(viewport.cell_height - 1.0);

    Some(KittyOverlayGeometry {
        x: viewport.origin_x + placement.dest_col as f32 * viewport.cell_width + x_offset,
        y: viewport.origin_y + screen_row as f32 * viewport.cell_height + y_offset,
        width: display_width as f32,
        height: display_height as f32,
        source_rect,
    })
}

/// One displayed sixel/iTerm2 image region, anchored to grid content
/// (DEC semantics — unlike floating kitty placements, these clip under
/// text and erase operations and shift with region scrolls).
///
/// The source rectangle lives in display-pixel space at insert time:
/// initially `(0, 0, total_w, total_h)`; overwrite splits subtract
/// cell-aligned holes, producing children referencing the same texture
/// with adjusted crops (no pixel copies). Normalized texture
/// coordinates fall out directly since display space is a uniform
/// scale of the image.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasPlacement {
    /// Texture key (`atlas_image_key(GraphicId)`).
    pub image_key: u64,
    /// Anchor row in the stable absolute space
    /// (`lines_evicted + history + screen_row` at insert).
    pub abs_row: i64,
    /// Leftmost grid column.
    pub col: usize,
    /// Cell span of this (possibly split) region.
    pub columns: usize,
    pub rows: usize,
    /// Crop in display pixels at insert scale.
    pub src_x: u32,
    pub src_y: u32,
    pub src_width: u32,
    pub src_height: u32,
    /// Full display size at insert (normalization denominator).
    pub total_width: u32,
    pub total_height: u32,
    /// Cell stride at insert; rendering scales by live/insert.
    pub insert_cell_w: u16,
    pub insert_cell_h: u16,
}

impl AtlasPlacement {
    /// Child covering `rows` [row0, row1) x `cols` [col0, col1) of this
    /// placement (caller guarantees a non-empty intersection with the
    /// placement's rect). The crop follows in display-pixel space;
    /// edges that coincide with the parent's keep its exact bounds so
    /// partial edge cells stay partial.
    pub(crate) fn slice(
        &self,
        row0: i64,
        row1: i64,
        col0: usize,
        col1: usize,
    ) -> AtlasPlacement {
        let icw = self.insert_cell_w as u32;
        let ich = self.insert_cell_h as u32;
        let src_x0 = self.src_x + (col0 - self.col) as u32 * icw;
        let src_y0 = self.src_y + (row0 - self.abs_row) as u32 * ich;
        let src_x1 = (self.src_x + (col1 - self.col) as u32 * icw)
            .min(self.src_x + self.src_width);
        let src_y1 = (self.src_y + (row1 - self.abs_row) as u32 * ich)
            .min(self.src_y + self.src_height);
        AtlasPlacement {
            image_key: self.image_key,
            abs_row: row0,
            col: col0,
            columns: col1 - col0,
            rows: (row1 - row0) as usize,
            src_x: src_x0,
            src_y: src_y0,
            src_width: src_x1.saturating_sub(src_x0),
            src_height: src_y1.saturating_sub(src_y0),
            total_width: self.total_width,
            total_height: self.total_height,
            insert_cell_w: self.insert_cell_w,
            insert_cell_h: self.insert_cell_h,
        }
    }

    /// Subtract a cell-aligned hole (rows [hr0, hr1) x cols [hc0, hc1))
    /// from this placement. Returns `None` when the hole misses the
    /// placement entirely (keep it as is); otherwise the surviving
    /// pieces — up to four children referencing the same texture with
    /// adjusted crops, zero pixel copies — are appended to `out` (an
    /// empty append means the hole swallowed the whole placement).
    pub fn subtract_rect(
        &self,
        hr0: i64,
        hr1: i64,
        hc0: usize,
        hc1: usize,
        out: &mut Vec<AtlasPlacement>,
    ) -> Option<()> {
        let p_r0 = self.abs_row;
        let p_r1 = self.abs_row + self.rows as i64;
        let p_c0 = self.col;
        let p_c1 = self.col + self.columns;
        if hr1 <= p_r0 || hr0 >= p_r1 || hc1 <= p_c0 || hc0 >= p_c1 {
            return None;
        }
        let ir0 = hr0.max(p_r0);
        let ir1 = hr1.min(p_r1);
        // Filter at each push (not a whole-vec retain: `out` may
        // accumulate children of many placements in the clip loops).
        let mut push = |child: AtlasPlacement| {
            if child.columns > 0
                && child.rows > 0
                && child.src_width > 0
                && child.src_height > 0
            {
                out.push(child);
            }
        };
        // Top and bottom keep the full placement width; left and right
        // cover only the hole's rows.
        if ir0 > p_r0 {
            push(self.slice(p_r0, ir0, p_c0, p_c1));
        }
        if ir1 < p_r1 {
            push(self.slice(ir1, p_r1, p_c0, p_c1));
        }
        if hc0 > p_c0 {
            push(self.slice(ir0, ir1, p_c0, hc0.min(p_c1)));
        }
        if hc1 < p_c1 {
            push(self.slice(ir0, ir1, hc1.max(p_c0), p_c1));
        }
        Some(())
    }
}

/// Compute where an atlas placement lands on screen. Same viewport
/// contract as `kitty_overlay_geometry`; display size scales from the
/// insert-time cell stride to the live one so images track font size.
pub fn atlas_overlay_geometry(
    placement: &AtlasPlacement,
    viewport: &OverlayViewport,
) -> Option<KittyOverlayGeometry> {
    let screen_row =
        placement.abs_row - (viewport.history_size - viewport.display_offset);
    let bottom_row = screen_row + placement.rows as i64;
    if bottom_row <= 0 || screen_row >= viewport.screen_lines {
        return None;
    }
    if placement.src_width == 0
        || placement.src_height == 0
        || placement.total_width == 0
        || placement.total_height == 0
    {
        return None;
    }

    let scale_x = viewport.cell_width / placement.insert_cell_w.max(1) as f32;
    let scale_y = viewport.cell_height / placement.insert_cell_h.max(1) as f32;
    let total_w = placement.total_width as f32;
    let total_h = placement.total_height as f32;

    Some(KittyOverlayGeometry {
        x: viewport.origin_x + placement.col as f32 * viewport.cell_width,
        y: viewport.origin_y + screen_row as f32 * viewport.cell_height,
        width: placement.src_width as f32 * scale_x,
        height: placement.src_height as f32 * scale_y,
        source_rect: [
            placement.src_x as f32 / total_w,
            placement.src_y as f32 / total_h,
            (placement.src_x + placement.src_width) as f32 / total_w,
            (placement.src_y + placement.src_height) as f32 / total_h,
        ],
    })
}

/// Virtual placement metadata for Kitty graphics protocol
/// Stored separately from direct graphics in cells
#[derive(Debug, Clone, PartialEq)]
pub struct VirtualPlacement {
    pub image_id: u32,
    pub placement_id: u32,
    pub columns: u32,
    pub rows: u32,
    /// Raw source rectangle (`x=`/`y=`/`w=`/`h=`), resolved against
    /// the image's current dimensions at render time like direct
    /// placements.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Per-screen Kitty graphics state.
///
/// Each terminal has two screens (main and alt). Per the kitty graphics
/// spec each screen owns its own image cache, placements, and number
/// mappings, so swapping into the alt screen hides main-screen images
/// (and vice versa) instead of leaking them across the boundary.
///
/// `Graphics` keeps the *active* screen's state inline (so existing
/// rendering code can read `graphics.kitty_*` directly without changes)
/// and stores the *inactive* screen's state in this struct. On
/// `swap_kitty_screen_state` the two are exchanged with `mem::swap`.
#[derive(Debug, Default)]
pub struct KittyScreenState {
    pub kitty_images: FxHashMap<u32, StoredImage>,
    pub kitty_image_numbers: FxHashMap<u32, u32>,
    pub kitty_placements: FxHashMap<(u32, u32), KittyPlacement>,
    pub kitty_virtual_placements: FxHashMap<(u32, u32), VirtualPlacement>,
    pub atlas_placements: Vec<AtlasPlacement>,
    pub atlas_key_refs: FxHashMap<u64, u32>,
}

/// Track changes in the grid to add or to remove graphics.
#[derive(Debug)]
pub struct Graphics {
    /// Last generated identifier.
    pub last_id: u64,

    /// New atlas graphics (sixel/iTerm2), received from the PTY.
    pub pending: Vec<GraphicData>,

    /// New image textures (kitty), keyed by image_id.
    pub pending_images: Vec<(u32, GraphicData)>,

    /// Graphics removed from the grid.
    pub texture_operations: Arc<Mutex<Vec<u64>>>,

    /// Shared palette for Sixel graphics.
    pub sixel_shared_palette: Option<Vec<ColorRgb>>,

    /// Cell height in pixels.
    pub cell_height: f32,

    /// Cell width in pixels.
    pub cell_width: f32,

    /// Current Sixel parser.
    pub sixel_parser: Option<Box<sixel::Parser>>,

    /// Kitty graphics: Cache of transmitted images (by image_id)
    /// Allows placing the same image multiple times without re-transmission
    pub kitty_images: FxHashMap<u32, StoredImage>,

    /// Kitty graphics: Image number to ID mapping (for I= parameter)
    /// Maps image number to the most recently transmitted image with that number
    pub kitty_image_numbers: FxHashMap<u32, u32>,

    /// Kitty graphics: Virtual placements (when U=1)
    /// Key is (image_id, placement_id), value is placement metadata
    pub kitty_virtual_placements: FxHashMap<(u32, u32), VirtualPlacement>,

    /// Kitty graphics: State for chunked image transmissions
    /// Stores incomplete transmissions and tracks current transmission key
    pub kitty_chunking_state: crate::ansi::kitty_graphics_protocol::KittyGraphicsState,

    /// Total bytes of image data currently stored in memory
    /// Includes both pending graphics and stored Kitty images
    pub total_bytes: usize,

    /// Memory limit for graphics storage (default 320MB per kitty spec)
    /// If this is exceeded, oldest/unused images will be evicted
    pub total_limit: usize,

    /// Tracks when each graphic was added (for eviction priority)
    /// Maps GraphicId to insertion timestamp
    pub image_timestamps: FxHashMap<GraphicId, std::time::Instant>,

    /// Weak references to placed textures, for O(1) liveness checks.
    /// Avoids scanning the entire grid to find which graphics are in use.
    /// When an image loses its last placement, the
    /// will report strong_count() == 0, meaning the graphic is no longer displayed.

    /// Kitty graphics: Overlay placements.
    /// Key is (image_id, placement_id). Rendered as overlays, not in grid cells.
    pub kitty_placements: FxHashMap<(u32, u32), KittyPlacement>,

    /// Sixel/iTerm2 placements (DEC grid-plane semantics: clip under
    /// text/erase, shift with region scrolls, expire off the ring).
    pub atlas_placements: Vec<AtlasPlacement>,

    /// How many placements reference each atlas image key; the last
    /// release queues the key for pixel-store + GPU texture removal.
    pub atlas_key_refs: FxHashMap<u64, u32>,

    /// Kitty graphics state for the *inactive* screen.
    /// When the terminal toggles between main and alt screens this is
    /// swapped with the active fields (`kitty_images`, `kitty_placements`,
    /// `kitty_image_numbers`, `kitty_virtual_placements`) so each screen
    /// keeps its own image set.
    pub kitty_inactive_screen: KittyScreenState,

    /// Counter for auto-assigning internal placement IDs.
    ///
    /// Per kitty spec, when a client asks to place an image without an
    /// explicit `p=` (or with `p=0`), the terminal must allocate a
    /// unique placement_id internally so multiple placements of the
    /// same image don't collide at key `(image_id, 0)`. We allocate
    /// from `0x80000000..` so internal IDs do not collide with the
    /// client-supplied range (`1..0x80000000`).
    pub next_internal_placement_id: u32,

    /// Signals the renderer that overlay placements have changed.
    pub kitty_graphics_dirty: bool,
}

impl Default for Graphics {
    fn default() -> Self {
        Self {
            last_id: 0,
            pending: Vec::new(),
            pending_images: Vec::new(),
            texture_operations: Arc::new(Mutex::new(Vec::new())),
            sixel_shared_palette: None,
            cell_height: 0.0,
            cell_width: 0.0,
            sixel_parser: None,
            kitty_images: FxHashMap::default(),
            kitty_image_numbers: FxHashMap::default(),
            kitty_virtual_placements: FxHashMap::default(),
            kitty_chunking_state:
                crate::ansi::kitty_graphics_protocol::KittyGraphicsState::default(),
            total_bytes: 0,
            total_limit: 320 * 1024 * 1024, // 320MB per kitty spec
            image_timestamps: FxHashMap::default(),
            kitty_placements: FxHashMap::default(),
            atlas_placements: Vec::new(),
            atlas_key_refs: FxHashMap::default(),
            kitty_inactive_screen: KittyScreenState::default(),
            next_internal_placement_id: 0,
            kitty_graphics_dirty: false,
        }
    }
}

impl Graphics {
    /// Create a new instance, and initialize it with the dimensions of the
    /// window.
    pub fn new<S: Dimensions>(size: &S) -> Self {
        let mut graphics = Graphics::default();
        graphics.resize(size);
        graphics
    }

    /// Generate a new graphic identifier (for sixel/iTerm2 atlas graphics).
    pub fn next_id(&mut self) -> GraphicId {
        self.last_id += 1;
        GraphicId::new(self.last_id)
    }

    /// Get queues to update graphics in the grid.
    ///
    /// If all queues are empty, it returns `None`.
    pub fn has_pending_updates(&self) -> bool {
        !self.pending.is_empty()
            || !self.pending_images.is_empty()
            || !self.texture_operations.lock().is_empty()
    }

    pub fn take_queues(&mut self) -> Option<UpdateQueues> {
        let remove_queue = {
            let mut queue = self.texture_operations.lock();
            if queue.is_empty() {
                Vec::new()
            } else {
                mem::take(&mut *queue)
            }
        };

        if remove_queue.is_empty()
            && self.pending.is_empty()
            && self.pending_images.is_empty()
        {
            return None;
        }

        Some(UpdateQueues {
            pending: mem::take(&mut self.pending),
            pending_images: mem::take(&mut self.pending_images),
            remove_queue,
        })
    }

    /// Update cell dimensions.
    pub fn resize<S: Dimensions>(&mut self, size: &S) {
        self.cell_height = size.square_height();
        self.cell_width = size.square_width();
    }

    /// Allocate a unique internal placement_id.
    ///
    /// Used by `place_kitty_overlay` whenever a placement request comes
    /// in with `placement_id == 0`. Without this, two `a=T` calls (or a
    /// client running `kitten icat` repeatedly) for the same image_id
    /// would each insert at key `(image_id, 0)` and the second would
    /// silently overwrite the first.
    pub fn allocate_internal_placement_id(&mut self) -> u32 {
        if self.next_internal_placement_id < 0x80000000 {
            self.next_internal_placement_id = 0x80000000;
        }
        let id = self.next_internal_placement_id;
        self.next_internal_placement_id = self
            .next_internal_placement_id
            .checked_add(1)
            .unwrap_or(0x80000000);
        id
    }

    /// Swap kitty graphics state between the active and inactive screen.
    ///
    /// Called by `Crosswords::swap_alt` so each screen (main vs alt)
    /// keeps its own image cache, placements, number mappings, and
    /// virtual placements. Marks the kitty overlay layer dirty so the
    /// renderer rebuilds its overlay set against the new active screen.
    /// Recount atlas image key references from the placement vec (the
    /// single source of truth) after any placement mutation. Keys that
    /// lost their last placement are queued so the frontend frees the
    /// pixel store and the GPU texture. The vec is tiny, so a full
    /// recount beats distributed per-child bookkeeping.
    pub fn recount_atlas_keys(&mut self) {
        let mut refs: FxHashMap<u64, u32> = FxHashMap::default();
        for placement in &self.atlas_placements {
            *refs.entry(placement.image_key).or_insert(0) += 1;
        }
        let mut removals = self.texture_operations.lock();
        for key in self.atlas_key_refs.keys() {
            if !refs.contains_key(key) {
                removals.push(*key);
            }
        }
        drop(removals);
        self.atlas_key_refs = refs;
    }

    pub fn swap_kitty_screen_state(&mut self) {
        std::mem::swap(
            &mut self.kitty_images,
            &mut self.kitty_inactive_screen.kitty_images,
        );
        std::mem::swap(
            &mut self.kitty_image_numbers,
            &mut self.kitty_inactive_screen.kitty_image_numbers,
        );
        std::mem::swap(
            &mut self.kitty_placements,
            &mut self.kitty_inactive_screen.kitty_placements,
        );
        std::mem::swap(
            &mut self.kitty_virtual_placements,
            &mut self.kitty_inactive_screen.kitty_virtual_placements,
        );
        std::mem::swap(
            &mut self.atlas_placements,
            &mut self.kitty_inactive_screen.atlas_placements,
        );
        std::mem::swap(
            &mut self.atlas_key_refs,
            &mut self.kitty_inactive_screen.atlas_key_refs,
        );
        self.kitty_graphics_dirty = true;
    }

    /// Clear all kitty graphics state on both screens. Used by full reset.
    pub fn clear_all_kitty_state(&mut self) {
        // Subtract bytes from the inactive screen before dropping it,
        // since total_bytes is the *global* counter.
        let inactive_bytes: usize = self
            .kitty_inactive_screen
            .kitty_images
            .values()
            .map(|s| s.data.pixels.len())
            .sum();
        self.total_bytes = self.total_bytes.saturating_sub(inactive_bytes);

        self.kitty_images.clear();
        self.kitty_image_numbers.clear();
        self.kitty_placements.clear();
        self.kitty_virtual_placements.clear();

        // Sixel/iTerm2 placements die with the reset too; queue every
        // referenced key (both screens) so the frontend frees the
        // pixel store and GPU textures.
        {
            let mut removals = self.texture_operations.lock();
            for key in self.atlas_key_refs.keys() {
                removals.push(*key);
            }
            for key in self.kitty_inactive_screen.atlas_key_refs.keys() {
                removals.push(*key);
            }
        }
        self.atlas_placements.clear();
        self.atlas_key_refs.clear();

        self.kitty_inactive_screen = KittyScreenState::default();
        self.kitty_graphics_dirty = true;
    }

    /// Store a kitty graphics image for later placement.
    /// Evicts old images if over memory limit.
    pub fn store_kitty_image(
        &mut self,
        image_id: u32,
        image_number: Option<u32>,
        mut data: GraphicData,
    ) {
        let now = std::time::Instant::now();
        data.transmit_time = now;

        // Evict before storing to protect images with active placements
        let new_bytes = data.pixels.len();
        if self.total_bytes + new_bytes > self.total_limit {
            // Collect active IDs — images with placements are protected
            let mut active = std::collections::HashSet::new();
            for placement in self.kitty_placements.values() {
                active.insert(placement.image_id as u64);
            }
            // Also protect the image we're about to store
            active.insert(image_id as u64);
            self.evict_images(new_bytes, &active);
        }

        // If replacing an existing image, subtract its bytes first
        if let Some(old) = self.kitty_images.get(&image_id) {
            self.total_bytes = self.total_bytes.saturating_sub(old.data.pixels.len());
        }

        self.kitty_images.insert(
            image_id,
            StoredImage {
                data,
                transmission_time: now,
            },
        );
        self.total_bytes += new_bytes;
        self.kitty_graphics_dirty = true;

        // Update image number mapping if provided
        if let Some(number) = image_number {
            self.kitty_image_numbers.insert(number, image_id);
        }
    }

    /// Get a stored kitty graphics image by ID
    pub fn get_kitty_image(&self, image_id: u32) -> Option<&StoredImage> {
        self.kitty_images.get(&image_id)
    }

    /// Get a stored kitty graphics image by number (I= parameter)
    /// Returns the most recently transmitted image with that number
    pub fn get_kitty_image_by_number(&self, image_number: u32) -> Option<&StoredImage> {
        self.kitty_image_numbers
            .get(&image_number)
            .and_then(|id| self.kitty_images.get(id))
    }

    /// Delete kitty graphics images
    pub fn delete_kitty_images(
        &mut self,
        predicate: impl Fn(&u32, &StoredImage) -> bool,
    ) {
        let before = self.kitty_images.len();
        self.kitty_images.retain(|id, img| !predicate(id, img));
        if self.kitty_images.len() != before {
            self.kitty_graphics_dirty = true;
        }
        // Clean up stale number mappings
        self.kitty_image_numbers
            .retain(|_, id| self.kitty_images.contains_key(id));
    }

    /// Calculate the memory size of a graphic in bytes
    fn calculate_graphic_bytes(graphic: &GraphicData) -> usize {
        graphic.pixels.len()
    }

    /// Evict images to make space for required_bytes.
    /// Returns true if enough space was freed, false otherwise.
    ///
    /// Eviction priority (per kitty spec, extended for per-screen state):
    /// 1. Inactive-screen images (the user is not looking at them)
    /// 2. Active-screen unused images (no live placement)
    /// 3. Active-screen used images (visible — last resort)
    ///
    /// Within each tier we evict oldest by timestamp first.
    pub fn evict_images(
        &mut self,
        required_bytes: usize,
        used_ids: &std::collections::HashSet<u64>,
    ) -> bool {
        use tracing::debug;

        if self.total_bytes + required_bytes <= self.total_limit {
            return true; // No eviction needed
        }

        let bytes_to_free = (self.total_bytes + required_bytes) - self.total_limit;
        debug!("Graphics memory: need to evict {} bytes (current: {}, limit: {}, required: {})",
            bytes_to_free, self.total_bytes, self.total_limit, required_bytes);

        /// Where an eviction candidate lives, so removal touches the
        /// right map. `Pending` is a sixel/iTerm2 atlas graphic in
        /// `self.pending`; the kitty variants are screen-scoped.
        #[derive(Copy, Clone, PartialEq, Eq)]
        enum CandidateSource {
            Pending,
            ActiveKitty,
            InactiveKitty,
        }

        // Tier scale: lower number = evict first
        // 0 = inactive kitty, 1 = active unused, 2 = active used (pending or kitty)
        fn tier_for(source: CandidateSource, is_used: bool) -> u8 {
            match source {
                CandidateSource::InactiveKitty => 0,
                CandidateSource::Pending | CandidateSource::ActiveKitty => {
                    if is_used {
                        2
                    } else {
                        1
                    }
                }
            }
        }

        // Candidate: (sentinel GraphicId, timestamp, is_used, bytes, source)
        let mut candidates: Vec<(
            GraphicId,
            std::time::Instant,
            bool,
            usize,
            CandidateSource,
        )> = Vec::new();

        // Check pending graphics (sixel/iTerm2 — atlas based, single screen)
        for graphic in &self.pending {
            if let Some(&timestamp) = self.image_timestamps.get(&graphic.id) {
                let is_used = used_ids.contains(&graphic.id.get());
                // A used pending graphic has grid cells referencing it
                // while its pixels haven't reached the renderer yet;
                // evicting it here would blank those cells permanently
                // (nothing re-triggers the upload). Prefer briefly
                // exceeding the byte budget over losing the image.
                if is_used {
                    continue;
                }
                let bytes = Self::calculate_graphic_bytes(graphic);
                candidates.push((
                    graphic.id,
                    timestamp,
                    is_used,
                    bytes,
                    CandidateSource::Pending,
                ));
            }
        }

        // Check active-screen kitty images
        for (&kitty_id, stored) in &self.kitty_images {
            let id_as_u64 = kitty_id as u64;
            let is_used = used_ids.contains(&id_as_u64);
            let bytes = Self::calculate_graphic_bytes(&stored.data);
            candidates.push((
                GraphicId::new(id_as_u64),
                stored.transmission_time,
                is_used,
                bytes,
                CandidateSource::ActiveKitty,
            ));
        }

        // Check inactive-screen kitty images. These are not visible to
        // the user, so they're the first tier to evict regardless of
        // whether they have a placement.
        for (&kitty_id, stored) in &self.kitty_inactive_screen.kitty_images {
            let id_as_u64 = kitty_id as u64;
            let bytes = Self::calculate_graphic_bytes(&stored.data);
            candidates.push((
                GraphicId::new(id_as_u64),
                stored.transmission_time,
                false, // not displayed (we're on the other screen)
                bytes,
                CandidateSource::InactiveKitty,
            ));
        }

        if candidates.is_empty() {
            debug!("No candidates for eviction");
            return false;
        }

        // Sort by tier (ascending), then oldest first within tier.
        candidates.sort_by(|a, b| {
            let ta = tier_for(a.4, a.2);
            let tb = tier_for(b.4, b.2);
            ta.cmp(&tb).then_with(|| a.1.cmp(&b.1))
        });

        let mut freed_bytes = 0usize;
        let mut evicted: Vec<(GraphicId, CandidateSource)> = Vec::new();

        for (graphic_id, _, is_used, bytes, source) in candidates {
            if freed_bytes >= bytes_to_free {
                break;
            }

            evicted.push((graphic_id, source));
            freed_bytes += bytes;

            debug!(
                "Evicting graphic id={}, bytes={}, used={}",
                graphic_id.get(),
                bytes,
                is_used
            );
        }

        // Actually remove the evicted graphics from the right home.
        for (id, source) in evicted {
            let evicted_u32 = id.get() as u32;
            match source {
                CandidateSource::Pending => {
                    self.pending.retain(|g| g.id != id);
                }
                CandidateSource::ActiveKitty => {
                    self.kitty_images.remove(&evicted_u32);
                    self.kitty_image_numbers.retain(|_, v| *v != evicted_u32);
                    self.kitty_graphics_dirty = true;
                }
                CandidateSource::InactiveKitty => {
                    self.kitty_inactive_screen.kitty_images.remove(&evicted_u32);
                    self.kitty_inactive_screen
                        .kitty_image_numbers
                        .retain(|_, v| *v != evicted_u32);
                }
            }

            // Remove timestamp (only used for pending atlas graphics)
            self.image_timestamps.remove(&id);

            // Add to removal queue so GPU textures get cleaned up.
            // The key namespace depends on where the image lived:
            // kitty ids map verbatim, atlas graphics live above 2^32.
            let key = match source {
                CandidateSource::Pending => crate::sugarloaf::atlas_image_key(id.get()),
                CandidateSource::ActiveKitty | CandidateSource::InactiveKitty => {
                    crate::sugarloaf::kitty_image_key(id.get() as u32)
                }
            };
            self.texture_operations.lock().push(key);
        }

        // Sweep dangling placements on both screens. A placement is
        // dangling if its referenced image_id is no longer in the
        // matching screen's image cache. This catches both:
        //   - kitty placements whose image was just evicted
        //   - cross-namespace coincidences where a sixel/iTerm2 atlas
        //     graphic with the same numeric id as a kitty image is
        //     evicted (the test_eviction_removes_dangling_placements
        //     test pins this defensive behaviour)
        let active_ids: std::collections::HashSet<u32> =
            self.kitty_images.keys().copied().collect();
        self.kitty_placements
            .retain(|_, p| active_ids.contains(&p.image_id));
        let inactive_ids: std::collections::HashSet<u32> = self
            .kitty_inactive_screen
            .kitty_images
            .keys()
            .copied()
            .collect();
        self.kitty_inactive_screen
            .kitty_placements
            .retain(|_, p| inactive_ids.contains(&p.image_id));

        // Update total_bytes
        self.total_bytes = self.total_bytes.saturating_sub(freed_bytes);

        debug!(
            "Evicted {} bytes, new total: {}",
            freed_bytes, self.total_bytes
        );
        freed_bytes >= bytes_to_free
    }

    /// Collect IDs of graphics still displayed on the grid or as
    /// overlays. O(number of placements).
    pub fn collect_active_graphic_ids(&mut self) -> std::collections::HashSet<u64> {
        let mut active = std::collections::HashSet::new();
        // Sixel/iTerm2 liveness: placements are the single owners.
        for placement in &self.atlas_placements {
            // Atlas keys live above 2^32; recover the GraphicId part.
            active.insert(placement.image_key - (1u64 << 32));
        }
        // Overlay-based (kitty) liveness — use image_id directly
        for placement in self.kitty_placements.values() {
            active.insert(placement.image_id as u64);
        }
        active
    }

    /// Track a new graphic's memory usage and timestamp
    pub fn track_graphic(&mut self, graphic_id: GraphicId, bytes: usize) {
        self.image_timestamps
            .insert(graphic_id, std::time::Instant::now());
        self.total_bytes += bytes;
        debug!(
            "Tracked graphic id={}, bytes={}, total_bytes={}",
            graphic_id.0, bytes, self.total_bytes
        );
    }

    /// Update total_bytes when a graphic is removed
    pub fn untrack_graphic(&mut self, graphic_id: GraphicId, bytes: usize) {
        self.image_timestamps.remove(&graphic_id);
        self.total_bytes = self.total_bytes.saturating_sub(bytes);
        debug!(
            "Untracked graphic id={}, bytes={}, total_bytes={}",
            graphic_id.0, bytes, self.total_bytes
        );
    }
}

#[test]
fn check_opaque_region() {
    use sugarloaf::ColorType;
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 10,
        height: 10,
        color_type: ColorType::Rgb,
        pixels: vec![255; 10 * 10 * 3],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };

    assert!(graphic.is_filled(1, 1, 3, 3));
    assert!(!graphic.is_filled(8, 8, 10, 10));

    let pixels = {
        // Put a transparent 3x3 box inside the picture.
        let mut data = vec![255; 10 * 10 * 4];
        for y in 3..6 {
            let offset = y * 10 * 4;
            data[offset..offset + 3 * 4].fill(0);
        }
        data
    };

    let graphic = GraphicData {
        id: GraphicId::new(1),
        pixels,
        width: 10,
        height: 10,
        color_type: ColorType::Rgba,
        is_opaque: false,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };

    assert!(graphic.is_filled(0, 0, 3, 3));
    assert!(!graphic.is_filled(1, 1, 4, 4));
}

#[test]
fn test_graphics_memory_tracking() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics::default();

    // Create a small graphic (100x100 RGBA = 40,000 bytes)
    let pixels = vec![255u8; 100 * 100 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 100,
        height: 100,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };

    let bytes = Graphics::calculate_graphic_bytes(&graphic);
    assert_eq!(bytes, 40_000);

    // Track the graphic
    graphics.track_graphic(GraphicId::new(1), bytes);
    assert_eq!(graphics.total_bytes, 40_000);
    assert!(graphics.image_timestamps.contains_key(&GraphicId::new(1)));

    // Untrack the graphic
    graphics.untrack_graphic(GraphicId::new(1), bytes);
    assert_eq!(graphics.total_bytes, 0);
    assert!(!graphics.image_timestamps.contains_key(&GraphicId::new(1)));
}

#[test]
fn test_graphics_eviction_unused_first() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics {
        total_limit: 100_000, // 100KB limit for testing
        ..Graphics::default()
    };

    // Add 3 graphics (50KB each = 150KB total, will exceed limit)
    let mut used_ids = std::collections::HashSet::new();

    // Graphic 1: 50KB, used
    let pixels1 = vec![255u8; 50_000];
    let graphic1 = GraphicData {
        id: GraphicId::new(1),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels1.clone(),
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.pending.push(graphic1);
    graphics.track_graphic(GraphicId::new(1), pixels1.len());
    used_ids.insert(1); // Mark as used

    std::thread::sleep(std::time::Duration::from_millis(10));

    // Graphic 2: 50KB, unused (should be evicted first)
    let pixels2 = vec![255u8; 50_000];
    let graphic2 = GraphicData {
        id: GraphicId::new(2),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels2.clone(),
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.pending.push(graphic2);
    graphics.track_graphic(GraphicId::new(2), pixels2.len());
    // Not marked as used

    // Try to add Graphic 3 (will trigger eviction)
    let pixels3_len = 50_000;
    let success = graphics.evict_images(pixels3_len, &used_ids);

    assert!(success, "Eviction should succeed");
    // Graphic 2 (unused) should be evicted, Graphic 1 (used) should remain
    assert_eq!(graphics.pending.len(), 1);
    assert_eq!(graphics.pending[0].id, GraphicId::new(1));
    assert!(graphics.image_timestamps.contains_key(&GraphicId::new(1)));
    assert!(!graphics.image_timestamps.contains_key(&GraphicId::new(2)));
}

#[test]
fn test_graphics_eviction_oldest_first() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics {
        total_limit: 100_000, // 100KB limit
        ..Graphics::default()
    };

    let used_ids = std::collections::HashSet::new(); // No images used

    // Add 3 graphics, all unused
    // Graphic 1: oldest
    let pixels1 = vec![255u8; 50_000];
    let graphic1 = GraphicData {
        id: GraphicId::new(1),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels1.clone(),
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.pending.push(graphic1);
    graphics.track_graphic(GraphicId::new(1), pixels1.len());

    std::thread::sleep(std::time::Duration::from_millis(10));

    // Graphic 2: middle
    let pixels2 = vec![255u8; 50_000];
    let graphic2 = GraphicData {
        id: GraphicId::new(2),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels2.clone(),
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.pending.push(graphic2);
    graphics.track_graphic(GraphicId::new(2), pixels2.len());

    // Try to add Graphic 3 (will trigger eviction, oldest should go first)
    let pixels3_len = 50_000;
    let success = graphics.evict_images(pixels3_len, &used_ids);

    assert!(success);
    // Graphic 1 (oldest) should be evicted
    assert_eq!(graphics.pending.len(), 1);
    assert_eq!(graphics.pending[0].id, GraphicId::new(2));
}

#[test]
fn test_graphics_eviction_fails_when_not_enough_space() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics {
        total_limit: 100_000, // 100KB limit
        ..Graphics::default()
    };

    let mut used_ids = std::collections::HashSet::new();

    // Add one 90KB graphic that's in use
    let pixels1 = vec![255u8; 90_000];
    let graphic1 = GraphicData {
        id: GraphicId::new(1),
        width: 150,
        height: 150,
        color_type: ColorType::Rgba,
        pixels: pixels1.clone(),
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.pending.push(graphic1);
    graphics.track_graphic(GraphicId::new(1), pixels1.len());
    used_ids.insert(1); // Mark as used

    // Try to add another 90KB (total would be 180KB, exceeds limit).
    // A used pending graphic must NOT be evicted: its cells reference
    // pixels that haven't reached the renderer yet, so eviction would
    // blank them permanently. The byte budget is soft here.
    let pixels2_len = 90_000;
    let success = graphics.evict_images(pixels2_len, &used_ids);

    assert!(
        !success,
        "no evictable candidates: the pending image is in use"
    );
    assert_eq!(graphics.pending.len(), 1, "used pending image survives");
}

#[test]
fn test_graphics_no_eviction_when_under_limit() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics {
        total_limit: 200_000, // 200KB limit
        ..Graphics::default()
    };

    let used_ids = std::collections::HashSet::new();

    // Add one 50KB graphic
    let pixels1 = vec![255u8; 50_000];
    let graphic1 = GraphicData {
        id: GraphicId::new(1),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels1.clone(),
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.pending.push(graphic1);
    graphics.track_graphic(GraphicId::new(1), pixels1.len());

    // Try to add another 50KB (total 100KB, well under limit)
    let pixels2_len = 50_000;
    let success = graphics.evict_images(pixels2_len, &used_ids);

    assert!(success);
    // No eviction should occur
    assert_eq!(graphics.pending.len(), 1);
    assert_eq!(graphics.total_bytes, 50_000);
}
