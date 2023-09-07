// Fow now Rio implement programming ligatures based
// on pre-compiled combinations and not on font features
//
// Some good resources about ligatures in programming fonts
// - https://practicaltypography.com/ligatures-in-programming-fonts-hell-no.html
//
//
// Good to remember that programming ligatures they contradict unicode
// so it should only work in the renderer level, any time an user copy
// the sugarloaf content it should come as the original sequence of characters

use std::collections::HashMap;

type LigatureStartingInput = char;
type LigatureSequence = Vec<char>;

struct Ligature {
    sequence: LigatureSequence,
    value: String,
}

impl Ligature {
    fn new(sequence: LigatureSequence, value: &str) -> Ligature {
        Ligature {
            sequence,
            value: value.to_string(),
        }
    }
}

struct Ligatures {
    combinations: HashMap<LigatureStartingInput, Ligature>,
}

impl Ligatures {
    fn new() -> Ligatures {
        let mut combinations = HashMap::new();
        combinations.insert('-', Ligature::new(vec!['>'], ""));

        Ligatures { combinations }
    }
}
