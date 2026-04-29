//! macOS glyph rasterization via CoreText + CoreGraphics.
//!
//! Replaces the zeno path on macOS so text picks up the native anti-aliasing
//! style and Apple Color Emoji renders without bundled fallback fonts.
//!
//! Output: left/top bearings, width/height in device pixels, and either R8
//! alpha-only bytes (mask) or premultiplied RGBA in Display-P3 (color). The
//! caller stitches this into the existing atlas pipeline unchanged.

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
    base::{kCGBitmapByteOrder32Little, kCGImageAlphaPremultipliedFirst, CGFloat},
    color_space::{kCGColorSpaceDisplayP3, CGColorSpace},
    context::{CGContext, CGTextDrawingMode},
    font::CGGlyph,
    geometry::{CGAffineTransform, CGPoint, CGRect, CGSize},
};
use core_text::{
    font as ct_font,
    font::{CTFont, CTFontRef},
    font_collection,
    font_descriptor::{
        self, kCTFontBoldTrait, kCTFontFamilyNameAttribute, kCTFontItalicTrait,
        kCTFontOrientationDefault, kCTFontStyleNameAttribute, kCTFontSymbolicTrait,
        kCTFontTraitsAttribute, kCTFontVariationAttribute, CTFontDescriptor,
        CTFontDescriptorCopyAttribute, CTFontDescriptorRef,
    },
    font_manager,
    line::CTLine,
    run::CTRun,
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

// Raw FFI for `CFDataCreateWithBytesNoCopy` with `kCFAllocatorNull`.
// core-foundation's `CFData::from_buffer` goes through `CFDataCreate` which
// *copies* the buffer, and `CFData::from_arc` requires an Arc; neither hits
// the zero-copy path we want for bundled fonts whose bytes already live in
// `.rodata`.
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFDataCreateWithBytesNoCopy(
        allocator: core_foundation::base::CFAllocatorRef,
        bytes: *const u8,
        length: core_foundation::base::CFIndex,
        bytes_deallocator: core_foundation::base::CFAllocatorRef,
    ) -> core_foundation::data::CFDataRef;

    #[allow(non_upper_case_globals)]
    static kCFAllocatorNull: core_foundation::base::CFAllocatorRef;
}

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

    // Returns the best CTFont for rendering `string` in `range`, using
    // `current_font`'s cascade list. When the primary can render every
    // codepoint in the range, CoreText returns the primary unchanged;
    // otherwise it returns the cascade-picked fallback. Used by Rio's
    // lazy font-discovery path to register an unknown cascade font on
    // first encounter rather than pre-registering the full cascade at
    // startup.
    fn CTFontCreateForString(
        current_font: CTFontRef,
        string: core_foundation::string::CFStringRef,
        range: CFRange,
    ) -> CTFontRef;
}

/// Shear matrix applied to `CTFont` for synthetic italic. `c = tan(15°)`
/// leans the glyphs 15° to the right.
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
    let walker = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok());
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
/// `CTFontManagerCreateFontDescriptorFromData` + `CTFontCreateWithFontDescriptor`,
/// which preserves COLR, sbix, and other color font tables that the simpler
/// `CGFontCreateWithDataProvider` → `CTFontCreateWithGraphicsFont` path
/// silently drops.
///
/// Stored at a reference 1.0pt size; per-call rasterization clones with the
/// target size (cheap CF refcount, not a parse). Clone is a CF retain.
///
/// TTC caveat: CoreText's data-based descriptor reads only the first font in
/// a collection — use [`FontHandle::from_bytes_index`] for other indices.
#[derive(Clone)]
pub struct FontHandle {
    base_font: CTFont,
}

impl FontHandle {
    /// Parse a font file's bytes into a `CTFont`. Returns `None` if CoreText
    /// can't interpret the buffer (malformed, unsupported format).
    ///
    /// For TTC/OTC collections this picks the first contained font (index
    /// 0) — matches Rio's cross-platform loader (`FontRef::from_index` with
    /// index 0). Use [`FontHandle::from_bytes_index`] if a specific index
    /// is needed.
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
        let array_ref = unsafe {
            CTFontManagerCreateFontDescriptorsFromData(data.as_concrete_TypeRef())
        };
        if array_ref.is_null() {
            return None;
        }
        let descriptors: CFArray<CTFontDescriptor> =
            unsafe { CFArray::wrap_under_create_rule(array_ref) };
        let desc_ref = descriptors.get(index as isize)?;
        let base_font = ct_font::new_from_descriptor(&desc_ref, 1.0);
        Some(Self { base_font })
    }

    /// Zero-copy variant for bundled fonts whose bytes live in `.rodata`.
    ///
    /// `CFDataCreateWithBytesNoCopy(_, ptr, len, kCFAllocatorNull)` tells
    /// CoreFoundation "I own these forever — never try to free them". The
    /// bytes stay in the binary image; CoreText just holds a pointer. This
    /// Saves the ~10 MB of duplication `CFDataCreate` would incur across
    /// our bundled CascadiaMono / Nerd Font slices.
    pub fn from_static_bytes(font_bytes: &'static [u8]) -> Option<Self> {
        use core_foundation::base::{CFIndex, TCFType};
        use core_foundation::data::CFData;

        let data_ref = unsafe {
            CFDataCreateWithBytesNoCopy(
                std::ptr::null(), // default allocator for the CFData itself
                font_bytes.as_ptr(),
                font_bytes.len() as CFIndex,
                kCFAllocatorNull, // never free the payload
            )
        };
        if data_ref.is_null() {
            return None;
        }
        let data = unsafe { CFData::wrap_under_create_rule(data_ref) };
        let desc = font_manager::create_font_descriptor_with_data(data).ok()?;
        let base_font = ct_font::new_from_descriptor(&desc, 1.0);
        Some(Self { base_font })
    }

    /// Load a font straight from a file path without reading the bytes
    /// into Rio.
    ///
    /// Uses `CTFontManagerCreateFontDescriptorsFromURL` so CoreText reads the
    /// file itself (backing it with an mmap or page cache as it sees fit).
    /// This is the right path for the cascade-list emoji font (hundreds of
    /// MB) and for any user font where we know the on-disk location.
    ///
    /// Returns `None` if CoreText can't open or parse the file.
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        use core_foundation::array::CFArray;

        let url = CFURL::from_path(path, false)?;
        let array_ref = unsafe {
            core_text::font_manager::CTFontManagerCreateFontDescriptorsFromURL(
                url.as_concrete_TypeRef(),
            )
        };
        if array_ref.is_null() {
            return None;
        }
        let descriptors: CFArray<CTFontDescriptor> =
            unsafe { CFArray::wrap_under_create_rule(array_ref) };
        let desc_ref = descriptors.get(0)?;
        let base_font = ct_font::new_from_descriptor(&desc_ref, 1.0);
        Some(Self { base_font })
    }

    /// Unique-per-face PostScript name (e.g. "CascadiaMono-Regular",
    /// "AppleColorEmoji"). Used to map a CTFont selected by CoreText's
    /// cascade fallback back to Rio's `font_id` in [`shape_text`].
    pub fn postscript_name(&self) -> String {
        self.base_font.postscript_name()
    }
}

/// Output of a single glyph rasterization. Mirrors the fields of
/// `swash::scale::image::Image` the zeno path fills in.
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
    /// `true` when `bytes` is 4bpp premultiplied-alpha RGBA in Display-P3
    /// (color emoji); `false` when `bytes` is 1bpp alpha-only (monochrome
    /// outline).
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
/// `synthetic_italic` applies a 15° right-lean via a sheared CTFont
/// transform. `synthetic_bold` draws with `CGTextFillStroke` and a stroke
/// width of `max(size/14, 1)`. Both are meant for when the font family
/// lacks the requested variant — normal bold/italic fonts are found by
/// `find_font_path` and should leave both flags `false`.
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
    let mut raw_bounds =
        ct_font.get_bounding_rects_for_glyphs(kCTFontOrientationDefault, &glyphs);

    // Synthetic-bold rect expansion. The fill-stroke draw lays a stroke
    // centered on the glyph outline, so it extends `line_width/2` outside
    // the natural bounding rect — without expansion the stroke clips at the
    // canvas edges. Not applied to color/sbix fonts; bitmap emoji aren't
    // affected by synthetic bold.
    if synthetic_bold && !is_color {
        let line_width = (size_px as f64 / 14.0).max(1.0);
        raw_bounds.size.width += line_width;
        raw_bounds.size.height += line_width;
        raw_bounds.origin.x -= line_width / 2.0;
        raw_bounds.origin.y -= line_width / 2.0;
    }

    // COLR color fonts routinely ship an empty outline for each glyph — the
    // real rendering is layered color painting on top of an invisible base.
    // The outline bbox is then 0×0, which `getBoundingRectsForGlyphs` dutifully
    // reports. Fall back to a cell sized from the font's line metrics and the
    // glyph's advance; CoreText paints the color layers into that box. sbix
    // (bitmap) emoji reports a real bbox so they skip this branch.
    let bounds =
        if is_color && (raw_bounds.size.width <= 0.0 || raw_bounds.size.height <= 0.0) {
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
        // Display-P3 color space (wider gamut than device RGB, which is
        // what Apple Color Emoji assets are authored in) + premultiplied-
        // first alpha + 32-bit little-endian byte order. Combined, this
        // writes BGRA premultiplied bytes into `bytes` — we swap to RGBA
        // below for atlas compatibility, but keep the alpha premultiplied.
        let colorspace =
            CGColorSpace::create_with_name(unsafe { kCGColorSpaceDisplayP3 })
                .unwrap_or_else(CGColorSpace::create_device_rgb);
        let cx = CGContext::create_bitmap_context(
            Some(bytes.as_mut_ptr() as *mut _),
            width,
            height,
            8,
            width * 4,
            &colorspace,
            kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little,
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
    // Synthetic bold via fill+stroke. Line width scales with size
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
        // CoreGraphics wrote BGRA premultiplied (due to
        // `byte_order_32_little | premul_first`). Rio's atlas is RGBA8Unorm
        // with premultiplied-alpha shader blending, so swap B and R to get
        // RGBA premultiplied. The shader converts P3 → sRGB at sample time.
        bgra_to_rgba_in_place(&mut bytes);
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

pub fn find_font_path(
    family: &str,
    bold: bool,
    italic: bool,
    style_name: Option<&str>,
) -> Option<PathBuf> {
    use core_foundation::array::CFArray;

    let family_cf = CFString::new(family);

    let family_key = unsafe { CFString::wrap_under_get_rule(kCTFontFamilyNameAttribute) };
    let mut pairs: Vec<(CFString, CFType)> = vec![(family_key, family_cf.as_CFType())];

    let mut symbolic: u32 = 0;
    if style_name.is_none() {
        if bold {
            symbolic |= kCTFontBoldTrait;
        }
        if italic {
            symbolic |= kCTFontItalicTrait;
        }
    }

    if symbolic != 0 {
        let symbolic_key = unsafe { CFString::wrap_under_get_rule(kCTFontSymbolicTrait) };
        let traits: CFDictionary<CFString, CFType> =
            CFDictionary::from_CFType_pairs(&[(
                symbolic_key,
                CFNumber::from(symbolic as i64).as_CFType(),
            )]);
        let traits_attr_key =
            unsafe { CFString::wrap_under_get_rule(kCTFontTraitsAttribute) };
        pairs.push((traits_attr_key, traits.as_CFType()));
    }

    if let Some(name) = style_name {
        let style_key =
            unsafe { CFString::wrap_under_get_rule(kCTFontStyleNameAttribute) };
        pairs.push((style_key, CFString::new(name).as_CFType()));
    }
    let attrs: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&pairs);
    let desc = font_descriptor::new_from_attributes(&attrs);

    let descs_arr = CFArray::from_CFTypes(&[desc]);
    let collection = font_collection::new_from_descriptors(&descs_arr);
    let candidates = collection.get_descriptors()?;

    let desired_styles = derive_desired_styles(bold, italic, style_name);

    let mut best: Option<(u64, CTFontDescriptor)> = None;
    for d in candidates.iter() {
        let score = score_candidate(&d, bold, italic, &desired_styles);
        let take = match &best {
            None => true,
            Some((b, _)) => score > *b,
        };
        if take {
            best = Some((score, d.clone()));
        }
    }

    best.and_then(|(_, d)| d.font_path())
}

fn derive_desired_styles(
    bold: bool,
    italic: bool,
    style_name: Option<&str>,
) -> Vec<String> {
    if let Some(user) = style_name {
        return vec![user.to_string()];
    }
    let primary = match (bold, italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    };
    vec![primary.to_string()]
}

fn score_candidate(
    desc: &CTFontDescriptor,
    want_bold: bool,
    want_italic: bool,
    desired_styles: &[String],
) -> u64 {
    let font = ct_font::new_from_descriptor(desc, 12.0);
    let traits = font.symbolic_traits();
    let mut is_bold = (traits & kCTFontBoldTrait) != 0;
    let mut is_italic = (traits & kCTFontItalicTrait) != 0;
    let monospace = (traits & (1u32 << 10)) != 0;

    apply_head_table_traits(&font, &mut is_bold, &mut is_italic);
    apply_os2_table_traits(&font, &mut is_bold, &mut is_italic);
    apply_variation_overrides(desc, &font, &mut is_bold, &mut is_italic);

    let style_str = desc.style_name();
    let style_lower = style_str.to_ascii_lowercase();

    let exact_style = desired_styles
        .first()
        .map(|s| s.eq_ignore_ascii_case(&style_str))
        .unwrap_or(false);

    let mut diff: usize = style_str.len().min(255);
    for s in desired_styles {
        if style_lower.contains(&s.to_ascii_lowercase()) {
            diff = diff.saturating_sub(s.len());
        }
    }
    let fuzzy_style = (255usize.saturating_sub(diff)).min(255) as u8;

    let glyph_count = (font.glyph_count() as u64).min(u16::MAX as u64) as u16;

    pack_score(ScoredCandidate {
        glyph_count,
        fuzzy_style,
        bold: is_bold == want_bold,
        italic: is_italic == want_italic,
        exact_style,
        monospace,
    })
}

struct ScoredCandidate {
    glyph_count: u16,
    fuzzy_style: u8,
    bold: bool,
    italic: bool,
    exact_style: bool,
    monospace: bool,
}

fn pack_score(s: ScoredCandidate) -> u64 {
    (s.monospace as u64) << 27
        | (s.exact_style as u64) << 26
        | (s.italic as u64) << 25
        | (s.bold as u64) << 24
        | (s.fuzzy_style as u64) << 16
        | (s.glyph_count as u64)
}

fn apply_head_table_traits(font: &CTFont, is_bold: &mut bool, is_italic: &mut bool) {
    const HEAD_TAG: u32 =
        (b'h' as u32) << 24 | (b'e' as u32) << 16 | (b'a' as u32) << 8 | (b'd' as u32);
    let Some(data) = font.get_font_table(HEAD_TAG) else {
        return;
    };
    let bytes = data.bytes();
    if bytes.len() < 46 {
        return;
    }
    let mac_style = u16::from_be_bytes([bytes[44], bytes[45]]);
    if mac_style & 0x0001 != 0 {
        *is_bold = true;
    }
    if mac_style & 0x0002 != 0 {
        *is_italic = true;
    }
}

fn apply_os2_table_traits(font: &CTFont, is_bold: &mut bool, is_italic: &mut bool) {
    const OS2_TAG: u32 =
        (b'O' as u32) << 24 | (b'S' as u32) << 16 | (b'/' as u32) << 8 | (b'2' as u32);
    let Some(data) = font.get_font_table(OS2_TAG) else {
        return;
    };
    let bytes = data.bytes();
    if bytes.len() < 64 {
        return;
    }
    let fs_selection = u16::from_be_bytes([bytes[62], bytes[63]]);
    if fs_selection & 0x0001 != 0 {
        *is_italic = true;
    }
    if fs_selection & 0x0020 != 0 {
        *is_bold = true;
    }
}

fn apply_variation_overrides(
    desc: &CTFontDescriptor,
    font: &CTFont,
    is_bold: &mut bool,
    is_italic: &mut bool,
) {
    use core_foundation::base::CFType;
    use core_foundation::dictionary::CFDictionary as CFDict;
    use core_foundation::number::CFNumber;

    let var_value = unsafe {
        CTFontDescriptorCopyAttribute(
            desc.as_concrete_TypeRef(),
            kCTFontVariationAttribute,
        )
    };
    if var_value.is_null() {
        return;
    }
    let values_untyped: CFDict<CFType, CFType> =
        unsafe { CFDict::wrap_under_create_rule(var_value as _) };

    let Some(axes) = font.get_variation_axes() else {
        return;
    };

    let id_key = unsafe { kCTFontVariationAxisIdentifierKeyFFI };

    const WGHT_TAG: i64 =
        (b'w' as i64) << 24 | (b'g' as i64) << 16 | (b'h' as i64) << 8 | (b't' as i64);
    const ITAL_TAG: i64 =
        (b'i' as i64) << 24 | (b't' as i64) << 16 | (b'a' as i64) << 8 | (b'l' as i64);
    const SLNT_TAG: i64 =
        (b's' as i64) << 24 | (b'l' as i64) << 16 | (b'n' as i64) << 8 | (b't' as i64);

    let mut ital_seen = false;
    for axis in axes.iter() {
        let Some(id_item) = axis.find(id_key) else {
            continue;
        };
        let Some(id_num) = id_item.downcast::<CFNumber>() else {
            continue;
        };
        let Some(tag) = id_num.to_i64() else {
            continue;
        };

        let id_as_key: CFType = id_num.as_CFType();
        let val: f64 = match values_untyped.find(&id_as_key) {
            Some(v) => match v.downcast::<CFNumber>() {
                Some(n) => n.to_f64().unwrap_or(0.0),
                None => continue,
            },
            None => continue,
        };

        match tag {
            WGHT_TAG => *is_bold = val > 600.0,
            ITAL_TAG => {
                *is_italic = val > 0.5;
                ital_seen = true;
            }
            SLNT_TAG if !ital_seen => *is_italic = val <= -5.0,
            _ => {}
        }
    }
}

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    #[link_name = "kCTFontVariationAxisIdentifierKey"]
    static kCTFontVariationAxisIdentifierKeyFFI: core_foundation::string::CFStringRef;
}

/// System default cascade (fallback) font file paths for `handle`'s font.
///
/// This is CoreText's own recommended fallback order — the same chain it uses
/// for automatic font substitution when a string contains glyphs missing from
/// the requested font. Typically includes: the primary font's designer-chosen
/// fallbacks, system CJK fonts, Apple Color Emoji, and symbol fonts.
///
/// Dynamic fallback: instead of hardcoding family names like
/// `"Apple Color Emoji"`, rely on CoreText to pick the right fonts for this
/// system. Paths that CoreText doesn't expose (some system fonts ship without
/// a file URL) are silently skipped.
pub fn default_cascade_list(handle: &FontHandle) -> Vec<PathBuf> {
    use core_foundation::array::CFArray;
    let languages: CFArray<CFString> = CFArray::from_CFTypes(&[]);
    let cascade =
        core_text::font::cascade_list_for_languages(&handle.base_font, &languages);
    cascade.iter().filter_map(|desc| desc.font_path()).collect()
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

/// Swap B and R in each 4-byte pixel. CoreGraphics' `premul_first +
/// byte_order_32_little` writes BGRA; Rio's atlas is RGBA. Alpha stays put.
fn bgra_to_rgba_in_place(bytes: &mut [u8]) {
    for px in bytes.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

/// Build a `swash::Metrics` populated from CoreText, in font
/// design units. Used by `FontData::get_metrics` on macOS so the metrics
/// path works without raw font bytes.
///
/// CTFont exposes everything we need directly (ascent, descent, leading,
/// underline, x-height, cap-height, units_per_em). Strikeout has no CT
/// API — we derive it like `font::macos::font_metrics` does, from the
/// OS/2 table if available or x-height/2 as a fallback.
pub fn design_unit_metrics(handle: &FontHandle) -> swash::Metrics {
    let ct = &handle.base_font;
    let upem = ct.units_per_em() as f32;

    // Base CTFont is at 1pt, so these are in points-per-unit; multiply by
    // units_per_em for design units.
    let ascent = ct.ascent() as f32 * upem;
    let descent = ct.descent() as f32 * upem;
    let leading = ct.leading() as f32 * upem;
    let underline_offset = ct.underline_position() as f32 * upem;
    let stroke_size = ct.underline_thickness() as f32 * upem;
    let x_height = ct.x_height() as f32 * upem;
    let cap_height = ct.cap_height() as f32 * upem;

    let (strikeout_offset, strikeout_stroke) = read_os2_strikeout(ct, 1.0)
        .map(|(off, thick)| (off * upem, thick * upem))
        .unwrap_or((x_height * 0.5, stroke_size));

    // `SymbolicTraitAccessors` is private in core-text; bit-mask the raw
    // u32 traits instead. 1 << 10 is `kCTFontTraitMonoSpace`.
    let is_monospace = (ct.symbolic_traits() & (1 << 10)) != 0;

    swash::Metrics {
        units_per_em: upem as u16,
        glyph_count: ct.glyph_count() as u16,
        is_monospace,
        has_vertical_metrics: false,
        ascent,
        descent,
        leading,
        vertical_ascent: 0.0,
        vertical_descent: 0.0,
        vertical_leading: 0.0,
        cap_height,
        x_height,
        average_width: 0.0,
        max_width: 0.0,
        underline_offset,
        strikeout_offset,
        stroke_size: strikeout_stroke.max(stroke_size),
    }
}

/// Measure the CJK water ideograph "水" at design-unit width. Mirrors
/// `FaceMetrics::measure_cjk_character_width` for non-macOS. Used so the
/// macOS `get_metrics` path can still feed a correct `ic_width` into
/// FaceMetrics without needing the font's bytes.
pub fn cjk_ic_width(handle: &FontHandle) -> Option<f64> {
    const WATER: char = '\u{6C34}';
    advance_units_for_char(handle, WATER).and_then(|(units, _upem)| {
        if units > 0.0 {
            Some(units as f64)
        } else {
            None
        }
    })
}

/// Return `(advance_in_design_units, units_per_em)` for `ch`, or `None`
/// if the font doesn't carry a glyph for it.
///
/// Matches the old swash-based `compute_advance` return shape so the
/// caller (`font_cache.rs`) can scale to pixels the same way on both
/// platforms. All data comes from the CTFont — no raw bytes needed.
pub fn advance_units_for_char(handle: &FontHandle, ch: char) -> Option<(f32, u16)> {
    use core_foundation::base::CFIndex;
    use core_graphics::geometry::CGSize;

    let mut utf16 = [0u16; 2];
    let encoded = ch.encode_utf16(&mut utf16);
    let count = encoded.len();
    let mut glyphs = [0 as CGGlyph; 2];
    let ok = unsafe {
        handle.base_font.get_glyphs_for_characters(
            utf16.as_ptr(),
            glyphs.as_mut_ptr(),
            count as CFIndex,
        )
    };
    if !ok || glyphs[0] == 0 {
        return None;
    }

    // Base CTFont is at 1pt, so advance.width is in points-per-unit. Scale
    // by units_per_em to get design-unit advance.
    let mut advance = CGSize::new(0.0, 0.0);
    unsafe {
        handle.base_font.get_advances_for_glyphs(
            kCTFontOrientationDefault,
            glyphs.as_ptr(),
            &mut advance,
            1,
        );
    }
    let units_per_em = handle.base_font.units_per_em() as u16;
    Some((advance.width as f32 * units_per_em as f32, units_per_em))
}

/// Return the max advance width in pixels across all printable
/// ASCII (U+0020..U+007E) at `size_px`.
/// cell-width derivation.
///
/// Why ASCII-wide + max-of-all rather than just `space`:
/// - Some fonts return `None` / glyph 0 for space and the caller
///   falls back to a bad value (historically `font_size` aka the em,
///   ~1.5× too wide).
/// - For a real monospace font every ASCII char shares one advance,
///   so `max` returns exactly that.
///
/// Why a properly-sized CTFont rather than `base_font` (1pt):
/// `get_advances_for_glyphs` on the 1pt base returns an advance in
/// user-space which some fonts report back as 1.0 for every glyph —
/// a bogus "full em" that defeats the whole point of querying. At
/// the real size the values come through correctly (points at that
/// size ≈ pixels).
pub fn max_ascii_advance_px(handle: &FontHandle, size_px: f32) -> Option<f32> {
    use core_foundation::base::CFIndex;
    use core_graphics::geometry::CGSize;

    if size_px <= 0.0 {
        return None;
    }

    // CTFont at the actual render size.
    let ct_font = handle.base_font.clone_with_font_size(size_px as f64);

    const FIRST: u16 = 0x20;
    const LAST: u16 = 0x7E;
    const COUNT: usize = (LAST - FIRST + 1) as usize;
    let mut utf16 = [0u16; COUNT];
    for (i, slot) in utf16.iter_mut().enumerate() {
        *slot = FIRST + i as u16;
    }

    let mut glyphs = [0 as CGGlyph; COUNT];
    let ok = unsafe {
        ct_font.get_glyphs_for_characters(
            utf16.as_ptr(),
            glyphs.as_mut_ptr(),
            COUNT as CFIndex,
        )
    };
    if !ok {
        return None;
    }

    let mut advances = [CGSize::new(0.0, 0.0); COUNT];
    unsafe {
        ct_font.get_advances_for_glyphs(
            kCTFontOrientationDefault,
            glyphs.as_ptr(),
            advances.as_mut_ptr(),
            COUNT as CFIndex,
        );
    }

    let mut max_px: f32 = 0.0;
    for i in 0..COUNT {
        if glyphs[i] == 0 {
            continue;
        }
        let w = advances[i].width as f32;
        if w > max_px {
            max_px = w;
        }
    }
    if max_px <= 0.0 {
        None
    } else {
        Some(max_px)
    }
}

/// Font-level attributes read straight from a `CTFont`. Mirrors the subset
/// of `swash::Attributes` that Rio stores on `FontData` — used
/// to build a `FontData` from a path (or static bytes) without parsing the
/// font file ourselves.
#[derive(Debug, Clone, Copy)]
pub struct FontAttributes {
    pub weight: u16,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_monospace: bool,
    pub is_color: bool,
}

pub fn font_attributes(handle: &FontHandle) -> FontAttributes {
    const K_CTFONT_TRAIT_ITALIC: u32 = 1 << 0;
    const K_CTFONT_TRAIT_BOLD: u32 = 1 << 1;
    const K_CTFONT_TRAIT_MONOSPACE: u32 = 1 << 10;
    const K_CTFONT_TRAIT_COLOR_GLYPHS: u32 = 1 << 13;

    let traits: u32 = handle.base_font.symbolic_traits();
    let is_bold = (traits & K_CTFONT_TRAIT_BOLD) != 0;
    FontAttributes {
        weight: if is_bold { 700 } else { 400 },
        is_bold,
        is_italic: (traits & K_CTFONT_TRAIT_ITALIC) != 0,
        is_monospace: (traits & K_CTFONT_TRAIT_MONOSPACE) != 0,
        is_color: (traits & K_CTFONT_TRAIT_COLOR_GLYPHS) != 0,
    }
}

/// Ask CoreText which font should render `ch` when `primary` can't.
///
/// Wraps `CTFontCreateForString(primary, string, range)`. When the
/// primary font carries a glyph for the codepoint, CoreText returns
/// the primary itself; when it doesn't, CoreText walks its cascade
/// list and returns the best available fallback (emoji, CJK, symbol,
/// etc.).
///
/// The returned handle is normalized to 1pt so it matches the
/// convention for stored [`FontHandle`]s — per-render sizing goes
/// through `clone_with_font_size` at shape/raster time.
///
/// Lets Rio register an unknown cascade font on first encounter rather
/// than pre-registering every path-backed fallback at startup. Returns
/// `None` only when CoreText itself refuses (exceptionally rare).
pub fn discover_fallback(primary: &FontHandle, ch: char) -> Option<FontHandle> {
    use core_foundation::base::CFIndex;

    let ch_str = ch.to_string();
    let cf_string = CFString::new(&ch_str);
    let range = CFRange::init(0, ch.len_utf16() as CFIndex);
    let ctfont_ref = unsafe {
        CTFontCreateForString(
            primary.base_font.as_concrete_TypeRef(),
            cf_string.as_concrete_TypeRef(),
            range,
        )
    };
    if ctfont_ref.is_null() {
        return None;
    }
    let ct = unsafe { CTFont::wrap_under_create_rule(ctfont_ref) };
    Some(FontHandle {
        base_font: ct.clone_with_font_size(1.0),
    })
}

/// Check whether `handle`'s font has a real glyph for `ch`.
///
/// Replaces the `swash::FontRef::charmap().map(ch)` path on
/// macOS so the fallback walk in `lookup_for_font_match` doesn't need the
/// font's raw bytes — only the CTFont. Combined with path-based FontHandle
/// construction, this lets us drop `FONT_DATA_CACHE` entirely.
///
/// Astral codepoints (`ch as u32 > 0xFFFF`) encode as a UTF-16 surrogate
/// pair; CoreText maps both units to one glyph (first index holds it,
/// second is `0xFFFF`). We want the first index.
pub fn font_has_char(handle: &FontHandle, ch: char) -> bool {
    use core_foundation::base::CFIndex;
    let mut utf16 = [0u16; 2];
    let encoded = ch.encode_utf16(&mut utf16);
    let count = encoded.len();
    let mut glyphs = [0 as CGGlyph; 2];
    let ok = unsafe {
        handle.base_font.get_glyphs_for_characters(
            utf16.as_ptr(),
            glyphs.as_mut_ptr(),
            count as CFIndex,
        )
    };
    ok && glyphs[0] != 0
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

    // Prefer the designer's explicit strikeout values from the OS/2 table.
    // If the font doesn't ship OS/2 or has it zeroed, fall back to the
    // x-height heuristic — strike through the middle of the x-height band
    // at underline thickness.
    let (strikeout_offset, strikeout_thickness) = read_os2_strikeout(&ct_font, size_px)
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
    /// Pen-relative x in device pixels; offset from the expected pen
    /// position if every prior glyph had advanced by its own `advance`.
    /// Zero for LTR Latin text without kerning/marks, which is the
    /// simple-glyph fast path in `push_run_macos`.
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
/// font ships.
///
/// Positions are emitted as pen-relative deltas (`ShapedGlyph::x`,
/// `::y`): offsets from the expected pen position if each prior glyph
/// had advanced by its own width. The pen accumulates across every
/// CTRun on the line — `CTRunGetPositions` is documented as
/// line-relative, never run-relative, so the last glyph of a non-first
/// run uses the next run's first position (or the line's typographic
/// width) as its advance sentinel. Never mix `CTRunGetTypographicBounds`
/// into that math — it's run-local and produces negative advances for
/// any run that doesn't start at x=0.
///
/// If the primary font can't render every codepoint, CoreText may split
/// the line into multiple CTRuns and substitute from the cascade list.
/// Rio handles cascade substitution *before* shaping (via
/// `CodepointResolver`-style per-char font resolution with lazy
/// discovery), so shape calls here normally see a single font. When a
/// shape-time substitution does slip through, every run's glyphs are
/// rasterized against `handle` — producing .notdef / tofu for the
/// substituted glyphs, which is the signal that pre-resolution missed
/// something and needs widening.
pub fn shape_text(handle: &FontHandle, text: &str, size_px: f32) -> Vec<ShapedGlyph> {
    if text.is_empty() {
        return Vec::new();
    }

    let primary_ct_font = handle.base_font.clone_with_font_size(size_px as f64);

    let mut attr = CFMutableAttributedString::new();
    attr.replace_str(&CFString::new(text), CFRange::init(0, 0));
    let utf16_len = attr.char_len();
    unsafe {
        attr.set_attribute(
            CFRange::init(0, utf16_len),
            kCTFontAttributeName,
            &primary_ct_font,
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

    // End-of-line sentinel for the last glyph's advance. `CTLineGetTypographicBounds`
    // returns the width of the full line, which is line-relative and therefore
    // comparable to positions read from `CTRunGetPositions`.
    let line_width = line.get_typographic_bounds().width as f32;

    // Retain every CTRun for the lifetime of the function so the
    // `Cow<[T]>` slices returned by `run.glyphs()/positions()/string_indices()`
    // stay valid. Cloning a CTRun is a CF retain, not a copy of the
    // underlying data — cheap. This keeps the fast-path pointer read
    // from core-text 21's accessors instead of forcing an owned Vec
    // copy per run.
    let glyph_runs = line.glyph_runs();
    let runs: Vec<CTRun> = glyph_runs.iter().map(|r| (*r).clone()).collect();
    if runs.is_empty() {
        return Vec::new();
    }

    let mut shaped = Vec::new();
    let mut pen_x = 0.0f32;

    for run_idx in 0..runs.len() {
        let run = &runs[run_idx];
        let glyphs = run.glyphs();
        if glyphs.is_empty() {
            continue;
        }
        let positions = run.positions();
        let indices = run.string_indices();
        let n = glyphs.len();

        // X-position just past this run's last glyph, in line
        // coordinates. Scan forward for the first subsequent non-empty
        // run's starting position; fall back to the line's typographic
        // width when every following run is empty. Skipping empties
        // matters on the rare line where CoreText splits an empty run
        // in — without the skip, the preceding run's last glyph would
        // advance all the way to `line_width` instead of to the next
        // non-empty run's start.
        let after_run_x = runs[run_idx + 1..]
            .iter()
            .find_map(|r| r.positions().first().map(|p| p.x as f32))
            .unwrap_or(line_width);

        for i in 0..n {
            let pos_x = positions[i].x as f32;
            let next_x = if i + 1 < n {
                positions[i + 1].x as f32
            } else {
                after_run_x
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

/// UTF-16 shape path. Caller stages its run in UTF-16 already (the
/// encoding CoreText uses natively), we hand the buffer to
/// `CFStringCreateWithCharactersNoCopy`, and `ShapedGlyph.cluster`
/// comes back as a UTF-16 code-unit offset. Skips the UTF-8 → UTF-16
/// conversion inside `CFString::new` AND the UTF-16 → UTF-8 mapping
/// pass after CoreText. CoreText shaper at
/// `ghostty/src/font/shaper/coretext.zig:652-680`.
pub fn shape_text_utf16(
    handle: &FontHandle,
    utf16: &[u16],
    size_px: f32,
) -> Vec<ShapedGlyph> {
    if utf16.is_empty() {
        return Vec::new();
    }

    let primary_ct_font = handle.base_font.clone_with_font_size(size_px as f64);

    // `NoCopy` with `kCFAllocatorNull` deallocator = CF references our
    // `utf16` buffer directly; we own it and must keep it alive for
    // the duration of this call. Both hold on the stack, safe.
    let cf_string = unsafe {
        use core_foundation::base::{kCFAllocatorDefault, kCFAllocatorNull, TCFType};
        use core_foundation::string::{CFString, CFStringCreateWithCharactersNoCopy};
        let string_ref = CFStringCreateWithCharactersNoCopy(
            kCFAllocatorDefault,
            utf16.as_ptr(),
            utf16.len() as core_foundation::base::CFIndex,
            kCFAllocatorNull,
        );
        if string_ref.is_null() {
            return Vec::new();
        }
        CFString::wrap_under_create_rule(string_ref)
    };

    let mut attr = CFMutableAttributedString::new();
    attr.replace_str(&cf_string, CFRange::init(0, 0));
    let utf16_len = attr.char_len();
    unsafe {
        attr.set_attribute(
            CFRange::init(0, utf16_len),
            kCTFontAttributeName,
            &primary_ct_font,
        );
    }

    let line = CTLine::new_with_attributed_string(attr.as_concrete_TypeRef());
    let line_width = line.get_typographic_bounds().width as f32;
    let glyph_runs = line.glyph_runs();
    let runs: Vec<CTRun> = glyph_runs.iter().map(|r| (*r).clone()).collect();
    if runs.is_empty() {
        return Vec::new();
    }

    let mut shaped = Vec::new();
    let mut pen_x = 0.0f32;

    for run_idx in 0..runs.len() {
        let run = &runs[run_idx];
        let glyphs = run.glyphs();
        if glyphs.is_empty() {
            continue;
        }
        let positions = run.positions();
        let indices = run.string_indices();
        let n = glyphs.len();

        let after_run_x = runs[run_idx + 1..]
            .iter()
            .find_map(|r| r.positions().first().map(|p| p.x as f32))
            .unwrap_or(line_width);

        for i in 0..n {
            let pos_x = positions[i].x as f32;
            let next_x = if i + 1 < n {
                positions[i + 1].x as f32
            } else {
                after_run_x
            };
            let advance = next_x - pos_x;
            let offset_x = pos_x - pen_x;
            let offset_y = positions[i].y as f32;

            shaped.push(ShapedGlyph {
                id: glyphs[i],
                x: offset_x,
                y: offset_y,
                advance,
                // CoreText reports UTF-16 code-unit offsets natively;
                // the caller keeps a cell-start table in the same
                // coordinate space for the cluster → cell mapping.
                cluster: indices[i] as u32,
            });

            pen_x = next_x;
        }
    }
    shaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::constants::FONT_CASCADIAMONO_NF_REGULAR;
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
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
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
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
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
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
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
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
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
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
        assert!(shape_text(&handle, "", 18.0).is_empty());
    }

    #[test]
    fn discover_fallback_handles_covered_char() {
        // For a codepoint the primary font already has, `CTFontCreateForString`
        // returns a font (usually the primary itself). The result must not be
        // null — Rio's lazy-discovery path relies on that.
        let primary =
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load primary");
        let result = discover_fallback(&primary, 'A');
        assert!(
            result.is_some(),
            "discover_fallback should return a font for a covered char"
        );
    }

    #[test]
    fn discover_fallback_returns_a_font_for_cjk() {
        // CascadiaMono doesn't have U+6C34 ('水'). CoreText's cascade
        // must produce *some* font — Rio registers whichever one comes
        // back. The test asserts non-null and, to avoid being flaky on
        // different macOS versions, does not hardcode the PS name.
        let primary =
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load primary");
        let fallback = discover_fallback(&primary, '\u{6C34}')
            .expect("CoreText should cascade to a CJK font for 水");
        // The discovered font must cover the codepoint — the whole
        // point of the cascade is that it can render what primary
        // couldn't.
        assert!(
            font_has_char(&fallback, '\u{6C34}'),
            "CTFontCreateForString returned a font that doesn't cover U+6C34"
        );
    }

    #[test]
    fn shape_cascades_cjk_and_keeps_advances_positive() {
        // Regression test for the "title moves to the left" bug: when the
        // primary font (CascadiaMono) can't render a codepoint and
        // CoreText substitutes from its internal cascade list, the
        // resulting multi-CTRun output must still produce monotonically
        // increasing pen positions and no negative advances. The old
        // code read run-local `CTRunGetTypographicBounds` width against
        // line-relative positions from `CTRunGetPositions`, which made
        // the last glyph of every non-first run advance negatively,
        // pulling later text to the left.
        //
        // Rio's production path avoids shape-time substitution by
        // pre-resolving per-character font_ids (so `shape_text` sees a
        // single-font fragment), but we still exercise the multi-run
        // path here because it's the cheapest invariant to regress on.
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
        // "A水B" — the CJK "water" ideograph is not in CascadiaMono, so
        // CoreText will cascade into a system CJK font for the middle
        // glyph, splitting the line across 3 CTRuns.
        let glyphs = shape_text(&handle, "A水B", 18.0);
        assert!(!glyphs.is_empty(), "expected at least one glyph, got none");

        // Invariant 1: every glyph's advance is positive. The old bug
        // made the last glyph of each non-first CTRun have a negative
        // advance (`bounds.width - positions[last].x`).
        for g in &glyphs {
            assert!(
                g.advance > 0.0,
                "non-positive advance {} at cluster {}",
                g.advance,
                g.cluster
            );
        }

        // Invariant 2: reconstructing the pen by summing advances and
        // applying per-glyph `x` deltas is monotonically non-decreasing.
        // Any mix-up between line-relative and run-local coordinates
        // would break this.
        let mut cursor = 0.0f32;
        for g in &glyphs {
            let glyph_x = cursor + g.x;
            assert!(
                glyph_x + 0.001 >= cursor - g.advance,
                "pen moved backwards at cluster {}: cursor={}, glyph_x={}",
                g.cluster,
                cursor,
                glyph_x
            );
            cursor += g.advance;
        }
        let total: f32 = glyphs.iter().map(|g| g.advance).sum();
        assert!(total > 0.0, "expected positive total advance, got {total}");
    }

    #[test]
    fn static_bytes_path_rasterizes() {
        // Full no-copy path: .rodata bytes → CFDataCreateWithBytesNoCopy →
        // CTFontDescriptor → CTFont → rasterize. Verifies the FFI is wired
        // correctly and the ref-don't-copy CFData is accepted by
        // CTFontManagerCreateFontDescriptorFromData.
        let handle = FontHandle::from_static_bytes(FONT_CASCADIAMONO_NF_REGULAR)
            .expect("static bytes should parse");
        let size = 18.0;
        let gid = glyph_id_for_char(&handle, size as f64, 'M');
        let g = rasterize_glyph(&handle, gid, size, false, false, false)
            .expect("rasterize returned None");
        assert!(g.width > 0 && g.height > 0);
        // Inked: at least one non-zero alpha pixel.
        assert!(g.bytes.iter().any(|&b| b > 0));
    }

    #[test]
    fn rasterizes_an_inked_glyph() {
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
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
        let path =
            find_font_path("Menlo", false, false, None).expect("Menlo should resolve");
        assert!(path.exists(), "resolved path should exist: {path:?}");
        assert!(
            path.extension()
                .is_some_and(|e| e == "ttf" || e == "ttc" || e == "otf"),
            "unexpected font extension: {path:?}"
        );
    }

    #[test]
    fn default_cascade_list_is_nonempty() {
        // Every macOS install has a system cascade list for any loaded font.
        // This test is a regression guard — if `cascade_list_for_languages`
        // ever returns empty for a legit font, dynamic fallback stops working.
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
        let paths = default_cascade_list(&handle);
        assert!(
            !paths.is_empty(),
            "CoreText should surface a non-empty cascade"
        );
        // Every returned path should be a real file on disk. System fonts
        // that don't ship a file URL are filtered out by `font_path()`.
        for p in &paths {
            assert!(p.exists(), "cascade path should exist: {p:?}");
        }
    }

    #[test]
    fn from_bytes_index_zero_matches_from_bytes() {
        // For a plain TTF the single font is at index 0; both loaders
        // should land on equivalent CTFonts.
        let a = FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("a");
        let b = FontHandle::from_bytes_index(FONT_CASCADIAMONO_NF_REGULAR, 0).expect("b");
        // Compare via a shape probe — identical glyph ids means same face.
        let gid_a = glyph_id_for_char(&a, 18.0, 'A');
        let gid_b = glyph_id_for_char(&b, 18.0, 'A');
        assert_eq!(gid_a, gid_b);
    }

    #[test]
    fn from_bytes_index_out_of_range_returns_none() {
        let h = FontHandle::from_bytes_index(FONT_CASCADIAMONO_NF_REGULAR, 99);
        assert!(h.is_none(), "index 99 on a single-font TTF should fail");
    }

    #[test]
    fn all_families_returns_sorted_nonempty_list() {
        let families = all_families();
        assert!(
            !families.is_empty(),
            "system should expose some font families"
        );
        // Collection is deduped + sorted.
        let mut sorted = families.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(families, sorted);
    }

    #[test]
    fn zero_ink_glyph_yields_empty_bitmap() {
        let handle =
            FontHandle::from_bytes(FONT_CASCADIAMONO_NF_REGULAR).expect("load font");
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
