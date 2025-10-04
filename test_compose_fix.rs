// Test to verify the compose_pair fix works
use std::str;

// Copy the essential composition functions from Rio
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
            Some(unsafe { core::char::from_u32_unchecked(a + (b - TBASE)) })
        } else {
            None
        }
    } else {
        let li = a - LBASE;
        let vi = b - VBASE;
        Some(unsafe { core::char::from_u32_unchecked(SBASE + li * NCOUNT + vi * TCOUNT) })
    }
}

fn compose_pair(a: char, b: char) -> Option<char> {
    if let Some(c) = compose_hangul(a, b) {
        return Some(c);
    }
    // For non-Hangul, return None (we don't need full composition logic for this test)
    None
}

fn main() {
    // Test the specific bytes from the issue
    let bytes = b"\xe1\x84\x92\xe1\x85\xa1\xe1\x86\xab";
    
    // Parse as UTF-8
    let s = match str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            println!("Invalid UTF-8: {:?}", e);
            return;
        }
    };
    
    println!("Input string: {}", s);
    
    // Test the composition logic that would be used in the cluster
    let chars: Vec<char> = s.chars().collect();
    if chars.len() == 3 {
        println!("\nTesting step-by-step composition:");
        
        // Simulate the cluster composition logic
        let mut last = chars[0];
        println!("Start with: '{}' U+{:04X}", last, last as u32);
        
        for (i, &ch) in chars[1..].iter().enumerate() {
            println!("Trying to compose '{}' U+{:04X} with '{}' U+{:04X}", 
                    last, last as u32, ch, ch as u32);
                    
            if let Some(comp) = compose_pair(last, ch) {
                println!("  -> Success: '{}' U+{:04X}", comp, comp as u32);
                last = comp;
            } else {
                println!("  -> Failed to compose");
                last = ch;
            }
        }
        
        println!("\nFinal result: '{}' U+{:04X}", last, last as u32);
        println!("Expected:    '한' U+{:04X}", '한' as u32);
        println!("Match: {}", last == '한');
    }
}