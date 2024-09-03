use objc2::{extern_protocol, ProtocolType};

extern_protocol!(
    pub(crate) unsafe trait NSTextInputClient {
        // TODO: Methods
    }

    unsafe impl ProtocolType for dyn NSTextInputClient {}
);
