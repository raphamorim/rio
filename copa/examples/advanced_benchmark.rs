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

#[derive(Debug, Clone)]
struct BenchmarkResult {
    name: String,
    duration_ms: f64,
    throughput_mbps: f64,
    data_size: usize,
    iterations: usize,
}

impl BenchmarkResult {
    fn new(name: &str, data: &[u8], iterations: usize) -> Self {
        // Warm up
        for _ in 0..10 {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, data);
        }

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
}

struct BenchmarkSuite {
    results: Vec<BenchmarkResult>,
}

impl BenchmarkSuite {
    fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    fn add_test(&mut self, name: &str, data: &[u8], iterations: usize) {
        let result = BenchmarkResult::new(name, data, iterations);
        self.results.push(result);
    }

    fn run_all_tests(&mut self) {
        println!("Running comprehensive Copa parser benchmarks...");
        println!();

        // Test 1: ASCII performance
        let ascii_small = b"Hello, World! This is ASCII text.".repeat(10);
        self.add_test("ASCII Small", &ascii_small, 10000);

        let ascii_large = b"Hello, World! This is ASCII text.".repeat(1000);
        self.add_test("ASCII Large", &ascii_large, 1000);

        // Test 2: UTF-8 2-byte characters
        let utf8_2byte = "café naïve résumé".repeat(100);
        self.add_test("UTF-8 2-byte", utf8_2byte.as_bytes(), 5000);

        // Test 3: UTF-8 3-byte characters (CJK)
        let utf8_3byte = "中文测试 日本語 한국어".repeat(100);
        self.add_test("UTF-8 3-byte (CJK)", utf8_3byte.as_bytes(), 5000);

        // Test 4: UTF-8 4-byte characters (emojis)
        let utf8_4byte = "🦀🚀🌟💫🎉✨🌍🔥".repeat(100);
        self.add_test("UTF-8 4-byte (emoji)", utf8_4byte.as_bytes(), 5000);

        // Test 5: Mixed content
        let mixed = "Hello 🌍! Welcome to Rust 🦀. This is a test with café, naïve, 中文, العربية, русский язык.".repeat(50);
        self.add_test("Mixed UTF-8", mixed.as_bytes(), 3000);

        // Test 6: Terminal escape sequences
        let escape_seq = b"\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[34mBlue\x1b[0m \x1b[1mBold\x1b[0m".repeat(100);
        self.add_test("Escape Sequences", &escape_seq, 3000);

        // Test 7: OSC sequences with UTF-8
        let osc_seq =
            b"\x1b]2;Terminal Title: \xF0\x9F\x92\xBB Rust Terminal\x07".repeat(100);
        self.add_test("OSC with UTF-8", &osc_seq, 3000);

        // Test 8: CSI sequences
        let csi_seq = b"\x1b[1;32mBold Green\x1b[0m \x1b[4mUnderlined\x1b[0m \x1b[38;2;255;0;255mTruecolor\x1b[0m".repeat(100);
        self.add_test("CSI Sequences", &csi_seq, 3000);

        // Test 9: Real-world ls output
        let mut ls_output = Vec::new();
        for i in 0..100 {
            ls_output.extend_from_slice(
                format!("drwxr-xr-x  2 user group  4096 Jan  1 12:00 📁folder_{i}\n")
                    .as_bytes(),
            );
            ls_output.extend_from_slice(
                format!(
                    "-rw-r--r--  1 user group  1024 Jan  1 12:00 📄file_{}_{}.txt\n",
                    i, "🦀"
                )
                .as_bytes(),
            );
        }
        self.add_test("LS Output", &ls_output, 1000);

        // Test 10: Git log output
        let mut git_log = Vec::new();
        for i in 0..50 {
            git_log.extend_from_slice(format!(
                "\x1b[33mcommit abc123{i}\x1b[0m\nAuthor: Dev 👨‍💻 <dev@example.com>\nDate: Mon Jan 1 12:00:00 2024\n\n    🚀 Feature {i} with 中文 support\n\n"
            ).as_bytes());
        }
        self.add_test("Git Log", &git_log, 1000);

        // Test 11: Source code with UTF-8 comments
        let mut source_code = Vec::new();
        for i in 0..100 {
            source_code.extend_from_slice(format!(
                "// Comment with UTF-8: 🦀 Rust line {i}\nfn function_{i}() -> Result<(), Error> {{\n    println!(\"Hello, 世界! 🌍\");\n    Ok(())\n}}\n\n"
            ).as_bytes());
        }
        self.add_test("Source Code", &source_code, 1000);

        // Test 12: Chunked processing
        let chunked_data =
            "🎉🦀🚀 Rust is amazing! 中文测试 العربية русский язык 🌟✨💫".repeat(100);
        let iterations = 1000;

        // Warm up
        for _ in 0..10 {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            for chunk in chunked_data.as_bytes().chunks(16) {
                parser.advance(&mut performer, chunk);
            }
        }

        let start = Instant::now();
        for _ in 0..iterations {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            for chunk in chunked_data.as_bytes().chunks(16) {
                parser.advance(&mut performer, chunk);
            }
        }
        let duration = start.elapsed();
        let duration_ms = duration.as_secs_f64() * 1000.0;
        let throughput_mbps = (chunked_data.len() * iterations) as f64
            / duration.as_secs_f64()
            / 1_000_000.0;

        self.results.push(BenchmarkResult {
            name: "Chunked Processing".to_string(),
            duration_ms,
            throughput_mbps,
            data_size: chunked_data.len(),
            iterations,
        });
    }

    fn print_results(&self) {
        println!("{:-<90}", "");
        println!(
            "{:25} | {:>10} | {:>10} | {:>12} | {:>8}",
            "Test Case", "Time", "Throughput", "Data Size", "Iterations"
        );
        println!("{:-<90}", "");

        for result in &self.results {
            println!(
                "{:25} | {:8.2} ms | {:8.2} MB/s | {:6} bytes | {:4} iter",
                result.name,
                result.duration_ms,
                result.throughput_mbps,
                result.data_size,
                result.iterations
            );
        }

        println!("{:-<90}", "");
    }

    fn print_summary(&self) {
        let total_throughput: f64 = self.results.iter().map(|r| r.throughput_mbps).sum();
        let avg_throughput = total_throughput / self.results.len() as f64;
        let max_throughput = self
            .results
            .iter()
            .map(|r| r.throughput_mbps)
            .fold(0.0, f64::max);
        let min_throughput = self
            .results
            .iter()
            .map(|r| r.throughput_mbps)
            .fold(f64::INFINITY, f64::min);

        println!();
        println!("Summary Statistics:");
        println!("  Average throughput: {avg_throughput:.2} MB/s");
        println!("  Maximum throughput: {max_throughput:.2} MB/s");
        println!("  Minimum throughput: {min_throughput:.2} MB/s");
        println!("  Total test cases: {}", self.results.len());

        // Category analysis
        let utf8_tests: Vec<_> = self
            .results
            .iter()
            .filter(|r| {
                r.name.contains("UTF-8")
                    || r.name.contains("emoji")
                    || r.name.contains("CJK")
            })
            .collect();

        let ascii_tests: Vec<_> = self
            .results
            .iter()
            .filter(|r| r.name.contains("ASCII"))
            .collect();

        let real_world_tests: Vec<_> = self
            .results
            .iter()
            .filter(|r| {
                r.name.contains("LS")
                    || r.name.contains("Git")
                    || r.name.contains("Source")
            })
            .collect();

        println!();
        println!("Category Analysis:");

        if !utf8_tests.is_empty() {
            let utf8_avg: f64 = utf8_tests.iter().map(|r| r.throughput_mbps).sum::<f64>()
                / utf8_tests.len() as f64;
            println!(
                "  UTF-8 heavy workloads: {:.2} MB/s average ({} tests)",
                utf8_avg,
                utf8_tests.len()
            );
        }

        if !ascii_tests.is_empty() {
            let ascii_avg: f64 =
                ascii_tests.iter().map(|r| r.throughput_mbps).sum::<f64>()
                    / ascii_tests.len() as f64;
            println!(
                "  ASCII workloads: {:.2} MB/s average ({} tests)",
                ascii_avg,
                ascii_tests.len()
            );
        }

        if !real_world_tests.is_empty() {
            let real_world_avg: f64 = real_world_tests
                .iter()
                .map(|r| r.throughput_mbps)
                .sum::<f64>()
                / real_world_tests.len() as f64;
            println!(
                "  Real-world scenarios: {:.2} MB/s average ({} tests)",
                real_world_avg,
                real_world_tests.len()
            );
        }
    }

    fn export_json(&self) -> String {
        let mut json = String::from("{\n");
        json.push_str(&format!(
            "  \"timestamp\": \"{}\",\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        json.push_str(&format!(
            "  \"implementation\": \"{}\",\n",
            detect_implementation()
        ));
        json.push_str(&format!(
            "  \"rust_version\": \"{}\",\n",
            std::env::var("RUSTC_VERSION").unwrap_or_else(|_| "unknown".to_string())
        ));
        json.push_str("  \"results\": [\n");

        for (i, result) in self.results.iter().enumerate() {
            json.push_str("    {\n");
            json.push_str(&format!("      \"name\": \"{}\",\n", result.name));
            json.push_str(&format!(
                "      \"duration_ms\": {:.2},\n",
                result.duration_ms
            ));
            json.push_str(&format!(
                "      \"throughput_mbps\": {:.2},\n",
                result.throughput_mbps
            ));
            json.push_str(&format!("      \"data_size\": {},\n", result.data_size));
            json.push_str(&format!("      \"iterations\": {}\n", result.iterations));
            json.push_str("    }");
            if i < self.results.len() - 1 {
                json.push(',');
            }
            json.push('\n');
        }

        json.push_str("  ]\n");
        json.push_str("}\n");
        json
    }
}

fn detect_implementation() -> &'static str {
    // Try to detect if we're using simdutf8 by checking if it's available
    match std::panic::catch_unwind(|| simdutf8::basic::from_utf8(b"test")) {
        Ok(_) => "simdutf8",
        Err(_) => "std",
    }
}

fn print_system_info() {
    println!("Copa Parser Advanced Benchmark Suite");
    println!("====================================");
    println!();

    let implementation = detect_implementation();
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

    let mut suite = BenchmarkSuite::new();
    suite.run_all_tests();
    suite.print_results();
    suite.print_summary();

    // Export results to JSON for comparison
    let json_output = suite.export_json();
    let filename = format!("copa_benchmark_{}.json", detect_implementation());

    if let Err(e) = std::fs::write(&filename, json_output) {
        eprintln!("Warning: Could not write results to {filename}: {e}");
    } else {
        println!();
        println!("Results exported to: {filename}");
    }

    println!();
    println!("Usage Instructions:");
    println!("==================");
    println!();
    println!("To compare performance between implementations:");
    println!("1. Run on main branch:      git checkout main && cargo run --example advanced_benchmark --release");
    println!("2. Run on simd-utf8 branch: git checkout simd-utf8 && cargo run --example advanced_benchmark --release");
    println!("3. Compare the generated JSON files or console output");
    println!();
    println!("For statistical analysis with criterion:");
    println!("  cargo bench --bench parser_benchmark");
    println!();
    println!("For HTML reports:");
    println!("  cargo bench && open target/criterion/report/index.html");
}
