// font_introspector was retired from https://github.com/dfrg/swash
// which is licensed under MIT license

/*!
Character properties and textual analysis.
*/

// Avoid errors for generated Unicode data.
#![allow(clippy::upper_case_acronyms)]

mod analyze;
mod compose;
mod lang;
mod lang_data;
mod unicode;
mod unicode_data;

pub mod cluster;

#[allow(unused)]
pub use analyze::{analyze, Analyze};
#[allow(unused)]
pub use lang::{Cjk, Language};
pub use unicode::*;
