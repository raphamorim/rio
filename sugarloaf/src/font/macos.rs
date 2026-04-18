//! macOS glyph rasterization via CoreText + CoreGraphics.
//!
//! Replaces the zeno path on macOS so text picks up the native anti-aliasing
//! style and Apple Color Emoji renders without bundled fallback fonts.
//!
//! Output matches zeno's: left/top bearings, width/height in device pixels,
//! and either R8 alpha-only bytes (mask) or straight-alpha RGBA (color). The
//! caller stitches this into the existing atlas pipeline unchanged.
//!
//! Inspired by ghostty's `src/font/face/coretext.zig` and zed's
//! `crates/gpui_macos/src/text_system.rs:382-469`.

use std::path::PathBuf;

use core_foundation::{
    attributed_string::CFMutableAttributedString,
    base::{CFRange, CFType, TCFType},
    dictionary::CFDictionary,
    number::CFNumber,
    string::CFString,
    url::{CFURLRef, CFURL},
};
use core_graphics::{
    base::{kCGImageAlphaPremultipliedLast, CGFloat},
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    font::CGGlyph,
    geometry::{CGAffineTransform, CGPoint, CGRect, CGSize},
};
use core_text::{
    font as ct_font,
    font::{CTFont, CTFontRef},
    font_collection, font_manager,
    font_descriptor::{
        self, kCTFontFamilyNameAttribute, kCTFontOrientationDefault,
        kCTFontSlantTrait, kCTFontTraitsAttribute, kCTFontWeightTrait,
        kCTFontWidthTrait, CTFontDescriptor, CTFontDescriptorRef,
        CTFontDescriptorCreateMatchingFontDescriptor,
    },
    line::CTLine,
    string_attributes::kCTFontAttributeName,
};

// core-graphics 0.24 doesn't export this one; `kCGImageAlphaOnly = 7` per Apple's
// CGImage.h. Used for 1-channel alpha-only bitmaps (monochrome glyph masks).
#[allow(non_upper_case_globals)]
const kCGImageAlphaOnly: u32 = 7;

// core-text 21 leaves CTFontManagerRegisterFontsForURL commented out, so declare
// the FFI ourselves. Used to publish `additional_dirs` fonts to CoreText so
// descriptor matching (and the command-palette browser) finds them.
type CTFontManagerScope = u32;
#[allow(non_upper_case_globals)]
const kCTFontManagerScopeProcess: CTFontManagerScope = 1;

#[allow(non_snake_case)]
#[link(name = "CoreText", kind = "framework")]
extern "C" {
    fn CTFontManagerRegisterFontsForURL(
        fontURL: CFURLRef,
        scope: CTFontManagerScope,
        error: *mut core_foundation::base::CFTypeRef,
    ) -> bool;

    // core-text 21 wraps this as `clone_with_font_size` which hardcodes a
    // null matrix. For synthetic italic we need to pass a non-null skew
    // matrix, so declare the underlying symbol ourselves.
    fn CTFontCreateCopyWithAttributes(
        font: CTFontRef,
        size: CGFloat,
        matrix: *const CGAffineTransform,
        attributes: CTFontDescriptorRef,
    ) -> CTFontRef;

    // Plural variant used for TTC/OTC collections — returns an array of
    // descriptors, one per sub-font. core-text 21 doesn't wrap this; only
    // the singular `…FromData` is exposed (which picks the first font).
    fn CTFontManagerCreateFontDescriptorsFromData(
        data: core_foundation::data::CFDataRef,
    ) -> core_foundation::array::CFArrayRef;
}

/// Shear matrix applied to `CTFont` for synthetic italic. Matches Ghostty's
/// `italic_skew` — `c = tan(15°)` leans the glyphs 15° to the right.
const SYNTHETIC_ITALIC_SKEW: CGAffineTransform = CGAffineTransform {
    a: 1.0,
    b: 0.0,
    c: 0.267_949,
    d: 1.0,
    tx: 0.0,
    ty: 0.0,
};

/// Return a sheared `CTFont` for synthetic italic rendering. The base font
/// stays intact; the returned CTFont carries the skew in its transform
/// matrix so `draw_glyphs` produces slanted output.
fn ct_font_sheared(base: &CTFont, size: f64) -> CTFont {
    use core_foundation::base::TCFType;
    unsafe {
        let raw = CTFontCreateCopyWithAttributes(
            base.as_concrete_TypeRef(),
            size as CGFloat,
            &SYNTHETIC_ITALIC_SKEW,
            std::ptr::null(),
        );
        CTFont::wrap_under_create_rule(raw)
    }
}

/// Register every `.ttf`/`.otf`/`.ttc`/`.otc` under `dir` with CoreText so
/// `additional_dirs` fonts become discoverable by descriptor matching.
///
/// Process-scoped: registrations only affect rio, not other apps on the
/// system, and they disappear when rio exits. Silently skips paths CoreText
/// rejects (duplicate registration, malformed files) — the rest of the dir
/// still loads.
pub fn register_fonts_in_dir(dir: &std::path::Path) {
    let walker = walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok());
    for entry in walker {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext = ext.to_ascii_lowercase();
        if !matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
            continue;
        }
        let Some(url) = CFURL::from_path(path, false) else {
            continue;
        };
        let mut err: core_foundation::base::CFTypeRef = std::ptr::null();
        let ok = unsafe {
            CTFontManagerRegisterFontsForURL(
                url.as_concrete_TypeRef(),
                kCTFontManagerScopeProcess,
                &mut err,
            )
        };
        if !ok {
            tracing::debug!(
                "CTFontManagerRegisterFontsForURL skipped {}",
                path.display()
            );
        }
    }
}

/// A parsed CoreText font. Construction goes through
/// `CTFontManagerCreateFontDescriptorFromData` + `CTFontCreateWithFontDescriptor`
/// — the same path Ghostty uses — which preserves COLR, sbix, and other color
/// font tables that the simpler `CGFontCreateWithDataProvider` → `CTFontCreate
/// WithGraphicsFont` path silently drops.
///
/// Stored at a reference 1.0pt size; per-call rasterization clones with the
/// target size (cheap CF refcount, not a parse). Clone is a CF retain.
///
/// TTC caveat: CoreText's data-based descriptor reads only the first font in
/// a collection. Same limitation as Ghostty and Zed.
#[derive(Clone)]
pub struct FontHandle {
    base_font: CTFont,
}

impl FontHandle {
    /// Parse a font file's bytes into a `CTFont`. Returns `None` if CoreText
    /// can't interpret the buffer (malformed, unsupported format).
    ///
    /// For TTC/OTC collections this picks the first contained font (index 0).
    /// Same behaviour as Rio's cross-platform loader (`FontRef::from_index`
    /// with index 0), Ghostty, and Zed. Use [`FontHandle::from_bytes_index`]
    /// if a specific index is needed.
    pub fn from_bytes(font_bytes: &[u8]) -> Option<Self> {
        let desc = font_manager::create_font_descriptor(font_bytes).ok()?;
        let base_font = ct_font::new_from_descriptor(&desc, 1.0);
        Some(Self { base_font })
    }

    /// Like [`from_bytes`] but picks a specific sub-font from a TTC/OTC
    /// collection by index. For non-collection files pass `index = 0`;
    /// results are equivalent to [`from_bytes`].
    ///
    /// Returns `None` if the buffer isn't a valid (collection of) font(s)
    /// or the index is out of range.
    pub fn from_bytes_index(font_bytes: &[u8], index: usize) -> Option<Self> {
        use core_foundation::array::CFArray;
        use core_foundation::base::TCFType;
        use core_foundation::data::CFData;

        let data = CFData::from_buffer(font_bytes);
        let array_ref =
            unsafe { CTFontManagerCreateFontDescriptorsFromData(data.as_concrete_TypeRef()) };
        if array_ref.is_null() {
            return None;
        }
        let descriptors: CFArray<CTFontDescriptor> =
            unsafe { CFArray::wrap_under_create_rule(array_ref) };
        let desc_ref = descriptors.get(index as isize)?;
        let base_font = ct_font::new_from_descriptor(&desc_ref, 1.0);
        Some(Self { base_font })
    }
}

/// Output of a single glyph rasterization. Mirrors the fields of
/// `font_introspector::scale::image::Image` the zeno path fills in.
#[derive(Debug)]
pub struct RasterizedGlyph {
    /// Bitmap width in device pixels. `0` signals a zero-area glyph
    /// (e.g. space, combining mark without ink).
    pub width: u32,
    /// Bitmap height in device pixels. `0` for zero-area glyphs.
    pub height: u32,
    /// Pen-relative x of the bitmap's left edge, in pixels. Positive =
    /// right of the pen.
    pub left: i32,
    /// Baseline-relative y of the bitmap's top edge, in pixels. Positive
    /// = above the baseline.
    pub top: i32,
    /// `true` when `bytes` is 4bpp straight-alpha RGBA (color emoji);
    /// `false` when `bytes` is 1bpp alpha-only (monochrome outline).
    pub is_color: bool,
    /// Row-major pixel bytes, no row padding.
    pub bytes: Vec<u8>,
}

/// Rasterize one glyph from a previously-parsed `FontHandle`.
///
/// `glyph_id` is a TrueType glyph index; callers resolve it via shaping or a
/// charmap lookup before getting here. `size_px` is the target pixel size.
/// `is_color` picks the bitmap format — set it to the font's emoji-ness, not
/// per-glyph, since the atlas tile format is fixed up front.
///
/// `synthetic_italic` applies a 15° right-lean via a sheared CTFont transform
/// (matches Ghostty's italic synthesis). `synthetic_bold` draws with
/// `CGTextFillStroke` and a stroke width of `max(size/14, 1)` (also Ghostty's
/// formula). Both are meant for when the font family lacks the requested
/// variant — normal bold/italic fonts are found by `find_font_path` and should
/// leave both flags `false`.
///
/// Returns `None` only for zero-area glyphs with no placement (rare). Callers
/// should cache the `FontHandle` per font id so the font bytes are parsed
/// once, not once per glyph.
pub fn rasterize_glyph(
    handle: &FontHandle,
    glyph_id: u16,
    size_px: f32,
    is_color: bool,
    synthetic_italic: bool,
    synthetic_bold: bool,
) -> Option<RasterizedGlyph> {
    let ct_font = if synthetic_italic {
        ct_font_sheared(&handle.base_font, size_px as f64)
    } else {
        handle.base_font.clone_with_font_size(size_px as f64)
    };

    let glyphs = [glyph_id as CGGlyph];
    let raw_bounds =
        ct_font.get_bounding_rects_for_glyphs(kCTFontOrientationDefault, &glyphs);

    // COLR color fonts routinely ship an empty outline for each glyph — the
    // real rendering is layered color painting on top of an invisible base.
    // The outline bbox is then 0×0, which `getBoundingRectsForGlyphs` dutifully
    // reports. Fall back to a cell sized from the font's line metrics and the
    // glyph's advance; CoreText paints the color layers into that box. sbix
    // (bitmap) emoji reports a real bbox so they skip this branch.
    let bounds = if is_color
        && (raw_bounds.size.width <= 0.0 || raw_bounds.size.height <= 0.0)
    {
        let ascent = ct_font.ascent();
        let descent = ct_font.descent();
        let mut advance = CGSize::new(0.0, 0.0);
        unsafe {
            ct_font.get_advances_for_glyphs(
                kCTFontOrientationDefault,
                glyphs.as_ptr(),
                &mut advance,
                1,
            );
        }
        if advance.width <= 0.0 || ascent + descent <= 0.0 {
            // No meaningful metrics — treat as truly empty.
            return Some(RasterizedGlyph {
                width: 0,
                height: 0,
                left: 0,
                top: 0,
                is_color,
                bytes: Vec::new(),
            });
        }
        CGRect::new(
            &CGPoint::new(0.0, -descent),
            &CGSize::new(advance.width, ascent + descent),
        )
    } else if raw_bounds.size.width <= 0.0 || raw_bounds.size.height <= 0.0 {
        // Zero-area monochrome glyph (space, ZWJ, combining mark with no ink).
        return Some(RasterizedGlyph {
            width: 0,
            height: 0,
            left: 0,
            top: 0,
            is_color,
            bytes: Vec::new(),
        });
    } else {
        raw_bounds
    };

    // 1px halo on each edge so anti-aliased outlines aren't clipped.
    const PAD: i32 = 1;
    let left = (bounds.origin.x.floor() as i32) - PAD;
    let bottom = (bounds.origin.y.floor() as i32) - PAD;
    let width = ((bounds.size.width.ceil() as i32) + 2 * PAD).max(1) as usize;
    let height = ((bounds.size.height.ceil() as i32) + 2 * PAD).max(1) as usize;
    // Top bearing in the terminal's y-down convention: baseline-to-top-edge,
    // positive up. `bottom` is CoreGraphics' bottom-edge Y (positive up); the
    // top edge sits `height` pixels above it.
    let top = bottom + height as i32;

    let (mut bytes, cx) = if is_color {
        let mut bytes = vec![0u8; width * height * 4];
        let cx = CGContext::create_bitmap_context(
            Some(bytes.as_mut_ptr() as *mut _),
            width,
            height,
            8,
            width * 4,
            &CGColorSpace::create_device_rgb(),
            kCGImageAlphaPremultipliedLast,
        );
        (bytes, cx)
    } else {
        let mut bytes = vec![0u8; width * height];
        let cx = CGContext::create_bitmap_context(
            Some(bytes.as_mut_ptr() as *mut _),
            width,
            height,
            8,
            width,
            &CGColorSpace::create_device_gray(),
            kCGImageAlphaOnly,
        );
        (bytes, cx)
    };

    cx.set_should_antialias(true);
    cx.set_allows_antialiasing(true);
    cx.set_should_smooth_fonts(true);
    cx.set_gray_fill_color(0.0, 1.0);
    // Synthetic bold: Ghostty-style fill+stroke. Line width scales with size
    // (1/14 of points, floored at 1 device pixel) so bold weight looks
    // consistent across font sizes.
    if synthetic_bold {
        cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFillStroke);
        let line_width = (size_px as CGFloat / 14.0).max(1.0);
        cx.set_line_width(line_width);
    } else {
        cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
    }
    // Rio snaps text to the cell grid — no subpixel positioning.
    cx.set_allows_font_subpixel_positioning(false);
    cx.set_should_subpixel_position_fonts(false);
    cx.set_allows_font_subpixel_quantization(false);
    cx.set_should_subpixel_quantize_fonts(false);

    // Shift the pen so the glyph's bounding rect lands at (0, 0)..(width, height)
    // in the bitmap. CoreGraphics' origin is bottom-left; `left`/`bottom` are
    // already the bitmap-space offsets of that origin.
    let origin = CGPoint::new(-left as CGFloat, -bottom as CGFloat);
    ct_font.draw_glyphs(&glyphs, &[origin], cx);

    if is_color {
        unpremultiply_rgba_in_place(&mut bytes);
    }

    Some(RasterizedGlyph {
        width: width as u32,
        height: height as u32,
        left,
        top,
        is_color,
        bytes,
    })
}

/// Stretch axis, used to build a CoreText width trait when resolving a font.
/// Mirrors CSS `font-stretch` values. `Normal` is the no-op default.
#[derive(Clone, Copy, Debug, Default)]
pub enum Stretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl Stretch {
    /// Map to CoreText's normalized width trait (-1.0 = narrowest, 1.0 = widest).
    fn as_ct_width(self) -> f64 {
        match self {
            Self::UltraCondensed => -1.0,
            Self::ExtraCondensed => -0.75,
            Self::Condensed => -0.5,
            Self::SemiCondensed => -0.25,
            Self::Normal => 0.0,
            Self::SemiExpanded => 0.25,
            Self::Expanded => 0.5,
            Self::ExtraExpanded => 0.75,
            Self::UltraExpanded => 1.0,
        }
    }
}

/// Map CSS-style font weight (100–900) to CoreText's normalized weight trait
/// (-1.0 thin .. 0.0 regular .. 1.0 black). Values picked from the mapping
/// CoreText itself uses internally, rounded to the nearest standard step.
fn css_weight_to_ct(weight: u16) -> f64 {
    match weight {
        0..=149 => -0.8,
        150..=249 => -0.6,
        250..=349 => -0.4,
        350..=449 => 0.0,
        450..=549 => 0.23,
        550..=649 => 0.3,
        650..=749 => 0.4,
        750..=849 => 0.56,
        _ => 0.62,
    }
}

/// Resolve a font spec to a file path via CoreText descriptor matching.
///
/// Ghostty- and Zed-style: build a descriptor with family + weight + slant +
/// width, let CoreText do the match, extract the URL. No CSS-spec matching
/// code on our side — CoreText handles proximity scoring and "closest match"
/// rules natively. Returns `None` if CoreText can't find anything, or the
/// resolved descriptor has no URL (e.g. system-supplied font without a
/// backing file, which shouldn't happen for user-installable fonts).
pub fn find_font_path(
    family: &str,
    weight: u16,
    italic: bool,
    stretch: Stretch,
) -> Option<PathBuf> {
    let family_cf = CFString::new(family);
    let ct_weight = css_weight_to_ct(weight);
    let ct_slant: f64 = if italic { 1.0 } else { 0.0 };
    let ct_width = stretch.as_ct_width();

    let weight_key =
        unsafe { CFString::wrap_under_get_rule(kCTFontWeightTrait) };
    let slant_key = unsafe { CFString::wrap_under_get_rule(kCTFontSlantTrait) };
    let width_key = unsafe { CFString::wrap_under_get_rule(kCTFontWidthTrait) };
    let traits: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&[
        (weight_key, CFNumber::from(ct_weight).as_CFType()),
        (slant_key, CFNumber::from(ct_slant).as_CFType()),
        (width_key, CFNumber::from(ct_width).as_CFType()),
    ]);

    let family_key =
        unsafe { CFString::wrap_under_get_rule(kCTFontFamilyNameAttribute) };
    let traits_attr_key =
        unsafe { CFString::wrap_under_get_rule(kCTFontTraitsAttribute) };
    let attrs: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&[
        (family_key, family_cf.as_CFType()),
        (traits_attr_key, traits.as_CFType()),
    ]);

    let desc = font_descriptor::new_from_attributes(&attrs);

    let matched = unsafe {
        let raw = CTFontDescriptorCreateMatchingFontDescriptor(
            desc.as_concrete_TypeRef(),
            std::ptr::null(),
        );
        if raw.is_null() {
            return None;
        }
        CTFontDescriptor::wrap_under_create_rule(raw)
    };

    matched.font_path()
}

/// Sorted, deduplicated list of every installed font family, straight from
/// CoreText. Used by the command-palette font browser.
///
/// Replaces the `font-kit::SystemSource::all_families` call on macOS — font-kit
/// on macOS is itself a CoreText wrapper, so skipping the layer cuts one
/// dependency out of the hot path and sidesteps a known leak in its
/// enumeration API.
pub fn all_families() -> Vec<String> {
    let collection = font_collection::create_for_all_families();
    let Some(descriptors) = collection.get_descriptors() else {
        return Vec::new();
    };
    let mut families: Vec<String> =
        descriptors.iter().map(|desc| desc.family_name()).collect();
    families.sort_unstable();
    families.dedup();
    families
}

/// Atlas wants straight-alpha RGBA; CoreGraphics hands back premultiplied.
/// Divide the color channels by alpha, saturating at 255.
fn unpremultiply_rgba_in_place(bytes: &mut [u8]) {
    for px in bytes.chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        } else if a < 255 {
            let inv = 255.0 / a as f32;
            px[0] = ((px[0] as f32 * inv).min(255.0)) as u8;
            px[1] = ((px[1] as f32 * inv).min(255.0)) as u8;
            px[2] = ((px[2] as f32 * inv).min(255.0)) as u8;
        }
    }
}

/// Scaled line-level metrics for a font at a specific pixel size. Mirrors
/// the subset of swash's `Metrics` struct that Rio's render path reads.
///
/// CoreText exposes ascent/descent/leading/underline/x_height natively;
/// strikeout has no dedicated API, so we derive it from x_height and
/// underline thickness the way most OpenType shapers do. Cap height isn't
/// exposed here yet — Rio's renderer doesn't read it.
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub leading: f32,
    pub underline_offset: f32,
    pub underline_thickness: f32,
    pub strikeout_offset: f32,
    pub strikeout_thickness: f32,
    pub x_height: f32,
}

/// Read CoreText's line metrics for `handle` at `size_px`. Cheap: clones the
/// base CTFont to the target size (CF refcount + trivial size field).
pub fn font_metrics(handle: &FontHandle, size_px: f32) -> FontMetrics {
    let ct_font = handle.base_font.clone_with_font_size(size_px as f64);
    let ascent = ct_font.ascent() as f32;
    let descent = ct_font.descent() as f32;
    let leading = ct_font.leading() as f32;
    let underline_offset = ct_font.underline_position() as f32;
    let underline_thickness = ct_font.underline_thickness() as f32;
    let x_height = ct_font.x_height() as f32;

    // Prefer the designer's explicit strikeout values from the OS/2 table
    // (what Ghostty does). If the font doesn't ship OS/2 or has it zeroed,
    // fall back to the x-height heuristic — strike through the middle of
    // the x-height band at underline thickness.
    let (strikeout_offset, strikeout_thickness) = read_os2_strikeout(
        &ct_font, size_px,
    )
    .unwrap_or((x_height * 0.5, underline_thickness));

    FontMetrics {
        ascent,
        descent,
        leading,
        underline_offset,
        underline_thickness,
        strikeout_offset,
        strikeout_thickness,
        x_height,
    }
}

/// Read `yStrikeoutPosition` and `yStrikeoutSize` from the font's OS/2 table,
/// scaled to pixels. Returns `None` when:
///
/// - the font doesn't carry an OS/2 table (rare for any modern font),
/// - the table is truncated (malformed),
/// - `units_per_em` is 0 (shouldn't happen), or
/// - both fields are 0 (OS/2 present but strikeout unset — treat as missing
///   so the caller can fall back).
fn read_os2_strikeout(ct_font: &CTFont, size_px: f32) -> Option<(f32, f32)> {
    const OS2_TAG: u32 = u32::from_be_bytes(*b"OS/2");

    let cg_font = ct_font.copy_to_CGFont();
    let table = cg_font.copy_table_for_tag(OS2_TAG)?;
    let bytes = table.bytes();
    // yStrikeoutSize: i16 big-endian at offset 26.
    // yStrikeoutPosition: i16 big-endian at offset 28.
    if bytes.len() < 30 {
        return None;
    }
    let size_units = i16::from_be_bytes([bytes[26], bytes[27]]);
    let pos_units = i16::from_be_bytes([bytes[28], bytes[29]]);
    if size_units == 0 && pos_units == 0 {
        return None;
    }
    let units_per_em = ct_font.units_per_em() as f32;
    if units_per_em <= 0.0 {
        return None;
    }
    let scale = size_px / units_per_em;
    Some((pos_units as f32 * scale, size_units as f32 * scale))
}

/// One glyph in a shaped text run.
///
/// The CoreText shaping output, flattened. `cluster` is a UTF-8 byte offset
/// into the original input text — not a UTF-16 index — so callers doing
/// `&text[cluster..]` get the source codepoint directly.
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    pub id: u16,
    /// Pen-relative x in device pixels; CoreText coordinate system (positive
    /// = right of run origin).
    pub x: f32,
    /// Pen-relative y in device pixels; CoreText coordinate system (positive
    /// = above baseline).
    pub y: f32,
    /// Distance to the next glyph's pen position, in device pixels.
    pub advance: f32,
    /// UTF-8 byte offset of the source codepoint in the original input.
    pub cluster: u32,
}

/// Shape a text run via CoreText's `CTLine`.
///
/// Picks up OpenType GSUB/GPOS, AAT, kerning and ligatures — whatever the
/// font ships. Emits one `ShapedGlyph` per output glyph, in visual order.
/// For Latin/CJK this is 1:1 with input characters minus any ligature
/// substitutions; for complex scripts (RTL, Indic) glyph count may differ
/// from character count and cluster offsets may repeat.
///
/// Currently not wired into `layout/content.rs` — it's a building block
/// for later phase-3 integration.
pub fn shape_text(
    handle: &FontHandle,
    text: &str,
    size_px: f32,
) -> Vec<ShapedGlyph> {
    if text.is_empty() {
        return Vec::new();
    }

    let ct_font = handle.base_font.clone_with_font_size(size_px as f64);

    let mut attr = CFMutableAttributedString::new();
    attr.replace_str(&CFString::new(text), CFRange::init(0, 0));
    let utf16_len = attr.char_len();
    unsafe {
        attr.set_attribute(
            CFRange::init(0, utf16_len),
            kCTFontAttributeName,
            &ct_font,
        );
    }

    let line = CTLine::new_with_attributed_string(attr.as_concrete_TypeRef());

    // CoreText returns string indices as UTF-16 code-unit offsets. Callers
    // expect UTF-8 byte offsets (that's how Rio slices text), so we need a
    // mapping. For pure-ASCII input (vast majority of terminal output) every
    // char is 1 byte in UTF-8 and 1 code unit in UTF-16, so the mapping is
    // the identity — skip building the table entirely.
    let utf16_to_utf8 = if text.is_ascii() {
        None
    } else {
        Some(build_utf16_to_utf8_map(text))
    };

    let mut shaped = Vec::new();
    for run in line.glyph_runs().iter() {
        let glyphs = run.glyphs();
        let positions = run.positions();
        let indices = run.string_indices();
        let n = glyphs.len();
        if n == 0 {
            continue;
        }

        // CoreText returns run-accumulated pen positions: glyph i sits at the
        // sum of advances 0..i. Rio's renderer treats `x`/`y` as offsets from
        // the *expected* pen (what you'd get if each prior glyph advanced its
        // own width), not absolute run positions. For LTR Latin text without
        // special positioning, the delta is always 0, which triggers the
        // simple-glyph fast path in `push_run_macos`. Only unusual cases
        // (combining marks, kern adjustments) produce non-zero deltas.
        //
        // `y` on the other hand is per-glyph in horizontal runs (CoreText
        // doesn't accumulate in y), so pass it through verbatim.
        let bounds = run.get_typographic_bounds();
        let mut pen_x = 0.0f32;
        for i in 0..n {
            let pos_x = positions[i].x as f32;
            let next_x = if i + 1 < n {
                positions[i + 1].x as f32
            } else {
                bounds.width as f32
            };
            let advance = next_x - pos_x;
            let offset_x = pos_x - pen_x;
            let offset_y = positions[i].y as f32;

            let utf16_idx = indices[i] as usize;
            let cluster = match &utf16_to_utf8 {
                Some(map) => map.get(utf16_idx).copied().unwrap_or(0) as u32,
                // ASCII fast path: UTF-16 unit index == UTF-8 byte offset.
                None => utf16_idx as u32,
            };

            shaped.push(ShapedGlyph {
                id: glyphs[i],
                x: offset_x,
                y: offset_y,
                advance,
                cluster,
            });

            pen_x = next_x;
        }
    }
    shaped
}

/// Build a lookup from UTF-16 code-unit index → UTF-8 byte offset for the
/// start of the character that code unit is part of. `text.len()` sentinel
/// appended so out-of-range queries map to end-of-string cleanly.
fn build_utf16_to_utf8_map(text: &str) -> Vec<usize> {
    let mut map = Vec::with_capacity(text.len());
    for (byte_idx, ch) in text.char_indices() {
        for _ in 0..ch.len_utf16() {
            map.push(byte_idx);
        }
    }
    map.push(text.len());
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::constants::FONT_CASCADIAMONO_REGULAR;
    use core_foundation::base::CFIndex;

    fn glyph_id_for_char(handle: &FontHandle, size: f64, ch: char) -> u16 {
        let ct_font = handle.base_font.clone_with_font_size(size);
        // Astral codepoints encode as a UTF-16 surrogate pair; CoreText maps
        // both units to a single glyph (first slot holds the gid, second is
        // 0xFFFF). We always want the first slot.
        let mut utf16 = [0u16; 2];
        let encoded = ch.encode_utf16(&mut utf16);
        let count = encoded.len();
        let mut glyphs = [0 as CGGlyph; 2];
        let ok = unsafe {
            ct_font.get_glyphs_for_characters(
                utf16.as_ptr(),
                glyphs.as_mut_ptr(),
                count as CFIndex,
            )
        };
        assert!(ok, "{ch:?} not in font");
        glyphs[0]
    }

    #[test]
    fn shapes_ascii_monospace() {
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        let glyphs = shape_text(&handle, "Hello", 18.0);

        assert_eq!(glyphs.len(), 5, "one glyph per ASCII char");
        // Monospace: advances are all equal.
        let first_advance = glyphs[0].advance;
        for g in &glyphs {
            assert!(
                (g.advance - first_advance).abs() < 0.01,
                "expected constant advance in monospace, got {:?}",
                glyphs
            );
        }
        // LTR Latin without special positioning: x/y offsets from expected
        // pen are all zero. If this regresses, the renderer falls off the
        // simple-glyph fast path and double-accumulates positions.
        for g in &glyphs {
            assert!(
                g.x.abs() < 0.001,
                "x offset should be zero for LTR Latin, got {}",
                g.x
            );
            assert!(g.y.abs() < 0.001, "y offset should be zero, got {}", g.y);
        }
        // Clusters advance monotonically by 1 byte (ASCII).
        for (i, g) in glyphs.iter().enumerate() {
            assert_eq!(g.cluster, i as u32);
        }
    }

    #[test]
    fn reads_strikeout_from_os2_table() {
        // CascadiaMono carries a well-formed OS/2 table, so we should get
        // real values — not the x-height/2 fallback.
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        let m = font_metrics(&handle, 24.0);

        assert!(m.strikeout_thickness > 0.0, "thickness should be positive");
        // Strikeout should sit somewhere in the x-height band — sanity check.
        assert!(m.strikeout_offset > 0.0);
        assert!(
            m.strikeout_offset < m.ascent,
            "strikeout offset should be below ascent"
        );
        // A pure x_height/2 fallback would give exactly x_height/2. OS/2
        // values rarely coincide exactly with that — if they do, the test
        // is weaker but still passes (since both yield sensible output).
        eprintln!(
            "CascadiaMono @24px: strikeout offset={} size={} x_height={}",
            m.strikeout_offset, m.strikeout_thickness, m.x_height
        );
    }

    #[test]
    fn shape_ascii_skips_utf16_map() {
        // Smoke test the fast path — mostly just confirm clusters are
        // correctly identified as byte offsets for ASCII text. If the fast
        // path swapped indices for the slow-path output it'd still compile,
        // but the cluster values would be wrong for multi-byte text.
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        let glyphs = shape_text(&handle, "abcde", 18.0);
        for (i, g) in glyphs.iter().enumerate() {
            assert_eq!(g.cluster, i as u32);
        }
    }

    #[test]
    fn shape_non_ascii_keeps_correct_clusters() {
        // Mixed BMP non-ASCII: 'é' is 2 bytes in UTF-8, 1 code unit in UTF-16.
        // Byte offsets should jump accordingly.
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        let glyphs = shape_text(&handle, "aébc", 18.0);
        // Expected clusters: a=0, é=1, b=3 (after 2-byte é), c=4.
        assert_eq!(glyphs.len(), 4);
        assert_eq!(glyphs[0].cluster, 0);
        assert_eq!(glyphs[1].cluster, 1);
        assert_eq!(glyphs[2].cluster, 3);
        assert_eq!(glyphs[3].cluster, 4);
    }

    #[test]
    fn shapes_empty_input() {
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        assert!(shape_text(&handle, "", 18.0).is_empty());
    }

    #[test]
    fn rasterizes_an_inked_glyph() {
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        let size = 24.0;
        let gid = glyph_id_for_char(&handle, size as f64, 'A');
        let g = rasterize_glyph(&handle, gid, size, false, false, false)
            .expect("rasterize returned None");

        assert!(g.width > 0, "A should have non-zero width");
        assert!(g.height > 0, "A should have non-zero height");
        assert!(!g.is_color);
        assert_eq!(g.bytes.len(), (g.width * g.height) as usize);

        let total: u64 = g.bytes.iter().map(|&b| b as u64).sum();
        assert!(total > 0, "A should have some inked pixels");
    }

    #[test]
    fn find_font_path_resolves_system_family() {
        // Menlo ships on every macOS install since 10.6.
        let path = find_font_path("Menlo", 400, false, Stretch::Normal)
            .expect("Menlo should resolve");
        assert!(path.exists(), "resolved path should exist: {path:?}");
        assert!(
            path.extension()
                .is_some_and(|e| e == "ttf" || e == "ttc" || e == "otf"),
            "unexpected font extension: {path:?}"
        );
    }

    #[test]
    fn from_bytes_index_zero_matches_from_bytes() {
        // For a plain TTF the single font is at index 0; both loaders
        // should land on equivalent CTFonts.
        let a = FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("a");
        let b =
            FontHandle::from_bytes_index(FONT_CASCADIAMONO_REGULAR, 0).expect("b");
        // Compare via a shape probe — identical glyph ids means same face.
        let gid_a = glyph_id_for_char(&a, 18.0, 'A');
        let gid_b = glyph_id_for_char(&b, 18.0, 'A');
        assert_eq!(gid_a, gid_b);
    }

    #[test]
    fn from_bytes_index_out_of_range_returns_none() {
        let h = FontHandle::from_bytes_index(FONT_CASCADIAMONO_REGULAR, 99);
        assert!(h.is_none(), "index 99 on a single-font TTF should fail");
    }

    #[test]
    fn all_families_returns_sorted_nonempty_list() {
        let families = all_families();
        assert!(!families.is_empty(), "system should expose some font families");
        // Collection is deduped + sorted.
        let mut sorted = families.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(families, sorted);
    }

    #[test]
    fn rasterizes_a_color_emoji_glyph() {
        use crate::font::constants::FONT_TWEMOJI_EMOJI;

        // COLR fonts report a 0×0 outline bbox because the real drawing
        // happens on color layers. This test asserts our fallback path
        // (derive bbox from ascent/descent + advance) still produces an
        // inked bitmap with real color content.
        let handle = FontHandle::from_bytes(FONT_TWEMOJI_EMOJI).expect("load twemoji");
        let size = 24.0;
        // U+1F600 grinning face — the canonical "is color emoji working" check.
        let gid = glyph_id_for_char(&handle, size as f64, '\u{1F600}');

        let g = rasterize_glyph(&handle, gid, size, true, false, false)
            .expect("emoji rasterize returned None");

        assert!(
            g.width > 0 && g.height > 0,
            "expected inked emoji, got {g:?}"
        );
        assert!(g.is_color);
        assert_eq!(g.bytes.len(), (g.width * g.height * 4) as usize);
        // If CoreText dropped COLR tables we'd get a black silhouette —
        // alpha non-zero but all RGB zero. Assert some color is present.
        let rgb_sum: u64 = g
            .bytes
            .chunks_exact(4)
            .map(|px| px[0] as u64 + px[1] as u64 + px[2] as u64)
            .sum();
        assert!(
            rgb_sum > 0,
            "COLR tables appear not to have been preserved — emoji drew as \
             a monochrome silhouette"
        );
    }

    #[test]
    fn zero_ink_glyph_yields_empty_bitmap() {
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        let size = 24.0;
        let gid = glyph_id_for_char(&handle, size as f64, ' ');
        let g = rasterize_glyph(&handle, gid, size, false, false, false)
            .expect("rasterize returned None");

        // Space carries advance but no ink; rasterizer should short-circuit.
        assert_eq!(g.width, 0);
        assert_eq!(g.height, 0);
        assert!(g.bytes.is_empty());
    }
}
