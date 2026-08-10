//! Temporary audit: quantify per-char width drift vs unicode-width-16
//! (the crate rio ships today). Differences should be attributable to
//! Unicode 16→17 additions and upstream 0.2.x per-char corrections.
use rio_unicode::UnicodeWidthChar as New;
use unicode_width_16::UnicodeWidthChar as Old;

#[test]
fn report_char_width_drift() {
    let mut diffs: Vec<(u32, Option<usize>, Option<usize>)> = Vec::new();
    for cp in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        let old = Old::width(c);
        let new = New::width(c);
        if old != new {
            diffs.push((cp, old, new));
        }
    }
    eprintln!("total differing codepoints: {}", diffs.len());
    // Group into ranges for readability.
    let mut i = 0;
    while i < diffs.len() {
        let start = diffs[i];
        let mut j = i;
        while j + 1 < diffs.len()
            && diffs[j + 1].0 == diffs[j].0 + 1
            && diffs[j + 1].1 == start.1
            && diffs[j + 1].2 == start.2
        {
            j += 1;
        }
        eprintln!(
            "U+{:04X}..U+{:04X}  old={:?} new={:?}",
            start.0, diffs[j].0, start.1, start.2
        );
        i = j + 1;
    }
}
