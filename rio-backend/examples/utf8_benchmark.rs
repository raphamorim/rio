//! Benchmark comparing std UTF-8 validation vs SIMD UTF-8 validation
//!
//! Run with: cargo run --release --example utf8_benchmark

#![allow(clippy::uninlined_format_args)]

use std::time::Instant;

fn main() {
    println!("Rio Terminal - SIMD UTF-8 Validation Benchmark");
    println!("================================================");

    // Test data: mix of ASCII and UTF-8
    let test_cases = vec![
        (
            "Pure ASCII",
            "Hello, World! This is a pure ASCII string for testing performance."
                .repeat(100),
        ),
        (
            "Mixed UTF-8",
            "Hello, 世界! 🌍 UTF-8 text with émojis and ñoñ-ASCII çhars.".repeat(100),
        ),
        (
            "Heavy UTF-8",
            "🚀🌟💫⭐🌈🎉🎊🎁🎂🍰🍕🍔🍟🌮🌯🥙🥗🍜🍲🥘🍱🍣🍤🍙🍘".repeat(50),
        ),
    ];

    for (name, data) in &test_cases {
        println!("\nTesting: {} ({} bytes)", name, data.len());

        let bytes = data.as_bytes();
        let iterations = 10000;

        // Benchmark std::str::from_utf8
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = std::str::from_utf8(bytes);
        }
        let std_duration = start.elapsed();

        // Benchmark simdutf8::basic::from_utf8
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = simdutf8::basic::from_utf8(bytes);
        }
        let simd_duration = start.elapsed();

        // Benchmark simdutf8::basic::from_utf8 (was compat)
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = simdutf8::basic::from_utf8(bytes);
        }
        let simd_compat_duration = start.elapsed();

        let speedup_basic =
            std_duration.as_nanos() as f64 / simd_duration.as_nanos() as f64;
        let speedup_compat =
            std_duration.as_nanos() as f64 / simd_compat_duration.as_nanos() as f64;

        println!(
            "  std::str::from_utf8:        {:>8.2}ms",
            std_duration.as_secs_f64() * 1000.0
        );
        println!(
            "  simdutf8::basic::from_utf8: {:>8.2}ms ({:.1}x faster)",
            simd_duration.as_secs_f64() * 1000.0,
            speedup_basic
        );
        println!(
            "  simdutf8::basic::from_utf8 (was compat):{:>8.2}ms ({:.1}x faster)",
            simd_compat_duration.as_secs_f64() * 1000.0,
            speedup_compat
        );
    }

    println!("\n🚀 SIMD UTF-8 validation is now active in Rio terminal!");
    println!("   This improves performance for:");
    println!("   • ANSI escape sequence parsing");
    println!("   • Terminal text processing");
    println!("   • OSC parameter handling");
    println!("   • Hyperlink and title processing");
}
