// WA is a fork of https://github.com/rust-windowing/wa/
// wa is is licensed under Apache 2.0 license https://github.com/rust-windowing/wa/blob/master/LICENSE

use objc2::{extern_protocol, ProtocolType};

extern_protocol!(
    #[allow(clippy::missing_safety_doc)]
    pub(crate) unsafe trait NSTextInputClient {
        // TODO: Methods
    }

    unsafe impl ProtocolType for dyn NSTextInputClient {}
);
