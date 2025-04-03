// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::Rect;
use crate::ComposedQuad;
use serde::Deserialize;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SugarCursor {
    Block([f32; 4]),
    HollowBlock([f32; 4]),
    Caret([f32; 4]),
    Underline([f32; 4]),
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichText {
    pub id: usize,
    pub position: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    NewLayer,
    Rect(Rect),
    Quad(ComposedQuad),
    RichText(RichText),
}

pub enum CornerType {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuiltinChar {
    Horizontal,     // ─
    Vertical,       // │
    TopRight,       // └
    TopLeft,        // ┘
    BottomRight,    // ┌
    BottomLeft,     // ┐
    Cross,          // ┼
    VerticalRight,  // ├
    VerticalLeft,   // ┤
    HorizontalDown, // ┬
    HorizontalUp,   // ┴

    // Horizontal dashes
    HorizontalLightDash,       // ┄
    HorizontalHeavyDash,       // ┅
    HorizontalLightDoubleDash, // ┈
    HorizontalHeavyDoubleDash, // ┉
    HorizontalLightTripleDash, // ╌
    HorizontalHeavyTripleDash, // ╍

    // Vertical dashes
    VerticalLightDash,       // ┆
    VerticalHeavyDash,       // ┇
    VerticalLightDoubleDash, // ┊
    VerticalHeavyDoubleDash, // ┋
    VerticalLightTripleDash, // ╎
    VerticalHeavyTripleDash, // ╏

    // Powerline triangles
    PowerlineLeftSolid,   //
    PowerlineRightSolid,  //
    PowerlineLeftHollow,  //
    PowerlineRightHollow, //
}

impl TryFrom<char> for BuiltinChar {
    type Error = char;

    fn try_from(val: char) -> Result<Self, Self::Error> {
        let boxchar = match val {
            '─' => BuiltinChar::Horizontal,
            '│' => BuiltinChar::Vertical,
            '└' => BuiltinChar::TopRight,
            '┘' => BuiltinChar::TopLeft,
            '┌' => BuiltinChar::BottomRight,
            '┐' => BuiltinChar::BottomLeft,
            '┼' => BuiltinChar::Cross,
            '├' => BuiltinChar::VerticalRight,
            '┤' => BuiltinChar::VerticalLeft,
            '┬' => BuiltinChar::HorizontalDown,
            '┴' => BuiltinChar::HorizontalUp,

            '┄' => BuiltinChar::HorizontalLightDash,
            '┅' => BuiltinChar::HorizontalHeavyDash,
            '┈' => BuiltinChar::HorizontalLightDoubleDash,
            '┉' => BuiltinChar::HorizontalHeavyDoubleDash,
            '╌' => BuiltinChar::HorizontalLightTripleDash,
            '╍' => BuiltinChar::HorizontalHeavyTripleDash,
            '┆' => BuiltinChar::VerticalLightDash,
            '┇' => BuiltinChar::VerticalHeavyDash,
            '┊' => BuiltinChar::VerticalLightDoubleDash,
            '┋' => BuiltinChar::VerticalHeavyDoubleDash,
            '╎' => BuiltinChar::VerticalLightTripleDash,
            '╏' => BuiltinChar::VerticalHeavyTripleDash,

            '\u{e0b2}' => BuiltinChar::PowerlineLeftSolid,
            '\u{e0b0}' => BuiltinChar::PowerlineRightSolid,
            // '' => PowerlineLeftHollow,
            // '' => PowerlineRightHollow,
            _ => return Err(val),
        };
        Ok(boxchar)
    }
}
