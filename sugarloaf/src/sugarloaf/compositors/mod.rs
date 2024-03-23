// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

pub mod advanced;
pub mod elementary;

#[derive(Default)]
pub struct SugarCompositors {
    pub advanced: advanced::Advanced,
    pub elementary: elementary::Elementary,
}
