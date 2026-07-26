// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! The slice of configuration the terminal core needs: the color palette
//! types and the config-loading error. The full user-facing `Config`
//! (fonts, window, navigation, key bindings, …) lives in `rio-backend`,
//! which re-exports these so `rio_backend::config::{colors, ConfigError}`
//! keeps resolving.

pub mod colors;

// Preserve the `crate::config::Colors` path that colors' descendant
// modules (and rio-backend) use.
pub use colors::Colors;

#[derive(Clone, Debug)]
pub enum ConfigError {
    ErrLoadingConfig(String),
    ErrLoadingTheme(String),
    PathNotFound,
}
