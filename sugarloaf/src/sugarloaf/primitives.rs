// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::Rect;
use serde::Deserialize;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SugarCursor {
    Block([f32; 4]),
    Caret([f32; 4]),
    Underline([f32; 4]),
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
    #[serde(default = "Option::default")]
    pub width: Option<f32>,
    #[serde(default = "Option::default")]
    pub height: Option<f32>,
    #[serde(default = "f32::default")]
    pub x: f32,
    #[serde(default = "f32::default")]
    pub y: f32,
}

#[derive(Default, Debug, PartialEq, Clone)]
pub struct Text {
    pub position: (f32, f32),
    pub content: String,
    pub font_id: usize,
    pub font_size: f32,
    pub color: [f32; 4],
    pub single_line: bool,
}

impl Text {
    #[inline]
    pub fn single_line(
        position: (f32, f32),
        content: String,
        font_size: f32,
        color: [f32; 4],
    ) -> Self {
        Text {
            position,
            content,
            font_size,
            font_id: 0,
            color,
            single_line: true,
        }
    }

    #[inline]
    pub fn multi_line(
        position: (f32, f32),
        content: String,
        font_size: f32,
        color: [f32; 4],
    ) -> Self {
        Text {
            position,
            content,
            font_size,
            font_id: 0,
            color,
            single_line: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Rect(Rect),
    Text(Text),
}
