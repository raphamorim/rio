use std::fmt;

/// UTF-8 validation error with the same shape as `simdutf8`'s compat error
/// type: a successful prefix length, plus an optional invalid-sequence
/// length (`None` = the input ended mid-sequence and more bytes are
/// needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utf8Error {
    valid_up_to: usize,
    error_len: Option<usize>,
}

impl Utf8Error {
    /// Number of leading bytes that successfully validated.
    #[inline]
    pub fn valid_up_to(&self) -> usize {
        self.valid_up_to
    }

    /// Length of the invalid sequence, or `None` if the input ended
    /// mid-codepoint (caller should buffer for the next chunk).
    #[inline]
    pub fn error_len(&self) -> Option<usize> {
        self.error_len
    }
}

impl fmt::Display for Utf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.error_len {
            Some(len) => write!(
                f,
                "invalid utf-8 sequence of {} bytes from index {}",
                len, self.valid_up_to
            ),
            None => write!(
                f,
                "incomplete utf-8 byte sequence from index {}",
                self.valid_up_to
            ),
        }
    }
}

impl std::error::Error for Utf8Error {}

/// Validate `bytes` and return the corresponding `&str` on success.
///
/// Single SIMD pass via `simdutf::validate_utf8_with_errors` — gives both
/// the success-as-`&str` view and the (`valid_up_to`, `error_len`) pair on
/// failure, replacing the `simdutf8` "basic + compat" two-call dance.
#[inline]
pub fn validate(bytes: &[u8]) -> Result<&str, Utf8Error> {
    let result = simdutf::validate_utf8_with_errors(bytes);
    if result.error == simdutf::ErrorCode::Success {
        // SAFETY: `simdutf` confirmed the entire byte slice is valid UTF-8.
        return Ok(unsafe { std::str::from_utf8_unchecked(bytes) });
    }
    let valid_up_to = result.count;
    let error_len = compute_error_len(bytes, valid_up_to);
    Err(Utf8Error {
        valid_up_to,
        error_len,
    })
}

/// Determine the length of the invalid UTF-8 sequence starting at
/// `valid_up_to`, mirroring `simdutf8::compat::from_utf8`'s `error_len()`
/// semantics: `Some(n)` for a definitively-invalid `n`-byte sequence,
/// `None` for an unfinished sequence at end-of-input.
fn compute_error_len(bytes: &[u8], valid_up_to: usize) -> Option<usize> {
    if valid_up_to >= bytes.len() {
        return None;
    }
    let lead = bytes[valid_up_to];

    // Determine the expected sequence length from the lead byte.
    let expected_len = match lead {
        0x00..=0x7F => return Some(1), // unreachable in practice
        0x80..=0xBF => return Some(1), // unexpected continuation
        0xC0..=0xC1 => return Some(1), // overlong 2-byte lead
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        0xF5..=0xFF => return Some(1), // out-of-range lead
    };

    let remaining = bytes.len() - valid_up_to;
    if remaining < expected_len {
        // Either truncated (None) or invalid continuation in the partial bytes.
        let mut i = 1;
        while i < remaining {
            if bytes[valid_up_to + i] & 0xC0 != 0x80 {
                return Some(i);
            }
            i += 1;
        }
        return None;
    }

    // Find the first non-continuation byte in the expected range.
    let mut i = 1;
    while i < expected_len {
        if bytes[valid_up_to + i] & 0xC0 != 0x80 {
            return Some(i);
        }
        i += 1;
    }
    // All continuation bytes look right but `simdutf` flagged the sequence —
    // overlong, surrogate, or out-of-range codepoint.
    Some(expected_len)
}

/// Compatibility shim — same as [`validate`].
#[inline]
pub fn from_utf8_fast(bytes: &[u8]) -> Result<&str, Utf8Error> {
    validate(bytes)
}

/// Compatibility shim — same as [`validate`].
#[inline]
pub fn from_utf8_compat(bytes: &[u8]) -> Result<&str, Utf8Error> {
    validate(bytes)
}

#[inline]
pub fn from_utf8_to_string(bytes: &[u8]) -> Result<String, Utf8Error> {
    validate(bytes).map(|s| s.to_string())
}

/// Validate; on failure fall back to `String::from_utf8_lossy`.
#[inline]
pub fn from_utf8_lossy_fast(bytes: &[u8]) -> String {
    match validate(bytes) {
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
        assert!(err.to_string().contains("invalid utf-8"));
        assert_eq!(err.valid_up_to(), 5);
        assert_eq!(err.error_len(), Some(1));
    }

    #[test]
    fn truncated_utf8_returns_none() {
        // Lead byte for a 4-byte sequence, only 2 bytes provided.
        let bytes = b"\xF0\x9F";
        let err = validate(bytes).unwrap_err();
        assert_eq!(err.valid_up_to(), 0);
        assert_eq!(err.error_len(), None);
    }

    #[test]
    fn truncated_with_bad_continuation_returns_some() {
        // Lead byte for 3-byte sequence, second byte is invalid continuation.
        let bytes = b"\xE0\x20";
        let err = validate(bytes).unwrap_err();
        assert_eq!(err.valid_up_to(), 0);
        assert_eq!(err.error_len(), Some(1));
    }

    #[test]
    fn complete_invalid_sequence_returns_some_len() {
        // Surrogate codepoint encoded in UTF-8 (3 bytes) — valid bytes,
        // invalid Unicode value.
        let bytes = b"\xED\xA0\x80"; // U+D800, surrogate
        let err = validate(bytes).unwrap_err();
        assert_eq!(err.valid_up_to(), 0);
        assert_eq!(err.error_len(), Some(3));
    }
}
