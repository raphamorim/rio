//! SIMD base64 helpers backed by `simdutf`.
//!
//! Replaces the scalar `base64` crate's `Engine::decode` for the three hot
//! base64 paths in rio:
//! - OSC 52 clipboard_store (small, but synchronous on every paste).
//! - iTerm2 OSC 1337 inline images.
//! - Kitty graphics protocol APC chunks (4 KB+ per chunk, very hot during
//!   image-heavy TUIs).

use simdutf::{
    base64_to_binary, maximal_binary_length_from_base64, Base64Options, ErrorCode,
    LastChunkHandlingOptions,
};

/// Decode a standard-alphabet base64 byte slice (`+` and `/`, with padding)
/// to a freshly-allocated `Vec<u8>`. Returns `None` on invalid input.
#[inline]
pub fn decode(input: &[u8]) -> Option<Vec<u8>> {
    decode_with_options(
        input,
        Base64Options::Default,
        LastChunkHandlingOptions::Loose,
    )
}

/// Decode a standard-alphabet base64 byte slice without padding.
#[inline]
pub fn decode_no_pad(input: &[u8]) -> Option<Vec<u8>> {
    decode_with_options(
        input,
        Base64Options::DefaultNoPadding,
        LastChunkHandlingOptions::Loose,
    )
}

#[inline]
fn decode_with_options(
    input: &[u8],
    options: Base64Options,
    last_chunk: LastChunkHandlingOptions,
) -> Option<Vec<u8>> {
    if input.is_empty() {
        return Some(Vec::new());
    }
    // SAFETY: `input` is a valid byte slice.
    let max_len =
        unsafe { maximal_binary_length_from_base64(input.as_ptr(), input.len()) };
    let mut out = Vec::with_capacity(max_len);
    // SAFETY:
    //  - `input` is valid for reads of `input.len()` bytes.
    //  - `out` has capacity for `max_len` bytes; `simdutf` writes at most that.
    //  - `out.as_mut_ptr()` is an exclusive write pointer.
    let result = unsafe {
        base64_to_binary(
            input.as_ptr(),
            input.len(),
            out.as_mut_ptr(),
            options,
            last_chunk,
        )
    };
    if result.error != ErrorCode::Success {
        return None;
    }
    // SAFETY: `simdutf` wrote exactly `result.count` bytes into the buffer.
    unsafe {
        out.set_len(result.count);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_empty() {
        assert_eq!(decode(b"").unwrap(), b"");
    }

    #[test]
    fn decode_basic_padded() {
        assert_eq!(decode(b"aGVsbG8gd29ybGQ=").unwrap(), b"hello world");
    }

    #[test]
    fn decode_basic_unpadded_via_no_pad() {
        assert_eq!(decode_no_pad(b"aGVsbG8gd29ybGQ").unwrap(), b"hello world");
    }

    #[test]
    fn decode_invalid() {
        assert!(decode(b"not!valid#base64").is_none());
    }

    #[test]
    fn decode_round_trip_kitty_chunk() {
        // ~4 KB payload typical of kitty graphics.
        let bytes: Vec<u8> = (0..4096).map(|i| (i & 0xff) as u8).collect();
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let encoded = STANDARD.encode(&bytes);
        let decoded = decode(encoded.as_bytes()).unwrap();
        assert_eq!(decoded, bytes);
    }
}
