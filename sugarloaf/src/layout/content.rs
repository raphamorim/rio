// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// content.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use crate::layout::*;
use core::borrow::Borrow;
use core::ops::Range;

#[derive(Default, Clone)]
pub struct Content {
    pub spans: Vec<Span>,
    pub fragments: Vec<(u32, u32)>,
    pub text: String,
    pub roots: Vec<usize>,
}

impl PartialEq for Content {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && self.spans == other.spans
    }
}

impl Content {
    pub fn builder() -> ContentBuilder {
        ContentBuilder::default()
    }

    pub fn layout(&self, lcx: &mut ParagraphBuilder) {
        // println!("{:?}", self.roots);
        // for root in 0..self.line {
        for root in &self.roots {
            self.layout_span(*root, lcx);
        }
    }

    pub fn get_selection_into(&self, range: Range<usize>, buf: &mut String) {
        buf.clear();
        if let Some(s) = self.text.get(range) {
            buf.push_str(s);
        }
    }

    pub fn get_selection(&self, range: Range<usize>) -> String {
        let mut s = String::default();
        self.get_selection_into(range, &mut s);
        s
    }

    pub fn insert_str(&mut self, offset: usize, text: &str) -> Option<usize> {
        if self.text.is_char_boundary(offset) {
            self.text.insert_str(offset, text);
            let len = text.len() as u32;
            let frag_index = self.fragment_from_offset(offset).unwrap_or(0);
            self.fragments[frag_index].1 += len;
            for frag in &mut self.fragments[frag_index + 1..] {
                frag.0 += len;
                frag.1 += len;
            }
            return Some(offset + text.len());
        }
        None
    }

    pub fn insert(&mut self, offset: usize, ch: char) -> Option<usize> {
        if self.text.is_char_boundary(offset) {
            self.text.insert(offset, ch);
            let len = ch.len_utf8() as u32;
            let frag_index = self.fragment_from_offset(offset).unwrap_or(0);
            self.fragments[frag_index].1 += len;
            for frag in &mut self.fragments[frag_index + 1..] {
                frag.0 += len;
                frag.1 += len;
            }
            return Some(offset + len as usize);
        }
        None
    }

    pub fn erase(&mut self, erase: Erase) -> Option<usize> {
        let range = match erase {
            Erase::Full(range) => range,
            Erase::Last(range) => {
                let _start = range.start;
                let end = range.end;
                let last_char = self.text.get(range)?.chars().last()?;
                let len = last_char.len_utf8();
                end - len..end
            }
        };
        let start = range.start;
        let end = range.end;
        let len = (end - start) as u32;
        if self.text.is_char_boundary(start) && self.text.is_char_boundary(end) {
            self.text.replace_range(start..end, "");
            let frag_index = self.fragment_from_offset(start).unwrap_or(0);
            let first = &mut self.fragments[frag_index];
            first.1 = first.1.saturating_sub(len);
            for frag in &mut self.fragments[frag_index + 1..] {
                frag.0 = frag.0.saturating_sub(len);
                frag.1 = frag.1.saturating_sub(len);
            }
        }
        Some(start)
    }

    pub fn erase2(&mut self, offset: usize) -> Option<usize> {
        let _frag_index = self.fragment_from_offset(offset).unwrap_or(0);
        if self.text.is_char_boundary(offset) {
            self.text.remove(offset);
            return Some(offset);
        }
        None
    }

    fn layout_span(&self, span: usize, lcx: &mut ParagraphBuilder) {
        let span = &self.spans[span];
        lcx.push_span(&span.properties);
        for e in &span.elements {
            match e {
                SpanElement::Span(i) => self.layout_span(*i, lcx),
                SpanElement::Fragment(i) => {
                    let (start, end) = self.fragments[*i as usize];
                    if start < end {
                        if let Some(s) = self.text.get(start as usize..end as usize) {
                            lcx.add_text(s);
                        }
                    }
                }
                SpanElement::BreakLine => {
                    lcx.new_line();
                }
            }
        }
        lcx.pop_span();
    }

    fn fragment_from_offset(&self, offset: usize) -> Option<usize> {
        for (i, frag) in self.fragments.iter().enumerate() {
            if offset >= frag.0 as usize && offset < frag.1 as usize {
                return Some(i);
            }
        }
        self.fragments.len().checked_sub(1)
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct Span {
    pub properties: Vec<SpanStyle<'static>>,
    pub elements: Vec<SpanElement>,
}

impl Span {
    pub fn set_property(&mut self, property: &SpanStyle) {
        for prop in &mut self.properties {
            if prop.same_kind(property) {
                *prop = property.to_owned();
                return;
            }
        }
        self.properties.push(property.to_owned());
    }
}

#[derive(Copy, PartialEq, Clone)]
pub enum SpanElement {
    Fragment(u32),
    Span(usize),
    BreakLine,
}

#[derive(Default, Clone, PartialEq)]
pub struct ContentBuilder {
    content: Content,
    spans: Vec<u32>,
}

impl ContentBuilder {
    pub fn enter_span<'p, I>(&mut self, properties: I) -> u32
    where
        I: IntoIterator,
        I::Item: Borrow<SpanStyle<'p>>,
    {
        let span = Span {
            properties: properties
                .into_iter()
                .map(|p| p.borrow().to_owned())
                .collect(),
            elements: Vec::new(),
        };
        let size = self.content.spans.len();
        let index = size as u32;
        self.content.spans.push(span);
        if let Some(parent) = self.spans.last() {
            self.content.spans[*parent as usize]
                .elements
                .push(SpanElement::Span(size));
        } else {
            self.content.roots.push(size);
        }
        self.spans.push(index);
        index
    }

    #[inline]
    pub fn leave_span(&mut self) {
        self.spans.pop();
    }

    #[inline]
    pub fn add_text(&mut self, text: &str) {
        if let Some(span) = self.spans.last() {
            let index = self.content.fragments.len() as u32;
            let start = self.content.text.len() as u32;
            self.content.text.push_str(text);
            let end = self.content.text.len() as u32;
            self.content.fragments.push((start, end));
            self.content.spans[*span as usize]
                .elements
                .push(SpanElement::Fragment(index));
        }
    }

    #[inline]
    pub fn add_char(&mut self, text: char) {
        if let Some(span) = self.spans.last() {
            let index = self.content.fragments.len() as u32;
            let start = self.content.text.len() as u32;
            self.content.text.push(text);
            let end = self.content.text.len() as u32;
            self.content.fragments.push((start, end));
            self.content.spans[*span as usize]
                .elements
                .push(SpanElement::Fragment(index));
        }
    }

    #[inline]
    pub fn break_line(&mut self) {
        if let Some(span) = self.spans.last() {
            self.content.spans[*span as usize]
                .elements
                .push(SpanElement::BreakLine);
        }
    }

    #[inline]
    pub fn build_ref(&self) -> &Content {
        &self.content
    }

    #[inline]
    pub fn build(self) -> Content {
        self.content
    }
}
