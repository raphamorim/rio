//! Benchmark comparing Copa parser vs BatchedParser performance
//!
//! Run with: cargo run --release --example parser_comparison_benchmark

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

    #[allow(unused)]
    fn reset(&mut self) {
        self.chars_received = 0;
        self.escapes_received = 0;
    }
}

fn main() {
    println!("Rio Terminal - Parser Performance Comparison");
    println!("============================================");

    let test_cases = vec![
        (
            "Small typing chunks",
            create_typing_chunks(100),
            "Simulates normal terminal typing",
        ),
        (
            "Medium ANSI sequences",
            create_ansi_chunks(50),
            "Simulates colorized terminal output",
        ),
        (
            "Large paste operation",
            create_paste_chunks(10),
            "Simulates large text paste",
        ),
        (
            "Mixed terminal workload",
            create_mixed_workload(30),
            "Simulates real terminal usage",
        ),
        (
            "Heavy TUI output",
            create_tui_chunks(20),
            "Simulates TUI applications like htop",
        ),
    ];

    for (name, chunks, description) in &test_cases {
        println!("\n📊 Testing: {} ({})", name, chunks.len());
        println!("   Description: {}", description);

        let total_bytes: usize = chunks.iter().map(|chunk| chunk.len()).sum();
        println!("   Total bytes: {}", total_bytes);

        let iterations = if total_bytes > 10000 { 100 } else { 1000 };

        // Benchmark Copa parser
        let copa_duration = benchmark_copa_parser(chunks, iterations);

        // Benchmark BatchedParser
        let batched_duration = benchmark_batched_parser(chunks, iterations);

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
    }

    println!("\n🚀 Parser comparison analysis:");
    println!("   • Small chunks: Should show minimal overhead");
    println!("   • Medium chunks: Should show similar performance");
    println!("   • Large chunks: Should show batching benefits");
    println!("   • Mixed workload: Should show real-world performance");
    println!("   • TUI output: Should show benefits for heavy output");
}

fn benchmark_copa_parser(chunks: &[Vec<u8>], iterations: usize) -> std::time::Duration {
    let start = Instant::now();

    for _ in 0..iterations {
        let mut parser = Parser::<1024>::default();
        let mut performer = MockPerformer::new();

        for chunk in chunks {
            parser.advance(&mut performer, chunk);
        }
    }

    start.elapsed()
}

fn benchmark_batched_parser(
    chunks: &[Vec<u8>],
    iterations: usize,
) -> std::time::Duration {
    let start = Instant::now();

    for _ in 0..iterations {
        let mut parser = BatchedParser::<1024>::new();
        let mut performer = MockPerformer::new();

        for chunk in chunks {
            parser.advance(&mut performer, chunk);
        }
        // Ensure any pending input is flushed
        parser.flush(&mut performer);
    }

    start.elapsed()
}

fn create_typing_chunks(count: usize) -> Vec<Vec<u8>> {
    let patterns = ["h", "e", "l", "l", "o", " ", "w", "o", "r", "l", "d", "\n"];
    (0..count)
        .map(|i| patterns[i % patterns.len()].as_bytes().to_vec())
        .collect()
}

fn create_ansi_chunks(count: usize) -> Vec<Vec<u8>> {
    let patterns = [
        "\x1b[31mRed text\x1b[0m",
        "\x1b[32mGreen text\x1b[0m",
        "\x1b[33mYellow text\x1b[0m",
        "\x1b[34mBlue text\x1b[0m",
        "\x1b[1mBold text\x1b[0m",
        "\x1b[2J\x1b[H", // Clear screen
    ];
    (0..count)
        .map(|i| patterns[i % patterns.len()].as_bytes().to_vec())
        .collect()
}

fn create_paste_chunks(count: usize) -> Vec<Vec<u8>> {
    let large_text =
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(50);
    (0..count).map(|_| large_text.as_bytes().to_vec()).collect()
}

fn create_mixed_workload(count: usize) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();

    for i in 0..count {
        match i % 4 {
            0 => chunks.push("ls -la\n".as_bytes().to_vec()),
            1 => chunks.push("\x1b[32m✓\x1b[0m Success\n".as_bytes().to_vec()),
            2 => chunks.push("A".repeat(100).as_bytes().to_vec()), // Medium chunk
            3 => chunks.push("Error: file not found\n".as_bytes().to_vec()),
            _ => unreachable!(),
        }
    }

    chunks
}

fn create_tui_chunks(count: usize) -> Vec<Vec<u8>> {
    // Simulate htop-like output with lots of ANSI sequences
    let tui_line = format!(
        "\x1b[2K\x1b[{}H\x1b[32m{:>5}\x1b[0m \x1b[33m{}\x1b[0m \x1b[36m{:>6.1}%\x1b[0m \x1b[35m{:>6.1}%\x1b[0m {}",
        1, 1234, "user", 15.5, 8.2, "some_process"
    );

    (0..count)
        .map(|i| format!("{}{}", tui_line, i).as_bytes().to_vec())
        .collect()
}
