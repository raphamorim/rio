//! Benchmark comparing advance_until_terminated performance
//!
//! Run with: cargo run --release --example advance_until_terminated_benchmark

#![allow(clippy::uninlined_format_args)]

use copa::{Parser, Perform};
use rio_backend::batched_parser::BatchedParser;
use std::time::Instant;

// Mock performer for testing
struct MockPerformer {
    chars_received: usize,
    escapes_received: usize,
}

impl Perform for MockPerformer {
    fn print(&mut self, _c: char) {
        self.chars_received += 1;
    }

    fn execute(&mut self, _byte: u8) {
        self.escapes_received += 1;
    }

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
        self.escapes_received += 1;
    }
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        self.escapes_received += 1;
    }
}

impl MockPerformer {
    fn new() -> Self {
        Self {
            chars_received: 0,
            escapes_received: 0,
        }
    }
}

fn main() {
    println!("Rio Terminal - advance_until_terminated Performance Comparison");
    println!("=============================================================");

    let test_cases = vec![
        (
            "Tiny chunks (1-10 bytes)",
            create_tiny_chunks(200),
            "Individual keystrokes and small commands",
        ),
        (
            "Small chunks (10-100 bytes)",
            create_small_chunks(100),
            "Short commands and responses",
        ),
        (
            "Medium chunks (100-1000 bytes)",
            create_medium_chunks(50),
            "Command output and ANSI sequences",
        ),
        (
            "Large chunks (1KB+ bytes)",
            create_large_chunks(20),
            "Paste operations and heavy output",
        ),
        (
            "Very large chunks (10KB+ bytes)",
            create_very_large_chunks(5),
            "Large file operations and TUI redraws",
        ),
    ];

    for (name, chunks, description) in &test_cases {
        println!("\n📊 Testing: {} ({})", name, chunks.len());
        println!("   Description: {}", description);

        let total_bytes: usize = chunks.iter().map(|chunk| chunk.len()).sum();
        let avg_chunk_size = total_bytes / chunks.len();
        println!(
            "   Total bytes: {}, Avg chunk: {} bytes",
            total_bytes, avg_chunk_size
        );

        let iterations = if total_bytes > 50000 {
            50
        } else if total_bytes > 10000 {
            100
        } else {
            1000
        };

        // Benchmark Copa parser
        let copa_duration = benchmark_copa_advance_until_terminated(chunks, iterations);

        // Benchmark BatchedParser
        let batched_duration =
            benchmark_batched_advance_until_terminated(chunks, iterations);

        let speedup =
            copa_duration.as_nanos() as f64 / batched_duration.as_nanos() as f64;

        println!(
            "   Copa parser:        {:>8.2}ms",
            copa_duration.as_secs_f64() * 1000.0
        );
        println!(
            "   BatchedParser:      {:>8.2}ms ({:.2}x {})",
            batched_duration.as_secs_f64() * 1000.0,
            speedup,
            if speedup > 1.0 { "faster" } else { "slower" }
        );

        let performance_impact = if speedup > 1.0 {
            format!("{}% faster", ((speedup - 1.0) * 100.0) as i32)
        } else {
            format!("{}% slower", ((1.0 - speedup) * 100.0) as i32)
        };
        println!("   Performance impact: {}", performance_impact);

        // Show batching behavior
        let batching_threshold = 1024;
        let chunks_that_batch: usize = chunks
            .iter()
            .filter(|c| c.len() >= batching_threshold)
            .count();
        println!(
            "   Chunks that trigger batching: {}/{}",
            chunks_that_batch,
            chunks.len()
        );
    }

    println!("\n🚀 advance_until_terminated analysis:");
    println!("   • Tiny chunks: Should show minimal overhead (immediate processing)");
    println!("   • Small chunks: Should show similar performance (immediate processing)");
    println!(
        "   • Medium chunks: Should show similar performance (immediate processing)"
    );
    println!("   • Large chunks: Should show batching benefits (1KB+ threshold)");
    println!("   • Very large chunks: Should show significant batching benefits");
    println!("\n💡 Batching threshold: 1024 bytes (only large inputs are batched)");
}

fn benchmark_copa_advance_until_terminated(
    chunks: &[Vec<u8>],
    iterations: usize,
) -> std::time::Duration {
    let start = Instant::now();

    for _ in 0..iterations {
        let mut parser = Parser::<1024>::default();
        let mut performer = MockPerformer::new();

        for chunk in chunks {
            let _ = parser.advance_until_terminated(&mut performer, chunk);
        }
    }

    start.elapsed()
}

fn benchmark_batched_advance_until_terminated(
    chunks: &[Vec<u8>],
    iterations: usize,
) -> std::time::Duration {
    let start = Instant::now();

    for _ in 0..iterations {
        let mut parser = BatchedParser::<1024>::new();
        let mut performer = MockPerformer::new();

        for chunk in chunks {
            parser.advance_until_terminated(&mut performer, chunk);
        }
        // Ensure any pending input is flushed
        parser.flush(&mut performer);
    }

    start.elapsed()
}

fn create_tiny_chunks(count: usize) -> Vec<Vec<u8>> {
    let patterns = ["a", "b", "c", "\n", "\x1b", "[", "A"];
    (0..count)
        .map(|i| patterns[i % patterns.len()].as_bytes().to_vec())
        .collect()
}

fn create_small_chunks(count: usize) -> Vec<Vec<u8>> {
    let patterns = [
        "ls\n",
        "pwd\n",
        "echo hello\n",
        "\x1b[32mOK\x1b[0m\n",
        "cd /tmp\n",
    ];
    (0..count)
        .map(|i| patterns[i % patterns.len()].as_bytes().to_vec())
        .collect()
}

fn create_medium_chunks(count: usize) -> Vec<Vec<u8>> {
    let base_text =
        "This is a medium-sized chunk of text that represents typical terminal output. ";
    (0..count)
        .map(|i| {
            format!("{}{}\n", base_text.repeat(2), i)
                .as_bytes()
                .to_vec()
        })
        .collect()
}

fn create_large_chunks(count: usize) -> Vec<Vec<u8>> {
    let base_text = "This is a large chunk of text that would trigger batching. ";
    (0..count)
        .map(|i| {
            format!("{}{}\n", base_text.repeat(20), i)
                .as_bytes()
                .to_vec()
        })
        .collect()
}

fn create_very_large_chunks(count: usize) -> Vec<Vec<u8>> {
    let base_text = "This is a very large chunk of text simulating heavy terminal output or paste operations. ";
    (0..count)
        .map(|i| {
            format!("{}{}\n", base_text.repeat(100), i)
                .as_bytes()
                .to_vec()
        })
        .collect()
}
