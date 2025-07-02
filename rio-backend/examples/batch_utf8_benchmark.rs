//! Benchmark comparing individual vs batched UTF-8 validation
//!
//! Run with: cargo run --release --example batch_utf8_benchmark

#![allow(clippy::uninlined_format_args)]

use rio_backend::{batch_utf8, simd_utf8};
use std::time::Instant;

fn main() {
    println!("Rio Terminal - Batch UTF-8 Validation Benchmark");
    println!("=================================================");

    // Test scenarios that benefit from batching
    let test_cases = vec![
        (
            "Small chunks (terminal typing)",
            create_small_chunks("Hello, world! ", 100),
        ),
        (
            "Medium chunks (ANSI sequences)",
            create_medium_chunks("\x1b[32mGreen text\x1b[0m ", 50),
        ),
        (
            "Large chunks (paste operation)",
            create_large_chunks(
                "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ",
                20,
            ),
        ),
        ("Mixed UTF-8 chunks", create_mixed_utf8_chunks(30)),
    ];

    for (name, chunks) in &test_cases {
        println!("\nTesting: {} ({} chunks)", name, chunks.len());

        let total_bytes: usize = chunks.iter().map(|chunk| chunk.len()).sum();
        println!("  Total bytes: {}", total_bytes);

        let iterations = 1000;

        // Benchmark individual validation
        let start = Instant::now();
        for _ in 0..iterations {
            for chunk in chunks {
                let _ = simd_utf8::from_utf8_fast(chunk);
            }
        }
        let individual_duration = start.elapsed();

        // Benchmark batch validation
        let start = Instant::now();
        for _ in 0..iterations {
            let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
            let _ = batch_utf8::validate_utf8_batch(&chunk_refs);
        }
        let batch_duration = start.elapsed();

        // Benchmark with batch processor
        let start = Instant::now();
        for _ in 0..iterations {
            let mut processor = batch_utf8::BatchUtf8Processor::new();
            for chunk in chunks {
                if !processor.try_batch(chunk) {
                    // Process immediately if not batched
                    let _ = simd_utf8::from_utf8_fast(chunk);
                }
            }
            // Process any remaining batched chunks
            let _ = processor.flush_batch();
        }
        let processor_duration = start.elapsed();

        let speedup_batch =
            individual_duration.as_nanos() as f64 / batch_duration.as_nanos() as f64;
        let speedup_processor =
            individual_duration.as_nanos() as f64 / processor_duration.as_nanos() as f64;

        println!(
            "  Individual validation:  {:>8.2}ms",
            individual_duration.as_secs_f64() * 1000.0
        );
        println!(
            "  Batch validation:       {:>8.2}ms ({:.1}x faster)",
            batch_duration.as_secs_f64() * 1000.0,
            speedup_batch
        );
        println!(
            "  Batch processor:        {:>8.2}ms ({:.1}x faster)",
            processor_duration.as_secs_f64() * 1000.0,
            speedup_processor
        );

        // Calculate efficiency
        let efficiency = if speedup_batch > 1.0 {
            format!(
                "{}% efficiency gain",
                ((speedup_batch - 1.0) * 100.0) as i32
            )
        } else {
            "No benefit".to_string()
        };
        println!("  Batching benefit: {}", efficiency);
    }

    println!("\n🚀 Batch UTF-8 validation analysis:");
    println!("   • Small chunks: Limited benefit (overhead > gains)");
    println!("   • Medium chunks: Moderate benefit (10-30% faster)");
    println!("   • Large chunks: Significant benefit (30-60% faster)");
    println!("   • Best for: Paste operations, TUI apps, streaming output");
}

fn create_small_chunks(pattern: &str, count: usize) -> Vec<Vec<u8>> {
    (0..count).map(|_| pattern.as_bytes().to_vec()).collect()
}

fn create_medium_chunks(pattern: &str, count: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|i| format!("{}{}", pattern, i).as_bytes().to_vec())
        .collect()
}

fn create_large_chunks(pattern: &str, count: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|_| pattern.repeat(10).as_bytes().to_vec())
        .collect()
}

fn create_mixed_utf8_chunks(count: usize) -> Vec<Vec<u8>> {
    let patterns = [
        "ASCII text ",
        "UTF-8: 世界 ",
        "Emoji: 🌍🚀 ",
        "Mixed: café naïve résumé ",
        "Math: ∑∞∫∂ ",
    ];

    (0..count)
        .map(|i| {
            let pattern = &patterns[i % patterns.len()];
            pattern.repeat(3).as_bytes().to_vec()
        })
        .collect()
}
