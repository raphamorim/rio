//! Batched UTF-8 validation utilities for improved performance
//!
//! This module provides batched processing of UTF-8 validation operations,
//! which can significantly improve performance when processing multiple
//! text chunks in sequence (common in terminal applications).

use crate::simd_utf8;

/// Batch processor for UTF-8 validation operations
pub struct BatchUtf8Processor {
    /// Buffer for accumulating chunks to validate together
    buffer: Vec<u8>,
    /// Maximum buffer size before forcing a flush
    max_buffer_size: usize,
    /// Minimum chunk size to consider for batching
    min_chunk_size: usize,
}

impl Default for BatchUtf8Processor {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchUtf8Processor {
    /// Create a new batch processor with default settings
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(8192), // 8KB initial capacity
            max_buffer_size: 16384,           // 16KB max buffer
            min_chunk_size: 64,               // Only batch chunks >= 64 bytes
        }
    }

    /// Create a new batch processor with custom settings
    pub fn with_capacity(max_buffer_size: usize, min_chunk_size: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_buffer_size / 2),
            max_buffer_size,
            min_chunk_size,
        }
    }

    /// Add a chunk to the batch for validation
    /// Returns true if the chunk was batched, false if it should be processed immediately
    pub fn try_batch(&mut self, chunk: &[u8]) -> bool {
        // Don't batch very small chunks - process them immediately
        if chunk.len() < self.min_chunk_size {
            return false;
        }

        // Don't batch if it would exceed our buffer limit
        if self.buffer.len() + chunk.len() > self.max_buffer_size {
            return false;
        }

        // Add to batch
        self.buffer.extend_from_slice(chunk);
        true
    }

    /// Process all batched chunks and return the results
    /// Returns a vector of (offset, length, validation_result) tuples
    pub fn flush_batch(
        &mut self,
    ) -> Vec<(usize, usize, Result<(), simdutf8::basic::Utf8Error>)> {
        if self.buffer.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();
        // For now, we validate the entire buffer at once
        // In the future, we could implement chunk boundary tracking
        let validation_result = simd_utf8::from_utf8_fast(&self.buffer);
        results.push((0, self.buffer.len(), validation_result.map(|_| ())));

        // Clear the buffer for next batch
        self.buffer.clear();
        results
    }

    /// Get the current buffer size
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the buffer has data waiting to be processed
    pub fn has_pending(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Force flush if buffer is getting full
    pub fn should_flush(&self) -> bool {
        self.buffer.len() > self.max_buffer_size / 2
    }
}

/// Batch validation result
#[derive(Debug)]
pub struct BatchValidationResult {
    /// Total bytes processed
    pub bytes_processed: usize,
    /// Number of chunks in the batch
    pub chunk_count: usize,
    /// Overall validation success
    pub is_valid: bool,
    /// First error position if validation failed
    pub error_position: Option<usize>,
}

/// High-level batch validation function
pub fn validate_utf8_batch(chunks: &[&[u8]]) -> BatchValidationResult {
    if chunks.is_empty() {
        return BatchValidationResult {
            bytes_processed: 0,
            chunk_count: 0,
            is_valid: true,
            error_position: None,
        };
    }

    // Calculate total size
    let total_size: usize = chunks.iter().map(|chunk| chunk.len()).sum();

    // For small total sizes, don't bother batching
    if total_size < 256 {
        // Validate each chunk individually
        let mut bytes_processed = 0;
        for (i, chunk) in chunks.iter().enumerate() {
            if simd_utf8::from_utf8_fast(chunk).is_err() {
                return BatchValidationResult {
                    bytes_processed,
                    chunk_count: i,
                    is_valid: false,
                    error_position: Some(bytes_processed),
                };
            }
            bytes_processed += chunk.len();
        }

        return BatchValidationResult {
            bytes_processed,
            chunk_count: chunks.len(),
            is_valid: true,
            error_position: None,
        };
    }

    // Create a single buffer for batch validation
    let mut buffer = Vec::with_capacity(total_size);
    for chunk in chunks {
        buffer.extend_from_slice(chunk);
    }

    // Validate the entire batch at once
    let is_valid = simd_utf8::from_utf8_fast(&buffer).is_ok();

    BatchValidationResult {
        bytes_processed: total_size,
        chunk_count: chunks.len(),
        is_valid,
        error_position: if is_valid { None } else { Some(0) }, // TODO: Find exact error position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_processor_basic() {
        let mut processor = BatchUtf8Processor::new();

        // Small chunk should not be batched
        assert!(!processor.try_batch(b"small"));

        // Large chunk should be batched
        let large_chunk = "a".repeat(100);
        assert!(processor.try_batch(large_chunk.as_bytes()));
        assert!(processor.has_pending());

        // Flush and verify
        let results = processor.flush_batch();
        assert_eq!(results.len(), 1);
        assert!(results[0].2.is_ok());
    }

    #[test]
    fn test_batch_validation() {
        let chunks = vec![
            b"Hello, ".as_slice(),
            b"world! ".as_slice(),
            "🌍 UTF-8 text".as_bytes(),
        ];

        let result = validate_utf8_batch(&chunks);
        assert!(result.is_valid);
        assert_eq!(result.chunk_count, 3);
        assert!(result.bytes_processed > 0);
    }

    #[test]
    fn test_batch_validation_invalid() {
        let chunks = vec![
            b"Valid text".as_slice(),
            b"\xFF\xFE invalid".as_slice(), // Invalid UTF-8
        ];

        let result = validate_utf8_batch(&chunks);
        assert!(!result.is_valid);
        assert!(result.error_position.is_some());
    }

    #[test]
    fn test_buffer_size_limits() {
        let mut processor = BatchUtf8Processor::with_capacity(100, 10);

        // Should batch normal chunks
        assert!(processor.try_batch(&[b'a'; 50]));

        // Should reject chunk that would exceed limit
        assert!(!processor.try_batch(&[b'b'; 60]));
    }
}
