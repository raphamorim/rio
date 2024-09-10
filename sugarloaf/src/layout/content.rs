// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// content.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use crate::layout::*;

#[derive(PartialEq, Debug, Clone)]
pub struct Fragment {
    start: u32,
    end: u32,
    style: FragmentStyle,
}

#[derive(PartialEq, Debug, Clone)]
pub struct LineFragments {
    data: Vec<Fragment>,
}

#[derive(Clone)]
pub struct Content {
    pub fragments: Vec<LineFragments>,
    pub text: String,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            fragments: vec![LineFragments { data: vec![] }],
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
    pub fn clear(&mut self) {
        self.fragments.clear();
        self.text.clear();
    }

    #[inline]
    pub fn layout(&self, lcx: &mut ParagraphBuilder) {
        for line in &self.fragments {
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
    pub fn finish_line(&mut self) {
        self.content.fragments.push(LineFragments { data: vec![] });
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
