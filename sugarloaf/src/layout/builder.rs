// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// builder.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

//! Render data builder.

use super::builder_data::*;
use super::MAX_ID;
use crate::font::{FontContext, FontLibrary, FontLibraryData};
use crate::layout::render_data::{RenderData, RunCacheEntry};
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use swash::shape::ShapeContext;
use swash::text::cluster::{CharCluster, CharInfo, Parser, Token};
use swash::text::{analyze, Script};
use swash::{Setting, Synthesis};

pub struct RunCache {
    inner: FxHashMap<u64, RunCacheEntry>,
}

impl RunCache {
    #[inline]
    fn new() -> Self {
        Self {
            inner: FxHashMap::default(),
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
    font_features: Vec<swash::Setting<u16>>,
    scx: ShapeContext,
    state: BuilderState,
    cache: RunCache,
    fonts_to_load: Vec<(usize, PathBuf)>,
}

impl LayoutContext {
    /// Creates a new layout context with the specified font library.
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            fonts: font_library.clone(),
            fcx: FontContext::default(),
            scx: ShapeContext::new(),
            state: BuilderState::new(),
            cache: RunCache::new(),
            fonts_to_load: vec![],
            font_features: vec![],
        }
    }

    #[inline]
    pub fn font_library(&self) -> &FontLibrary {
        &self.fonts
    }

    #[inline]
    pub fn set_font_features(&mut self, font_features: Vec<swash::Setting<u16>>) {
        self.font_features = font_features;
    }

    /// Creates a new builder for computing a paragraph layout with the
    /// specified direction, language and scaling factor.
    #[inline]
    pub fn builder(&mut self, scale: f32) -> ParagraphBuilder {
        self.state.clear();
        self.state.begin();
        self.state.scale = scale;
        ParagraphBuilder {
            fcx: &mut self.fcx,
            // bidi: &mut self.bidi,
            // needs_bidi: false,
            font_features: &self.font_features,
            fonts: &self.fonts,
            scx: &mut self.scx,
            s: &mut self.state,
            last_offset: 0,
            cache: &mut self.cache,
            fonts_to_load: &mut self.fonts_to_load,
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
    fonts: &'a FontLibrary,
    font_features: &'a Vec<swash::Setting<u16>>,
    scx: &'a mut ShapeContext,
    s: &'a mut BuilderState,
    last_offset: u32,
    cache: &'a mut RunCache,
    fonts_to_load: &'a mut Vec<(usize, PathBuf)>,
}

impl<'a> ParagraphBuilder<'a> {
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

        macro_rules! push_char {
            ($ch: expr) => {{
                line.text.content.push($ch);
                line.text.offsets.push(offset);
                offset += ($ch).len_utf8() as u32;
            }};
        }

        let start = line.text.content.len();

        for ch in text.chars() {
            push_char!(ch);
        }

        let end = line.text.content.len();
        let break_shaping = if let Some(prev_frag) = line.fragments.last() {
            let prev_style = line.styles[prev_frag.span];
            if prev_style == style {
                false
            } else {
                style.font_size != prev_style.font_size
                    || style.letter_spacing != prev_style.letter_spacing
                    // || style.lang != prev_style.lang
                    // || style.font_features != prev_style.font_features
                    // || style.font_attrs != prev_style.font_attrs
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
            vars: style.font_vars,
        });

        self.last_offset = offset;
        Some(())
    }

    /// Consumes the builder and fills the specified paragraph with the result.
    pub fn build_into(mut self, render_data: &mut RenderData) {
        self.resolve(render_data);
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

            self.itemize(line_number);
            self.shape(render_data, line_number);
        }

        // In this case, we actually have found fonts that have not been loaded yet
        // We need to load and then restart the whole resolve function again
        if !self.fonts_to_load.is_empty() {
            {
                let font_library = { &mut self.fonts.inner.write().unwrap() };
                while let Some(font_to_load) = self.fonts_to_load.pop() {
                    let (font_id, path) = font_to_load;
                    font_library.upsert(font_id, path);
                }
            }

            *render_data = RenderData::default();
            self.cache.inner.clear();
            for line_number in 0..self.s.lines.len() {
                self.shape(render_data, line_number);
            }
        };
    }

    #[inline]
    fn itemize(&mut self, line_number: usize) {
        let line = &mut self.s.lines[line_number];
        let limit = line.text.content.len();
        if line.text.frags.is_empty() || limit == 0 {
            return;
        }
        // let mut last_script = line
        //     .text
        //     .info
        //     .iter()
        //     .map(|i| i.script())
        //     .find(|s| real_script(*s))
        //     .unwrap_or(Script::Latin);
        let mut last_frag = line.fragments.first().unwrap();
        // let last_level = 0;
        let mut last_vars = last_frag.vars;
        let mut item = ItemData {
            // script: last_script,
            // level: last_level,
            start: last_frag.start,
            end: last_frag.start,
            vars: last_vars,
        };
        macro_rules! push_item {
            () => {
                if item.start < limit && item.start < item.end {
                    // item.script = last_script;
                    // item.level = last_level;
                    item.vars = last_vars;
                    line.items.push(item);
                    item.start = item.end;
                }
            };
        }
        for frag in &line.fragments {
            if frag.break_shaping || frag.start != last_frag.end {
                push_item!();
                item.start = frag.start;
                item.end = frag.start;
            }
            last_frag = frag;
            last_vars = frag.vars;
            let range = frag.start..frag.end;
            // for &props in &line.text.info[range] {
            for &_props in &line.text.info[range] {
                //     let script = props.script();
                // let real = real_script(script);
                // if script != last_script && real {
                //     //item.end += 1;
                //     // push_item!();
                //     if real {
                //         last_script = script;
                //     }
                // } else {
                item.end += 1;
                // }
            }
        }
        push_item!();
    }

    #[inline]
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
                self.font_features,
                item,
                &mut char_cluster,
                render_data,
                line_number,
                self.cache,
                self.fonts_to_load,
            );
        }
        // let duration = start.elapsed();
        // println!("Time elapsed in shape is: {:?}", duration);
    }
}

// #[inline]
// fn real_script(script: Script) -> bool {
//     script != Script::Common && script != Script::Inherited && script != Script::Unknown
// }

struct ShapeState<'a> {
    state: &'a BuilderState,
    features: &'a [Setting<u16>],
    synth: Synthesis,
    vars: &'a [Setting<f32>],
    script: Script,
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
    font_features: &[swash::Setting<u16>],
    item: &ItemData,
    cluster: &mut CharCluster,
    render_data: &mut RenderData,
    current_line: usize,
    cache: &mut RunCache,
    fonts_to_load: &mut Vec<(usize, PathBuf)>,
) -> Option<()> {
    let range = item.start..item.end;
    let span_index = state.lines[current_line].text.spans[item.start];
    let style = state.lines[current_line].styles[span_index];
    let vars = state.vars.get(item.vars);
    let mut shape_state = ShapeState {
        // script: item.script,
        script: Script::Latin,
        features: font_features,
        vars,
        synth: Synthesis::default(),
        state,
        span: &state.lines[current_line].styles[span_index],
        font_id: None,
        span_index,
        size: style.font_size,
    };

    let chars = state.lines[current_line].text.content[range.to_owned()]
        .iter()
        .zip(&state.lines[current_line].text.offsets[range.to_owned()])
        .zip(&state.lines[current_line].text.spans[range.to_owned()])
        .zip(&state.lines[current_line].text.info[range])
        .map(|z| {
            let (((&ch, &offset), &span_index), &info) = z;
            Token {
                ch,
                offset,
                len: ch.len_utf8() as u8,
                info,
                data: span_index as u32,
            }
        });

    let mut parser = Parser::new(Script::Latin, chars);
    if !parser.next(cluster) {
        return Some(());
    }
    let font_library = { &fonts.inner.read().unwrap() };
    shape_state.font_id = fcx.map_cluster(
        cluster,
        &mut shape_state.synth,
        font_library,
        fonts_to_load,
        &style,
    );
    while shape_clusters(
        fcx,
        font_library,
        scx,
        &mut shape_state,
        &mut parser,
        cluster,
        // dir,
        render_data,
        current_line,
        fonts_to_load,
    ) {}

    if let Some(line_hash) = state.lines[current_line].hash {
        cache.insert(line_hash, render_data.last_cached_run.to_owned());
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
    render_data: &mut RenderData,
    current_line: usize,
    fonts_to_load: &mut Vec<(usize, PathBuf)>,
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
        // .language(state.span.lang)
        // .direction(dir)
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
                current_line as u32,
                state.state.lines[current_line].hash,
                shaper,
            );
            return false;
        }

        let cluster_span = cluster.user_data() as usize;
        if cluster_span != state.span_index {
            state.span_index = cluster_span;
            state.span = &state.state.lines[current_line].styles[state.span_index];
        }

        let next_font =
            fcx.map_cluster(cluster, &mut synth, fonts, fonts_to_load, state.span);
        if next_font != state.font_id || synth != state.synth {
            render_data.push_run(
                &state.state.lines[current_line].styles,
                &current_font_id,
                state.size,
                current_line as u32,
                state.state.lines[current_line].hash,
                shaper,
            );
            state.font_id = next_font;
            state.synth = synth;
            return true;
        }
    }
}
