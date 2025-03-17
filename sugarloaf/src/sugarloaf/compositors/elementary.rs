// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::Rect;
use crate::ComposedQuad;

#[derive(Default)]
pub struct Elementary {
    pub rects: Vec<Rect>,
    pub quads: Vec<ComposedQuad>,
    current_row: u16,
}

impl Elementary {
    #[inline]
    pub fn rects(&mut self) -> &Vec<Rect> {
        &self.rects
    }

    #[inline]
    pub fn clean(&mut self) {
        self.current_row = 0;
        self.rects.clear();
        self.quads.clear();
    }
}
