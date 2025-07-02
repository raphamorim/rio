use copa::{Params, Parser, Perform};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box as std_black_box;

/// A minimal performer that does nothing to avoid overhead in benchmarks
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

/// Generate test data with various UTF-8 scenarios
fn generate_test_data() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        // ASCII only
        (
            "ascii_text",
            b"Hello, World! This is a simple ASCII text.".to_vec(),
        ),
        // Mixed ASCII and UTF-8
        (
            "mixed_utf8",
            "Hello 🌍! This is mixed ASCII and UTF-8: café, naïve, 中文"
                .as_bytes()
                .to_vec(),
        ),
        // Heavy UTF-8 content
        (
            "heavy_utf8",
            "🎉🦀🚀 Rust is amazing! 中文测试 العربية русский язык 🌟✨💫"
                .as_bytes()
                .to_vec(),
        ),
        // Terminal escape sequences with UTF-8
        (
            "escape_sequences",
            b"\x1b[31mRed text\x1b[0m Normal \x1b[32m\xF0\x9F\x8C\xB1 Green\x1b[0m"
                .to_vec(),
        ),
        // OSC sequences with UTF-8
        (
            "osc_utf8",
            b"\x1b]2;Terminal Title with UTF-8: \xF0\x9F\x92\xBB\x07".to_vec(),
        ),
        // CSI sequences
        (
            "csi_sequences",
            b"\x1b[1;32mBold Green\x1b[0m \x1b[4mUnderlined\x1b[0m".to_vec(),
        ),
        // Large text block (simulating real terminal output)
        ("large_text", {
            let mut data = Vec::new();
            for i in 0..1000 {
                data.extend_from_slice(
                    format!("Line {}: Hello 🌍 World! 中文 {}\n", i, "🦀".repeat(5))
                        .as_bytes(),
                );
            }
            data
        }),
        // Vim-like output (complex escape sequences)
        ("vim_like", {
            let mut data = Vec::new();
            // Simulate vim startup with lots of escape sequences
            data.extend_from_slice(
                b"\x1b[?1049h\x1b[22;0;0t\x1b[1;24r\x1b[?12h\x1b[?12l",
            );
            data.extend_from_slice(
                b"\x1b[22;2t\x1b[22;1t\x1b[27m\x1b[23m\x1b[29m\x1b[m\x1b[H\x1b[2J",
            );
            data.extend_from_slice("VIM - Vi IMproved 🚀 version 9.0".as_bytes());
            data.extend_from_slice(b"\x1b[1;1H\x1b[42m\x1b[30m  NORMAL  \x1b[m");
            data
        }),
        // Partial UTF-8 sequences (stress test)
        ("partial_utf8", {
            let mut data = Vec::new();
            // Add some valid UTF-8
            data.extend_from_slice("Valid: 🦀".as_bytes());
            // Add partial UTF-8 that would be completed in next chunk
            data.extend_from_slice(&[0xF0, 0x9F]); // Partial 4-byte UTF-8
            data
        }),
        // Invalid UTF-8 mixed with valid
        ("invalid_utf8", {
            let mut data = Vec::new();
            data.extend_from_slice(b"Valid text ");
            data.extend_from_slice(&[0xFF, 0xFE]); // Invalid UTF-8
            data.extend_from_slice(" more valid text".as_bytes());
            data
        }),
    ]
}

fn bench_parser_advance(c: &mut Criterion) {
    let test_data = generate_test_data();

    let mut group = c.benchmark_group("parser_advance");

    for (name, data) in test_data.iter() {
        group.bench_with_input(BenchmarkId::new("advance", name), data, |b, data| {
            b.iter(|| {
                let mut parser = Parser::new();
                let mut performer = NoOpPerformer;
                parser.advance(&mut performer, std_black_box(data));
            });
        });
    }

    group.finish();
}

fn bench_parser_advance_chunked(c: &mut Criterion) {
    let test_data = generate_test_data();

    let mut group = c.benchmark_group("parser_advance_chunked");

    for (name, data) in test_data.iter() {
        if data.len() < 100 {
            continue;
        } // Skip small data for chunked tests

        group.bench_with_input(BenchmarkId::new("chunked_8", name), data, |b, data| {
            b.iter(|| {
                let mut parser = Parser::new();
                let mut performer = NoOpPerformer;

                // Process in 8-byte chunks to stress UTF-8 handling
                for chunk in data.chunks(8) {
                    parser.advance(&mut performer, std_black_box(chunk));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("chunked_64", name), data, |b, data| {
            b.iter(|| {
                let mut parser = Parser::new();
                let mut performer = NoOpPerformer;

                // Process in 64-byte chunks
                for chunk in data.chunks(64) {
                    parser.advance(&mut performer, std_black_box(chunk));
                }
            });
        });
    }

    group.finish();
}

fn bench_parser_advance_until_terminated(c: &mut Criterion) {
    struct TerminatingPerformer {
        count: usize,
        terminate_at: usize,
    }

    impl TerminatingPerformer {
        fn new(terminate_at: usize) -> Self {
            Self {
                count: 0,
                terminate_at,
            }
        }
    }

    impl Perform for TerminatingPerformer {
        fn print(&mut self, _c: char) {
            self.count += 1;
        }

        fn execute(&mut self, _byte: u8) {
            self.count += 1;
        }

        fn terminated(&self) -> bool {
            self.count >= self.terminate_at
        }

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

    let test_data = generate_test_data();

    let mut group = c.benchmark_group("parser_advance_until_terminated");

    for (name, data) in test_data.iter() {
        if data.len() < 50 {
            continue;
        } // Skip small data

        group.bench_with_input(
            BenchmarkId::new("terminate_early", name),
            data,
            |b, data| {
                b.iter(|| {
                    let mut parser = Parser::new();
                    let mut performer = TerminatingPerformer::new(10); // Terminate after 10 characters
                    let _processed = parser
                        .advance_until_terminated(&mut performer, std_black_box(data));
                });
            },
        );
    }

    group.finish();
}

fn bench_utf8_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("utf8_scenarios");

    // Pure ASCII (should be fastest)
    let ascii_data = "a".repeat(1000).into_bytes();
    group.bench_function("pure_ascii_1k", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, std_black_box(&ascii_data));
        });
    });

    // Pure UTF-8 (2-byte characters)
    let utf8_2byte = "é".repeat(1000).into_bytes();
    group.bench_function("utf8_2byte_1k", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, std_black_box(&utf8_2byte));
        });
    });

    // Pure UTF-8 (3-byte characters)
    let utf8_3byte = "中".repeat(1000).into_bytes();
    group.bench_function("utf8_3byte_1k", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, std_black_box(&utf8_3byte));
        });
    });

    // Pure UTF-8 (4-byte characters - emojis)
    let utf8_4byte = "🦀".repeat(1000).into_bytes();
    group.bench_function("utf8_4byte_1k", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, std_black_box(&utf8_4byte));
        });
    });

    group.finish();
}

fn bench_real_world_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world");

    // Simulate ls -la output with UTF-8 filenames
    let ls_output = {
        let mut data = Vec::new();
        for i in 0..100 {
            data.extend_from_slice(
                format!("drwxr-xr-x  2 user group  4096 Jan  1 12:00 📁folder_{i}\n")
                    .as_bytes(),
            );
            data.extend_from_slice(
                format!(
                    "-rw-r--r--  1 user group  1024 Jan  1 12:00 📄file_{}_{}.txt\n",
                    i, "🦀"
                )
                .as_bytes(),
            );
        }
        data
    };

    group.bench_function("ls_output", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, std_black_box(&ls_output));
        });
    });

    // Simulate git log output with UTF-8 commit messages
    let git_log = {
        let mut data = Vec::new();
        for i in 0..50 {
            data.extend_from_slice(
                format!("\x1b[33mcommit abc123{i}\x1b[0m\n").as_bytes(),
            );
            data.extend_from_slice("Author: Developer 👨‍💻 <dev@example.com>\n".as_bytes());
            data.extend_from_slice("Date: Mon Jan 1 12:00:00 2024 +0000\n\n".as_bytes());
            data.extend_from_slice(
                format!("    🚀 Add feature {i} with 中文 support\n\n").as_bytes(),
            );
        }
        data
    };

    group.bench_function("git_log", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, std_black_box(&git_log));
        });
    });

    // Simulate cat on a source code file with UTF-8 comments
    let source_code = {
        let mut data = Vec::new();
        for i in 0..200 {
            data.extend_from_slice(
                format!("// This is a comment with UTF-8: 🦀 Rust code line {i}\n")
                    .as_bytes(),
            );
            data.extend_from_slice(
                format!("fn function_{i}() -> Result<(), Error> {{\n").as_bytes(),
            );
            data.extend_from_slice("    println!(\"Hello, 世界! 🌍\");\n".as_bytes());
            data.extend_from_slice(b"    Ok(())\n}\n\n");
        }
        data
    };

    group.bench_function("source_code", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            let mut performer = NoOpPerformer;
            parser.advance(&mut performer, std_black_box(&source_code));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parser_advance,
    bench_parser_advance_chunked,
    bench_parser_advance_until_terminated,
    bench_utf8_scenarios,
    bench_real_world_scenarios
);
criterion_main!(benches);
