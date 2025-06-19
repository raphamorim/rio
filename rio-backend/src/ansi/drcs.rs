/// DRCS (Dynamically Redefinable Character Set) support for VT320 terminal
/// Implements handling for the soft character set functionality
/// Based on https://vt100.net/dec/vt320/soft_characters

use rustc_hash::FxHashMap;
use std::str;
use base64::engine::general_purpose::STANDARD as Base64;
use base64::Engine;

#[derive(Debug)]
pub struct DrcsCharacter {
    pub data: Vec<u8>,
    pub width: u8,
    pub height: u8,
}

#[derive(Default, Debug)]
pub struct DrcsSet {
    characters: FxHashMap<u8, DrcsCharacter>,
    // Current active DRCS set ID
    active_set: u8,
}

impl DrcsSet {
    pub fn new() -> Self {
        Self {
            characters: FxHashMap::default(),
            active_set: 0,
        }
    }
    pub fn define_character(&mut self, char_code: u8, width: u8, height: u8, data: Vec<u8>) {
        self.characters.insert(
            char_code,
            DrcsCharacter {
                data,
                width,
                height,
            },
        );
    }
    pub fn set_active_set(&mut self, set_id: u8) {
        // Set the active DRCS character set
        self.active_set = set_id;
    }
    pub fn get_character(&self, char_code: u8) -> Option<&DrcsCharacter> {
        self.characters.get(&char_code)
    }
    pub fn clear(&mut self) {
        self.characters.clear();
    }
}

/// Parse OSC parameters for DRCS soft character definition
pub fn parse_drcs(params: &[&[u8]]) -> Option<(u8, u8, u8, Vec<u8>)> {
    // Format: OSC 53 ; char_code ; width ; height ; base64_data ST
    if params.len() < 5 {
        return None;
    }

    // Parse character code
    let char_code = str::from_utf8(params[1])
        .ok()?
        .parse::<u8>()
        .ok()?;

    // Parse width and height
    let width = str::from_utf8(params[2])
        .ok()?
        .parse::<u8>()
        .ok()?;
    
    let height = str::from_utf8(params[3])
        .ok()?
        .parse::<u8>()
        .ok()?;

    // Parse base64 data
    let bitmap_data = Base64.decode(params[4]).ok()?;

    // Verify the bitmap data has the expected size
    let expected_size = ((width as usize) * (height as usize) + 7) / 8; // ceil(width * height / 8)
    if bitmap_data.len() != expected_size {
        return None;
    }

    Some((char_code, width, height, bitmap_data))
}

/// Parse OSC parameters for selecting a DRCS set
pub fn parse_drcs_select(params: &[&[u8]]) -> Option<u8> {
    // Format: OSC 54 ; set_id ST
    if params.len() < 2 {
        return None;
    }

    // Parse set ID
    str::from_utf8(params[1])
        .ok()?
        .parse::<u8>()
        .ok()
}


// Convert a DRCS bitmap to a displayable format as string
// Rio case need to be sugarloaf
pub fn drcs_to_string(data: &[u8], width: u8, height: u8) -> String {
    let mut result = String::new();

    for y in 0..height {
        for x in 0..width {
            let byte_index = (y as usize * width as usize + x as usize) / 8;
            let bit_index = 7 - ((y as usize * width as usize + x as usize) % 8);

            if byte_index < data.len() {
                let pixel = (data[byte_index] >> bit_index) & 1;
                result.push(if pixel == 1 { '█' } else { ' ' });
            } else {
                result.push('?');
            }
        }
        result.push('\n');
    }

    result
}

/// Create a DRCS bitmap from a text representation
pub fn string_to_drcs(text: &str, width: u8, height: u8) -> Vec<u8> {
    let mut data = vec![0u8; ((width as usize * height as usize) + 7) / 8];

    for (i, c) in text.chars().enumerate() {
        if i >= width as usize * height as usize {
            break;
        }

        let y = i / width as usize;
        let x = i % width as usize;

        if c != ' ' {
            let byte_index = (y * width as usize + x) / 8;
            let bit_index = 7 - ((y * width as usize + x) % 8);

            data[byte_index] |= 1 << bit_index;
        }
    }

    data
}

pub fn test() {
    // Define a simple character (a smiley face)
    let _smiley_data = string_to_drcs(
        "  ####  \
         #      #\
         # #  # #\
         #      #\
         #  ##  #\
         # #  # #\
         #      #\
         #      #\
          ####  ",
        8, 8
    );

    // Define the smiley face as character code 65 ('A')
    // terminal.define_soft_character(65, 8, 8, smiley_data);

    // if terminal.is_drcs_character(65) {
        // if let Some(bitmap) = terminal.render_drcs_character(65) {
            // let visual = utils::drcs_to_string(&bitmap, 8, 8);
            // println!("DRCS character 65:\n{}", visual);
        // }
}
