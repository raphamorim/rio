// Test Korean Hangul Jamo composition using Rio's compose function
use std::char;

// Copy the Hangul composition constants and functions from Rio
const LBASE: u32 = 0x1100;
const VBASE: u32 = 0x1161;
const TBASE: u32 = 0x11A7;
const LCOUNT: u32 = 19;
const VCOUNT: u32 = 21;
const TCOUNT: u32 = 28;
const NCOUNT: u32 = VCOUNT * TCOUNT;
const SCOUNT: u32 = LCOUNT * NCOUNT;
const SBASE: u32 = 0xAC00;

fn is_hangul(c: char) -> bool {
    let c = c as u32;
    (SBASE..(SBASE + SCOUNT)).contains(&c)
}

fn compose_hangul(a: char, b: char) -> Option<char> {
    let a = a as u32;
    let b = b as u32;
    if !(VBASE..(TBASE + TCOUNT)).contains(&b) {
        return None;
    }
    if !(LBASE..(LBASE + LCOUNT)).contains(&a) && !(SBASE..(SBASE + SCOUNT)).contains(&a)
    {
        return None;
    }
    if a >= SBASE {
        if (a - SBASE).is_multiple_of(TCOUNT) {
            Some(unsafe { char::from_u32_unchecked(a + (b - TBASE)) })
        } else {
            None
        }
    } else {
        let li = a - LBASE;
        let vi = b - VBASE;
        Some(unsafe { char::from_u32_unchecked(SBASE + li * NCOUNT + vi * TCOUNT) })
    }
}

fn main() {
    // Test the specific bytes from the issue
    let bytes = b"\xe1\x84\x92\xe1\x85\xa1\xe1\x86\xab";
    
    // Parse as UTF-8
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            println!("Invalid UTF-8: {:?}", e);
            return;
        }
    };
    
    println!("Input string: {}", s);
    println!("String length: {}", s.len());
    
    // Show each character with its code point
    for (i, ch) in s.chars().enumerate() {
        println!("Char {}: '{}' U+{:04X}", i, ch, ch as u32);
    }
    
    // Test composition manually using Rio's compose_hangul function
    let chars: Vec<char> = s.chars().collect();
    if chars.len() == 3 {
        println!("\nTesting composition:");
        
        // First, check if these are the right Hangul Jamo characters
        let first = chars[0] as u32;
        let second = chars[1] as u32;
        let third = chars[2] as u32;
        
        println!("First char U+{:04X} - LBASE+{}: {}", first, first - LBASE, first >= LBASE && first < LBASE + LCOUNT);
        println!("Second char U+{:04X} - VBASE+{}: {}", second, second - VBASE, second >= VBASE && second < VBASE + VCOUNT);
        println!("Third char U+{:04X} - TBASE+{}: {}", third, third - TBASE, third > TBASE && third < TBASE + TCOUNT);
        
        // Try to compose first two (L + V)
        if let Some(lv_composed) = compose_hangul(chars[0], chars[1]) {
            println!("L+V composition: '{}' U+{:04X}", lv_composed, lv_composed as u32);
            
            // Then try to add the trailing consonant (T)
            if let Some(final_composed) = compose_hangul(lv_composed, chars[2]) {
                println!("L+V+T composition: '{}' U+{:04X}", final_composed, final_composed as u32);
            } else {
                println!("Failed to compose with trailing consonant");
            }
        } else {
            println!("Failed to compose first two characters");
        }
    }
    
    // Compare with expected result
    let expected = '한';
    println!("\nExpected: '{}' U+{:04X}", expected, expected as u32);
    
    // Test if expected character is Hangul
    println!("Is '한' Hangul? {}", is_hangul(expected));
}