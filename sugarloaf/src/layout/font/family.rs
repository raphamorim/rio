// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// This file was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use super::types::{FamilyKey, GenericFamily};
use crate::components::rich_text::util::{atomic::AtomicCounter, string::SmallString};

/// Ordered sequence of family names for font selection.
#[derive(Clone, Debug)]
pub struct FamilyList {
    names: SmallString,
    key: u64,
}

pub(crate) static FONT_FAMILY_KEYS: AtomicCounter = AtomicCounter::new();

impl FamilyList {
    /// Creates a new font descriptor from a CSS style list of family names.
    pub fn new(names: &str) -> Self {
        Self {
            names: SmallString::new(names),
            key: FONT_FAMILY_KEYS.next(),
        }
    }

    /// Returns the family names.
    pub fn names(&self) -> &str {
        self.names.as_str()
    }

    /// Returns an iterator over the font families represented
    /// by the names.
    pub fn families(&self) -> impl Iterator<Item = FamilyKey<'_>> + Clone {
        parse_families(self.names())
    }

    pub(crate) fn key(&self) -> u64 {
        self.key
    }
}

impl Default for FamilyList {
    fn default() -> Self {
        Self {
            names: SmallString::new(""),
            key: !0,
        }
    }
}

impl PartialEq for FamilyList {
    fn eq(&self, other: &Self) -> bool {
        self.names == other.names
    }
}

impl Eq for FamilyList {}

impl From<&str> for FamilyList {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

pub fn parse_families(families: &str) -> impl Iterator<Item = FamilyKey<'_>> + Clone {
    FamilyParser {
        source: families.as_bytes(),
        cur: 0,
        len: families.len(),
    }
}

#[derive(Clone)]
struct FamilyParser<'a> {
    source: &'a [u8],
    cur: usize,
    len: usize,
}

impl<'a> Iterator for FamilyParser<'a> {
    type Item = FamilyKey<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut quote = None;
        let mut cur = self.cur;
        while cur < self.len && {
            let ch = self.source[cur];
            ch.is_ascii_whitespace() || ch == b','
        } {
            cur += 1;
        }
        self.cur = cur;
        if cur >= self.len {
            return None;
        }
        let first = self.source[cur];
        let mut start = cur;
        match first {
            b'"' | b'\'' => {
                quote = Some(first);
                cur += 1;
                start += 1;
            }
            _ => {}
        }
        if let Some(quote) = quote {
            while cur < self.len {
                if self.source[cur] == quote {
                    self.cur = cur + 1;
                    return Some(FamilyKey::Name(
                        core::str::from_utf8(self.source.get(start..cur)?)
                            .ok()?
                            .trim(),
                    ));
                }
                cur += 1;
            }
            self.cur = cur;
            return Some(FamilyKey::Name(
                core::str::from_utf8(self.source.get(start..cur)?)
                    .ok()?
                    .trim(),
            ));
        }
        let mut end = start;
        while cur < self.len {
            if self.source[cur] == b',' {
                cur += 1;
                break;
            }
            cur += 1;
            end += 1;
        }
        self.cur = cur;
        let name = core::str::from_utf8(self.source.get(start..end)?)
            .ok()?
            .trim();
        Some(match GenericFamily::parse(name) {
            Some(family) => FamilyKey::Generic(family),
            _ => FamilyKey::Name(name),
        })
    }
}
