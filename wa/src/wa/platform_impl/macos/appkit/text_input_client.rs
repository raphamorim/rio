use objc2::{extern_protocol, ProtocolType};

extern_protocol!(
    #[allow(clippy::missing_safety_doc)]
    pub(crate) unsafe trait NSTextInputClient {
        // TODO: Methods
    }

    unsafe impl ProtocolType for dyn NSTextInputClient {}
);
