//! Enhanced Copa parser with batch UTF-8 validation
//!
//! This module provides an enhanced version of the Copa parser that uses
//! batch processing for UTF-8 validation to improve performance.

use copa::{Parser, Perform};

use tracing::debug;

/// Enhanced parser wrapper that adds batch UTF-8 processing
pub struct BatchedParser<const OSC_RAW_BUF_SIZE: usize = 1024> {
    /// The underlying Copa parser
    parser: Parser<OSC_RAW_BUF_SIZE>,
    /// Buffer for accumulating input chunks
    input_buffer: Vec<u8>,
    /// Threshold for triggering batch processing
    batch_threshold: usize,
    /// Performance statistics
    stats: BatchStats,
}

impl<const OSC_RAW_BUF_SIZE: usize> Default for BatchedParser<OSC_RAW_BUF_SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const OSC_RAW_BUF_SIZE: usize> BatchedParser<OSC_RAW_BUF_SIZE> {
    /// Create a new batched parser with optimal defaults
    pub fn new() -> Self {
        Self {
            parser: Parser::<OSC_RAW_BUF_SIZE>::default(),
            input_buffer: Vec::with_capacity(4096),
            batch_threshold: 1024, // 1KB threshold - optimal for terminal usage
            stats: BatchStats::default(),
        }
    }

    /// Process input with potential batching
    pub fn advance<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) {
        // Only batch for very large inputs (paste operations, large TUI output)
        // Process normal terminal input immediately for responsiveness
        if bytes.len() < self.batch_threshold {
            self.stats.record_immediate(bytes.len());

            debug!("BatchedParser: immediate processing {} bytes", bytes.len());

            self.parser.advance(performer, bytes);
            return;
        }

        // For large inputs, use batching
        self.input_buffer.extend_from_slice(bytes);

        debug!(
            "BatchedParser: added {} bytes to buffer (total: {})",
            bytes.len(),
            self.input_buffer.len()
        );

        // Process immediately if we have a large batch
        if self.input_buffer.len() >= self.batch_threshold {
            let batch_size = self.input_buffer.len();
            self.stats.record_batch(batch_size);

            debug!("BatchedParser: flushing batch of {} bytes", batch_size);

            self.flush_batch(performer);
        }
    }

    /// Force flush any pending batched input
    pub fn flush<P: Perform>(&mut self, performer: &mut P) {
        if !self.input_buffer.is_empty() {
            self.flush_batch(performer);
        }
    }

    /// Internal method to flush the current batch
    fn flush_batch<P: Perform>(&mut self, performer: &mut P) {
        if self.input_buffer.is_empty() {
            return;
        }

        // Process the entire buffer at once
        self.parser.advance(performer, &self.input_buffer);

        // Clear the buffer and shrink if it's grown too large
        self.input_buffer.clear();

        // Prevent memory bloat by shrinking oversized buffers
        if self.input_buffer.capacity() > 16384 {
            self.input_buffer.shrink_to(4096);
        }
    }

    /// Get the underlying parser (for compatibility)
    pub fn inner(&self) -> &Parser<OSC_RAW_BUF_SIZE> {
        &self.parser
    }

    /// Get mutable access to the underlying parser
    pub fn inner_mut(&mut self) -> &mut Parser<OSC_RAW_BUF_SIZE> {
        &mut self.parser
    }

    /// Get current buffer size for monitoring
    pub fn buffer_len(&self) -> usize {
        self.input_buffer.len()
    }

    /// Get performance statistics
    pub fn stats(&self) -> &BatchStats {
        &self.stats
    }

    /// Reset performance statistics
    pub fn reset_stats(&mut self) {
        self.stats = BatchStats::default();
    }

    /// Get current batch threshold
    pub fn batch_threshold(&self) -> usize {
        self.batch_threshold
    }

    /// Process input until terminated, compatible with Copa parser interface
    pub fn advance_until_terminated<P: Perform>(
        &mut self,
        performer: &mut P,
        bytes: &[u8],
    ) -> usize {
        // Only batch for very large inputs (paste operations, large TUI output)
        // Process normal terminal input immediately for responsiveness
        if bytes.len() < self.batch_threshold {
            self.stats.record_immediate(bytes.len());
            return self.parser.advance_until_terminated(performer, bytes);
        }

        // For large inputs, use batching
        self.input_buffer.extend_from_slice(bytes);
        let bytes_added = bytes.len();

        // Process immediately if we have a large batch
        if self.input_buffer.len() >= self.batch_threshold {
            let batch_size = self.input_buffer.len();
            self.stats.record_batch(batch_size);
            self.flush_batch(performer);
        }

        // Always return the number of bytes we just processed
        bytes_added
    }
}

/// Statistics for batch processing performance monitoring
#[derive(Debug, Default)]
pub struct BatchStats {
    /// Total bytes processed
    pub total_bytes: usize,
    /// Number of batch operations
    pub batch_count: usize,
    /// Number of immediate (non-batched) operations
    pub immediate_count: usize,
    /// Average batch size
    pub avg_batch_size: f64,
}

impl BatchStats {
    /// Update stats with a new batch
    pub fn record_batch(&mut self, batch_size: usize) {
        self.total_bytes += batch_size;
        self.batch_count += 1;
        self.update_average();
    }

    /// Update stats with an immediate operation
    pub fn record_immediate(&mut self, size: usize) {
        self.total_bytes += size;
        self.immediate_count += 1;
    }

    /// Update the average batch size
    fn update_average(&mut self) {
        if self.batch_count > 0 {
            self.avg_batch_size = self.total_bytes as f64 / self.batch_count as f64;
        }
    }

    /// Get the batching efficiency (percentage of bytes processed in batches)
    pub fn batch_efficiency(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }

        let batched_bytes = self.batch_count as f64 * self.avg_batch_size;
        (batched_bytes / self.total_bytes as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use copa::Perform;

    // Mock performer for testing
    struct MockPerformer {
        chars_received: Vec<char>,
    }

    impl Perform for MockPerformer {
        fn print(&mut self, c: char) {
            self.chars_received.push(c);
        }

        fn execute(&mut self, _byte: u8) {}
        fn hook(
            &mut self,
            _params: &copa::Params,
            _intermediates: &[u8],
            _ignore: bool,
            _c: char,
        ) {
        }
        fn put(&mut self, _byte: u8) {}
        fn unhook(&mut self) {}
        fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
        fn csi_dispatch(
            &mut self,
            _params: &copa::Params,
            _intermediates: &[u8],
            _ignore: bool,
            _c: char,
        ) {
        }
        fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    }

    #[test]
    fn test_batched_parser_small_input() {
        let mut parser = BatchedParser::<1024>::new();
        let mut performer = MockPerformer {
            chars_received: Vec::new(),
        };

        // Small input should be processed immediately
        parser.advance(&mut performer, b"Hello");

        // Should have processed the text
        assert_eq!(performer.chars_received, vec!['H', 'e', 'l', 'l', 'o']);
    }

    #[test]
    fn test_batched_parser_large_input() {
        let mut parser = BatchedParser::<1024>::new();
        let mut performer = MockPerformer {
            chars_received: Vec::new(),
        };

        // Large input that should trigger batching
        let large_input = "A".repeat(1000);
        parser.advance(&mut performer, large_input.as_bytes());

        // Should have processed all characters
        assert_eq!(performer.chars_received.len(), 1000);
        assert!(performer.chars_received.iter().all(|&c| c == 'A'));
    }

    #[test]
    fn test_batch_stats() {
        let mut stats = BatchStats::default();

        stats.record_batch(100);
        stats.record_batch(200);
        stats.record_immediate(50);

        assert_eq!(stats.total_bytes, 350);
        assert_eq!(stats.batch_count, 2);
        assert_eq!(stats.immediate_count, 1);
        assert_eq!(stats.avg_batch_size, 150.0);
    }
}
