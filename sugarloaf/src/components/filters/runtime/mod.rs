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

/// Concatenates provided arrays.
#[macro_export]
macro_rules! concat_arrays {
    ($( $array:expr ),*) => ({
        #[repr(C)]
        struct ArrayConcatDecomposed<T, A, B>(core::mem::ManuallyDrop<[T; 0]>, core::mem::ManuallyDrop<A>, core::mem::ManuallyDrop<B>);

        impl<T> ArrayConcatDecomposed<T, [T; 0], [T; 0]> {
            #[inline(always)]
            const fn default() -> Self {
                Self::new(core::mem::ManuallyDrop::new([]), [])
            }
        }
        impl<T, A, B> ArrayConcatDecomposed<T, A, B> {
            #[inline(always)]
            const fn new(a: core::mem::ManuallyDrop<A>, b: B) -> Self {
                Self(core::mem::ManuallyDrop::new([]), a, core::mem::ManuallyDrop::new(b))
            }
            #[inline(always)]
            const fn concat<const N: usize>(self, v: [T; N]) -> ArrayConcatDecomposed<T, A, ArrayConcatDecomposed<T, B, [T; N]>> {
                ArrayConcatDecomposed::new(self.1, ArrayConcatDecomposed::new(self.2, v))
            }
        }

        #[repr(C)]
        union ArrayConcatComposed<T, A, B, const N: usize> {
            full: core::mem::ManuallyDrop<[T; N]>,
            decomposed: core::mem::ManuallyDrop<ArrayConcatDecomposed<T, A, B>>,
        }

        impl<T, A, B, const N: usize> ArrayConcatComposed<T, A, B, N> {
            const HAVE_SAME_SIZE: bool = core::mem::size_of::<[T; N]>() == core::mem::size_of::<Self>();

            const PANIC: bool = !["Size mismatch"][!Self::HAVE_SAME_SIZE as usize].is_empty();

            #[inline(always)]
            const fn have_same_size(&self) -> bool {
                Self::PANIC
            }
        }

        let composed = ArrayConcatComposed {
            decomposed: core::mem::ManuallyDrop::new(
                ArrayConcatDecomposed::default()$(.concat($array))*,
            )
        };

        // Sanity check that composed's two fields are the same size
        composed.have_same_size();

        // SAFETY: Sizes of both fields in composed are the same so this assignment should be sound
        core::mem::ManuallyDrop::into_inner(unsafe { composed.full })
    });
}

use librashader_runtime::impl_filter_chain_parameters;
impl_filter_chain_parameters!(FilterChain);

#[inline]
pub fn format_from_image_to_texture(
    format: &librashader_common::ImageFormat,
) -> Option<wgpu::TextureFormat> {
    match format {
        librashader_common::ImageFormat::Unknown => None,
        librashader_common::ImageFormat::R8Unorm => Some(wgpu::TextureFormat::R8Unorm),
        librashader_common::ImageFormat::R8Uint => Some(wgpu::TextureFormat::R8Uint),
        librashader_common::ImageFormat::R8Sint => Some(wgpu::TextureFormat::R8Sint),
        librashader_common::ImageFormat::R8G8Unorm => Some(wgpu::TextureFormat::Rg8Unorm),
        librashader_common::ImageFormat::R8G8Uint => Some(wgpu::TextureFormat::Rg8Uint),
        librashader_common::ImageFormat::R8G8Sint => Some(wgpu::TextureFormat::Rg8Sint),
        librashader_common::ImageFormat::R8G8B8A8Unorm => {
            Some(wgpu::TextureFormat::Rgba8Unorm)
        }
        librashader_common::ImageFormat::R8G8B8A8Uint => {
            Some(wgpu::TextureFormat::Rgba8Uint)
        }
        librashader_common::ImageFormat::R8G8B8A8Sint => {
            Some(wgpu::TextureFormat::Rgba8Sint)
        }
        librashader_common::ImageFormat::R8G8B8A8Srgb => {
            Some(wgpu::TextureFormat::Rgba8UnormSrgb)
        }
        librashader_common::ImageFormat::A2B10G10R10UnormPack32 => {
            Some(wgpu::TextureFormat::Rgb10a2Unorm)
        }
        librashader_common::ImageFormat::A2B10G10R10UintPack32 => {
            Some(wgpu::TextureFormat::Rgb10a2Uint)
        }
        librashader_common::ImageFormat::R16Uint => Some(wgpu::TextureFormat::R16Uint),
        librashader_common::ImageFormat::R16Sint => Some(wgpu::TextureFormat::R16Sint),
        librashader_common::ImageFormat::R16Sfloat => Some(wgpu::TextureFormat::R16Float),
        librashader_common::ImageFormat::R16G16Uint => {
            Some(wgpu::TextureFormat::Rg16Uint)
        }
        librashader_common::ImageFormat::R16G16Sint => {
            Some(wgpu::TextureFormat::Rg16Sint)
        }
        librashader_common::ImageFormat::R16G16Sfloat => {
            Some(wgpu::TextureFormat::Rg16Float)
        }
        librashader_common::ImageFormat::R16G16B16A16Uint => {
            Some(wgpu::TextureFormat::Rgba16Uint)
        }
        librashader_common::ImageFormat::R16G16B16A16Sint => {
            Some(wgpu::TextureFormat::Rgba16Sint)
        }
        librashader_common::ImageFormat::R16G16B16A16Sfloat => {
            Some(wgpu::TextureFormat::Rgba16Float)
        }
        librashader_common::ImageFormat::R32Uint => Some(wgpu::TextureFormat::R32Uint),
        librashader_common::ImageFormat::R32Sint => Some(wgpu::TextureFormat::R32Sint),
        librashader_common::ImageFormat::R32Sfloat => Some(wgpu::TextureFormat::R32Float),
        librashader_common::ImageFormat::R32G32Uint => {
            Some(wgpu::TextureFormat::Rg32Uint)
        }
        librashader_common::ImageFormat::R32G32Sint => {
            Some(wgpu::TextureFormat::Rg32Sint)
        }
        librashader_common::ImageFormat::R32G32Sfloat => {
            Some(wgpu::TextureFormat::Rg32Float)
        }
        librashader_common::ImageFormat::R32G32B32A32Uint => {
            Some(wgpu::TextureFormat::Rgba32Uint)
        }
        librashader_common::ImageFormat::R32G32B32A32Sint => {
            Some(wgpu::TextureFormat::Rgba32Sint)
        }
        librashader_common::ImageFormat::R32G32B32A32Sfloat => {
            Some(wgpu::TextureFormat::Rgba32Float)
        }
    }
}
