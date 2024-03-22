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

#[derive(Clone)]
pub struct Content {
    pub fragments: Vec<Vec<Fragment>>,
    pub text: String,
    pub last_line: usize,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            fragments: vec![vec![]],
            text: String::default(),
            last_line: 0,
        }
    }
}

impl PartialEq for Content {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.fragments == other.fragments
            && self.last_line == self.last_line
    }
}

impl Content {
    #[inline]
    pub fn builder() -> ContentBuilder {
        ContentBuilder::default()
    }

    #[inline]
    pub fn layout(&self, lcx: &mut ParagraphBuilder) {
        for line in 0..self.last_line + 1 {
            for e in &self.fragments[line] {
                if e.start < e.end {
                    if let Some(s) = self.text.get(e.start as usize..e.end as usize) {
                        lcx.add_text(s, Some(e.style));
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
            let frag_index = self.fragment_from_offset(offset).unwrap_or(0);
            self.fragments[self.last_line][frag_index].end += len;
            for frag in &mut self.fragments[self.last_line][frag_index + 1..] {
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
            let frag_index = self.fragment_from_offset(offset).unwrap_or(0);
            self.fragments[self.last_line][frag_index].end += len;
            for frag in &mut self.fragments[self.last_line][frag_index + 1..] {
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
        for (i, frag) in self.fragments[self.last_line].iter().enumerate() {
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
        self.content.fragments[self.content.last_line].push(Fragment {
            start,
            end,
            style,
        });
    }

    #[inline]
    pub fn add_char(&mut self, text: char, style: FragmentStyle) {
        let start = self.content.text.len() as u32;
        self.content.text.push(text);
        let end = self.content.text.len() as u32;
        self.content.fragments[self.content.last_line].push(Fragment {
            start,
            end,
            style,
        });
    }

    #[inline]
    pub fn break_line(&mut self) {
        // Hacky: under the hood it will ignore this "\n" for break_line
        // however whenever process styles from span like background color
        // will apply the line width based on last char before \n and not
        // the remaining space.
        self.add_char('\n', FragmentStyle::default());

        self.content.last_line += 1;
        self.content.fragments.push(vec![]);
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
