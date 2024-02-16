// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// builder.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

//! Paragraph builder.

use super::bidi::*;
use super::builder_data::*;
use super::span_style::*;
use super::Paragraph;
use super::{SpanId, MAX_ID};
use crate::font::{FontData, FontLibrary};
use core::borrow::Borrow;
use swash::shape::{self, ShapeContext};
use swash::text::cluster::{CharCluster, CharInfo, Parser, Token};
use swash::text::{analyze, Language, Script};
use swash::{Setting, Synthesis};

/// Context for paragraph layout.
pub struct LayoutContext {
    fcx: FontLibrary,
    bidi: BidiResolver,
    scx: ShapeContext,
    state: BuilderState,
}

impl LayoutContext {
    /// Creates a new layout context with the specified font library.
    pub fn new(fcx: FontLibrary) -> Self {
        Self {
            fcx,
            bidi: BidiResolver::new(),
            scx: ShapeContext::new(),
            state: BuilderState::new(),
        }
    }

    #[inline]
    pub fn font_library(&self) -> &FontLibrary {
        &self.fcx
    }

    /// Creates a new builder for computing a paragraph layout with the
    /// specified direction, language and scaling factor.
    pub fn builder(
        &mut self,
        direction: Direction,
        language: Option<Language>,
        scale: f32,
    ) -> ParagraphBuilder {
        self.state.clear();
        self.state.begin(direction, language, scale);
        self.state.scale = scale;
        ParagraphBuilder {
            fcx: &mut self.fcx,
            bidi: &mut self.bidi,
            needs_bidi: false,
            dir: direction,
            scale,
            scx: &mut self.scx,
            dir_depth: 0,
            s: &mut self.state,
            last_offset: 0,
        }
    }
}

/// Builder for computing the layout of a paragraph.
pub struct ParagraphBuilder<'a> {
    fcx: &'a mut FontLibrary,
    bidi: &'a mut BidiResolver,
    needs_bidi: bool,
    dir: Direction,
    scale: f32,
    scx: &'a mut ShapeContext,
    dir_depth: u32,
    s: &'a mut BuilderState,
    last_offset: u32,
}

impl<'a> ParagraphBuilder<'a> {
    /// Enters a new span with the specified styles.
    pub fn push_span<'p, I>(&mut self, styles: I) -> Option<SpanId>
    where
        I: IntoIterator,
        I::Item: Borrow<SpanStyle<'p>>,
    {
        // let (id, dir) = self.s.push(self.fcx, self.scale, styles)?;
        let (id, dir) = self.s.push(self.scale, styles)?;
        if let Some(dir) = dir {
            const LRI: char = '\u{2066}';
            const RLI: char = '\u{2067}';
            const FSI: char = '\u{2068}';
            match dir {
                Direction::Auto => self.push_char(FSI),
                Direction::LeftToRight => self.push_char(LRI),
                Direction::RightToLeft => self.push_char(RLI),
            }
            self.dir_depth += 1;
        }
        Some(id)
    }

    /// Pops the current span, restoring the styles of the parent.
    pub fn pop_span(&mut self) {
        if let Some((_, dir_changed)) = self.s.pop() {
            if dir_changed {
                const PDI: char = '\u{2069}';
                self.dir_depth = self.dir_depth.saturating_sub(1);
                self.push_char(PDI);
            }
        }
    }

    /// Adds a text fragment to the paragraph.
    pub fn add_text(&mut self, text: &str) -> Option<()> {
        let id = self.s.fragments.len() as u32;
        if id > MAX_ID {
            return None;
        }
        let span_id = *self.s.span_stack.last()?;
        let span = self.s.spans.get(span_id.to_usize())?;
        let mut offset = self.last_offset;
        macro_rules! push_char {
            ($ch: expr) => {{
                self.s.text.push($ch);
                self.s.text_offsets.push(offset);
                offset += ($ch).len_utf8() as u32;
            }};
        }
        let start = self.s.text.len();
        match span.text_transform {
            TextTransform::Uppercase => {
                if let Some(lang) = &span.lang {
                    match lang.language() {
                        "tr" | "az" | "crh" | "tt" | "ba" => {
                            for ch in text.chars() {
                                match ch {
                                    'i' => push_char!('İ'),
                                    'ı' => push_char!('I'),
                                    _ => {
                                        for ch in ch.to_uppercase() {
                                            push_char!(ch);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            for ch in text.chars() {
                                for ch in ch.to_uppercase() {
                                    push_char!(ch);
                                }
                            }
                        }
                    }
                } else {
                    for ch in text.chars() {
                        for ch in ch.to_uppercase() {
                            push_char!(ch);
                        }
                    }
                }
            }
            TextTransform::Lowercase => {
                let mut iter = text.chars().peekable();
                while let Some(ch) = iter.next() {
                    if ch == 'Σ' {
                        match iter.peek() {
                            Some(ch) => {
                                if ch.is_alphanumeric() || *ch == '-' {
                                    push_char!('σ');
                                } else {
                                    push_char!('ς');
                                }
                            }
                            None => {
                                push_char!('ς');
                            }
                        }
                    } else {
                        for ch in ch.to_lowercase() {
                            push_char!(ch);
                        }
                    }
                }
            }
            TextTransform::Capitalize => {
                let is_turkic = if let Some(lang) = &span.lang {
                    matches!(lang.language(), "tr" | "az" | "crh" | "tt" | "ba")
                } else {
                    false
                };
                let mut cap_next = true;
                for ch in text.chars() {
                    if !ch.is_alphabetic() {
                        // if ch.is_whitespace() || ch == '፡' {
                        cap_next = true;
                        push_char!(ch);
                    } else if cap_next {
                        if !ch.is_alphabetic() {
                            push_char!(ch);
                            continue;
                        }
                        if is_turkic {
                            match ch {
                                'i' => push_char!('İ'),
                                'ı' => push_char!('I'),
                                _ => {
                                    for ch in ch.to_uppercase() {
                                        push_char!(ch);
                                    }
                                }
                            }
                        } else {
                            for ch in ch.to_uppercase() {
                                push_char!(ch);
                            }
                        }
                        cap_next = false;
                    } else {
                        push_char!(ch);
                    }
                }
            }
            _ => {
                for ch in text.chars() {
                    push_char!(ch);
                }
            }
        }
        let end = self.s.text.len();
        let break_shaping = if let Some(prev_frag) = self.s.fragments.last() {
            if prev_frag.is_text {
                if prev_frag.span == span_id {
                    false
                } else {
                    let s = self.s.spans.get(prev_frag.span.to_usize())?;
                    s.font_size != span.font_size
                        || s.letter_spacing != span.letter_spacing
                        || s.lang != span.lang
                        // || s.font != span.font
                        || s.font_features != span.font_features
                        || s.font_vars != span.font_vars
                }
            } else {
                true
            }
        } else {
            true
        };
        let len = end - start;
        self.s.text_frags.reserve(len);
        for _ in 0..len {
            self.s.text_frags.push(id);
        }
        self.s.text_spans.reserve(len);
        for _ in 0..len {
            self.s.text_spans.push(span_id.0);
        }
        self.s.fragments.push(FragmentData {
            span: span_id,
            is_text: true,
            break_shaping,
            start,
            end,
            font: span.font,
            features: span.font_features,
            vars: span.font_vars,
        });
        self.last_offset = offset;
        Some(())
    }

    /// Consumes the builder and fills the specified paragraph with the result.
    pub fn build_into(mut self, para: &mut Paragraph) {
        self.resolve(para);
        para.finish();
        // self.fcx.reset_group_state();
    }

    /// Consumes the builder and returns the resulting paragraph.
    pub fn build(self) -> Paragraph {
        let mut para = Paragraph::default();
        self.build_into(&mut para);
        para
    }
}

impl<'a> ParagraphBuilder<'a> {
    fn resolve(&mut self, layout: &mut Paragraph) {
        // Bit of a hack: add a single trailing space fragment to account for
        // empty paragraphs and to force an extra break if the paragraph ends
        // in a newline.
        self.s
            .span_stack
            .push(SpanId(self.s.spans.len() as u32 - 1));
        self.add_text(" ");
        for _ in 0..self.dir_depth {
            const PDI: char = '\u{2069}';
            self.push_char(PDI);
        }
        let mut analysis = analyze(self.s.text.iter());
        for (props, boundary) in analysis.by_ref() {
            self.s.text_info.push(CharInfo::new(props, boundary));
        }
        if analysis.needs_bidi_resolution() || self.dir != Direction::LeftToRight {
            let dir = match self.dir {
                Direction::Auto => None,
                Direction::LeftToRight => Some(BidiDirection::LeftToRight),
                Direction::RightToLeft => Some(BidiDirection::RightToLeft),
            };
            self.bidi.resolve_with_types(
                &self.s.text,
                self.s.text_info.iter().map(|i| i.bidi_class()),
                dir,
            );
            self.needs_bidi = true;
        }
        self.itemize();
        self.shape(layout);
    }

    fn itemize(&mut self) {
        let limit = self.s.text.len();
        if self.s.fragments.is_empty() || limit == 0 {
            return;
        }
        let mut last_script = self
            .s
            .text_info
            .iter()
            .map(|i| i.script())
            .find(|s| real_script(*s))
            .unwrap_or(Script::Latin);
        let levels = self.bidi.levels();
        let mut last_frag = self.s.fragments.first().unwrap();
        let mut last_level = if self.needs_bidi {
            levels[last_frag.start]
        } else {
            0
        };
        let mut last_features = last_frag.features;
        let mut last_vars = last_frag.vars;
        let mut item = ItemData {
            script: last_script,
            level: last_level,
            start: last_frag.start,
            end: last_frag.start,
            features: last_features,
            vars: last_vars,
        };
        macro_rules! push_item {
            () => {
                if item.start < limit && item.start < item.end {
                    item.script = last_script;
                    item.level = last_level;
                    item.vars = last_vars;
                    item.features = last_features;
                    self.s.items.push(item);
                    item.start = item.end;
                }
            };
        }
        if self.needs_bidi {
            for frag in &self.s.fragments {
                if frag.break_shaping || frag.start != last_frag.end {
                    push_item!();
                    item.start = frag.start;
                    item.end = frag.start;
                }
                last_frag = frag;
                last_features = frag.features;
                last_vars = frag.vars;
                let range = frag.start..frag.end;
                for (&props, &level) in
                    self.s.text_info[range.clone()].iter().zip(&levels[range])
                {
                    let script = props.script();
                    let real = real_script(script);
                    if (script != last_script && real) || level != last_level {
                        //item.end += 1;
                        push_item!();
                        if real {
                            last_script = script;
                        }
                        last_level = level;
                    }
                    item.end += 1;
                }
            }
        } else {
            for frag in &self.s.fragments {
                if frag.break_shaping || frag.start != last_frag.end {
                    push_item!();
                    item.start = frag.start;
                    item.end = frag.start;
                }
                last_frag = frag;
                last_features = frag.features;
                last_vars = frag.vars;
                let range = frag.start..frag.end;
                for &props in &self.s.text_info[range] {
                    let script = props.script();
                    let real = real_script(script);
                    if script != last_script && real {
                        //item.end += 1;
                        push_item!();
                        if real {
                            last_script = script;
                        }
                    } else {
                        item.end += 1;
                    }
                }
            }
        }
        push_item!();
    }

    fn shape(&mut self, layout: &mut Paragraph) {
        let start = std::time::Instant::now();
        let mut char_cluster = CharCluster::new();
        for item in &self.s.items {
            shape_item(self.fcx, self.scx, self.s, item, &mut char_cluster, layout);
        }
        layout.apply_spacing(&self.s.spans);
        let duration = start.elapsed();
        println!("Time elapsed in shape is: {:?}", duration);
    }
}

impl<'a> ParagraphBuilder<'a> {
    #[inline]
    fn push_char(&mut self, ch: char) {
        self.s.text.push(ch);
        self.s.text_frags.push(0);
        self.s.text_spans.push(0);
        self.s.text_offsets.push(0);
    }
}

#[inline]
fn real_script(script: Script) -> bool {
    script != Script::Common && script != Script::Inherited && script != Script::Unknown
}

struct ShapeState<'a> {
    state: &'a BuilderState,
    features: &'a [Setting<u16>],
    synth: Synthesis,
    vars: &'a [Setting<f32>],
    script: Script,
    level: u8,
    span_index: u32,
    span: &'a SpanData,
    font_id: Option<usize>,
    size: f32,
}

#[inline]
fn shape_item(
    fcx: &mut FontLibrary,
    scx: &mut ShapeContext,
    state: &BuilderState,
    item: &ItemData,
    cluster: &mut CharCluster,
    layout: &mut Paragraph,
) -> Option<()> {
    let dir = if item.level & 1 != 0 {
        shape::Direction::RightToLeft
    } else {
        shape::Direction::LeftToRight
    };
    let range = item.start..item.end;
    let span_index = state.text_spans[item.start];
    let span = state.spans.get(span_index as usize)?;
    let features = state.features.get(item.features);
    let vars = state.vars.get(item.vars);
    let mut shape_state = ShapeState {
        script: item.script,
        level: item.level,
        features,
        vars,
        synth: Synthesis::default(),
        state,
        span_index,
        span,
        font_id: None,
        size: span.font_size,
    };
    // fcx.select_group(shape_state.font_id);
    // fcx.select_fallbacks(item.script, shape_state.span.lang.as_ref());
    if item.level & 1 != 0 {
        let chars = state.text[range.clone()]
            .iter()
            .zip(&state.text_offsets[range.clone()])
            .zip(&state.text_spans[range.clone()])
            .zip(&state.text_info[range])
            .map(|z| {
                use swash::text::Codepoint;
                let (((&ch, &offset), &span_index), &info) = z;
                let ch = ch.mirror().unwrap_or(ch);
                Token {
                    ch,
                    offset,
                    len: ch.len_utf8() as u8,
                    info,
                    data: span_index,
                }
            });

        let mut parser = Parser::new(item.script, chars);
        if !parser.next(cluster) {
            return Some(());
        }
        // fcx[shape_state.font_id].map_cluster(cluster, &mut shape_state.synth);
        shape_state.font_id = fcx.map_cluster(cluster, &mut shape_state.synth);
        while shape_clusters(
            fcx,
            scx,
            &mut shape_state,
            &mut parser,
            cluster,
            dir,
            layout,
        ) {}
    } else {
        let chars = state.text[range.clone()]
            .iter()
            .zip(&state.text_offsets[range.clone()])
            .zip(&state.text_spans[range.clone()])
            .zip(&state.text_info[range])
            .map(|z| {
                let (((&ch, &offset), &span_index), &info) = z;
                Token {
                    ch,
                    offset,
                    len: ch.len_utf8() as u8,
                    info,
                    data: span_index,
                }
            });

        let mut parser = Parser::new(item.script, chars);
        if !parser.next(cluster) {
            return Some(());
        }
        // fcx[shape_state.font_id].map_cluster(cluster, &mut shape_state.synth);
        shape_state.font_id = fcx.map_cluster(cluster, &mut shape_state.synth);
        while shape_clusters(
            fcx,
            scx,
            &mut shape_state,
            &mut parser,
            cluster,
            dir,
            layout,
        ) {}
    }
    Some(())
}

#[inline]
fn shape_clusters<I>(
    fcx: &mut FontLibrary,
    scx: &mut ShapeContext,
    state: &mut ShapeState,
    parser: &mut Parser<I>,
    cluster: &mut CharCluster,
    dir: shape::Direction,
    layout: &mut Paragraph,
) -> bool
where
    I: Iterator<Item = Token> + Clone,
{
    if state.font_id.is_none() {
        return false;
    }

    let current_font_id = state.font_id.unwrap();
    let font = fcx[current_font_id].clone();
    let mut shaper = scx
        .builder(font.as_ref())
        .script(state.script)
        .language(state.span.lang)
        .direction(dir)
        .size(state.size)
        .features(state.features.iter().copied())
        .variations(state.synth.variations().iter().copied())
        .variations(state.vars.iter().copied())
        .build();

    let mut synth = Synthesis::default();
    loop {
        shaper.add_cluster(cluster);
        if !parser.next(cluster) {
            layout.push_run(
                &state.state.spans,
                &current_font_id,
                state.size,
                state.level,
                shaper,
            );
            return false;
        }
        let cluster_span = cluster.user_data();
        if cluster_span != state.span_index {
            state.span_index = cluster_span;
            state.span = state.state.spans.get(cluster_span as usize).unwrap();
            // TODO?: Fix state.span.font overwrite
            // if state.span.font != current_font_id {
            // state.font_id = Some(state.span.font);
            // }
            // fcx.select_group(state.font_id);
        }

        let next_font = fcx.map_cluster(cluster, &mut synth);
        if next_font != state.font_id || synth != state.synth {
            layout.push_run(
                &state.state.spans,
                &current_font_id,
                state.size,
                state.level,
                shaper,
            );
            state.font_id = next_font;
            state.synth = synth;
            return true;
        }
    }
}
