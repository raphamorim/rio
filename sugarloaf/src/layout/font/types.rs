// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// This file was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

//! Basic types for interacting with a font library.

use swash::Attributes;

/// Identifier for a font in a library.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FontId(pub(super) u32);

impl FontId {
    pub(super) fn to_usize(self) -> usize {
        self.0 as usize
    }
}

/// Identifier for a font family in a library.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FamilyId(pub(super) u32);

impl FamilyId {
    pub(super) fn to_usize(self) -> usize {
        self.0 as usize
    }
}

/// Identifier for a font source in a library.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SourceId(pub(super) u32);

impl SourceId {
    pub(super) fn to_usize(self) -> usize {
        self.0 as usize
    }
}

/// Describes a generic font family.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUI,
    Math,
    Emoji,
    FangSong,
}

impl GenericFamily {
    /// Parses a generic font family from CSS style names.
    pub fn parse(family: &str) -> Option<Self> {
        Some(match family {
            "serif" => Self::Serif,
            "sans-serif" => Self::SansSerif,
            "monospace" => Self::Monospace,
            "cursive" => Self::Cursive,
            "fantasy" => Self::Fantasy,
            "system-ui" => Self::SystemUI,
            "math" => Self::Math,
            "emoji" => Self::Emoji,
            _ => return None,
        })
    }
}

/// Key used to select a font family from a library.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FamilyKey<'a> {
    Id(FamilyId),
    Generic(GenericFamily),
    Name(&'a str),
}

impl<'a> From<&'a str> for FamilyKey<'a> {
    fn from(name: &'a str) -> Self {
        Self::Name(name)
    }
}

impl From<FamilyId> for FamilyKey<'_> {
    fn from(id: FamilyId) -> Self {
        Self::Id(id)
    }
}

impl From<GenericFamily> for FamilyKey<'_> {
    fn from(family: GenericFamily) -> Self {
        Self::Generic(family)
    }
}

/// Key used to select a font from a library.
#[derive(Copy, Clone)]
pub enum FontKey<'a> {
    /// Font identifier.
    Id(FontId),
    /// Descriptor with family and attributes.
    Descriptor(FamilyKey<'a>, Attributes),
}

impl From<FontId> for FontKey<'_> {
    fn from(id: FontId) -> Self {
        Self::Id(id)
    }
}

impl<'a> From<&'a str> for FontKey<'a> {
    fn from(name: &'a str) -> Self {
        Self::Descriptor(FamilyKey::Name(name), Attributes::default())
    }
}

impl From<GenericFamily> for FontKey<'_> {
    fn from(family: GenericFamily) -> Self {
        Self::Descriptor(FamilyKey::Generic(family), Attributes::default())
    }
}

impl<'a> From<FamilyKey<'a>> for FontKey<'a> {
    fn from(family: FamilyKey<'a>) -> Self {
        Self::Descriptor(family, Attributes::default())
    }
}

impl<'a> From<(&'a str, Attributes)> for FontKey<'a> {
    fn from(desc: (&'a str, Attributes)) -> Self {
        Self::Descriptor(FamilyKey::Name(desc.0), desc.1)
    }
}

impl From<(GenericFamily, Attributes)> for FontKey<'_> {
    fn from(desc: (GenericFamily, Attributes)) -> Self {
        Self::Descriptor(FamilyKey::Generic(desc.0), desc.1)
    }
}

impl<'a> From<(FamilyKey<'a>, Attributes)> for FontKey<'a> {
    fn from(desc: (FamilyKey<'a>, Attributes)) -> Self {
        Self::Descriptor(desc.0, desc.1)
    }
}
