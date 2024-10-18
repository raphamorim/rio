// This file was originally taken from https://github.com/SnowflakePowered/librashader
// The file has changed to avoid use atomic reference counter of wgpu Device and Queue structs
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

//! librashader WGPU runtime
//!
//! This crate should not be used directly.
//! See [`librashader::runtime::wgpu`](https://docs.rs/librashader/latest/librashader/runtime/wgpu/index.html) instead.
#![deny(unsafe_op_in_unsafe_fn)]

mod buffer;
mod draw_quad;
mod filter_chain;
mod filter_pass;
mod framebuffer;
mod graphics_pipeline;
mod handle;
mod luts;
mod mipmap;
mod samplers;
mod texture;
mod util;

pub use filter_chain::FilterChain;
pub use framebuffer::WgpuOutputView;

pub mod error;
pub mod options;

use librashader_runtime::impl_filter_chain_parameters;
impl_filter_chain_parameters!(FilterChain);
