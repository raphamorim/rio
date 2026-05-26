//! Linux per-codepoint font discovery via fontconfig. Moral twin of
//! CoreText's `CTFontCreateForString` — used by
//! `FontLibrary::resolve_font_for_char` to lazily register a system
//! font that covers a codepoint missing from already-loaded fonts.

use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::sync::OnceLock;

// The crate publishes as `yeslogic-fontconfig-sys` (see Cargo.toml) but
// its `[lib]` name is `fontconfig_sys`. Constants (FC_FAMILY, FC_CHARSET,
// …) live in a `constants` submodule rather than the crate root, so we
// pull them in explicitly.
use fontconfig_sys as fc;
use fontconfig_sys::constants::{
    FC_CHARSET, FC_FAMILY, FC_FILE, FC_INDEX, FC_LANG, FC_MONO, FC_SLANT,
    FC_SLANT_ITALIC, FC_SPACING, FC_WEIGHT, FC_WEIGHT_BOLD,
};

/// Process-global fontconfig handle. `FcConfigGetCurrent` returns a
/// shared pointer the C library treats as immutable for our purposes
/// (we only call read-only query functions on it). Wrapping in a
/// newtype so it satisfies `Send + Sync` for `OnceLock`.
struct FcHandle(*mut fc::FcConfig);

// SAFETY: All call sites use only read-only fontconfig functions
// (`FcFontSort`, `FcPatternCreate`, etc.). The library guarantees
// these are thread-safe on a non-mutating config. We never call
// `FcConfigSetCurrent`, `FcConfigBuildFonts`, or any other mutating
// API on this pointer.
unsafe impl Send for FcHandle {}
unsafe impl Sync for FcHandle {}

static FC_HANDLE: OnceLock<FcHandle> = OnceLock::new();

fn fc_config() -> *mut fc::FcConfig {
    FC_HANDLE
        .get_or_init(|| FcHandle(unsafe { fc::FcConfigGetCurrent() }))
        .0
}

/// Find a font file installed on the system that contains `ch`.
///
/// `primary_family` is passed as a hint so fontconfig prefers fonts
/// matching the user's chosen family (Noto Sans CJK over Source Han Sans
/// when the primary is Noto Sans, etc.). `want_mono` biases toward
/// monospace candidates — terminals want consistent cell widths even
/// for fallback glyphs. Style hints (`want_bold`, `want_italic`) are
/// best-effort; many fallback families ship only Regular and
/// fontconfig will downgrade gracefully.
///
/// Returns `(path, face_index)` for the highest-ranked match that
/// actually contains the codepoint, or `None` when fontconfig has
/// no candidate (e.g. truly missing script support — emoji-only system
/// asked for U+0041 with the emoji font masking everything else).
pub fn discover_fallback(
    primary_family: &str,
    ch: char,
    want_mono: bool,
    want_bold: bool,
    want_italic: bool,
) -> Option<(PathBuf, u32)> {
    let cfg = fc_config();
    if cfg.is_null() {
        return None;
    }

    unsafe {
        // Build a CharSet containing the single codepoint we need.
        // Fontconfig will rank candidates that cover this character
        // ahead of those that don't.
        let charset = fc::FcCharSetCreate();
        if charset.is_null() {
            return None;
        }
        if fc::FcCharSetAddChar(charset, ch as fc::FcChar32) == 0 {
            fc::FcCharSetDestroy(charset);
            return None;
        }

        let pattern = fc::FcPatternCreate();
        if pattern.is_null() {
            fc::FcCharSetDestroy(charset);
            return None;
        }

        // Family hint — fontconfig prefers fonts whose family name
        // matches or that have a strong alias to the primary.
        let family_c = match CString::new(primary_family) {
            Ok(s) => s,
            Err(_) => CString::new("monospace").unwrap(),
        };
        fc::FcPatternAddString(
            pattern,
            FC_FAMILY.as_ptr(),
            family_c.as_ptr() as *const fc::FcChar8,
        );

        // Charset constraint — the killer feature. fontconfig's sort
        // gives heaviest weight to charset coverage, so the top
        // candidate is the best font that contains `ch`.
        fc::FcPatternAddCharSet(pattern, FC_CHARSET.as_ptr(), charset);

        // Language hint from the environment so CJK gets the right
        // regional variant (zh-CN vs zh-TW vs ja vs ko) when multiple
        // CJK fonts are installed. Best-effort — fontconfig falls back
        // to the system default if the tag is missing/unknown.
        if let Some(lang) = current_lang() {
            if let Ok(lang_c) = CString::new(lang) {
                fc::FcPatternAddString(
                    pattern,
                    FC_LANG.as_ptr(),
                    lang_c.as_ptr() as *const fc::FcChar8,
                );
            }
        }

        if want_mono {
            fc::FcPatternAddInteger(pattern, FC_SPACING.as_ptr(), FC_MONO);
        }
        if want_bold {
            fc::FcPatternAddInteger(pattern, FC_WEIGHT.as_ptr(), FC_WEIGHT_BOLD);
        }
        if want_italic {
            fc::FcPatternAddInteger(pattern, FC_SLANT.as_ptr(), FC_SLANT_ITALIC);
        }

        // Apply the standard substitution rules so user aliases
        // (e.g. `monospace` → `Source Code Pro`) are honored.
        fc::FcConfigSubstitute(cfg, pattern, fc::FcMatchPattern);
        fc::FcDefaultSubstitute(pattern);

        // FcFontSort returns a sorted FontSet; the first entry whose
        // charset actually contains `ch` is our answer. We re-check
        // the codepoint per-candidate because `FC_CHARSET` in the
        // pattern is a *preference*, not a hard filter — fontconfig
        // may rank a non-covering font ahead of a covering one if
        // family/style match strongly enough.
        let mut result: fc::FcResult = 0;
        let font_set = fc::FcFontSort(
            cfg,
            pattern,
            1, // trim: drop fonts whose charset is fully covered by an earlier match
            ptr::null_mut(),
            &mut result,
        );

        let answer = if !font_set.is_null() && result == fc::FcResultMatch {
            let set = &*font_set;
            let mut found = None;
            for i in 0..set.nfont as isize {
                let candidate = *set.fonts.offset(i);
                if pattern_has_char(candidate, ch) {
                    if let Some(pair) = pattern_path_and_index(candidate) {
                        found = Some(pair);
                        break;
                    }
                }
            }
            found
        } else {
            None
        };

        if !font_set.is_null() {
            fc::FcFontSetDestroy(font_set);
        }
        fc::FcPatternDestroy(pattern);
        fc::FcCharSetDestroy(charset);

        answer
    }
}

/// `true` if the candidate pattern's `FC_CHARSET` contains `ch`. Some
/// fonts in the sort result are ranked highly for family/style match
/// without actually covering the codepoint we asked for, so we re-check
/// here before committing to the path.
unsafe fn pattern_has_char(pattern: *mut fc::FcPattern, ch: char) -> bool {
    unsafe {
        let mut charset_ptr: *mut fc::FcCharSet = ptr::null_mut();
        let res =
            fc::FcPatternGetCharSet(pattern, FC_CHARSET.as_ptr(), 0, &mut charset_ptr);
        if res != fc::FcResultMatch || charset_ptr.is_null() {
            return false;
        }
        fc::FcCharSetHasChar(charset_ptr, ch as fc::FcChar32) != 0
    }
}

/// Pull out the on-disk file path + face index from a sorted candidate.
/// Returns `None` for patterns missing FC_FILE (in-memory fonts that
/// fontconfig somehow surfaced — shouldn't happen for system fonts but
/// guards us regardless).
unsafe fn pattern_path_and_index(pattern: *mut fc::FcPattern) -> Option<(PathBuf, u32)> {
    unsafe {
        let mut file_ptr: *mut fc::FcChar8 = ptr::null_mut();
        let res = fc::FcPatternGetString(pattern, FC_FILE.as_ptr(), 0, &mut file_ptr);
        if res != fc::FcResultMatch || file_ptr.is_null() {
            return None;
        }
        let path_str = CStr::from_ptr(file_ptr as *const std::ffi::c_char)
            .to_str()
            .ok()?
            .to_string();

        let mut index: i32 = 0;
        let _ = fc::FcPatternGetInteger(pattern, FC_INDEX.as_ptr(), 0, &mut index);
        Some((PathBuf::from(path_str), index.max(0) as u32))
    }
}

/// Best-effort BCP-47 tag from `LC_CTYPE` / `LANG`. fontconfig accepts
/// a fairly relaxed format ("zh-cn", "ja_JP.UTF-8") and normalizes
/// internally. Returns `None` when neither env var is set or POSIX/C
/// locale is reported (no language preference to express).
fn current_lang() -> Option<String> {
    let raw = std::env::var("LC_CTYPE")
        .ok()
        .or_else(|| std::env::var("LANG").ok())?;
    if raw.is_empty() || raw == "C" || raw == "POSIX" {
        return None;
    }
    // Strip codeset / modifier suffixes ("ja_JP.UTF-8@cjkv" → "ja_JP").
    let trimmed = raw
        .split(['.', '@'])
        .next()
        .unwrap_or(&raw)
        .replace('_', "-");
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
