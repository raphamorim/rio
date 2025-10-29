//! Test the enhanced BatchedParser functionality
//!
//! Run with: cargo run --release --example enhanced_batched_parser_test

#![allow(clippy::uninlined_format_args)]

use copa::Perform;
use rio_backend::batched_parser::BatchedParser;

// Mock performer for testing
struct MockPerformer {
    chars_received: usize,
}

impl Perform for MockPerformer {
    fn print(&mut self, _c: char) {
        self.chars_received += 1;
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

fn main() {
    println!("Enhanced BatchedParser Functionality Test");
    println!("========================================");

    // Test 1: Default configuration
    println!("\n🔧 Test 1: Default Configuration");
    let mut parser = BatchedParser::<1024>::new();
    println!(
        "   Default batch threshold: {} bytes",
        parser.batch_threshold()
    );
    println!("   Initial buffer length: {} bytes", parser.buffer_len());
    println!("   Initial stats: {:?}", parser.stats());

    // Test 2: Statistics tracking
    println!("\n📊 Test 2: Statistics Tracking");
    let mut performer = MockPerformer { chars_received: 0 };

    // Process small chunks (should be immediate)
    for i in 0..10 {
        let small_data = format!("small{}", i);
        parser.advance(&mut performer, small_data.as_bytes());
    }

    // Process large chunk (should be batched)
    let large_data = "A".repeat(1000);
    parser.advance(&mut performer, large_data.as_bytes());
    parser.flush(&mut performer);

    let stats = parser.stats();
    println!("   Total bytes processed: {}", stats.total_bytes);
    println!("   Immediate operations: {}", stats.immediate_count);
    println!("   Batch operations: {}", stats.batch_count);
    println!("   Batch efficiency: {:.1}%", stats.batch_efficiency());

    // Test 3: Statistics reset
    println!("\n🔄 Test 3: Statistics Reset");
    parser.reset_stats();
    let reset_stats = parser.stats();
    println!("   Stats after reset: {:?}", reset_stats);

    // Test 4: Memory management
    println!("\n💾 Test 4: Memory Management");
    let very_large_data = "B".repeat(20000);
    parser.advance(&mut performer, very_large_data.as_bytes());
    println!(
        "   Buffer length after large input: {} bytes",
        parser.buffer_len()
    );
    parser.flush(&mut performer);
    println!(
        "   Buffer length after flush: {} bytes",
        parser.buffer_len()
    );

    println!("\n✅ All enhanced functionality tests completed!");
    println!("   Characters processed: {}", performer.chars_received);
    println!("   Final stats: {:?}", parser.stats());
}
