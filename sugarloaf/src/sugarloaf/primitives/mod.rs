// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

pub mod fragment;
pub mod rectangle;
pub mod text;
pub mod style;
pub mod text_area;

use serde::Deserialize;

#[derive(Debug, Default, PartialEq, Copy, Clone)]
pub struct SugarPosition {
    pub x: f32,
    pub y: f32,
}

// pub enum SugarKind {
//     Text,
//     TextArea,
//     Rectangle,
// }

// pub trait Sugar {
//     fn kind(&self) -> SugarKind;
//     // fn calculate_diff(&self, other: &Self) -> bool {}
//     // fn should_update(&self, other: &Self) -> bool where Self: Sized {}
// }

#[derive(Default, Clone, Deserialize, Debug, PartialEq)]
pub struct ImageProperties {
    #[serde(default = "String::default")]
    pub path: String,
    #[serde(default = "f32::default")]
    pub width: f32,
    #[serde(default = "f32::default")]
    pub height: f32,
    #[serde(default = "f32::default")]
    pub x: f32,
    #[serde(default = "f32::default")]
    pub y: f32,
}
