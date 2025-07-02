use copa::{Params, Parser, Perform};
use std::time::Instant;

struct NoOpPerformer;

impl Perform for NoOpPerformer {
    fn print(&mut self, _c: char) {}
    fn execute(&mut self, _byte: u8) {}
    fn hook(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

#[derive(Debug)]
struct BenchmarkResult {
    name: String,
    duration_ms: f64,
    throughput_mbps: f64,
    data_size: usize,
    iterations: usize,
}

impl BenchmarkResult {
    fn new(name: &str, data: &[u8], iterations: usize) -> Self {
        let start = Instant::now();

        for _ in 0..iterations {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, data);
        }

        let duration = start.elapsed();
        let duration_ms = duration.as_secs_f64() * 1000.0;
        let throughput_mbps =
            (data.len() * iterations) as f64 / duration.as_secs_f64() / 1_000_000.0;

        Self {
            name: name.to_string(),
            duration_ms,
            throughput_mbps,
            data_size: data.len(),
            iterations,
        }
    }

    fn print(&self) {
        println!(
            "{:25} | {:8.2} ms | {:8.2} MB/s | {:6} bytes | {:4} iter",
            self.name,
            self.duration_ms,
            self.throughput_mbps,
            self.data_size,
            self.iterations
        );
    }
}

fn generate_test_data() -> Vec<(&'static str, Vec<u8>, usize)> {
    vec![
        // (name, data, iterations)
        ("ASCII Small", b"Hello, World! This is ASCII text.".repeat(10), 10000),
        ("ASCII Large", b"Hello, World! This is ASCII text.".repeat(1000), 1000),
        ("UTF-8 2-byte", "café naïve résumé".repeat(100).into_bytes(), 5000),
        ("UTF-8 3-byte (CJK)", "中文测试 日本語 한국어".repeat(100).into_bytes(), 5000),
        ("UTF-8 4-byte (emoji)", "🦀🚀🌟💫🎉✨🌍🔥".repeat(100).into_bytes(), 5000),
        ("Mixed UTF-8", "Hello 🌍! Welcome to Rust 🦀. This is a test with café, naïve, 中文, العربية, русский язык.".repeat(50).into_bytes(), 3000),
        ("Escape Sequences", b"\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[34mBlue\x1b[0m \x1b[1mBold\x1b[0m".repeat(100), 3000),
        ("OSC with UTF-8", b"\x1b]2;Terminal Title: \xF0\x9F\x92\xBB Rust Terminal\x07".repeat(100), 3000),
        ("CSI Sequences", b"\x1b[1;32mBold Green\x1b[0m \x1b[4mUnderlined\x1b[0m \x1b[38;2;255;0;255mTruecolor\x1b[0m".repeat(100), 3000),
        ("LS Output", {
            let mut data = Vec::new();
            for i in 0..100 {
                data.extend_from_slice(format!(
                    "drwxr-xr-x  2 user group  4096 Jan  1 12:00 📁folder_{}\n-rw-r--r--  1 user group  1024 Jan  1 12:00 📄file_{}_{}.txt\n",
                    i, i, "🦀"
                ).as_bytes());
            }
            data
        }, 1000),
        ("Git Log", {
            let mut data = Vec::new();
            for i in 0..50 {
                data.extend_from_slice(format!(
                    "\x1b[33mcommit abc123{i}\x1b[0m\nAuthor: Dev 👨‍💻 <dev@example.com>\nDate: Mon Jan 1 12:00:00 2024\n\n    🚀 Feature {i} with 中文 support\n\n"
                ).as_bytes());
            }
            data
        }, 1000),
        ("Source Code", {
            let mut data = Vec::new();
            for i in 0..100 {
                data.extend_from_slice(format!(
                    "// Comment with UTF-8: 🦀 Rust line {i}\nfn function_{i}() -> Result<(), Error> {{\n    println!(\"Hello, 世界! 🌍\");\n    Ok(())\n}}\n\n"
                ).as_bytes());
            }
            data
        }, 1000),
    ]
}

fn run_chunked_test() -> BenchmarkResult {
    let chunked_data =
        "🎉🦀🚀 Rust is amazing! 中文测试 العربية русский язык 🌟✨💫".repeat(100);
    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        let mut parser = Parser::new();
        let mut performer = NoOpPerformer;
        // Process in small chunks like real terminal input
        for chunk in chunked_data.as_bytes().chunks(16) {
            parser.advance(&mut performer, chunk);
        }
    }

    let duration = start.elapsed();
    let duration_ms = duration.as_secs_f64() * 1000.0;
    let throughput_mbps =
        (chunked_data.len() * iterations) as f64 / duration.as_secs_f64() / 1_000_000.0;

    BenchmarkResult {
        name: "Chunked Processing".to_string(),
        duration_ms,
        throughput_mbps,
        data_size: chunked_data.len(),
        iterations,
    }
}

fn print_system_info() {
    println!("Copa Parser Performance Benchmark");
    println!("=================================");
    println!();

    // Try to detect if we're using simdutf8 or std
    let implementation =
        match std::panic::catch_unwind(|| simdutf8::basic::from_utf8(b"test")) {
            Ok(_) => "simdutf8 (SIMD-accelerated)",
            Err(_) => "std::str (standard library)",
        };

    println!("Implementation: {implementation}");
    println!(
        "Rust version: {}",
        std::env::var("RUSTC_VERSION").unwrap_or_else(|_| "unknown".to_string())
    );
    println!(
        "Target: {}",
        std::env::var("TARGET").unwrap_or_else(|_| std::env::consts::ARCH.to_string())
    );

    // Try to get CPU info
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
        {
            if let Ok(cpu_info) = String::from_utf8(output.stdout) {
                println!("CPU: {}", cpu_info.trim());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in content.lines() {
                if line.starts_with("model name") {
                    if let Some(cpu_name) = line.split(':').nth(1) {
                        println!("CPU: {}", cpu_name.trim());
                        break;
                    }
                }
            }
        }
    }

    println!();
}

fn main() {
    print_system_info();

    let test_data = generate_test_data();

    println!("{:-<90}", "");
    println!(
        "{:25} | {:>10} | {:>10} | {:>12} | {:>8}",
        "Test Case", "Time", "Throughput", "Data Size", "Iterations"
    );
    println!("{:-<90}", "");

    let mut results = Vec::new();

    // Run chunked test first
    let chunked_result = run_chunked_test();
    chunked_result.print();
    results.push(chunked_result);

    // Run all other tests
    for (name, data, iterations) in test_data {
        let result = BenchmarkResult::new(name, &data, iterations);
        result.print();
        results.push(result);
    }

    println!("{:-<90}", "");

    // Calculate summary statistics
    let total_throughput: f64 = results.iter().map(|r| r.throughput_mbps).sum();
    let avg_throughput = total_throughput / results.len() as f64;
    let max_throughput = results
        .iter()
        .map(|r| r.throughput_mbps)
        .fold(0.0, f64::max);
    let min_throughput = results
        .iter()
        .map(|r| r.throughput_mbps)
        .fold(f64::INFINITY, f64::min);

    println!();
    println!("Summary Statistics:");
    println!("  Average throughput: {avg_throughput:.2} MB/s");
    println!("  Maximum throughput: {max_throughput:.2} MB/s");
    println!("  Minimum throughput: {min_throughput:.2} MB/s");
    println!("  Total test cases: {}", results.len());

    println!();
    println!("Performance Analysis:");

    // Categorize results
    let utf8_tests: Vec<_> = results
        .iter()
        .filter(|r| {
            r.name.contains("UTF-8") || r.name.contains("emoji") || r.name.contains("CJK")
        })
        .collect();

    let ascii_tests: Vec<_> = results
        .iter()
        .filter(|r| r.name.contains("ASCII"))
        .collect();

    let real_world_tests: Vec<_> = results
        .iter()
        .filter(|r| {
            r.name.contains("LS") || r.name.contains("Git") || r.name.contains("Source")
        })
        .collect();

    if !utf8_tests.is_empty() {
        let utf8_avg: f64 = utf8_tests.iter().map(|r| r.throughput_mbps).sum::<f64>()
            / utf8_tests.len() as f64;
        println!("  UTF-8 heavy workloads: {utf8_avg:.2} MB/s average");
    }

    if !ascii_tests.is_empty() {
        let ascii_avg: f64 = ascii_tests.iter().map(|r| r.throughput_mbps).sum::<f64>()
            / ascii_tests.len() as f64;
        println!("  ASCII workloads: {ascii_avg:.2} MB/s average");
    }

    if !real_world_tests.is_empty() {
        let real_world_avg: f64 = real_world_tests
            .iter()
            .map(|r| r.throughput_mbps)
            .sum::<f64>()
            / real_world_tests.len() as f64;
        println!("  Real-world scenarios: {real_world_avg:.2} MB/s average");
    }

    println!();
    println!("Usage Instructions:");
    println!("==================");
    println!();
    println!("To compare performance between implementations:");
    println!("1. Run on main branch:     git checkout main && cargo run --example benchmark_comparison --release");
    println!("2. Run on simd-utf8 branch: git checkout simd-utf8 && cargo run --example benchmark_comparison --release");
    println!("3. Compare the results manually or save outputs to files for analysis");
    println!();
    println!("For detailed statistical analysis:");
    println!("  cargo bench --bench parser_benchmark");
    println!();
    println!("For HTML reports with graphs:");
    println!("  cargo bench --bench parser_benchmark && open target/criterion/report/index.html");
}
