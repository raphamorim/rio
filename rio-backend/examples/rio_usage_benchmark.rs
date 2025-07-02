//! Benchmark simulating Rio's actual usage pattern
//!
//! Run with: cargo run --release --example rio_usage_benchmark

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

// Simulate Rio's actual advance method
fn simulate_rio_advance_copa(
    parser: &mut Parser<1024>,
    performer: &mut MockPerformer,
    bytes: &[u8],
) {
    let mut processed = 0;
    while processed != bytes.len() {
        processed += parser.advance_until_terminated(performer, &bytes[processed..]);
    }
}

fn simulate_rio_advance_batched(
    parser: &mut BatchedParser<1024>,
    performer: &mut MockPerformer,
    bytes: &[u8],
) {
    let mut processed = 0;
    while processed != bytes.len() {
        processed += parser.advance_until_terminated(performer, &bytes[processed..]);
    }
}

fn main() {
    println!("Rio Terminal - Real Usage Pattern Benchmark");
    println!("===========================================");

    let test_scenarios = vec![
        (
            "Interactive typing session",
            create_typing_session(),
            "User typing commands interactively",
        ),
        (
            "Command with colorized output",
            create_colorized_output(),
            "ls --color=always or similar",
        ),
        (
            "Large paste operation",
            create_paste_operation(),
            "Pasting large text into terminal",
        ),
        (
            "TUI application output",
            create_tui_output(),
            "htop, vim, or similar TUI app",
        ),
        (
            "Mixed real-world workload",
            create_mixed_workload(),
            "Combination of typical terminal usage",
        ),
    ];

    for (name, data, description) in &test_scenarios {
        println!("\n📊 Scenario: {}", name);
        println!("   Description: {}", description);
        println!("   Data size: {} bytes", data.len());

        let iterations = if data.len() > 50000 { 100 } else { 1000 };

        // Benchmark Copa parser
        let copa_duration = benchmark_copa_rio_usage(data, iterations);

        // Benchmark BatchedParser
        let batched_duration = benchmark_batched_rio_usage(data, iterations);

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

        // Memory usage simulation
        let chunks_over_1kb = data.windows(1024).filter(|w| w.len() >= 1024).count();
        println!(
            "   Potential batching opportunities: {} chunks ≥1KB",
            chunks_over_1kb
        );
    }

    println!("\n🚀 Real usage pattern analysis:");
    println!("   • Interactive typing: Should show minimal overhead");
    println!("   • Colorized output: Should show good performance");
    println!("   • Large paste: Should show batching benefits");
    println!("   • TUI output: Should show consistent performance");
    println!("   • Mixed workload: Should show overall real-world impact");
    println!("\n💡 This benchmark simulates Rio's actual processing loop");
}

fn benchmark_copa_rio_usage(data: &[u8], iterations: usize) -> std::time::Duration {
    let start = Instant::now();

    for _ in 0..iterations {
        let mut parser = Parser::<1024>::default();
        let mut performer = MockPerformer::new();
        simulate_rio_advance_copa(&mut parser, &mut performer, data);
    }

    start.elapsed()
}

fn benchmark_batched_rio_usage(data: &[u8], iterations: usize) -> std::time::Duration {
    let start = Instant::now();

    for _ in 0..iterations {
        let mut parser = BatchedParser::<1024>::new();
        let mut performer = MockPerformer::new();
        simulate_rio_advance_batched(&mut parser, &mut performer, data);
        parser.flush(&mut performer);
    }

    start.elapsed()
}

fn create_typing_session() -> Vec<u8> {
    let commands = [
        "ls -la\n",
        "cd Documents\n",
        "pwd\n",
        "echo 'Hello World'\n",
        "cat README.md\n",
        "git status\n",
        "vim file.txt\n",
        ":wq\n",
    ];

    commands.join("").as_bytes().to_vec()
}

fn create_colorized_output() -> Vec<u8> {
    let mut output = Vec::new();

    // Simulate ls --color output
    for i in 0..20 {
        let line = format!(
            "\x1b[34mdir{}\x1b[0m  \x1b[32mfile{}.txt\x1b[0m  \x1b[31mexecutable{}\x1b[0m\n",
            i, i, i
        );
        output.extend_from_slice(line.as_bytes());
    }

    output
}

fn create_paste_operation() -> Vec<u8> {
    // Simulate pasting a large code file
    let code_content = r#"
fn main() {
    println!("Hello, world!");
    
    let mut vec = Vec::new();
    for i in 0..1000 {
        vec.push(i);
    }
    
    let sum: i32 = vec.iter().sum();
    println!("Sum: {}", sum);
}

struct MyStruct {
    field1: String,
    field2: i32,
    field3: Vec<u8>,
}

impl MyStruct {
    fn new() -> Self {
        Self {
            field1: String::new(),
            field2: 0,
            field3: Vec::new(),
        }
    }
}
"#;

    code_content.repeat(10).as_bytes().to_vec()
}

fn create_tui_output() -> Vec<u8> {
    let mut output = Vec::new();

    // Simulate htop-like output with cursor movements and colors
    output.extend_from_slice(b"\x1b[2J\x1b[H"); // Clear screen, move to top

    for row in 1..25 {
        let line = format!(
            "\x1b[{};1H\x1b[2K\x1b[32m{:>5}\x1b[0m \x1b[33muser\x1b[0m \x1b[36m{:>6.1}%\x1b[0m \x1b[35m{:>6.1}%\x1b[0m process_{}\n",
            row, 1000 + row, (row as f32 * 2.5) % 100.0, (row as f32 * 1.8) % 100.0, row
        );
        output.extend_from_slice(line.as_bytes());
    }

    output
}

fn create_mixed_workload() -> Vec<u8> {
    let mut output = Vec::new();

    // Mix of different types of terminal output
    output.extend_from_slice(&create_typing_session());
    output.extend_from_slice(&create_colorized_output());
    output.extend_from_slice(b"Some regular text output\n");
    output.extend_from_slice(&create_tui_output()[..500]); // Partial TUI output
    output.extend_from_slice(b"\x1b[31mError: Something went wrong\x1b[0m\n");
    output.extend_from_slice(&create_paste_operation()[..1000]); // Partial paste

    output
}
