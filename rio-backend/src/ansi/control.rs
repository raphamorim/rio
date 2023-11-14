// Retired from https://github.com/alacritty/alacritty/blob/766a3b5582fa8ee13506c0f23c9c145ff0012078/alacritty_terminal/src/ansi.rs#L1469

/// C0 set of 7-bit control characters (from ANSI X3.4-1977).
#[allow(non_snake_case)]
pub mod C0 {
    /// Null filler, terminal should ignore this character.
    #[allow(dead_code)]
    pub const NUL: u8 = 0x00;
    /// Start of Header.
    #[allow(dead_code)]
    pub const SOH: u8 = 0x01;
    /// Start of Text, implied end of header.
    #[allow(dead_code)]
    pub const STX: u8 = 0x02;
    /// End of Text, causes some terminal to respond with ACK or NAK.
    #[allow(dead_code)]
    pub const ETX: u8 = 0x03;
    /// End of Transmission.
    #[allow(dead_code)]
    pub const EOT: u8 = 0x04;
    /// Enquiry, causes terminal to send ANSWER-BACK ID.
    #[allow(dead_code)]
    pub const ENQ: u8 = 0x05;
    /// Acknowledge, usually sent by terminal in response to ETX.
    #[allow(dead_code)]
    pub const ACK: u8 = 0x06;
    /// Bell, triggers the bell, buzzer, or beeper on the terminal.
    pub const BEL: u8 = 0x07;
    /// Backspace, can be used to define overstruck characters.
    pub const BS: u8 = 0x08;
    /// Horizontal Tabulation, move to next predetermined position.
    pub const HT: u8 = 0x09;
    /// Linefeed, move to same position on next line (see also NL).
    pub const LF: u8 = 0x0A;
    /// Vertical Tabulation, move to next predetermined line.
    pub const VT: u8 = 0x0B;
    /// Form Feed, move to next form or page.
    pub const FF: u8 = 0x0C;
    /// Carriage Return, move to first character of current line.
    pub const CR: u8 = 0x0D;
    /// Shift Out, switch to G1 (other half of character set).
    #[allow(dead_code)]
    pub const SO: u8 = 0x0E;
    /// Shift In, switch to G0 (normal half of character set).
    #[allow(dead_code)]
    pub const SI: u8 = 0x0F;
    /// Data Link Escape, interpret next control character specially.
    #[allow(dead_code)]
    pub const DLE: u8 = 0x10;
    /// (DC1) Terminal is allowed to resume transmitting.
    #[allow(dead_code)]
    pub const XON: u8 = 0x11;
    /// Device Control 2, causes ASR-33 to activate paper-tape reader.
    #[allow(dead_code)]
    pub const DC2: u8 = 0x12;
    /// (DC2) Terminal must pause and refrain from transmitting.
    #[allow(dead_code)]
    pub const XOFF: u8 = 0x13;
    /// Device Control 4, causes ASR-33 to deactivate paper-tape reader.
    #[allow(dead_code)]
    pub const DC4: u8 = 0x14;
    /// Negative Acknowledge, used sometimes with ETX and ACK.
    #[allow(dead_code)]
    pub const NAK: u8 = 0x15;
    /// Synchronous Idle, used to maintain timing in Sync communication.
    #[allow(dead_code)]
    pub const SYN: u8 = 0x16;
    /// End of Transmission block.
    #[allow(dead_code)]
    pub const ETB: u8 = 0x17;
    /// Cancel (makes VT100 abort current escape sequence if any).
    #[allow(dead_code)]
    pub const CAN: u8 = 0x18;
    /// End of Medium.
    #[allow(dead_code)]
    pub const EM: u8 = 0x19;
    /// Substitute (VT100 uses this to display parity errors).
    #[allow(dead_code)]
    pub const SUB: u8 = 0x1A;
    /// Prefix to an escape sequence.
    #[allow(dead_code)]
    pub const ESC: u8 = 0x1B;
    /// File Separator.
    #[allow(dead_code)]
    pub const FS: u8 = 0x1C;
    /// Group Separator.
    #[allow(dead_code)]
    pub const GS: u8 = 0x1D;
    /// Record Separator (sent by VT132 in block-transfer mode).
    #[allow(dead_code)]
    pub const RS: u8 = 0x1E;
    /// Unit Separator.
    #[allow(dead_code)]
    pub const US: u8 = 0x1F;
    /// Delete, should be ignored by terminal.
    #[allow(dead_code)]
    pub const DEL: u8 = 0x7f;
}
