#[cfg(target_os = "macos")]
pub fn external_fallbacks() -> Vec<String> {
    vec![
        String::from(".SF NS"),
        String::from("Menlo"),
        String::from("Geneva"),
        String::from("Arial Unicode MS"),
        // String::from("Noto Emoji"),
        // String::from("Noto Color Emoji"),
    ]
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
