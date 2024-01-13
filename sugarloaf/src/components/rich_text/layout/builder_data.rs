use super::span_style::*;
use super::{
    font::internal::{FontContext, FontGroupId},
    SpanId, MAX_ID,
};
use core::borrow::Borrow;
use swash::text::{cluster::CharInfo, Script};
use swash::{Stretch, Style, Setting, Weight};

/// Data that describes a fragment.
#[derive(Copy, Clone)]
pub struct FragmentData {
    /// Identifier of the span that contains the fragment.
    pub span: SpanId,
    /// True if this fragment breaks shaping with the previous fragment.
    pub break_shaping: bool,
    /// True if this fragment is text.
    pub is_text: bool,
    /// Offset of the text.
    pub start: usize,
    /// End of the text.
    pub end: usize,
    /// Internal identifier for a list of font families and attributes.
    pub font: FontGroupId,
    /// Font features.
    pub features: FontSettingKey,
    /// Font variations.
    pub vars: FontSettingKey,
}

/// Data that describes an item.
#[derive(Copy, Clone)]
pub struct ItemData {
    /// Script of the item.
    pub script: Script,
    /// Bidi level of the item.
    pub level: u8,
    /// Offset of the text.
    pub start: usize,
    /// End of the text.
    pub end: usize,
    /// Font features.
    pub features: FontSettingKey,
    /// Font variations.
    pub vars: FontSettingKey,
}

/// Data that describes a span.
#[derive(Clone)]
pub struct SpanData {
    /// Identifier of the span.
    pub id: SpanId,
    /// Identifier of the parent span.
    pub parent: Option<SpanId>,
    /// Identifier of first child of the span.
    pub first_child: Option<SpanId>,
    /// Identifier of last child of the span.
    pub last_child: Option<SpanId>,
    /// Identifier of next sibling of the span.
    pub next: Option<SpanId>,
    /// Text direction.
    pub dir: Direction,
    /// Is the direction different from the parent?
    pub dir_changed: bool,
    /// Text language.
    pub lang: Option<Language>,
    /// Internal identifier for a list of font families and attributes.
    pub font: FontGroupId,
    /// Font family.
    pub font_family: FamilyList,
    /// Font attributes.
    pub font_attrs: (Stretch, Weight, Style),
    /// Font size in ppem.
    pub font_size: f32,
    /// Font features.
    pub font_features: FontSettingKey,
    /// Font variations.
    pub font_vars: FontSettingKey,
    /// Additional spacing between letters (clusters) of text.
    pub letter_spacing: f32,
    /// Additional spacing between words of text.
    pub word_spacing: f32,
    /// Multiplicative line spacing factor.
    pub line_spacing: f32,
    /// Enable underline decoration.
    pub underline: bool,
    /// Offset of an underline.
    pub underline_offset: Option<f32>,
    /// Thickness of an underline.
    pub underline_size: Option<f32>,
    /// Text case transformation.
    pub text_transform: TextTransform,
}

/// Builder state.
#[derive(Default)]
pub struct BuilderState {
    /// Combined text.
    pub text: Vec<char>,
    /// Fragment index per character.
    pub text_frags: Vec<u32>,
    /// Span index per character.
    pub text_spans: Vec<u32>,
    /// Character info per character.
    pub text_info: Vec<CharInfo>,
    /// Offset of each character relative to its fragment.
    pub text_offsets: Vec<u32>,
    /// Collection of all spans, in order of span identifier.
    pub spans: Vec<SpanData>,
    /// Stack of spans.
    pub span_stack: Vec<SpanId>,
    /// Font feature setting cache.
    pub features: FontSettingCache<u16>,
    /// Font variation setting cache.
    pub vars: FontSettingCache<f32>,
    /// Collection of fragments.
    pub fragments: Vec<FragmentData>,
    /// Collection of items.
    pub items: Vec<ItemData>,
    /// User specified scale.
    pub scale: f32,
}

impl BuilderState {
    /// Creates a new layout state.
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.text_frags.clear();
        self.text_spans.clear();
        self.text_info.clear();
        self.text_offsets.clear();
        self.spans.clear();
        self.span_stack.clear();
        self.features.clear();
        self.vars.clear();
        self.fragments.clear();
        self.items.clear();
    }

    pub fn begin(&mut self, dir: Direction, lang: Option<Language>, scale: f32) {
        self.spans.push(SpanData {
            id: SpanId(0),
            parent: None,
            first_child: None,
            last_child: None,
            next: None,
            dir,
            dir_changed: false,
            lang,
            font: FontGroupId(!0),
            font_family: FamilyList::new(""),
            font_attrs: (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
            font_size: 16. * scale,
            font_features: EMPTY_FONT_SETTINGS,
            font_vars: EMPTY_FONT_SETTINGS,
            letter_spacing: 0.,
            word_spacing: 0.,
            line_spacing: 1.,
            underline: false,
            underline_offset: None,
            underline_size: None,
            text_transform: TextTransform::None,
        });
        self.span_stack.push(SpanId(0));
    }

    /// Pushes a new span with the specified properties. Returns the new
    /// span identifier and a value indicating a new direction, if any.
    pub fn push<'a, I>(
        &mut self,
        fcx: &mut FontContext,
        scale: f32,
        styles: I,
    ) -> Option<(SpanId, Option<Direction>)>
    where
        I: IntoIterator,
        I::Item: Borrow<SpanStyle<'a>>,
    {
        let next_id = SpanId(self.spans.len() as u32);
        if next_id.0 > MAX_ID {
            return None;
        }
        let parent_id = *self.span_stack.last()?;
        let parent = self.spans.get_mut(parent_id.to_usize())?;
        let mut span = parent.clone();
        let last_child = if let Some(last_child) = parent.last_child {
            parent.last_child = Some(next_id);
            Some(last_child)
        } else {
            parent.first_child = Some(next_id);
            parent.last_child = Some(next_id);
            None
        };
        if let Some(last_child) = last_child {
            let prev_sibling = self.spans.get_mut(last_child.to_usize())?;
            prev_sibling.next = Some(next_id);
        }
        span.id = next_id;
        span.parent = Some(parent_id);
        span.dir_changed = false;
        let parent_dir = span.dir;
        let mut font_changed = false;
        for s in styles {
            use SpanStyle as S;
            match s.borrow() {
                S::Direction(dir) => {
                    if *dir != parent_dir {
                        span.dir = *dir;
                        span.dir_changed = true;
                    } else {
                        span.dir = *dir;
                        span.dir_changed = false;
                    }
                }
                S::Language(lang) => {
                    span.lang = Some(*lang);
                }
                S::FamilyList(families) => {
                    if families.key() != span.font_family.key() {
                        span.font_family = families.clone();
                        font_changed = true;
                    }
                }
                S::Stretch(value) => {
                    if *value != span.font_attrs.0 {
                        span.font_attrs.0 = *value;
                        font_changed = true;
                    }
                }
                S::Weight(value) => {
                    if *value != span.font_attrs.1 {
                        span.font_attrs.1 = *value;
                        font_changed = true;
                    }
                }
                S::Style(value) => {
                    if *value != span.font_attrs.2 {
                        span.font_attrs.2 = *value;
                        font_changed = true;
                    }
                }
                S::Size(size) => {
                    span.font_size = *size * scale;
                }
                S::Features(features) => {
                    span.font_features = self.features.add(features.iter().copied());
                }
                S::Variations(vars) => {
                    span.font_vars = self.vars.add(vars.iter().copied());
                }
                S::LetterSpacing(spacing) => {
                    span.letter_spacing = *spacing * scale;
                }
                S::WordSpacing(spacing) => {
                    span.word_spacing = *spacing * scale;
                }
                S::LineSpacing(spacing) => {
                    span.line_spacing = *spacing;
                }
                S::Underline(enable) => {
                    span.underline = *enable;
                }
                S::UnderlineOffset(offset) => {
                    span.underline_offset = (*offset).map(|o| o * scale);
                }
                S::UnderlineSize(size) => {
                    span.underline_size = (*size).map(|s| s * scale)
                }
                S::TextTransform(xform) => {
                    span.text_transform = *xform;
                }
            }
        }
        if font_changed {
            span.font = fcx.register_group(
                span.font_family.names(),
                span.font_family.key(),
                span.font_attrs.into(),
            );
        }
        let dir = if span.dir_changed {
            Some(span.dir)
        } else {
            None
        };
        self.spans.push(span);
        self.span_stack.push(next_id);
        Some((next_id, dir))
    }

    /// Pops the most recent span from the stack. Returns true if
    /// the direction changed.
    pub fn pop(&mut self) -> Option<(SpanId, bool)> {
        if self.span_stack.len() > 1 {
            let id = self.span_stack.pop().unwrap();
            Some((id, self.spans[id.to_usize()].dir_changed))
        } else {
            None
        }
    }
}

/// Index into a font setting cache.
pub type FontSettingKey = u32;

/// Sentinel for an empty set of font settings.
pub const EMPTY_FONT_SETTINGS: FontSettingKey = !0;

/// Cache of tag/value pairs for font settings.
#[derive(Default)]
pub struct FontSettingCache<T: Copy + PartialOrd + PartialEq> {
    settings: Vec<Setting<T>>,
    lists: Vec<FontSettingList>,
    tmp: Vec<Setting<T>>,
}

impl<T: Copy + PartialOrd + PartialEq> FontSettingCache<T> {
    pub fn add<I>(&mut self, settings: I) -> FontSettingKey
    where
        I: Iterator,
        I::Item: Into<Setting<T>>,
    {
        self.tmp.clear();
        self.tmp.extend(settings.map(|v| v.into()));
        let len = self.tmp.len();
        if len == 0 {
            return EMPTY_FONT_SETTINGS;
        }
        self.tmp.sort_unstable_by(|a, b| a.tag.cmp(&b.tag));
        'outer: for (i, list) in self.lists.iter().enumerate() {
            let other = list.get(&self.settings);
            if other.len() != len {
                continue;
            }
            for (a, b) in self.tmp.iter().zip(other) {
                if a.tag != b.tag || a.value != b.value {
                    continue 'outer;
                }
            }
            return i as u32;
        }
        let key = self.lists.len() as u32;
        let start = self.settings.len() as u32;
        self.settings.extend_from_slice(&self.tmp);
        let end = self.settings.len() as u32;
        self.lists.push(FontSettingList { start, end });
        key
    }

    pub fn get(&self, key: u32) -> &[Setting<T>] {
        if key == !0 {
            &[]
        } else {
            self.lists
                .get(key as usize)
                .map(|list| list.get(&self.settings))
                .unwrap_or(&[])
        }
    }

    pub fn clear(&mut self) {
        self.settings.clear();
        self.lists.clear();
        self.tmp.clear();
    }
}

/// Range within a font setting cache.
#[derive(Copy, Clone)]
struct FontSettingList {
    pub start: u32,
    pub end: u32,
}

impl FontSettingList {
    pub fn get<T>(self, elements: &Vec<T>) -> &[T] {
        elements
            .get(self.start as usize..self.end as usize)
            .unwrap_or(&[])
    }
}
