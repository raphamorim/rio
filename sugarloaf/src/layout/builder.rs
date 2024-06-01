// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// builder.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

//! Render data builder.

use super::bidi::*;
use super::builder_data::*;
use super::span_style::*;
use super::MAX_ID;
use crate::font::{FontContext, FontLibrary, FontLibraryData};
use crate::layout::render_data::{RenderData, RunCacheEntry};
use std::collections::HashMap;
use swash::shape::{self, ShapeContext};
use swash::text::cluster::{CharCluster, CharInfo, Parser, Token};
use swash::text::{analyze, Language, Script};
use swash::{Setting, Synthesis};

pub struct RunCache {
    inner: HashMap<u64, RunCacheEntry>,
}

impl RunCache {
    #[inline]
    fn new() -> Self {
        Self {
            inner: HashMap::default(),
        }
    }

    #[inline]
    fn insert(&mut self, line_hash: u64, data: RunCacheEntry) {
        if data.runs.is_empty() {
            return;
        }

        if let Some(line) = self.inner.get_mut(&line_hash) {
            *line = data;
        } else {
            self.inner.insert(line_hash, data);
        }
    }

    #[inline]
    fn clear_on_max_capacity(&mut self) {
        if self.inner.len() > 1024 {
            self.inner.clear();
        }
    }
}

/// Context for paragraph layout.
pub struct LayoutContext {
    fcx: FontContext,
    fonts: FontLibrary,
    bidi: BidiResolver,
    scx: ShapeContext,
    state: BuilderState,
    cache: RunCache,
}

impl LayoutContext {
    /// Creates a new layout context with the specified font library.
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            fonts: font_library.clone(),
            fcx: FontContext::default(),
            bidi: BidiResolver::new(),
            scx: ShapeContext::new(),
            state: BuilderState::new(),
            cache: RunCache::new(),
        }
    }

    #[inline]
    pub fn font_library(&self) -> &FontLibrary {
        &self.fonts
    }

    /// Creates a new builder for computing a paragraph layout with the
    /// specified direction, language and scaling factor.
    #[inline]
    pub fn builder(
        &mut self,
        direction: Direction,
        _language: Option<Language>,
        scale: f32,
    ) -> ParagraphBuilder {
        self.state.clear();
        self.state.begin();
        self.state.scale = scale;
        ParagraphBuilder {
            fcx: &mut self.fcx,
            bidi: &mut self.bidi,
            needs_bidi: false,
            dir: direction,
            fonts: &self.fonts,
            scx: &mut self.scx,
            s: &mut self.state,
            last_offset: 0,
            cache: &mut self.cache,
        }
    }

    #[inline]
    pub fn clear_cache(&mut self) {
        self.cache.inner.clear();
    }
}

/// Builder for computing the layout of a paragraph.
pub struct ParagraphBuilder<'a> {
    fcx: &'a mut FontContext,
    bidi: &'a mut BidiResolver,
    fonts: &'a FontLibrary,
    needs_bidi: bool,
    dir: Direction,
    scx: &'a mut ShapeContext,
    s: &'a mut BuilderState,
    last_offset: u32,
    cache: &'a mut RunCache,
}

impl<'a> ParagraphBuilder<'a> {
    /// Enters a new span with the specified styles.
    // pub fn push_span<'p, I>(&mut self, styles: I) -> Option<SpanId>
    // where
    //     I: IntoIterator,
    //     I::Item: Borrow<SpanStyle>,
    // {
    //     // let (id, dir) = self.s.push(self.fcx, self.scale, styles)?;
    //     let (id, dir) = self.s.push(self.scale, styles)?;
    //     if let Some(dir) = dir {
    //         const LRI: char = '\u{2066}';
    //         const RLI: char = '\u{2067}';
    //         const FSI: char = '\u{2068}';
    //         match dir {
    //             Direction::Auto => self.push_char(FSI),
    //             Direction::LeftToRight => self.push_char(LRI),
    //             Direction::RightToLeft => self.push_char(RLI),
    //         }
    //         self.dir_depth += 1;
    //     }
    //     Some(id)
    // }

    // /// Pops the current span, restoring the styles of the parent.
    // pub fn pop_span(&mut self) {
    //     if let Some((_, dir_changed)) = self.s.pop() {
    //         if dir_changed {
    //             const PDI: char = '\u{2069}';
    //             self.dir_depth = self.dir_depth.saturating_sub(1);
    //             self.push_char(PDI);
    //         }
    //     }
    // }

    #[inline]
    pub fn set_hash(&mut self, hash: u64) {
        if hash > 0 {
            let current_line = self.s.current_line();
            self.s.lines[current_line].hash = Some(hash);
        }
    }

    #[inline]
    pub fn new_line(&mut self) {
        self.s.new_line();
    }

    /// Adds a text fragment to the paragraph.
    pub fn add_text(&mut self, text: &str, mut style: FragmentStyle) -> Option<()> {
        let current_line = self.s.current_line();
        let line = &mut self.s.lines[current_line];
        let id = line.text.frags.len();
        if id > MAX_ID {
            return None;
        }

        let mut offset = self.last_offset;
        style.font_size *= self.s.scale;
        line.styles.push(style);
        let span_id = line.styles.len() - 1;

        // if let Some(dir) = style.dir {
        //     const LRI: char = '\u{2066}';
        //     const RLI: char = '\u{2067}';
        //     const FSI: char = '\u{2068}';
        //     match dir {
        //         Direction::Auto => self.push_char(FSI, style),
        //         Direction::LeftToRight => self.push_char(LRI, style),
        //         Direction::RightToLeft => self.push_char(RLI, style),
        //     }
        //     // self.dir_depth += 1;
        // }

        macro_rules! push_char {
            ($ch: expr) => {{
                line.text.content.push($ch);
                line.text.offsets.push(offset);
                offset += ($ch).len_utf8() as u32;
            }};
        }

        let start = line.text.content.len();
        match style.text_transform {
            TextTransform::Uppercase => {
                if let Some(lang) = &style.lang {
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
                let is_turkic = if let Some(lang) = &style.lang {
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
        let end = line.text.content.len();
        let break_shaping = if let Some(prev_frag) = line.fragments.last() {
            let prev_style = line.styles[prev_frag.span];
            if prev_style == style {
                false
            } else {
                style.font_size != prev_style.font_size
                    || style.letter_spacing != prev_style.letter_spacing
                    || style.lang != prev_style.lang
                    // || style.font != prev_stylefont
                    || style.font_features != prev_style.font_features
                    || style.font_vars != prev_style.font_vars
            }
        } else {
            true
        };

        let len = end - start;
        line.text.frags.reserve(len);
        for _ in 0..len {
            line.text.frags.push(id as u32);
        }

        line.text.spans.reserve(len);
        for _ in 0..len {
            line.text.spans.push(span_id);
        }
        line.fragments.push(FragmentData {
            span: span_id,
            break_shaping,
            start,
            end,
            font: style.font,
            features: style.font_features,
            vars: style.font_vars,
        });

        self.last_offset = offset;
        Some(())
    }

    /// Consumes the builder and fills the specified paragraph with the result.
    pub fn build_into(mut self, render_data: &mut RenderData) {
        self.resolve(render_data);
        render_data.finish();
    }

    /// Consumes the builder and returns the resulting paragraph.
    pub fn build(self) -> RenderData {
        let mut render_data = RenderData::default();
        self.build_into(&mut render_data);
        render_data
    }
}

impl<'a> ParagraphBuilder<'a> {
    #[inline]
    fn process_from_cache(
        &mut self,
        render_data: &mut RenderData,
        current_line: usize,
    ) -> bool {
        if let Some(line_hash) = self.s.lines[current_line].hash {
            if let Some(data) = self.cache.inner.get(&line_hash) {
                render_data.push_run_from_cached_line(data, current_line as u32);

                return true;
            }
        }

        false
    }

    fn resolve(&mut self, render_data: &mut RenderData) {
        // Bit of a hack: add a single trailing space fragment to account for
        // empty paragraphs and to force an extra break if the paragraph ends
        // in a newline.

        self.add_text(" ", FragmentStyle::default());
        // for _ in 0..self.dir_depth {
        // const PDI: char = '\u{2069}';
        // self.push_char(PDI);
        // }

        // Cache needs to be cleaned before build lines
        self.cache.clear_on_max_capacity();

        for line_number in 0..self.s.lines.len() {
            // In case should render only requested lines
            // and the line number isn't part of the requested then process from cache
            // if render_specific_lines && !lines_to_render.contains(&line_number) {
            if self.process_from_cache(render_data, line_number) {
                continue;
            }

            let line = &mut self.s.lines[line_number];
            let mut analysis = analyze(line.text.content.iter());
            for (props, boundary) in analysis.by_ref() {
                line.text.info.push(CharInfo::new(props, boundary));
            }
            if analysis.needs_bidi_resolution() || self.dir != Direction::LeftToRight {
                let dir = match self.dir {
                    Direction::Auto => None,
                    Direction::LeftToRight => Some(BidiDirection::LeftToRight),
                    Direction::RightToLeft => Some(BidiDirection::RightToLeft),
                };
                self.bidi.resolve_with_types(
                    &self.s.lines[line_number].text.content,
                    self.s.lines[line_number]
                        .text
                        .info
                        .iter()
                        .map(|i| i.bidi_class()),
                    dir,
                );
                if !self.needs_bidi {
                    self.needs_bidi = true;
                }
            }

            self.itemize(line_number);
            self.shape(render_data, line_number);
        }

        render_data.apply_spacing();
    }

    fn itemize(&mut self, line_number: usize) {
        let line = &mut self.s.lines[line_number];
        let limit = line.text.content.len();
        if line.text.frags.is_empty() || limit == 0 {
            return;
        }
        let mut last_script = line
            .text
            .info
            .iter()
            .map(|i| i.script())
            .find(|s| real_script(*s))
            .unwrap_or(Script::Latin);
        let levels = self.bidi.levels();
        let mut last_frag = line.fragments.first().unwrap();
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
                    line.items.push(item);
                    item.start = item.end;
                }
            };
        }
        if self.needs_bidi {
            for frag in &line.fragments {
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
                    line.text.info[range.clone()].iter().zip(&levels[range])
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
            for frag in &line.fragments {
                if frag.break_shaping || frag.start != last_frag.end {
                    push_item!();
                    item.start = frag.start;
                    item.end = frag.start;
                }
                last_frag = frag;
                last_features = frag.features;
                last_vars = frag.vars;
                let range = frag.start..frag.end;
                for &props in &line.text.info[range] {
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

    fn shape(&mut self, render_data: &mut RenderData, line_number: usize) {
        // let start = std::time::Instant::now();
        let mut char_cluster = CharCluster::new();
        let line = &self.s.lines[line_number];
        for item in &line.items {
            shape_item(
                self.fcx,
                self.fonts,
                self.scx,
                self.s,
                item,
                &mut char_cluster,
                render_data,
                line_number,
                self.cache,
            );
        }
        // let duration = start.elapsed();
        // println!("Time elapsed in shape is: {:?}", duration);
    }
}

// impl<'a> ParagraphBuilder<'a> {
//     #[inline]
//     fn push_char(&mut self, ch: char) {
//         let current_line = self.s.current_line();
//         self.s.lines[current_line].text.content.push(ch);
//         self.s.lines[current_line].text.frags.push(0);
//         self.s.lines[current_line].text.spans.push(0);
//         self.s.lines[current_line].text.offsets.push(0);
//     }
// }

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
    span: &'a FragmentStyle,
    font_id: Option<usize>,
    size: f32,
    span_index: usize,
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn shape_item(
    fcx: &mut FontContext,
    fonts: &FontLibrary,
    scx: &mut ShapeContext,
    state: &BuilderState,
    item: &ItemData,
    cluster: &mut CharCluster,
    render_data: &mut RenderData,
    current_line: usize,
    cache: &mut RunCache,
) -> Option<()> {
    let dir = if item.level & 1 != 0 {
        shape::Direction::RightToLeft
    } else {
        shape::Direction::LeftToRight
    };
    let range = item.start..item.end;
    let span_index = state.lines[current_line].text.spans[item.start];
    let style = state.lines[current_line].styles[span_index];
    let features = state.features.get(item.features);
    let vars = state.vars.get(item.vars);
    let mut shape_state = ShapeState {
        script: item.script,
        level: item.level,
        features,
        vars,
        synth: Synthesis::default(),
        state,
        span: &state.lines[current_line].styles[span_index],
        font_id: None,
        span_index,
        size: style.font_size,
    };

    if item.level & 1 != 0 {
        let chars = state.lines[current_line].text.content[range.clone()]
            .iter()
            .zip(&state.lines[current_line].text.offsets[range.clone()])
            .zip(&state.lines[current_line].text.spans[range.clone()])
            .zip(&state.lines[current_line].text.info[range])
            .map(|z| {
                use swash::text::Codepoint;
                let (((&ch, &offset), &span_index), &info) = z;
                let ch = ch.mirror().unwrap_or(ch);
                Token {
                    ch,
                    offset,
                    len: ch.len_utf8() as u8,
                    info,
                    data: span_index as u32,
                }
            });

        let mut parser = Parser::new(item.script, chars);
        if !parser.next(cluster) {
            return Some(());
        }
        let font_library = { &fonts.inner.read().unwrap() };
        shape_state.font_id =
            fcx.map_cluster(cluster, &mut shape_state.synth, font_library);

        while shape_clusters(
            fcx,
            font_library,
            scx,
            &mut shape_state,
            &mut parser,
            cluster,
            dir,
            render_data,
            current_line,
        ) {}

        if let Some(line_hash) = state.lines[current_line].hash {
            cache.insert(line_hash, render_data.last_cached_run.to_owned());
        }
    } else {
        let chars = state.lines[current_line].text.content[range.clone()]
            .iter()
            .zip(&state.lines[current_line].text.offsets[range.clone()])
            .zip(&state.lines[current_line].text.spans[range.clone()])
            .zip(&state.lines[current_line].text.info[range])
            .map(|z| {
                let (((&ch, &offset), &span_index), &info) = z;
                // if current_line == 0 {
                //     println!("{:?} {:?} {:?}", ch, span_index as u32, state.lines[current_line].styles[span_index]);
                // }
                Token {
                    ch,
                    offset,
                    len: ch.len_utf8() as u8,
                    info,
                    data: span_index as u32,
                }
            });

        let mut parser = Parser::new(item.script, chars);
        if !parser.next(cluster) {
            return Some(());
        }
        let font_library = { &fonts.inner.read().unwrap() };
        shape_state.font_id =
            fcx.map_cluster(cluster, &mut shape_state.synth, font_library);
        while shape_clusters(
            fcx,
            font_library,
            scx,
            &mut shape_state,
            &mut parser,
            cluster,
            dir,
            render_data,
            current_line,
        ) {}

        if let Some(line_hash) = state.lines[current_line].hash {
            cache.insert(line_hash, render_data.last_cached_run.to_owned());
        }
    }
    Some(())
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn shape_clusters<I>(
    fcx: &mut FontContext,
    fonts: &FontLibraryData,
    scx: &mut ShapeContext,
    state: &mut ShapeState,
    parser: &mut Parser<I>,
    cluster: &mut CharCluster,
    dir: shape::Direction,
    render_data: &mut RenderData,
    current_line: usize,
) -> bool
where
    I: Iterator<Item = Token> + Clone,
{
    if state.font_id.is_none() {
        return false;
    }

    let current_font_id = state.font_id.unwrap();
    let mut shaper = scx
        .builder(fonts[current_font_id].as_ref())
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
            render_data.push_run(
                &state.state.lines[current_line].styles,
                &current_font_id,
                state.size,
                state.level,
                current_line as u32,
                shaper,
            );
            return false;
        }

        let cluster_span = cluster.user_data() as usize;
        if cluster_span != state.span_index {
            state.span_index = cluster_span;
            state.span = &state.state.lines[current_line].styles[state.span_index];

            // TODO?: Fix state.span.font overwrite
            // if state.span.font != current_font_id {
            // state.font_id = Some(state.span.font);
            // }
            // fcx.select_group(state.font_id);
            // }
        }

        let next_font = fcx.map_cluster(cluster, &mut synth, fonts);
        if next_font != state.font_id || synth != state.synth {
            render_data.push_run(
                &state.state.lines[current_line].styles,
                &current_font_id,
                state.size,
                state.level,
                current_line as u32,
                shaper,
            );
            state.font_id = next_font;
            state.synth = synth;
            return true;
        }
    }
}
