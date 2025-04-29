use core::mem;

#[allow(dead_code)]
#[repr(u8)]
#[derive(PartialEq, Eq, Debug, Default, Copy, Clone)]
pub enum State {
    CsiEntry,
    CsiIgnore,
    CsiIntermediate,
    CsiParam,
    DcsEntry,
    DcsIgnore,
    DcsIntermediate,
    DcsParam,
    DcsPassthrough,
    Escape,
    EscapeIntermediate,
    OscString,
    SosString,
    ApcString,
    PmString,
    Anywhere,
    #[default]
    Ground,
}

// NOTE: Removing the unused actions prefixed with `_` will reduce performance.
#[allow(dead_code)]
#[repr(u8)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Action {
    None,
    _Clear,
    Collect,
    CsiDispatch,
    EscDispatch,
    Execute,
    _Hook,
    _Ignore,
    _OscEnd,
    OscPut,
    _OscStart,
    Param,
    _Print,
    Put,
    _Unhook,
}

/// Unpack a u8 into a State and Action
///
/// The implementation of this assumes that there are *precisely* 16 variants
/// for both Action and State. Furthermore, it assumes that the enums are
/// tag-only; that is, there is no data in any variant.
///
/// Bad things will happen if those invariants are violated.
#[inline(always)]
pub fn unpack(delta: u8) -> (State, Action) {
    unsafe {
        (
            // State is stored in bottom 4 bits
            mem::transmute::<u8, State>(delta & 0x0F),
            // Action is stored in top 4 bits
            mem::transmute::<u8, Action>(delta >> 4),
        )
    }
}

#[inline(always)]
pub const fn pack(state: State, action: Action) -> u8 {
    (action as u8) << 4 | state as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_state_action() {
        match unpack(0xEE) {
            (State::Ground, Action::_Unhook) => (),
            _ => panic!("unpack failed"),
        }

        match unpack(0x0E) {
            (State::Ground, Action::None) => (),
            _ => panic!("unpack failed"),
        }

        match unpack(0xE0) {
            (State::CsiEntry, Action::_Unhook) => (),
            _ => panic!("unpack failed"),
        }
    }

    #[test]
    fn pack_state_action() {
        assert_eq!(pack(State::Ground, Action::_Unhook), 0xEE);
        assert_eq!(pack(State::Ground, Action::None), 0x0E);
        assert_eq!(pack(State::CsiEntry, Action::_Unhook), 0xE0);
    }
}
