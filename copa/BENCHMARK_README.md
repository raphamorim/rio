# Copa Parser Benchmark Suite

This directory contains a comprehensive benchmark suite for the Copa parser, designed to compare performance between different UTF-8 validation implementations.

## 🚀 Quick Start

### Simple Performance Comparison
```bash
# Run basic benchmark
cargo run --example benchmark_comparison --release

# Run advanced benchmark with JSON export
cargo run --example advanced_benchmark --release
```

### Statistical Analysis with Criterion
```bash
# Run detailed statistical benchmarks
cargo bench --bench parser_benchmark

# View HTML reports
cargo bench && open target/criterion/report/index.html
```

## 📊 Benchmark Components

### 1. **Basic Benchmark** (`examples/benchmark_comparison.rs`)
- Quick performance overview
- 13 comprehensive test scenarios
- Real-time results display
- Usage instructions

### 2. **Advanced Benchmark** (`examples/advanced_benchmark.rs`)
- Detailed analysis with warmup
- JSON export for comparison
- Category-based performance analysis
- System information detection

### 3. **Criterion Benchmark** (`benches/parser_benchmark.rs`)
- Statistical analysis with confidence intervals
- HTML reports with graphs
- Multiple test scenarios including chunked processing
- Performance regression detection

## 🧪 Test Scenarios

The benchmark suite includes comprehensive test cases:

| Category | Test Cases | Purpose |
|----------|------------|---------|
| **ASCII** | Small/Large text | Baseline performance |
| **UTF-8 2-byte** | European characters (café, naïve) | Common international text |
| **UTF-8 3-byte** | CJK characters (中文, 日本語, 한국어) | Asian language support |
| **UTF-8 4-byte** | Emoji characters (🦀🚀🌟💫) | Modern Unicode support |
| **Mixed Content** | Real-world text combinations | Typical terminal output |
| **Escape Sequences** | Terminal control codes | ANSI escape processing |
| **OSC Sequences** | Operating System Commands | Terminal title/clipboard |
| **CSI Sequences** | Control Sequence Introducer | Colors/formatting |
| **Real-world** | ls output, git log, source code | Practical scenarios |
| **Chunked Processing** | Small input chunks | Streaming input simulation |

## 📈 Performance Comparison

### Current Results (simdutf8 vs std)

| Test Scenario | Main Branch | simd-utf8 Branch | Improvement |
|---------------|-------------|------------------|-------------|
| **UTF-8 4-byte (emoji)** | 1025.10 MB/s | 1406.58 MB/s | **+37.2%** |
| **UTF-8 3-byte (CJK)** | 727.06 MB/s | 1004.84 MB/s | **+38.2%** |
| **UTF-8 2-byte** | 414.44 MB/s | 495.18 MB/s | **+19.5%** |
| **Mixed UTF-8** | 467.75 MB/s | 526.41 MB/s | **+12.5%** |
| **Overall Average** | 544.55 MB/s | 622.76 MB/s | **+14.4%** |

## 🔄 Comparing Implementations

### Method 1: Manual Comparison
```bash
# Test main branch (std UTF-8)
git checkout main
cargo run --example advanced_benchmark --release > results_main.txt

# Test simd-utf8 branch
git checkout simd-utf8
cargo run --example advanced_benchmark --release > results_simd.txt

# Compare results
diff results_main.txt results_simd.txt
```

### Method 2: JSON Export
```bash
# Generate JSON results for both branches
git checkout main && cargo run --example advanced_benchmark --release
git checkout simd-utf8 && cargo run --example advanced_benchmark --release

# Compare JSON files
ls copa_benchmark_*.json
```

### Method 3: Criterion Analysis
```bash
# Run statistical benchmarks on both branches
git checkout main && cargo bench --bench parser_benchmark
git checkout simd-utf8 && cargo bench --bench parser_benchmark

# Criterion automatically detects performance changes
```

## 🛠️ Implementation Details

### Standard Library Implementation (main branch)
- Uses `str::from_utf8()` for UTF-8 validation
- Single-threaded validation
- Compatible with all platforms

### SIMD Implementation (simd-utf8 branch)
- Uses `simdutf8::basic::from_utf8()` for fast path
- Uses `simdutf8::compat::from_utf8()` for error details
- SIMD-accelerated validation on supported platforms
- Fallback to scalar implementation when needed

## 📋 Files Structure

```
copa/
├── Cargo.toml                          # Dependencies and benchmark config
├── benches/
│   └── parser_benchmark.rs             # Criterion statistical benchmarks
├── examples/
│   ├── benchmark_comparison.rs         # Basic performance comparison
│   ├── advanced_benchmark.rs           # Advanced analysis with JSON export
│   └── parselog.rs                     # Original example
└── src/
    └── lib.rs                          # Parser implementation
```

## 🎯 Key Metrics

The benchmarks measure:
- **Throughput**: MB/s processed
- **Latency**: Time per operation
- **Scalability**: Performance across different data sizes
- **Real-world Performance**: Practical terminal scenarios

## 🔧 Customization

### Adding New Test Cases
Edit `examples/advanced_benchmark.rs` and add to `generate_test_data()`:

```rust
("Custom Test", custom_data.into_bytes(), iterations),
```

### Modifying Iterations
Adjust iteration counts in the test data generation for different precision/speed tradeoffs.

### Platform-Specific Tests
Add platform-specific test data or scenarios as needed.

## 📊 Understanding Results

### Throughput Interpretation
- **Higher is better**: More MB/s means faster processing
- **UTF-8 heavy workloads**: Show the biggest improvements with simdutf8
- **ASCII workloads**: Show modest improvements
- **Real-world scenarios**: Represent typical terminal usage

### Statistical Significance
Criterion benchmarks provide:
- Confidence intervals
- Performance regression detection
- Outlier analysis
- Trend analysis over time

## 🚀 Performance Tips

1. **Always use `--release`** for accurate performance measurements
2. **Run multiple times** to account for system variance
3. **Close other applications** to reduce noise
4. **Use consistent hardware** for comparisons
5. **Consider thermal throttling** on laptops

---

*This benchmark suite is designed to provide comprehensive performance analysis for the Copa parser's UTF-8 processing capabilities.*