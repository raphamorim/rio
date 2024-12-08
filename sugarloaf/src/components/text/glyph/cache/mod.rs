// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

mod geometry;

pub use geometry::Rectangle;

use ::ab_glyph::*;
use linked_hash_map::LinkedHashMap;
use rustc_hash::{FxHashMap, FxHasher};
use std::{
    collections::{HashMap, HashSet},
    error, fmt,
    hash::BuildHasherDefault,
    ops,
};

/// (Texture coordinates, pixel coordinates)
pub type TextureCoords = (Rect, Rect);

type FxBuildHasher = BuildHasherDefault<FxHasher>;

/// Indicates where a glyph texture is stored in the cache
/// (row position, glyph index in row)
type TextureRowGlyphIndex = (u32, u32);

/// Texture lookup key that uses scale & offset as integers attained
/// by dividing by the relevant tolerance.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct LossyGlyphInfo {
    font_id: usize,
    glyph_id: GlyphId,
    /// x & y scales divided by `scale_tolerance` & rounded
    scale_over_tolerance: (u32, u32),
    /// Normalised subpixel positions divided by `position_tolerance` & rounded
    ///
    /// `u16` is enough as subpixel position `[-0.5, 0.5]` converted to `[0, 1]`
    ///  divided by the min `position_tolerance` (`0.001`) is small.
    offset_over_tolerance: (u16, u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ByteArray2d {
    inner_array: Vec<u8>,
    row: usize,
    col: usize,
}

impl ByteArray2d {
    #[inline]
    pub fn zeros(row: usize, col: usize) -> Self {
        ByteArray2d {
            inner_array: vec![0; row * col],
            row,
            col,
        }
    }

    #[inline]
    fn as_slice(&self) -> &[u8] {
        self.inner_array.as_slice()
    }

    #[inline]
    fn get_vec_index(&self, row: usize, col: usize) -> usize {
        debug_assert!(
            row < self.row,
            "row out of range: row={}, given={}",
            self.row,
            row
        );
        debug_assert!(
            col < self.col,
            "column out of range: col={}, given={}",
            self.col,
            col
        );
        row * self.col + col
    }
}

impl ops::Index<(usize, usize)> for ByteArray2d {
    type Output = u8;

    #[inline]
    fn index(&self, (row, col): (usize, usize)) -> &u8 {
        &self.inner_array[self.get_vec_index(row, col)]
    }
}

impl ops::IndexMut<(usize, usize)> for ByteArray2d {
    #[inline]
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut u8 {
        let vec_index = self.get_vec_index(row, col);
        &mut self.inner_array[vec_index]
    }
}

/// Row of pixel data
struct Row {
    /// Row pixel height
    height: u32,
    /// Pixel width current in use by glyphs
    width: u32,
    glyphs: Vec<GlyphTexInfo>,
}

struct GlyphTexInfo {
    glyph_info: LossyGlyphInfo,
    tex_coords: Rectangle<u32>,
    /// Used to calculate the bounds/texture pixel location for a similar glyph.
    ///
    /// Each ordinate is calculated: `(bounds_ord - positon_ord) / g.scale`
    bounds_minus_position_over_scale: Rect,
}

trait PaddingAware {
    fn unpadded(self) -> Self;
}

impl PaddingAware for Rectangle<u32> {
    /// A padded texture has 1 extra pixel on all sides
    fn unpadded(mut self) -> Self {
        self.min[0] += 1;
        self.min[1] += 1;
        self.max[0] -= 1;
        self.max[1] -= 1;
        self
    }
}

/// Builder & rebuilder for `DrawCache`.
///
/// # Example
///
/// use glyph_brush_draw_cache::DrawCache;
///
/// // Create a cache with all default values set explicitly
/// // equivalent to `DrawCache::builder().build()`
/// let default_cache = DrawCache::builder()
///     .dimensions(256, 256)
///     .scale_tolerance(0.1)
///     .position_tolerance(0.1)
///     .pad_glyphs(true)
///     .align_4x4(false)
///     .multithread(true)
///     .build();
///
/// // Create a cache with all default values, except with a dimension of 1024x1024
/// let bigger_cache = DrawCache::builder().dimensions(1024, 1024).build();
#[derive(Debug, Clone)]
pub struct DrawCacheBuilder {
    dimensions: (u32, u32),
    scale_tolerance: f32,
    position_tolerance: f32,
    pad_glyphs: bool,
    align_4x4: bool,
    multithread: bool,
}

impl Default for DrawCacheBuilder {
    fn default() -> Self {
        Self {
            dimensions: (256, 256),
            scale_tolerance: 0.1,
            position_tolerance: 0.1,
            pad_glyphs: true,
            align_4x4: false,
            multithread: true,
        }
    }
}

impl DrawCacheBuilder {
    /// `width` & `height` dimensions of the 2D texture that will hold the
    /// cache contents on the GPU.
    ///
    /// This must match the dimensions of the actual texture used, otherwise
    /// `cache_queued` will try to cache into coordinates outside the bounds of
    /// the texture.
    ///
    /// # Example (set to default value)
    ///
    /// # use glyph_brush_draw_cache::DrawCache;
    /// let cache = DrawCache::builder().dimensions(256, 256).build();
    pub fn dimensions(mut self, width: u32, height: u32) -> Self {
        self.dimensions = (width, height);
        self
    }

    /// Specifies the tolerances (maximum allowed difference) for judging
    /// whether an existing glyph in the cache is close enough to the
    /// requested glyph in scale to be used in its place. Due to floating
    /// point inaccuracies a min value of `0.001` is enforced.
    ///
    /// Both `scale_tolerance` and `position_tolerance` are measured in pixels.
    ///
    /// Tolerances produce even steps for scale and subpixel position. Only a
    /// single glyph texture will be used within a single step. For example,
    /// `scale_tolerance = 0.1` will have a step `9.95-10.05` so similar glyphs
    /// with scale `9.98` & `10.04` will match.
    ///
    /// A typical application will produce results with no perceptible
    /// inaccuracies with `scale_tolerance` and `position_tolerance` set to
    /// 0.1. Depending on the target DPI higher tolerance may be acceptable.
    ///
    /// # Example (set to default value)
    ///
    /// # use glyph_brush_draw_cache::DrawCache;
    /// let cache = DrawCache::builder().scale_tolerance(0.1).build();
    pub fn scale_tolerance<V: Into<f32>>(mut self, scale_tolerance: V) -> Self {
        self.scale_tolerance = scale_tolerance.into();
        self
    }
    /// Specifies the tolerances (maximum allowed difference) for judging
    /// whether an existing glyph in the cache is close enough to the requested
    /// glyph in subpixel offset to be used in its place. Due to floating
    /// point inaccuracies a min value of `0.001` is enforced.
    ///
    /// Both `scale_tolerance` and `position_tolerance` are measured in pixels.
    ///
    /// Tolerances produce even steps for scale and subpixel position. Only a
    /// single glyph texture will be used within a single step. For example,
    /// `scale_tolerance = 0.1` will have a step `9.95-10.05` so similar glyphs
    /// with scale `9.98` & `10.04` will match.
    ///
    /// Note that since `position_tolerance` is a tolerance of subpixel
    /// offsets, setting it to 1.0 or higher is effectively a "don't care"
    /// option.
    ///
    /// A typical application will produce results with no perceptible
    /// inaccuracies with `scale_tolerance` and `position_tolerance` set to
    /// 0.1. Depending on the target DPI higher tolerance may be acceptable.
    ///
    /// # Example (set to default value)
    ///
    /// # use glyph_brush_draw_cache::DrawCache;
    /// let cache = DrawCache::builder().position_tolerance(0.1).build();
    pub fn position_tolerance<V: Into<f32>>(mut self, position_tolerance: V) -> Self {
        self.position_tolerance = position_tolerance.into();
        self
    }
    /// Pack glyphs in texture with a padding of a single zero alpha pixel to
    /// avoid bleeding from interpolated shader texture lookups near edges.
    ///
    /// If glyphs are never transformed this may be set to `false` to slightly
    /// improve the glyph packing.
    ///
    /// # Example (set to default value)
    ///
    /// # use glyph_brush_draw_cache::DrawCache;
    /// let cache = DrawCache::builder().pad_glyphs(true).build();
    pub fn pad_glyphs(mut self, pad_glyphs: bool) -> Self {
        self.pad_glyphs = pad_glyphs;
        self
    }
    /// Align glyphs in texture to 4x4 texel boundaries.
    ///
    /// If your backend requires texture updates to be aligned to 4x4 texel
    /// boundaries (e.g. WebGL), this should be set to `true`.
    ///
    /// # Example (set to default value)
    ///
    /// # use glyph_brush_draw_cache::DrawCache;
    /// let cache = DrawCache::builder().align_4x4(false).build();
    pub fn align_4x4(mut self, align_4x4: bool) -> Self {
        self.align_4x4 = align_4x4;
        self
    }
    /// When multiple CPU cores are available spread rasterization work across
    /// all cores.
    ///
    /// Significantly reduces worst case latency in multicore environments.
    ///
    /// # Platform-specific behaviour
    ///
    /// This option has no effect on wasm32.
    ///
    /// # Example (set to default value)
    ///
    /// # use glyph_brush_draw_cache::DrawCache;
    /// let cache = DrawCache::builder().multithread(true).build();
    pub fn multithread(mut self, multithread: bool) -> Self {
        self.multithread = multithread;
        self
    }

    fn validated(self) -> Self {
        assert!(self.scale_tolerance >= 0.0);
        assert!(self.position_tolerance >= 0.0);
        let scale_tolerance = self.scale_tolerance.max(0.001);
        let position_tolerance = self.position_tolerance.max(0.001);
        #[cfg(not(target_arch = "wasm32"))]
        let multithread = self.multithread && rayon::current_num_threads() > 1;
        Self {
            scale_tolerance,
            position_tolerance,
            #[cfg(not(target_arch = "wasm32"))]
            multithread,
            ..self
        }
    }

    /// Constructs a new cache. Note that this is just the CPU side of the
    /// cache. The GPU texture is managed by the user.
    ///
    /// # Panics
    ///
    /// `scale_tolerance` or `position_tolerance` are less than or equal to
    /// zero.
    ///
    /// # Example
    ///
    /// # use glyph_brush_draw_cache::DrawCache;
    /// let cache = DrawCache::builder().build();
    pub fn build(self) -> DrawCache {
        let DrawCacheBuilder {
            dimensions: (width, height),
            scale_tolerance,
            position_tolerance,
            pad_glyphs,
            align_4x4,
            multithread,
        } = self.validated();

        DrawCache {
            scale_tolerance,
            position_tolerance,
            width,
            height,
            rows: LinkedHashMap::default(),
            space_start_for_end: {
                let mut m = HashMap::default();
                m.insert(height, 0);
                m
            },
            space_end_for_start: {
                let mut m = HashMap::default();
                m.insert(0, height);
                m
            },
            queue: Vec::new(),
            all_glyphs: HashMap::default(),
            pad_glyphs,
            align_4x4,
            multithread,
        }
    }

    /// Rebuilds a `DrawCache` with new attributes. All cached glyphs are cleared,
    /// however the glyph queue is retained unmodified.
    ///
    /// # Panics
    ///
    /// `scale_tolerance` or `position_tolerance` are less than or equal to
    /// zero.
    ///
    /// # Example
    ///
    /// # use glyph_brush_draw_cache::DrawCache;
    /// # let mut cache = DrawCache::builder().build();
    /// // Rebuild the cache with different dimensions
    /// cache.to_builder().dimensions(768, 768).rebuild(&mut cache);
    pub fn rebuild(self, cache: &mut DrawCache) {
        let DrawCacheBuilder {
            dimensions: (width, height),
            scale_tolerance,
            position_tolerance,
            pad_glyphs,
            align_4x4,
            multithread,
        } = self.validated();

        cache.width = width;
        cache.height = height;
        cache.scale_tolerance = scale_tolerance;
        cache.position_tolerance = position_tolerance;
        cache.pad_glyphs = pad_glyphs;
        cache.align_4x4 = align_4x4;
        cache.multithread = multithread;
        cache.clear();
    }
}

/// Returned from `DrawCache::cache_queued`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CacheWriteErr {
    /// At least one of the queued glyphs is too big to fit into the cache, even
    /// if all other glyphs are removed.
    GlyphTooLarge,
    /// Not all of the requested glyphs can fit into the cache, even if the
    /// cache is completely cleared before the attempt.
    NoRoomForWholeQueue,
}

impl fmt::Display for CacheWriteErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CacheWriteErr::GlyphTooLarge => "Glyph too large",
            CacheWriteErr::NoRoomForWholeQueue => "No room for whole queue",
        }
        .fmt(f)
    }
}

impl error::Error for CacheWriteErr {}

/// Successful method of caching of the queue.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CachedBy {
    /// Added any additional glyphs into the texture without affecting
    /// the position of any already cached glyphs in the latest queue.
    ///
    /// Glyphs not in the latest queue may have been removed.
    Adding,
    /// Fit the glyph queue by re-ordering all glyph texture positions.
    /// Previous texture positions are no longer valid.
    Reordering,
}

fn normalised_offset_from_position(position: Point) -> Point {
    let mut offset = point(position.x.fract(), position.y.fract());
    if offset.x > 0.5 {
        offset.x -= 1.0;
    } else if offset.x < -0.5 {
        offset.x += 1.0;
    }
    if offset.y > 0.5 {
        offset.y -= 1.0;
    } else if offset.y < -0.5 {
        offset.y += 1.0;
    }
    offset
}

/// Dynamic rasterization draw cache.
pub struct DrawCache {
    scale_tolerance: f32,
    position_tolerance: f32,
    width: u32,
    height: u32,
    rows: LinkedHashMap<u32, Row, FxBuildHasher>,
    /// Mapping of row gaps bottom -> top
    space_start_for_end: FxHashMap<u32, u32>,
    /// Mapping of row gaps top -> bottom
    space_end_for_start: FxHashMap<u32, u32>,
    queue: Vec<(usize, Glyph)>,
    all_glyphs: FxHashMap<LossyGlyphInfo, TextureRowGlyphIndex>,
    pad_glyphs: bool,
    align_4x4: bool,
    multithread: bool,
}

impl DrawCache {
    /// Returns a default `DrawCacheBuilder`.
    #[inline]
    pub fn builder() -> DrawCacheBuilder {
        DrawCacheBuilder::default()
    }

    /// Returns the current scale tolerance for the cache.
    pub fn scale_tolerance(&self) -> f32 {
        self.scale_tolerance
    }

    /// Returns the current subpixel position tolerance for the cache.
    pub fn position_tolerance(&self) -> f32 {
        self.position_tolerance
    }

    /// Returns the cache texture dimensions assumed by the cache. For proper
    /// operation this should match the dimensions of the used GPU texture.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Queue a glyph for caching by the next call to `cache_queued`. `font_id`
    /// is used to disambiguate glyphs from different fonts. The user should
    /// ensure that `font_id` is unique to the font the glyph is from.
    pub fn queue_glyph(&mut self, font_id: usize, glyph: Glyph) {
        self.queue.push((font_id, glyph));
    }

    /// Clears the cache. Does not affect the glyph queue.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.space_end_for_start.clear();
        self.space_end_for_start.insert(0, self.height);
        self.space_start_for_end.clear();
        self.space_start_for_end.insert(self.height, 0);
        self.all_glyphs.clear();
    }

    /// Clears the glyph queue.
    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }

    /// Returns a `DrawCacheBuilder` with this cache's attributes.
    pub fn to_builder(&self) -> DrawCacheBuilder {
        DrawCacheBuilder {
            dimensions: (self.width, self.height),
            position_tolerance: self.position_tolerance,
            scale_tolerance: self.scale_tolerance,
            pad_glyphs: self.pad_glyphs,
            align_4x4: self.align_4x4,
            multithread: self.multithread,
        }
    }

    /// Returns glyph info with accuracy according to the set tolerances.
    fn lossy_info_for(&self, font_id: usize, glyph: &Glyph) -> LossyGlyphInfo {
        let scale = glyph.scale;
        let offset = normalised_offset_from_position(glyph.position);

        LossyGlyphInfo {
            font_id,
            glyph_id: glyph.id,
            scale_over_tolerance: (
                (scale.x / self.scale_tolerance + 0.5) as u32,
                (scale.y / self.scale_tolerance + 0.5) as u32,
            ),
            // convert [-0.5, 0.5] -> [0, 1] then divide
            offset_over_tolerance: (
                ((offset.x + 0.5) / self.position_tolerance + 0.5) as u16,
                ((offset.y + 0.5) / self.position_tolerance + 0.5) as u16,
            ),
        }
    }

    /// Caches the queued glyphs. If this is unsuccessful, the queue is
    /// untouched. Any glyphs cached by previous calls to this function may be
    /// removed from the cache to make room for the newly queued glyphs. Thus if
    /// you want to ensure that a glyph is in the cache, the most recently
    /// cached queue must have contained that glyph.
    ///
    /// `uploader` is the user-provided function that should perform the texture
    /// uploads to the GPU. The information provided is the rectangular region
    /// to insert the pixel data into, and the pixel data itself. This data is
    /// provided in horizontal scanline format (row major), with stride equal to
    /// the rectangle width.
    ///
    /// If successful returns a `CachedBy` that can indicate the validity of
    /// previously cached glyph textures.
    pub fn cache_queued<F, U>(
        &mut self,
        fonts: &[F],
        mut uploader: U,
    ) -> Result<CachedBy, CacheWriteErr>
    where
        F: Font + Sync,
        U: FnMut(Rectangle<u32>, &[u8]),
    {
        let mut queue_success = true;
        let from_empty = self.all_glyphs.is_empty();

        {
            let (mut in_use_rows, uncached_glyphs) = {
                let mut in_use_rows = HashSet::with_capacity_and_hasher(
                    self.rows.len(),
                    FxBuildHasher::default(),
                );
                let mut uncached_glyphs = HashMap::with_capacity_and_hasher(
                    self.queue.len(),
                    BuildHasherDefault::<FxHasher>::default(),
                );

                // divide glyphs into texture rows where a matching glyph texture
                // already exists & glyphs where new textures must be cached
                for (font_id, ref glyph) in &self.queue {
                    let glyph_info = self.lossy_info_for(*font_id, glyph);
                    if let Some((row, ..)) = self.all_glyphs.get(&glyph_info) {
                        in_use_rows.insert(*row);
                    } else {
                        uncached_glyphs.insert(glyph_info, glyph);
                    }
                }

                (in_use_rows, uncached_glyphs)
            };

            for row in &in_use_rows {
                self.rows.get_refresh(row);
            }

            // outline
            let mut uncached_outlined: Vec<_> = uncached_glyphs
                .into_iter()
                .filter_map(|(info, glyph)| {
                    Some((info, fonts[info.font_id].outline_glyph(glyph.clone())?))
                })
                .collect();

            // tallest first gives better packing
            // can use 'sort_unstable' as order of equal elements is unimportant
            uncached_outlined.sort_unstable_by(|(_, ga), (_, gb)| {
                gb.px_bounds()
                    .height()
                    .partial_cmp(&ga.px_bounds().height())
                    .unwrap_or(core::cmp::Ordering::Equal)
            });

            self.all_glyphs.reserve(uncached_outlined.len());
            let mut draw_and_upload = Vec::with_capacity(uncached_outlined.len());

            'per_glyph: for (glyph_info, outlined) in uncached_outlined {
                let bounds = outlined.px_bounds();

                let (unaligned_width, unaligned_height) = {
                    if self.pad_glyphs {
                        (bounds.width() as u32 + 2, bounds.height() as u32 + 2)
                    } else {
                        (bounds.width() as u32, bounds.height() as u32)
                    }
                };
                let (aligned_width, aligned_height) = if self.align_4x4 {
                    // align to the next 4x4 texel boundary
                    ((unaligned_width + 3) & !3, (unaligned_height + 3) & !3)
                } else {
                    (unaligned_width, unaligned_height)
                };
                if aligned_width >= self.width || aligned_height >= self.height {
                    return Result::Err(CacheWriteErr::GlyphTooLarge);
                }
                // find row to put the glyph in, most used rows first
                let mut row_top = None;
                for (top, row) in self.rows.iter().rev() {
                    if row.height >= aligned_height
                        && self.width - row.width >= aligned_width
                    {
                        // found a spot on an existing row
                        row_top = Some(*top);
                        break;
                    }
                }

                if row_top.is_none() {
                    let mut gap = None;
                    // See if there is space for a new row
                    for (start, end) in &self.space_end_for_start {
                        if end - start >= aligned_height {
                            gap = Some((*start, *end));
                            break;
                        }
                    }
                    if gap.is_none() {
                        // Remove old rows until room is available
                        while !self.rows.is_empty() {
                            // check that the oldest row isn't also in use
                            if !in_use_rows.contains(self.rows.front().unwrap().0) {
                                // Remove row
                                let (top, row) = self.rows.pop_front().unwrap();

                                for g in row.glyphs {
                                    self.all_glyphs.remove(&g.glyph_info);
                                }

                                let (mut new_start, mut new_end) =
                                    (top, top + row.height);
                                // Update the free space maps
                                // Combine with neighbouring free space if possible
                                if let Some(end) =
                                    self.space_end_for_start.remove(&new_end)
                                {
                                    new_end = end;
                                }
                                if let Some(start) =
                                    self.space_start_for_end.remove(&new_start)
                                {
                                    new_start = start;
                                }
                                self.space_start_for_end.insert(new_end, new_start);
                                self.space_end_for_start.insert(new_start, new_end);
                                if new_end - new_start >= aligned_height {
                                    // The newly formed gap is big enough
                                    gap = Some((new_start, new_end));
                                    break;
                                }
                            }
                            // all rows left are in use
                            // try a clean insert of all needed glyphs
                            // if that doesn't work, fail
                            else if from_empty {
                                // already trying a clean insert, don't do it again
                                return Err(CacheWriteErr::NoRoomForWholeQueue);
                            } else {
                                // signal that a retry is needed
                                queue_success = false;
                                break 'per_glyph;
                            }
                        }
                    }
                    let (gap_start, gap_end) = gap.unwrap();
                    // fill space for new row
                    let new_space_start = gap_start + aligned_height;
                    self.space_end_for_start.remove(&gap_start);
                    if new_space_start == gap_end {
                        self.space_start_for_end.remove(&gap_end);
                    } else {
                        self.space_end_for_start.insert(new_space_start, gap_end);
                        self.space_start_for_end.insert(gap_end, new_space_start);
                    }
                    // add the row
                    self.rows.insert(
                        gap_start,
                        Row {
                            width: 0,
                            height: aligned_height,
                            glyphs: Vec::new(),
                        },
                    );
                    row_top = Some(gap_start);
                }
                let row_top = row_top.unwrap();
                // calculate the target rect
                let row = self.rows.get_refresh(&row_top).unwrap();
                let aligned_tex_coords = Rectangle {
                    min: [row.width, row_top],
                    max: [row.width + aligned_width, row_top + aligned_height],
                };
                let unaligned_tex_coords = Rectangle {
                    min: [row.width, row_top],
                    max: [row.width + unaligned_width, row_top + unaligned_height],
                };

                let g = outlined.glyph();

                // add the glyph to the row
                row.glyphs.push(GlyphTexInfo {
                    glyph_info,
                    tex_coords: unaligned_tex_coords,
                    bounds_minus_position_over_scale: Rect {
                        min: point(
                            (bounds.min.x - g.position.x) / g.scale.x,
                            (bounds.min.y - g.position.y) / g.scale.y,
                        ),
                        max: point(
                            (bounds.max.x - g.position.x) / g.scale.x,
                            (bounds.max.y - g.position.y) / g.scale.y,
                        ),
                    },
                });
                row.width += aligned_width;
                in_use_rows.insert(row_top);

                draw_and_upload.push((aligned_tex_coords, outlined));

                self.all_glyphs
                    .insert(glyph_info, (row_top, row.glyphs.len() as u32 - 1));
            }

            // draw & upload
            if queue_success {
                if from_empty && draw_and_upload.len() > 1 {
                    // if previously empty draw into memory and perform a single upload
                    let max_v = draw_and_upload
                        .iter()
                        .map(|rect| rect.0.max[1])
                        .max()
                        .unwrap();
                    let mut texture_up = vec![0; (self.width * max_v) as _];

                    self.draw_and_upload(draw_and_upload, &mut |rect, data| {
                        let min_h = rect.min[0] as usize;
                        let min_v = rect.min[1];
                        let glyph_w = rect.width() as usize;

                        for v in min_v..rect.max[1] {
                            let tex_left = min_h + (self.width * v) as usize;
                            let data_left = glyph_w * (v - min_v) as usize;
                            texture_up.splice(
                                tex_left..tex_left + glyph_w,
                                data[data_left..data_left + glyph_w].iter().copied(),
                            );
                        }
                    });

                    uploader(
                        Rectangle {
                            min: [0, 0],
                            max: [self.width, max_v],
                        },
                        &texture_up,
                    );
                } else {
                    self.draw_and_upload(draw_and_upload, &mut uploader);
                }
            }
        }

        if queue_success {
            self.queue.clear();
            Ok(CachedBy::Adding)
        } else {
            // clear the cache then try again with optimal packing
            self.clear();
            self.cache_queued(fonts, uploader)
                .map(|_| CachedBy::Reordering)
        }
    }

    /// Draw using the current thread & the rayon thread pool in a work-stealing manner.
    /// Uploads are called by the current thread only.
    ///
    /// Note: This fn uses non-wasm multithreading dependencies.
    #[cfg(not(target_arch = "wasm32"))]
    fn draw_and_upload<U>(
        &self,
        draw_and_upload: Vec<(Rectangle<u32>, OutlinedGlyph)>,
        uploader: &mut U,
    ) where
        U: FnMut(Rectangle<u32>, &[u8]),
    {
        use std::sync::Arc;

        let glyph_count = draw_and_upload.len();

        // Magnituge of work where we think it's worth using multithreaded-drawing.
        // Calculated from benchmarks comparing with single-threaded performance.
        const WORK_MAGNITUDE_FOR_MT: usize = 271002;
        // The first (tallest) glyph height is used to calculate work magnitude.
        let work_magnitude = {
            let tallest_h = draw_and_upload
                .first()
                .map(|(r, _)| r.height() as usize)
                .unwrap_or(0);
            glyph_count
                .saturating_mul(tallest_h)
                .saturating_mul(tallest_h)
        };

        if self.multithread && glyph_count > 1 && work_magnitude >= WORK_MAGNITUDE_FOR_MT
        {
            // multithread rasterization
            use crossbeam_channel::TryRecvError;
            use crossbeam_deque::Worker;
            use std::mem;

            let threads = rayon::current_num_threads().min(glyph_count);
            let rasterize_queue = Arc::new(crossbeam_deque::Injector::new());
            let (to_main, from_stealers) = crossbeam_channel::unbounded();
            let pad_glyphs = self.pad_glyphs;

            let mut worker_qs: Vec<_> =
                (0..threads).map(|_| Worker::new_fifo()).collect();
            let stealers: Arc<Vec<_>> =
                Arc::new(worker_qs.iter().map(|w| w.stealer()).collect());

            for el in draw_and_upload {
                rasterize_queue.push(el);
            }

            for _ in 0..threads.saturating_sub(1) {
                let rasterize_queue = Arc::clone(&rasterize_queue);
                let stealers = Arc::clone(&stealers);
                let to_main = to_main.clone();
                let local = worker_qs.pop().unwrap();

                rayon::spawn(move || loop {
                    let task = local.pop().or_else(|| {
                        std::iter::repeat_with(|| {
                            rasterize_queue
                                .steal_batch_and_pop(&local)
                                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
                        })
                        .find(|s| !s.is_retry())
                        .and_then(|s| s.success())
                    });

                    match task {
                        Some((tex_coords, glyph)) => {
                            let pixels = draw_glyph(tex_coords, &glyph, pad_glyphs);
                            to_main.send((tex_coords, pixels)).unwrap();
                        }
                        None => break,
                    }
                });
            }
            mem::drop(to_main);

            let local = worker_qs.pop().unwrap();
            let mut workers_finished = false;
            loop {
                let task = local.pop().or_else(|| {
                    std::iter::repeat_with(|| {
                        rasterize_queue
                            .steal_batch_and_pop(&local)
                            .or_else(|| stealers.iter().map(|s| s.steal()).collect())
                    })
                    .find(|s| !s.is_retry())
                    .and_then(|s| s.success())
                });

                match task {
                    Some((tex_coords, glyph)) => {
                        let pixels = draw_glyph(tex_coords, &glyph, pad_glyphs);
                        uploader(tex_coords, pixels.as_slice());
                    }
                    None if workers_finished => break,
                    None => {}
                }

                while !workers_finished {
                    match from_stealers.try_recv() {
                        Ok((tex_coords, pixels)) => {
                            uploader(tex_coords, pixels.as_slice())
                        }
                        Err(TryRecvError::Disconnected) => workers_finished = true,
                        Err(TryRecvError::Empty) => break,
                    }
                }
            }
        } else {
            self.draw_and_upload_1_thread(draw_and_upload, uploader);
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[inline]
    fn draw_and_upload<U>(
        &self,
        draw_and_upload: Vec<(Rectangle<u32>, OutlinedGlyph)>,
        uploader: &mut U,
    ) where
        U: FnMut(Rectangle<u32>, &[u8]),
    {
        self.draw_and_upload_1_thread(draw_and_upload, uploader)
    }

    /// Draw & upload seqentially.
    #[inline]
    fn draw_and_upload_1_thread<U>(
        &self,
        draw_and_upload: Vec<(Rectangle<u32>, OutlinedGlyph)>,
        uploader: &mut U,
    ) where
        U: FnMut(Rectangle<u32>, &[u8]),
    {
        for (tex_coords, outlined) in draw_and_upload {
            let pixels = draw_glyph(tex_coords, &outlined, self.pad_glyphs);
            uploader(tex_coords, pixels.as_slice());
        }
    }

    /// Retrieves the (floating point) texture coordinates of the quad for a
    /// glyph in the cache, as well as the pixel-space (integer) coordinates
    /// that this region should be drawn at. These pixel-space coordinates
    /// assume an origin at the top left of the quad. In the majority of cases
    /// these pixel-space coordinates should be identical to the bounding box of
    /// the input glyph. They only differ if the cache has returned a substitute
    /// glyph that is deemed close enough to the requested glyph as specified by
    /// the cache tolerance parameters.
    ///
    /// A successful result is `Some` if the glyph is not an empty glyph (no
    /// shape, and thus no rect to return).
    ///
    /// Ensure that `font_id` matches the `font_id` that was passed to
    /// `queue_glyph` with this `glyph`.
    pub fn rect_for(&self, font_id: usize, glyph: &Glyph) -> Option<TextureCoords> {
        let (row, index) = self.all_glyphs.get(&self.lossy_info_for(font_id, glyph))?;

        let (tex_width, tex_height) = (self.width as f32, self.height as f32);

        let GlyphTexInfo {
            tex_coords: mut tex_rect,
            bounds_minus_position_over_scale,
            ..
        } = self.rows[row].glyphs[*index as usize];
        if self.pad_glyphs {
            tex_rect = tex_rect.unpadded();
        }
        let uv_rect = Rect {
            min: point(
                tex_rect.min[0] as f32 / tex_width,
                tex_rect.min[1] as f32 / tex_height,
            ),
            max: point(
                tex_rect.max[0] as f32 / tex_width,
                tex_rect.max[1] as f32 / tex_height,
            ),
        };

        let equivalent_bounds = Rect {
            min: point(
                bounds_minus_position_over_scale.min.x * glyph.scale.x,
                bounds_minus_position_over_scale.min.y * glyph.scale.y,
            ) + glyph.position,
            max: point(
                bounds_minus_position_over_scale.max.x * glyph.scale.x,
                bounds_minus_position_over_scale.max.y * glyph.scale.y,
            ) + glyph.position,
        };

        Some((uv_rect, equivalent_bounds))
    }
}

#[inline]
fn draw_glyph(
    tex_coords: Rectangle<u32>,
    glyph: &OutlinedGlyph,
    pad_glyphs: bool,
) -> ByteArray2d {
    let mut pixels =
        ByteArray2d::zeros(tex_coords.height() as usize, tex_coords.width() as usize);
    if pad_glyphs {
        glyph.draw(|x, y, v| {
            // `+ 1` accounts for top/left glyph padding
            pixels[(y as usize + 1, x as usize + 1)] = (v * 255.0) as u8;
        });
    } else {
        glyph.draw(|x, y, v| {
            pixels[(y as usize, x as usize)] = (v * 255.0) as u8;
        });
    }
    pixels
}

#[cfg(test)]
mod test {
    use crate::components::text::glyph::cache::*;
    use crate::components::text::glyph::layout::*;
    use approx::*;

    const FONT: &[u8] =
        include_bytes!("../../../../../resources/test-fonts/WenQuanYiMicroHei.ttf");

    #[test]
    fn cache_test() {
        let font = FontRef::try_from_slice(FONT).unwrap();

        let mut cache = DrawCache::builder()
            .dimensions(32, 32)
            .scale_tolerance(0.1)
            .position_tolerance(0.1)
            .pad_glyphs(false)
            .build();
        let strings = [
            ("Hello World!", 15.0),
            ("Hello World!", 14.0),
            ("Hello World!", 10.0),
            ("Hello World!", 15.0),
            ("Hello World!", 14.0),
            ("Hello World!", 10.0),
        ];
        for &(text, scale) in &strings {
            println!("Caching {:?}", (text, scale));

            let glyphs = crate::components::text::glyph::Layout::default_single_line()
                .calculate_glyphs(
                    &[&font],
                    &SectionGeometry::default(),
                    &[SectionText {
                        text,
                        scale: scale.into(),
                        ..<_>::default()
                    }],
                );

            for SectionGlyph { glyph, .. } in glyphs {
                cache.queue_glyph(0, glyph);
            }
            cache.cache_queued(&[&font], |_, _| {}).unwrap();
        }
    }

    #[test]
    fn need_to_check_whole_cache() {
        let font = FontRef::try_from_slice(FONT).unwrap();

        let gid = font.glyph_id('l');
        let small_left = gid.with_scale_and_position(10.0, point(0.0, 0.0));
        let large_left = gid.with_scale_and_position(10.05, point(0.0, 0.0));
        let large_right = gid.with_scale_and_position(10.05, point(-0.2, 0.0));

        let mut cache = DrawCache::builder()
            .dimensions(32, 32)
            .scale_tolerance(0.1)
            .position_tolerance(0.1)
            .pad_glyphs(false)
            .build();

        cache.queue_glyph(0, small_left.clone());
        // Next line is noop since it's within the scale tolerance of small_left:
        cache.queue_glyph(0, large_left.clone());
        cache.queue_glyph(0, large_right.clone());

        cache.cache_queued(&[&font], |_, _| {}).unwrap();

        cache.rect_for(0, &small_left).unwrap();
        cache.rect_for(0, &large_left).unwrap();
        cache.rect_for(0, &large_right).unwrap();
    }

    #[test]
    fn lossy_info() {
        let font = FontRef::try_from_slice(FONT).unwrap();
        let gid = font.glyph_id('l');

        let cache = DrawCache::builder()
            .scale_tolerance(0.2)
            .position_tolerance(0.5)
            .build();

        let small = gid.with_scale_and_position(9.91, point(0.0, 0.0));
        let match_1 = gid.with_scale_and_position(10.09, point(-10.0, -0.1));
        let match_2 = gid.with_scale_and_position(10.09, point(5.1, 0.24));
        let match_3 = gid.with_scale_and_position(9.91, point(-100.2, 50.1));

        let miss_1 = gid.with_scale_and_position(10.11, point(0.0, 0.0));
        let miss_2 = gid.with_scale_and_position(12.0, point(0.0, 0.0));
        let miss_3 = gid.with_scale_and_position(9.91, point(0.3, 0.0));

        let small_info = cache.lossy_info_for(0, &small);

        assert_eq!(small_info, cache.lossy_info_for(0, &match_1));
        assert_eq!(small_info, cache.lossy_info_for(0, &match_2));
        assert_eq!(small_info, cache.lossy_info_for(0, &match_3));

        assert_ne!(small_info, cache.lossy_info_for(0, &miss_1));
        assert_ne!(small_info, cache.lossy_info_for(0, &miss_2));
        assert_ne!(small_info, cache.lossy_info_for(0, &miss_3));
    }

    #[test]
    fn cache_to_builder() {
        let cache = DrawCacheBuilder {
            dimensions: (32, 64),
            scale_tolerance: 0.2,
            position_tolerance: 0.3,
            pad_glyphs: false,
            align_4x4: false,
            multithread: false,
        }
        .build();

        let to_builder: DrawCacheBuilder = cache.to_builder();

        assert_eq!(to_builder.dimensions, (32, 64));
        assert_relative_eq!(to_builder.scale_tolerance, 0.2);
        assert_relative_eq!(to_builder.position_tolerance, 0.3);
        assert!(!to_builder.pad_glyphs);
        assert!(!to_builder.align_4x4);
        assert!(!to_builder.multithread);
    }

    #[test]
    fn builder_rebuild() {
        let mut cache = DrawCache::builder()
            .dimensions(32, 64)
            .scale_tolerance(0.2)
            .position_tolerance(0.3)
            .pad_glyphs(false)
            .align_4x4(true)
            .multithread(true)
            .build();

        let font = FontRef::try_from_slice(FONT).unwrap();
        cache.queue_glyph(0, font.glyph_id('l').with_scale(25.0));
        cache.cache_queued(&[&font], |_, _| {}).unwrap();

        cache.queue_glyph(0, font.glyph_id('a').with_scale(25.0));

        DrawCache::builder()
            .dimensions(64, 128)
            .scale_tolerance(0.05)
            .position_tolerance(0.15)
            .pad_glyphs(true)
            .align_4x4(false)
            .multithread(false)
            .rebuild(&mut cache);

        assert_eq!(cache.width, 64);
        assert_eq!(cache.height, 128);
        assert_relative_eq!(cache.scale_tolerance, 0.05);
        assert_relative_eq!(cache.position_tolerance, 0.15);
        assert!(cache.pad_glyphs);
        assert!(!cache.align_4x4);
        assert!(!cache.multithread);

        assert!(
            cache.all_glyphs.is_empty(),
            "cache should have been cleared"
        );

        assert_eq!(cache.queue.len(), 1, "cache should have an unchanged queue");
    }

    /// Provide to caller that the cache was re-ordered to fit the latest queue
    #[test]
    fn return_cache_by_reordering() {
        let font = FontRef::try_from_slice(FONT).unwrap();
        let fontmap = &[&font];

        let mut cache = DrawCache::builder()
            .dimensions(30, 25)
            .scale_tolerance(0.1)
            .position_tolerance(0.1)
            .build();

        let glyphs = crate::components::text::glyph::Layout::default_single_line()
            .calculate_glyphs(
                fontmap,
                &SectionGeometry::default(),
                &[SectionText {
                    text: "ABCDEF",
                    scale: 16.0.into(),
                    ..<_>::default()
                }],
            );
        for sg in glyphs {
            cache.queue_glyph(0, sg.glyph);
        }
        assert_eq!(cache.cache_queued(fontmap, |_, _| {}), Ok(CachedBy::Adding));

        let glyphs = crate::components::text::glyph::Layout::default_single_line()
            .calculate_glyphs(
                fontmap,
                &SectionGeometry::default(),
                &[SectionText {
                    text: "DEFHIK",
                    scale: 16.0.into(),
                    ..<_>::default()
                }],
            );
        for sg in glyphs {
            cache.queue_glyph(0, sg.glyph);
        }
        assert_eq!(
            cache.cache_queued(fontmap, |_, _| {}),
            Ok(CachedBy::Reordering)
        );
    }

    #[test]
    fn align_4x4() {
        // First, test align_4x4 disabled, to confirm non-4x4 alignment
        align_4x4_helper(false, 5, 19);
        // Now, test with align_4x4 enabled, to confirm 4x4 alignment
        align_4x4_helper(true, 8, 20);
    }

    fn align_4x4_helper(align_4x4: bool, expected_width: u32, expected_height: u32) {
        let mut cache = DrawCache::builder()
            .dimensions(64, 64)
            .align_4x4(align_4x4)
            .build();
        let font = FontRef::try_from_slice(FONT).unwrap();
        let glyph = font.glyph_id('l').with_scale(25.0);
        cache.queue_glyph(0, glyph.clone());
        cache
            .cache_queued(&[&font], |rect, _| {
                assert_eq!(rect.width(), expected_width);
                assert_eq!(rect.height(), expected_height);
            })
            .unwrap();
        let (uv, _screen_rect) = cache.rect_for(0, &glyph).unwrap();

        assert_relative_eq!(uv.min.x, 0.015_625);
        assert_relative_eq!(uv.min.y, 0.015_625);
        assert_relative_eq!(uv.max.x, 0.0625);
        assert_relative_eq!(uv.max.y, 0.28125);
    }
}
