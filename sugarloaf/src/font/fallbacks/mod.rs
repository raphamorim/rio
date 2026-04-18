#[cfg(target_os = "macos")]
pub fn external_fallbacks() -> Vec<String> {
    // Empty on macOS by design: CoreText's default cascade list
    // (`CTFontCopyDefaultCascadeListForLanguages`, wired in
    // `FontLibraryData::load`) already includes Menlo / Geneva / Arial
    // Unicode MS / Apple Color Emoji and whatever else the system considers
    // the right fallback chain for the primary font. Hardcoding family
    // names here would duplicate that list and fight it.
    Vec::new()
}

#[cfg(target_os = "windows")]
pub fn external_fallbacks() -> Vec<String> {
    vec![
        // Lucida Sans Unicode
        // Microsoft JhengHei
        String::from("Segoe UI"),
        // String::from("Segoe UI Emoji"),
        String::from("Segoe UI Symbol"),
        String::from("Segoe UI Historic"),
    ]
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn external_fallbacks() -> Vec<String> {
    vec![
        /* Sans-serif fallbacks */
        String::from("Noto Sans"),
        /* More sans-serif fallbacks */
        String::from("DejaVu Sans"),
        String::from("FreeSans"),
        /* Mono fallbacks */
        String::from("Noto Sans Mono"),
        String::from("DejaVu Sans Mono"),
        String::from("FreeMono"),
        /* Symbols fallbacks */
        String::from("Noto Sans Symbols"),
        String::from("Noto Sans Symbols2"),
        // String::from("Noto Color Emoji"),
    ]
}
