// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Glyph protocol support: a self-contained TrueType `glyf` outline
//! decoder and the registry that stores client-registered glyphs. Gated
//! behind the `glyph` feature so image-only embedders don't pull the
//! font-outline parser (`skrifa`).

pub mod glyf_decode;
pub mod glyph_registry;
