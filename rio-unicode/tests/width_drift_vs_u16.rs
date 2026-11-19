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
    // The audit is a tripwire, not a narration: regenerated tables
    // that drift beyond the known Unicode 16→17 delta must fail CI.
    // Every one of these 196 is a U17 addition or reclassification;
    // revisit the count (and the spot checks below) on the next
    // Unicode bump.
    assert_eq!(
        diffs.len(),
        196,
        "unexpected width drift vs unicode-width-16"
    );
    // Spot-pin a few known U17 changes in both directions.
    assert_eq!(New::width('\u{1ACF}'), Some(0), "new combining mark");
    assert_eq!(Old::width('\u{1ACF}'), Some(1));
    assert_eq!(New::width('\u{16FF2}'), Some(2), "new wide ideograph");
    assert_eq!(New::width('\u{1FAEA}'), Some(2), "new emoji");
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
