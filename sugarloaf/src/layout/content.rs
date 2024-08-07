// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// content.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use crate::layout::*;
use core::ops::Range;

#[derive(PartialEq, Debug, Clone)]
pub struct Fragment {
    start: u32,
    end: u32,
    style: FragmentStyle,
}

#[derive(PartialEq, Debug, Clone)]
pub struct LineFragments {
    data: Vec<Fragment>,
    hash: u64,
}

#[derive(Clone)]
pub struct Content {
    pub fragments: Vec<LineFragments>,
    pub text: String,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            fragments: vec![LineFragments {
                data: vec![],
                // 0 means uninitialized hash
                // that will reflect in uncached
                hash: 0,
            }],
            text: String::default(),
        }
    }
}

impl PartialEq for Content {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && self.fragments == other.fragments
    }
}

impl Content {
    #[inline]
    pub fn builder() -> ContentBuilder {
        ContentBuilder::default()
    }

    #[inline]
    pub fn layout(&self, lcx: &mut ParagraphBuilder) {
        for line in &self.fragments {
            lcx.set_hash(line.hash);

            for e in &line.data {
                if e.start < e.end {
                    if let Some(s) = self.text.get(e.start as usize..e.end as usize) {
                        lcx.add_text(s, e.style);
                    }
                }
            }

            lcx.new_line();
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

    #[inline]
    pub fn insert_str(&mut self, offset: usize, text: &str) -> Option<usize> {
        if self.text.is_char_boundary(offset) {
            self.text.insert_str(offset, text);
            let len = text.len() as u32;
            let last_line = self.fragments.len();
            let frag_index = self.fragment_from_offset(offset).unwrap_or(0);
            self.fragments[last_line].data[frag_index].end += len;
            for frag in &mut self.fragments[last_line].data[frag_index + 1..] {
                frag.start += len;
                frag.end += len;
            }
            return Some(offset + text.len());
        }
        None
    }

    #[inline]
    pub fn insert(&mut self, offset: usize, ch: char) -> Option<usize> {
        if self.text.is_char_boundary(offset) {
            self.text.insert(offset, ch);
            let len = ch.len_utf8() as u32;
            let last_line = self.fragments.len();
            let frag_index = self.fragment_from_offset(offset).unwrap_or(0);
            self.fragments[last_line].data[frag_index].end += len;
            for frag in &mut self.fragments[last_line].data[frag_index + 1..] {
                frag.start += len;
                frag.end += len;
            }
            return Some(offset + len as usize);
        }
        None
    }

    pub fn erase(&mut self, offset: usize) -> Option<usize> {
        let _frag_index = self.fragment_from_offset(offset).unwrap_or(0);
        if self.text.is_char_boundary(offset) {
            self.text.remove(offset);
            return Some(offset);
        }
        None
    }

    fn fragment_from_offset(&self, offset: usize) -> Option<usize> {
        let last_line = self.fragments.len();
        for (i, frag) in self.fragments[last_line].data.iter().enumerate() {
            if offset >= frag.start as usize && offset < frag.end as usize {
                return Some(i);
            }
        }
        self.fragments.len().checked_sub(1)
    }
}

#[derive(Default, Clone, PartialEq)]
pub struct ContentBuilder {
    content: Content,
}

impl ContentBuilder {
    #[inline]
    pub fn add_text(&mut self, text: &str, style: FragmentStyle) {
        let start = self.content.text.len() as u32;
        self.content.text.push_str(text);
        let end = self.content.text.len() as u32;
        let last_line = self.content.fragments.len() - 1;
        self.content.fragments[last_line]
            .data
            .push(Fragment { start, end, style });
    }

    #[inline]
    pub fn add_char(&mut self, text: char, style: FragmentStyle) {
        let start = self.content.text.len() as u32;
        self.content.text.push(text);
        let end = self.content.text.len() as u32;
        let last_line = self.content.fragments.len() - 1;
        self.content.fragments[last_line]
            .data
            .push(Fragment { start, end, style });
    }

    #[inline]
    pub fn set_hash_on_last_line(&mut self, hash: u64) {
        let last_line = self.content.fragments.len() - 1;
        self.content.fragments[last_line].hash = hash;
    }

    #[inline]
    pub fn break_line(&mut self) {
        self.content.fragments.push(LineFragments {
            data: vec![],
            hash: 0,
        });
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
