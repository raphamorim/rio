/// This is the state change table. It's indexed first by current state and then by the next
/// character in the pty stream.
use crate::definitions::{pack, Action, State};

use rio_proc_macros::generate_state_changes;

// Generate state changes at compile-time
pub static STATE_CHANGES: [[u8; 256]; 16] = state_changes();
generate_state_changes!(state_changes, {
    Anywhere {
        0x18 => (Ground, Execute),
        0x1a => (Ground, Execute),
        0x1b => (Escape, None),
    },

    Ground {
        0x00..=0x17 => (Anywhere, Execute),
        0x19        => (Anywhere, Execute),
        0x1c..=0x1f => (Anywhere, Execute),
        0x20..=0x7f => (Anywhere, Print),
        0x80..=0x8f => (Anywhere, Execute),
        0x91..=0x9a => (Anywhere, Execute),
        0x9c        => (Anywhere, Execute),
        // Beginning of UTF-8 2 byte sequence
        0xc2..=0xdf => (Utf8, BeginUtf8),
        // Beginning of UTF-8 3 byte sequence
        0xe0..=0xef => (Utf8, BeginUtf8),
        // Beginning of UTF-8 4 byte sequence
        0xf0..=0xf4 => (Utf8, BeginUtf8),
    },

    Escape {
        0x00..=0x17 => (Anywhere, Execute),
        0x19        => (Anywhere, Execute),
        0x1c..=0x1f => (Anywhere, Execute),
        0x7f        => (Anywhere, Ignore),
        0x20..=0x2f => (EscapeIntermediate, Collect),
        0x30..=0x4f => (Ground, EscDispatch),
        0x51..=0x57 => (Ground, EscDispatch),
        0x59        => (Ground, EscDispatch),
        0x5a        => (Ground, EscDispatch),
        0x5c        => (Ground, EscDispatch),
        0x60..=0x7e => (Ground, EscDispatch),
        0x5b        => (CsiEntry, None),
        0x5d        => (OscString, None),
        0x50        => (DcsEntry, None),
        0x58        => (SosPmApcString, None),
        0x5e        => (SosPmApcString, None),
        0x5f        => (SosPmApcString, None),
    },

    EscapeIntermediate {
        0x00..=0x17 => (Anywhere, Execute),
        0x19        => (Anywhere, Execute),
        0x1c..=0x1f => (Anywhere, Execute),
        0x20..=0x2f => (Anywhere, Collect),
        0x7f        => (Anywhere, Ignore),
        0x30..=0x7e => (Ground, EscDispatch),
    },

    CsiEntry {
        0x00..=0x17 => (Anywhere, Execute),
        0x19        => (Anywhere, Execute),
        0x1c..=0x1f => (Anywhere, Execute),
        0x7f        => (Anywhere, Ignore),
        0x20..=0x2f => (CsiIntermediate, Collect),
        0x30..=0x39 => (CsiParam, Param),
        0x3a..=0x3b => (CsiParam, Param),
        0x3c..=0x3f => (CsiParam, Collect),
        0x40..=0x7e => (Ground, CsiDispatch),
    },

    CsiIgnore {
        0x00..=0x17 => (Anywhere, Execute),
        0x19        => (Anywhere, Execute),
        0x1c..=0x1f => (Anywhere, Execute),
        0x20..=0x3f => (Anywhere, Ignore),
        0x7f        => (Anywhere, Ignore),
        0x40..=0x7e => (Ground, None),
    },

    CsiParam {
        0x00..=0x17 => (Anywhere, Execute),
        0x19        => (Anywhere, Execute),
        0x1c..=0x1f => (Anywhere, Execute),
        0x30..=0x39 => (Anywhere, Param),
        0x3a..=0x3b => (Anywhere, Param),
        0x7f        => (Anywhere, Ignore),
        0x3c..=0x3f => (CsiIgnore, None),
        0x20..=0x2f => (CsiIntermediate, Collect),
        0x40..=0x7e => (Ground, CsiDispatch),
    },

    CsiIntermediate {
        0x00..=0x17 => (Anywhere, Execute),
        0x19        => (Anywhere, Execute),
        0x1c..=0x1f => (Anywhere, Execute),
        0x20..=0x2f => (Anywhere, Collect),
        0x7f        => (Anywhere, Ignore),
        0x30..=0x3f => (CsiIgnore, None),
        0x40..=0x7e => (Ground, CsiDispatch),
    },

    DcsEntry {
        0x00..=0x17 => (Anywhere, Ignore),
        0x19        => (Anywhere, Ignore),
        0x1c..=0x1f => (Anywhere, Ignore),
        0x7f        => (Anywhere, Ignore),
        0x20..=0x2f => (DcsIntermediate, Collect),
        0x30..=0x39 => (DcsParam, Param),
        0x3a..=0x3b => (DcsParam, Param),
        0x3c..=0x3f => (DcsParam, Collect),
        0x40..=0x7e => (DcsPassthrough, None),
    },

    DcsIntermediate {
        0x00..=0x17 => (Anywhere, Ignore),
        0x19        => (Anywhere, Ignore),
        0x1c..=0x1f => (Anywhere, Ignore),
        0x20..=0x2f => (Anywhere, Collect),
        0x7f        => (Anywhere, Ignore),
        0x30..=0x3f => (DcsIgnore, None),
        0x40..=0x7e => (DcsPassthrough, None),
    },

    DcsIgnore {
        0x00..=0x17 => (Anywhere, Ignore),
        0x19        => (Anywhere, Ignore),
        0x1c..=0x1f => (Anywhere, Ignore),
        0x20..=0x7f => (Anywhere, Ignore),
        0x9c        => (Ground, None),
    },

    DcsParam {
        0x00..=0x17 => (Anywhere, Ignore),
        0x19        => (Anywhere, Ignore),
        0x1c..=0x1f => (Anywhere, Ignore),
        0x30..=0x39 => (Anywhere, Param),
        0x3a..=0x3b => (Anywhere, Param),
        0x7f        => (Anywhere, Ignore),
        0x3c..=0x3f => (DcsIgnore, None),
        0x20..=0x2f => (DcsIntermediate, Collect),
        0x40..=0x7e => (DcsPassthrough, None),
    },

    DcsPassthrough {
        0x00..=0x17 => (Anywhere, Put),
        0x19        => (Anywhere, Put),
        0x1c..=0x1f => (Anywhere, Put),
        0x20..=0x7e => (Anywhere, Put),
        0x7f        => (Anywhere, Ignore),
        0x9c        => (Ground, None),
    },

    SosPmApcString {
        0x00..=0x17 => (Anywhere, Ignore),
        0x19        => (Anywhere, Ignore),
        0x1c..=0x1f => (Anywhere, Ignore),
        0x20..=0x7f => (Anywhere, Ignore),
        0x9c        => (Ground, None),
    },

    OscString {
        0x00..=0x06 => (Anywhere, Ignore),
        0x07        => (Ground, None),
        0x08..=0x17 => (Anywhere, Ignore),
        0x19        => (Anywhere, Ignore),
        0x1c..=0x1f => (Anywhere, Ignore),
        0x20..=0xff => (Anywhere, OscPut),
    }
});
