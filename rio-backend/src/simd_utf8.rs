//! SIMD-accelerated UTF-8 validation and conversion utilities for Rio terminal.
//!
//! This module provides high-performance UTF-8 processing using the simdutf8 crate,
//! which can be up to 23x faster than std library validation on non-ASCII text.

/// Fast UTF-8 validation and conversion to string slice.
///
/// Uses SIMD acceleration when available, falls back to std implementation otherwise.
/// This is optimized for valid UTF-8 (common case in terminal processing).
#[inline]
pub fn from_utf8_fast(bytes: &[u8]) -> Result<&str, simdutf8::basic::Utf8Error> {
    simdutf8::basic::from_utf8(bytes)
}

/// Fast UTF-8 validation with basic error information.
///
/// Uses SIMD acceleration with basic error reporting.
/// Use this when you don't need detailed error position information.
#[inline]
pub fn from_utf8_compat(bytes: &[u8]) -> Result<&str, simdutf8::basic::Utf8Error> {
    simdutf8::basic::from_utf8(bytes)
}

/// Fast UTF-8 validation and conversion to owned String.
///
/// Optimized for terminal text processing where we often need owned strings.
#[inline]
pub fn from_utf8_to_string(bytes: &[u8]) -> Result<String, simdutf8::basic::Utf8Error> {
    simdutf8::basic::from_utf8(bytes).map(|s| s.to_string())
}

/// Fast UTF-8 validation with lossy conversion fallback.
///
/// Uses SIMD validation first, falls back to lossy conversion for invalid UTF-8.
/// This is useful for handling potentially corrupted terminal input.
#[inline]
pub fn from_utf8_lossy_fast(bytes: &[u8]) -> String {
    match simdutf8::basic::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_utf8() {
        let bytes = b"Hello, \xE2\x9D\xA4\xEF\xB8\x8F UTF-8!";
        let result = from_utf8_fast(bytes).unwrap();
        assert_eq!(result, "Hello, ❤️ UTF-8!");
    }

    #[test]
    fn test_invalid_utf8() {
        let bytes = b"Hello, \xFF invalid UTF-8!";
        assert!(from_utf8_fast(bytes).is_err());

        // Test lossy conversion
        let result = from_utf8_lossy_fast(bytes);
        assert!(result.contains("Hello"));
        assert!(result.contains("invalid UTF-8!"));
    }

    #[test]
    fn test_ascii_fast_path() {
        let bytes = b"Pure ASCII text";
        let result = from_utf8_fast(bytes).unwrap();
        assert_eq!(result, "Pure ASCII text");
    }

    #[test]
    fn test_compat_error_info() {
        let bytes = b"Valid\xFF\xFEInvalid";
        let err = from_utf8_compat(bytes).unwrap_err();
        // Basic error type doesn't provide detailed position info
        assert!(err.to_string().contains("invalid utf-8"));
    }
}
