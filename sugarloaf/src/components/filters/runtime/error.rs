// This file was originally taken from https://github.com/SnowflakePowered/librashader
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

//! wgpu shader runtime errors.
use librashader_preprocess::PreprocessError;
use librashader_presets::ParsePresetError;
use librashader_reflect::error::{ShaderCompileError, ShaderReflectError};
use librashader_runtime::image::ImageError;
use thiserror::Error;

/// Cumulative error type for wgpu filter chains.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum FilterChainError {
    #[error("shader preset parse error")]
    ShaderPresetError(#[from] ParsePresetError),
    #[error("shader preprocess error")]
    ShaderPreprocessError(#[from] PreprocessError),
    #[error("shader compile error")]
    ShaderCompileError(#[source] Box<ShaderCompileError>),
    #[error("shader reflect error")]
    ShaderReflectError(#[source] Box<ShaderReflectError>),
    #[error("lut loading error")]
    LutLoadError(#[from] ImageError),
    #[error("unreachable")]
    Infallible(#[from] std::convert::Infallible),
}

/// Result type for wgpu filter chains.
pub type Result<T> = std::result::Result<T, FilterChainError>;

impl From<ShaderCompileError> for FilterChainError {
    fn from(error: ShaderCompileError) -> Self {
        FilterChainError::ShaderCompileError(Box::new(error))
    }
}

impl From<ShaderReflectError> for FilterChainError {
    fn from(error: ShaderReflectError) -> Self {
        FilterChainError::ShaderReflectError(Box::new(error))
    }
}
