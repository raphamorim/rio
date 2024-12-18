// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use super::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    borrow::Cow,
    collections::hash_map::Entry,
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    mem, slice,
    sync::{Mutex, MutexGuard},
};

/// `SectionGlyph` iterator.
pub type SectionGlyphIter<'a> = slice::Iter<'a, SectionGlyph>;

/// Common glyph layout logic.
pub trait GlyphCruncher<F: Font = FontArc, X: Clone = Extra> {
    /// Returns an iterator over the `PositionedGlyph`s of the given section with a custom layout.
    ///
    /// Benefits from caching, see [caching behaviour](#caching-behaviour).
    fn glyphs_custom_layout<'a, 'b, S, L>(
        &'b mut self,
        section: S,
        custom_layout: &L,
    ) -> SectionGlyphIter<'b>
    where
        X: 'a,
        L: GlyphPositioner + Hash,
        S: Into<Cow<'a, Section<'a, X>>>;

    /// Returns an iterator over the `PositionedGlyph`s of the given section.
    ///
    /// Benefits from caching, see [caching behaviour](#caching-behaviour).
    #[inline]
    fn glyphs<'a, 'b, S>(&'b mut self, section: S) -> SectionGlyphIter<'b>
    where
        X: 'a,
        S: Into<Cow<'a, Section<'a, X>>>,
    {
        let section = section.into();
        let layout = section.layout;
        self.glyphs_custom_layout(section, &layout)
    }

    /// Returns the available fonts.
    ///
    /// The `FontId` corresponds to the index of the font data.
    fn fonts(&self) -> &[F];

    /// Returns a bounding box for the section glyphs calculated using each glyph's
    /// vertical & horizontal metrics.
    ///
    /// If the section is empty the call will return `None`.
    ///
    /// The bounds will always lay within the specified layout bounds, ie that returned
    /// by the layout's `bounds_rect` function.
    ///
    /// Benefits from caching, see [caching behaviour](#caching-behaviour).
    fn glyph_bounds_custom_layout<'a, S, L>(
        &mut self,
        section: S,
        custom_layout: &L,
    ) -> Option<Rect>
    where
        X: 'a,
        L: GlyphPositioner + Hash,
        S: Into<Cow<'a, Section<'a, X>>>;

    /// Returns a bounding box for the section glyphs calculated using each glyph's
    /// vertical & horizontal metrics.
    ///
    /// If the section is empty the call will return `None`.
    ///
    /// The bounds will always lay within the specified layout bounds, ie that returned
    /// by the layout's `bounds_rect` function.
    ///
    /// Benefits from caching, see [caching behaviour](#caching-behaviour).
    #[inline]
    fn glyph_bounds<'a, S>(&mut self, section: S) -> Option<Rect>
    where
        X: 'a,
        S: Into<Cow<'a, Section<'a, X>>>,
    {
        let section = section.into();
        let layout = section.layout;
        self.glyph_bounds_custom_layout(section, &layout)
    }
}

/// Cut down version of a [`GlyphBrush`](struct.GlyphBrush.html) that can calculate pixel bounds,
/// but is unable to actually render anything.
///
/// Build using a [`GlyphCalculatorBuilder`](struct.GlyphCalculatorBuilder.html).
///
/// # Caching behaviour
///
/// Calls to [`GlyphCalculatorGuard::glyph_bounds`](#method.glyph_bounds),
/// [`GlyphCalculatorGuard::glyphs`](#method.glyphs) calculate the positioned glyphs for a
/// section. This is cached so future calls to any of the methods for the same section are much
/// cheaper.
///
/// Unlike a [`GlyphBrush`](struct.GlyphBrush.html) there is no concept of actually drawing
/// the section to imply when a section is used / no longer used. Instead a `GlyphCalculatorGuard`
/// is created, that provides the calculation functionality. Dropping indicates the 'cache frame'
/// is over, similar to when a `GlyphBrush` draws. Section calculations are cached for the next
/// 'cache frame', if not used then they will be dropped.
pub struct GlyphCalculator<F = FontArc, X = Extra, H = DefaultSectionHasher> {
    fonts: Vec<F>,

    // cache of section-layout hash -> computed glyphs, this avoid repeated glyph computation
    // for identical layout/sections common to repeated frame rendering
    calculate_glyph_cache: Mutex<FxHashMap<u64, GlyphedSection<X>>>,

    section_hasher: H,
}

impl<F, X, H> fmt::Debug for GlyphCalculator<F, X, H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GlyphCalculator")
    }
}

impl<F: Font, X, H: BuildHasher + Clone> GlyphCalculator<F, X, H> {
    pub fn cache_scope(&self) -> GlyphCalculatorGuard<'_, F, X, H> {
        GlyphCalculatorGuard {
            fonts: &self.fonts,
            glyph_cache: self.calculate_glyph_cache.lock().unwrap(),
            cached: FxHashSet::default(),
            section_hasher: self.section_hasher.clone(),
        }
    }

    /// Returns the available fonts.
    ///
    /// The `FontId` corresponds to the index of the font data.
    pub fn fonts(&self) -> &[F] {
        &self.fonts
    }
}

/// [`GlyphCalculator`](struct.GlyphCalculator.html) scoped cache lock.
pub struct GlyphCalculatorGuard<
    'brush,
    F: 'brush = FontArc,
    X = Extra,
    H = DefaultSectionHasher,
> {
    fonts: &'brush Vec<F>,
    glyph_cache: MutexGuard<'brush, FxHashMap<u64, GlyphedSection<X>>>,
    cached: FxHashSet<u64>,
    section_hasher: H,
}

impl<F: Font, X: Clone + Hash, H: BuildHasher> GlyphCalculatorGuard<'_, F, X, H> {
    /// Returns the calculate_glyph_cache key for this sections glyphs
    fn cache_glyphs<L>(&mut self, section: &Section<'_, X>, layout: &L) -> u64
    where
        L: GlyphPositioner,
    {
        let section_hash = {
            let mut hasher = self.section_hasher.build_hasher();
            section.hash(&mut hasher);
            layout.hash(&mut hasher);
            hasher.finish()
        };

        if let Entry::Vacant(entry) = self.glyph_cache.entry(section_hash) {
            let geometry = SectionGeometry::from(section);
            let glyphs = layout.calculate_glyphs(self.fonts, &geometry, &section.text);

            entry.insert(GlyphedSection {
                bounds: layout.bounds_rect(&geometry),
                glyphs,
                extra: section.text.iter().map(|t| t.extra.clone()).collect(),
            });
        }

        section_hash
    }
}

impl<F: Font, X, H: BuildHasher> fmt::Debug for GlyphCalculatorGuard<'_, F, X, H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GlyphCalculatorGuard")
    }
}

impl<F: Font, X: Clone + Hash, H: BuildHasher> GlyphCruncher<F, X>
    for GlyphCalculatorGuard<'_, F, X, H>
{
    fn glyphs_custom_layout<'a, 'b, S, L>(
        &'b mut self,
        section: S,
        custom_layout: &L,
    ) -> SectionGlyphIter<'b>
    where
        X: 'a,
        L: GlyphPositioner + Hash,
        S: Into<Cow<'a, Section<'a, X>>>,
    {
        let section_hash = self.cache_glyphs(&section.into(), custom_layout);
        self.cached.insert(section_hash);
        self.glyph_cache[&section_hash].glyphs()
    }

    fn glyph_bounds_custom_layout<'a, S, L>(
        &mut self,
        section: S,
        custom_layout: &L,
    ) -> Option<Rect>
    where
        X: 'a,
        L: GlyphPositioner + Hash,
        S: Into<Cow<'a, Section<'a, X>>>,
    {
        let section = section.into();
        let geometry = SectionGeometry::from(section.as_ref());

        let section_hash = self.cache_glyphs(&section, custom_layout);
        self.cached.insert(section_hash);

        self.glyph_cache[&section_hash]
            .glyphs()
            .fold(None, |b: Option<Rect>, sg| {
                let sfont = self.fonts[sg.font_id.0].as_scaled(sg.glyph.scale);
                let pos = sg.glyph.position;
                let lbound = Rect {
                    min: point(
                        pos.x - sfont.h_side_bearing(sg.glyph.id),
                        pos.y - sfont.ascent(),
                    ),
                    max: point(
                        pos.x + sfont.h_advance(sg.glyph.id),
                        pos.y - sfont.descent(),
                    ),
                };
                b.map(|b| {
                    let min_x = b.min.x.min(lbound.min.x);
                    let max_x = b.max.x.max(lbound.max.x);
                    let min_y = b.min.y.min(lbound.min.y);
                    let max_y = b.max.y.max(lbound.max.y);
                    Rect {
                        min: point(min_x, min_y),
                        max: point(max_x, max_y),
                    }
                })
                .or(Some(lbound))
            })
            .map(|mut b| {
                // cap the glyph bounds to the layout specified max bounds
                let Rect { min, max } = custom_layout.bounds_rect(&geometry);
                b.min.x = b.min.x.max(min.x);
                b.min.y = b.min.y.max(min.y);
                b.max.x = b.max.x.min(max.x);
                b.max.y = b.max.y.min(max.y);
                b
            })
    }

    #[inline]
    fn fonts(&self) -> &[F] {
        self.fonts
    }
}

impl<F, X, H> Drop for GlyphCalculatorGuard<'_, F, X, H> {
    fn drop(&mut self) {
        let cached = mem::take(&mut self.cached);
        self.glyph_cache.retain(|key, _| cached.contains(key));
    }
}

/// Builder for a [`GlyphCalculator`](struct.GlyphCalculator.html).
///
/// # Example
///
/// use glyph_brush::{ab_glyph::FontArc, GlyphCalculator, GlyphCalculatorBuilder};
///
/// let dejavu = FontArc::try_from_slice(include_bytes!("../../fonts/DejaVuSans.ttf")).unwrap();
/// let mut glyphs: GlyphCalculator = GlyphCalculatorBuilder::using_font(dejavu).build();
#[derive(Debug, Clone)]
pub struct GlyphCalculatorBuilder<F = FontArc, H = DefaultSectionHasher> {
    font_data: Vec<F>,
    section_hasher: H,
}

impl<F: Font> GlyphCalculatorBuilder<F> {
    /// Specifies the default font used to render glyphs.
    /// Referenced with `FontId(0)`, which is default.
    pub fn using_font(font: F) -> Self {
        Self::using_fonts(vec![font])
    }

    pub fn using_fonts(fonts: Vec<F>) -> Self {
        GlyphCalculatorBuilder {
            font_data: fonts,
            section_hasher: DefaultSectionHasher::default(),
        }
    }
}

impl<F: Font, H: BuildHasher> GlyphCalculatorBuilder<F, H> {
    /// Adds additional fonts to the one added in [`using_font`](#method.using_font).
    ///
    /// Returns a [`FontId`](struct.FontId.html) to reference this font.
    pub fn add_font<I: Into<F>>(&mut self, font_data: I) -> FontId {
        self.font_data.push(font_data.into());
        FontId(self.font_data.len() - 1)
    }

    /// Sets the section hasher. `GlyphCalculator` cannot handle absolute section hash collisions
    /// so use a good hash algorithm.
    ///
    /// This hasher is used to distinguish sections, rather than for hashmap internal use.
    ///
    /// Defaults to [xxHash](https://docs.rs/twox-hash).
    pub fn section_hasher<T: BuildHasher>(
        self,
        section_hasher: T,
    ) -> GlyphCalculatorBuilder<F, T> {
        GlyphCalculatorBuilder {
            font_data: self.font_data,
            section_hasher,
        }
    }

    /// Builds a `GlyphCalculator`
    pub fn build<X>(self) -> GlyphCalculator<F, X, H> {
        GlyphCalculator {
            fonts: self.font_data,
            calculate_glyph_cache: Mutex::default(),
            section_hasher: self.section_hasher,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GlyphedSection<X> {
    pub bounds: Rect,
    pub glyphs: Vec<SectionGlyph>,
    pub extra: Vec<X>,
}

impl<X> GlyphedSection<X> {
    #[inline]
    pub(crate) fn glyphs(&self) -> SectionGlyphIter<'_> {
        self.glyphs.iter()
    }
}

#[cfg(test)]
mod test {
    use crate::components::text::glyph::*;
    use approx::*;
    use std::f32;
    use std::sync::LazyLock;

    static MONO_FONT: LazyLock<FontArc> = LazyLock::new(|| {
        FontArc::try_from_slice(include_bytes!(
            "../../../../resources/test-fonts/DejaVuSansMono.ttf"
        ) as &[u8])
        .unwrap()
    });
    static OPEN_SANS_LIGHT: LazyLock<FontArc> = LazyLock::new(|| {
        FontArc::try_from_slice(include_bytes!(
            "../../../../resources/test-fonts/OpenSans-Light.ttf"
        ) as &[u8])
        .unwrap()
    });

    #[test]
    fn glyph_bounds() {
        let glyphs = GlyphCalculatorBuilder::using_font(MONO_FONT.clone()).build();
        let mut glyphs = glyphs.cache_scope();

        let scale = PxScale::from(16.0);
        let section = Section::default()
            .add_text(Text::new("Hello World").with_scale(scale))
            .with_screen_position((0.0, 0.0));

        let g_bounds = glyphs.glyph_bounds(&section).expect("None bounds");

        for sg in glyphs.glyphs(&section) {
            eprintln!("{:?}", sg.glyph.position);
        }

        let sfont = MONO_FONT.as_scaled(scale);
        assert_relative_eq!(g_bounds.min.y, 0.0);
        assert_relative_eq!(g_bounds.max.y, sfont.ascent() - sfont.descent());

        // no left-side bearing expected
        assert_relative_eq!(g_bounds.min.x, 0.0);

        // the width should be to 11 * any glyph advance width as this font is monospaced
        let g_width = sfont.h_advance(MONO_FONT.glyph_id('W'));
        assert_relative_eq!(g_bounds.max.x, g_width * 11.0, epsilon = f32::EPSILON);
    }

    #[test]
    fn glyph_bounds_respect_layout_bounds() {
        let glyphs = GlyphCalculatorBuilder::using_font(MONO_FONT.clone()).build();
        let mut glyphs = glyphs.cache_scope();

        let section = Section::default()
            .add_text(Text::new("Hello\nWorld").with_scale(16.0))
            .with_screen_position((0.0, 20.0))
            .with_bounds((f32::INFINITY, 20.0));

        let g_bounds = glyphs.glyph_bounds(&section).expect("None bounds");
        let bounds_rect = Layout::default().bounds_rect(&SectionGeometry::from(&section));

        assert!(
            bounds_rect.min.y <= g_bounds.min.y,
            "expected {} <= {}",
            bounds_rect.min.y,
            g_bounds.min.y
        );

        assert!(
            bounds_rect.max.y >= g_bounds.max.y,
            "expected {} >= {}",
            bounds_rect.max.y,
            g_bounds.max.y
        );
    }

    #[test]
    fn glyphed_section_eq() {
        let glyph = MONO_FONT
            .glyph_id('a')
            .with_scale_and_position(16.0, point(50.0, 60.0));
        let color = [1.0, 0.9, 0.8, 0.7];

        let a = GlyphedSection {
            bounds: Rect {
                min: point(1.0, 2.0),
                max: point(300.0, 400.0),
            },
            glyphs: vec![SectionGlyph {
                section_index: 0,
                byte_index: 0,
                glyph: glyph.clone(),
                font_id: FontId(0),
            }],
            extra: vec![Extra { color, z: 0.444 }],
        };
        let mut b = GlyphedSection {
            bounds: Rect {
                min: point(1.0, 2.0),
                max: point(300.0, 400.0),
            },
            glyphs: vec![SectionGlyph {
                section_index: 0,
                byte_index: 0,
                glyph,
                font_id: FontId(0),
            }],
            extra: vec![Extra { color, z: 0.444 }],
        };

        assert_eq!(a, b);

        b.glyphs[0].glyph.position = point(50.0, 61.0);

        assert_ne!(a, b);
    }

    /// Issue #87
    #[test]
    fn glyph_bound_section_bound_consistency() {
        let calc = GlyphCalculatorBuilder::using_font(OPEN_SANS_LIGHT.clone()).build();
        let mut calc = calc.cache_scope();

        let section = Section::default()
            .add_text(Text::new("Eins Zwei Drei Vier Funf ").with_scale(20.0));

        let glyph_bounds = calc.glyph_bounds(&section).expect("None bounds");
        let glyphs: Vec<_> = calc.glyphs(&section).cloned().collect();

        // identical section with bounds that should be wide enough
        let bounded_section =
            section.with_bounds((glyph_bounds.width(), glyph_bounds.height()));

        let bounded_glyphs: Vec<_> = calc.glyphs(&bounded_section).collect();

        assert_eq!(glyphs.len(), bounded_glyphs.len());

        for (sg, bounded_sg) in glyphs.iter().zip(bounded_glyphs.into_iter()) {
            assert_relative_eq!(sg.glyph.position.x, bounded_sg.glyph.position.x);
            assert_relative_eq!(sg.glyph.position.y, bounded_sg.glyph.position.y);
        }
    }

    /// Issue #87
    #[test]
    fn glyph_bound_section_bound_consistency_trailing_space() {
        let calc = GlyphCalculatorBuilder::using_font(OPEN_SANS_LIGHT.clone()).build();
        let mut calc = calc.cache_scope();

        let section = Section::default()
            .add_text(Text::new("Eins Zwei Drei Vier Funf ").with_scale(20.0));

        let glyph_bounds = calc.glyph_bounds(&section).expect("None bounds");
        let glyphs: Vec<_> = calc.glyphs(&section).cloned().collect();

        // identical section with bounds that should be wide enough
        let bounded_section =
            section.with_bounds((glyph_bounds.width(), glyph_bounds.height()));

        let bounded_glyphs: Vec<_> = calc.glyphs(&bounded_section).collect();

        assert_eq!(glyphs.len(), bounded_glyphs.len());

        for (sg, bounded_sg) in glyphs.iter().zip(bounded_glyphs.into_iter()) {
            assert_relative_eq!(sg.glyph.position.x, bounded_sg.glyph.position.x);
            assert_relative_eq!(sg.glyph.position.y, bounded_sg.glyph.position.y);
        }
    }

    /// Similar to `glyph_bound_section_bound_consistency` but produces a floating point
    /// error between the calculated glyph_bounds bounds & those used during layout.
    #[test]
    fn glyph_bound_section_bound_consistency_floating_point() {
        let calc = GlyphCalculatorBuilder::using_font(MONO_FONT.clone()).build();
        let mut calc = calc.cache_scope();

        let section = Section::default().add_text(Text::new("Eins Zwei Drei Vier Funf"));

        let glyph_bounds = calc.glyph_bounds(&section).expect("None bounds");
        let glyphs: Vec<_> = calc.glyphs(&section).cloned().collect();

        // identical section with bounds that should be wide enough
        let bounded_section =
            section.with_bounds((glyph_bounds.width(), glyph_bounds.height()));
        let bounded_glyphs: Vec<_> = calc.glyphs(&bounded_section).collect();

        assert_eq!(glyphs.len(), bounded_glyphs.len());

        for (sg, bounded_sg) in glyphs.iter().zip(bounded_glyphs.into_iter()) {
            assert_relative_eq!(sg.glyph.position.x, bounded_sg.glyph.position.x);
            assert_relative_eq!(sg.glyph.position.y, bounded_sg.glyph.position.y);
        }
    }
}
