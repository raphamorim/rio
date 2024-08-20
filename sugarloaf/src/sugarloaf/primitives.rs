// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::Rect;
use serde::Deserialize;

#[derive(Debug, Default, PartialEq, Copy, Clone)]
pub enum SugarCursor {
    Block([f32; 4]),
    Caret([f32; 4]),
    Underline([f32; 4]),
    #[default]
    Disabled,
}

#[derive(Copy, PartialEq, Default, Debug, Clone)]
pub struct SugarloafStyle {
    pub screen_position: (f32, f32),
    pub line_height: f32,
    pub text_scale: f32,
}

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

#[derive(Default, Debug, PartialEq, Clone)]
pub struct SugarText {
    pub position: (f32, f32),
    pub content: String,
    pub font_id: usize,
    pub font_size: f32,
    pub color: [f32; 4],
    pub single_line: bool,
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct SugarBlock {
    pub rects: Vec<Rect>,
    pub text: Option<SugarText>,
}
