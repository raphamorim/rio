use criterion::{criterion_group, criterion_main, Criterion};
use std::collections::HashMap;

// Mock types for benchmarking
#[derive(Clone, Debug)]
struct MockChar(char);

use std::fmt;

impl fmt::Display for MockChar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Current implementation - allocates string for each character
fn current_char_to_string_hot_path(chars: &[MockChar]) -> Vec<String> {
    let mut result = Vec::new();
    for character in chars {
        result.push(character.to_string()); // This is the hot path allocation
    }
    result
}

// Optimized implementation - string interning
struct StringInterner {
    cache: HashMap<char, String>,
}

impl StringInterner {
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    fn get_or_intern(&mut self, c: char) -> &str {
        self.cache.entry(c).or_insert_with(|| c.to_string())
    }
}

fn optimized_char_to_string_with_interning(chars: &[MockChar]) -> Vec<String> {
    let mut interner = StringInterner::new();
    let mut result = Vec::new();
    for character in chars {
        result.push(interner.get_or_intern(character.0).to_string());
    }
    result
}

// Even more optimized - pre-allocated ASCII cache
struct AsciiStringCache {
    ascii_cache: [String; 128],
    extended_cache: HashMap<char, String>,
}

impl AsciiStringCache {
    fn new() -> Self {
        let ascii_cache = std::array::from_fn(|i| (i as u8 as char).to_string());

        Self {
            ascii_cache,
            extended_cache: HashMap::new(),
        }
    }

    fn get_string(&mut self, c: char) -> &str {
        let code = c as u32;
        if code < 128 {
            &self.ascii_cache[code as usize]
        } else {
            self.extended_cache
                .entry(c)
                .or_insert_with(|| c.to_string())
        }
    }
}

fn optimized_char_to_string_with_ascii_cache(chars: &[MockChar]) -> Vec<String> {
    let mut cache = AsciiStringCache::new();
    let mut result = Vec::new();
    for character in chars {
        result.push(cache.get_string(character.0).to_string());
    }
    result
}

// Best optimization - avoid string allocation entirely by working with &str
static ASCII_STRINGS: [&str; 128] = [
    "\0", "\x01", "\x02", "\x03", "\x04", "\x05", "\x06", "\x07", "\x08", "\t", "\n",
    "\x0b", "\x0c", "\r", "\x0e", "\x0f", "\x10", "\x11", "\x12", "\x13", "\x14", "\x15",
    "\x16", "\x17", "\x18", "\x19", "\x1a", "\x1b", "\x1c", "\x1d", "\x1e", "\x1f", " ",
    "!", "\"", "#", "$", "%", "&", "'", "(", ")", "*", "+", ",", "-", ".", "/", "0", "1",
    "2", "3", "4", "5", "6", "7", "8", "9", ":", ";", "<", "=", ">", "?", "@", "A", "B",
    "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "[", "\\", "]", "^", "_", "`", "a", "b", "c", "d",
    "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u",
    "v", "w", "x", "y", "z", "{", "|", "}", "~", "\x7f",
];

fn optimized_char_to_str_refs(chars: &[MockChar]) -> Vec<&'static str> {
    let mut result = Vec::new();
    for character in chars {
        let code = character.0 as u32;
        if code < 128 {
            result.push(ASCII_STRINGS[code as usize]);
        } else {
            // For non-ASCII, we'd need a different approach
            result.push(" "); // placeholder
        }
    }
    result
}

// Most optimized - pre-allocate result vector and avoid repeated allocations
fn optimized_with_capacity_and_ascii_cache(chars: &[MockChar]) -> Vec<String> {
    let mut cache = AsciiStringCache::new();
    let mut result = Vec::with_capacity(chars.len()); // Pre-allocate
    for character in chars {
        result.push(cache.get_string(character.0).to_string());
    }
    result
}

fn create_test_data() -> Vec<MockChar> {
    // Simulate typical terminal content - mostly ASCII with some unicode
    let mut chars = Vec::new();

    // Common ASCII characters (80% of content)
    for _ in 0..800 {
        chars.push(MockChar('a'));
        chars.push(MockChar(' '));
        chars.push(MockChar('1'));
        chars.push(MockChar('\n'));
    }

    // Some unicode (20% of content)
    for _ in 0..200 {
        chars.push(MockChar('α'));
        chars.push(MockChar('β'));
        chars.push(MockChar('γ'));
    }

    chars
}

fn bench_char_to_string_implementations(c: &mut Criterion) {
    let test_data = create_test_data();

    let mut group = c.benchmark_group("char_to_string_hot_path");

    group.bench_function("current_implementation", |b| {
        b.iter(|| current_char_to_string_hot_path(std::hint::black_box(&test_data)))
    });

    group.bench_function("with_string_interning", |b| {
        b.iter(|| {
            optimized_char_to_string_with_interning(std::hint::black_box(&test_data))
        })
    });

    group.bench_function("with_ascii_cache", |b| {
        b.iter(|| {
            optimized_char_to_string_with_ascii_cache(std::hint::black_box(&test_data))
        })
    });

    group.bench_function("with_str_refs", |b| {
        b.iter(|| optimized_char_to_str_refs(std::hint::black_box(&test_data)))
    });

    group.bench_function("with_capacity_and_cache", |b| {
        b.iter(|| {
            optimized_with_capacity_and_ascii_cache(std::hint::black_box(&test_data))
        })
    });

    group.finish();
}

criterion_group!(benches, bench_char_to_string_implementations);
criterion_main!(benches);
