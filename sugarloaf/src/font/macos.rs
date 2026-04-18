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
use std::sync::Arc;

use core_foundation::{
    base::{CFType, TCFType},
    dictionary::CFDictionary,
    number::CFNumber,
    string::CFString,
};
use core_graphics::{
    base::{kCGImageAlphaPremultipliedLast, CGFloat},
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    data_provider::CGDataProvider,
    font::{CGFont, CGGlyph},
    geometry::CGPoint,
};
use core_text::{
    font as ct_font, font_collection,
    font_descriptor::{
        self, kCTFontFamilyNameAttribute, kCTFontOrientationDefault,
        kCTFontSlantTrait, kCTFontTraitsAttribute, kCTFontWeightTrait,
        kCTFontWidthTrait, CTFontDescriptor,
        CTFontDescriptorCreateMatchingFontDescriptor,
    },
};

// core-graphics 0.24 doesn't export this one; `kCGImageAlphaOnly = 7` per Apple's
// CGImage.h. Used for 1-channel alpha-only bitmaps (monochrome glyph masks).
#[allow(non_upper_case_globals)]
const kCGImageAlphaOnly: u32 = 7;

/// A parsed CoreGraphics font. The wrapped `CGFont` retains the provider that
/// retains the underlying bytes, so callers can drop the source buffer after
/// construction — the handle keeps everything alive for the lifetime of the
/// rasterizer. Clone is a cheap CoreFoundation retain.
#[derive(Clone)]
pub struct FontHandle {
    cg_font: CGFont,
}

impl FontHandle {
    /// Parse a font file. Returns `None` if CoreGraphics can't read the bytes
    /// (malformed, unsupported format). Note this handles a single font — TTC
    /// collections need external index selection, which isn't wired yet.
    pub fn from_bytes(font_bytes: &[u8]) -> Option<Self> {
        let provider = CGDataProvider::from_buffer(Arc::new(font_bytes.to_vec()));
        let cg_font = CGFont::from_data_provider(provider).ok()?;
        Some(Self { cg_font })
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
/// Returns `None` only for zero-area glyphs with no placement (rare). Callers
/// should cache the `FontHandle` per font id so the font bytes are parsed
/// once, not once per glyph.
pub fn rasterize_glyph(
    handle: &FontHandle,
    glyph_id: u16,
    size_px: f32,
    is_color: bool,
) -> Option<RasterizedGlyph> {
    let ct_font = ct_font::new_from_CGFont(&handle.cg_font, size_px as f64);

    let glyphs = [glyph_id as CGGlyph];
    let bounds =
        ct_font.get_bounding_rects_for_glyphs(kCTFontOrientationDefault, &glyphs);

    // Zero-area glyph (space, ZWJ, combining mark with no ink). Return empty;
    // the caller still gets a cache entry and skips atlas allocation.
    if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
        return Some(RasterizedGlyph {
            width: 0,
            height: 0,
            left: 0,
            top: 0,
            is_color,
            bytes: Vec::new(),
        });
    }

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
    cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
    cx.set_gray_fill_color(0.0, 1.0);
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

    // CoreText is permissive: if the family doesn't exist it may hand us a
    // fallback match with a completely different family name. Reject matches
    // whose family doesn't match (case-insensitive) so callers see a clean
    // "not found" instead of silently rendering the wrong font.
    if !matched.family_name().eq_ignore_ascii_case(family) {
        return None;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::constants::FONT_CASCADIAMONO_REGULAR;
    use core_foundation::base::CFIndex;

    fn glyph_id_for_char(handle: &FontHandle, size: f64, ch: char) -> u16 {
        let ct_font = ct_font::new_from_CGFont(&handle.cg_font, size);
        let code = ch as u16;
        let mut glyphs = [0 as CGGlyph];
        let ok = unsafe {
            ct_font.get_glyphs_for_characters(
                &code as *const u16,
                glyphs.as_mut_ptr(),
                1 as CFIndex,
            )
        };
        assert!(ok, "{ch:?} not in font");
        glyphs[0]
    }

    #[test]
    fn rasterizes_an_inked_glyph() {
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        let size = 24.0;
        let gid = glyph_id_for_char(&handle, size as f64, 'A');
        let g = rasterize_glyph(&handle, gid, size, false)
            .expect("rasterize returned None");

        assert!(g.width > 0, "A should have non-zero width");
        assert!(g.height > 0, "A should have non-zero height");
        assert!(!g.is_color);
        assert_eq!(g.bytes.len(), (g.width * g.height) as usize);

        let total: u64 = g.bytes.iter().map(|&b| b as u64).sum();
        assert!(total > 0, "A should have some inked pixels");
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
    fn zero_ink_glyph_yields_empty_bitmap() {
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_REGULAR).expect("load font");
        let size = 24.0;
        let gid = glyph_id_for_char(&handle, size as f64, ' ');
        let g = rasterize_glyph(&handle, gid, size, false)
            .expect("rasterize returned None");

        // Space carries advance but no ink; rasterizer should short-circuit.
        assert_eq!(g.width, 0);
        assert_eq!(g.height, 0);
        assert!(g.bytes.is_empty());
    }
}
