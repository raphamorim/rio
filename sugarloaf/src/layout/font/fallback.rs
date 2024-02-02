// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// This file was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use super::types::FamilyId;

#[derive(Copy, Clone)]
pub struct Fallbacks {
    entries: [FamilyId; 6],
}

impl Fallbacks {
    pub fn new() -> Self {
        Self {
            entries: [FamilyId(0); 6],
        }
    }

    pub fn len(&self) -> usize {
        self.entries[5].to_usize()
    }

    pub fn push(&mut self, family: FamilyId) -> bool {
        let len = self.entries[5].to_usize();
        if len >= 5 {
            return false;
        }
        self.entries[len] = family;
        self.entries[5].0 += 1;
        true
    }

    pub fn get(&self) -> &[FamilyId] {
        let len = self.entries[5].to_usize();
        &self.entries[..len]
    }
}
