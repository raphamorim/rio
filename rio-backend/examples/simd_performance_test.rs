#![allow(clippy::uninlined_format_args)]

use std::time::Instant;

fn main() {
    println!("Rio Terminal - SIMD UTF-8 Performance Test");

    println!("==========================================");

    // Test cases representing common terminal content
    let test_cases = vec![
        ("Pure ASCII", create_ascii_content()),
        ("Mixed UTF-8", create_mixed_utf8_content()),
        ("Heavy UTF-8", create_heavy_utf8_content()),
        ("ANSI Sequences", create_ansi_content()),
    ];

    for (name, content) in &test_cases {
        println!("\nTesting: {} ({} bytes)", name, content.len());

        let iterations = if content.len() > 10000 { 1000 } else { 10000 };

        // Test std::str::from_utf8 performance
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = std::str::from_utf8(content);
        }
        let std_duration = start.elapsed();

        // Test simdutf8 performance
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = simdutf8::basic::from_utf8(content);
        }
        let simd_duration = start.elapsed();

        let speedup = std_duration.as_nanos() as f64 / simd_duration.as_nanos() as f64;

        println!(
            "  std::str::from_utf8: {:.2}ms",
            std_duration.as_secs_f64() * 1000.0
        );
        println!(
            "  simdutf8::from_utf8: {:.2}ms ({:.2}x {})",
            simd_duration.as_secs_f64() * 1000.0,
            speedup,
            if speedup > 1.0 { "faster" } else { "slower" }
        );

        // Test with chunks (simulating terminal input)
        let chunks: Vec<&[u8]> = content.chunks(64).collect();

        let start = Instant::now();
        for _ in 0..iterations {
            for chunk in &chunks {
                let _ = std::str::from_utf8(chunk);
            }
        }
        let std_chunked_duration = start.elapsed();

        let start = Instant::now();
        for _ in 0..iterations {
            for chunk in &chunks {
                let _ = simdutf8::basic::from_utf8(chunk);
            }
        }
        let simd_chunked_duration = start.elapsed();

        let chunked_speedup = std_chunked_duration.as_nanos() as f64
            / simd_chunked_duration.as_nanos() as f64;

        println!(
            "  std chunked:         {:.2}ms",
            std_chunked_duration.as_secs_f64() * 1000.0
        );
        println!(
            "  simd chunked:        {:.2}ms ({:.2}x {})",
            simd_chunked_duration.as_secs_f64() * 1000.0,
            chunked_speedup,
            if chunked_speedup > 1.0 {
                "faster"
            } else {
                "slower"
            }
        );
    }
}

fn create_ascii_content() -> Vec<u8> {
    "Hello, world! This is pure ASCII content for terminal testing.\n"
        .repeat(100)
        .into_bytes()
}

fn create_mixed_utf8_content() -> Vec<u8> {
    "Hello, 世界! Mixed ASCII and UTF-8: ❤️ 🚀 ⭐ 🎉\n"
        .repeat(100)
        .into_bytes()
}

fn create_heavy_utf8_content() -> Vec<u8> {
    "🚀 ⭐ 🎉 ❤️ 🌟 💫 ✨ 🔥 💎 🌈 🎯 🎪 🎨 🎭 🎪 🎨\n"
        .repeat(100)
        .into_bytes()
}

fn create_ansi_content() -> Vec<u8> {
    "\x1b[32mGreen text\x1b[0m \x1b[1;31mBold red\x1b[0m \x1b[4mUnderlined\x1b[0m\n"
        .repeat(100)
        .into_bytes()
}
