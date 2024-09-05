use tracing::warn;

#[derive(Debug, Eq, PartialEq)]
pub enum Mode {
    /// ?1
    CursorKeys = 1,
    /// Select 80 or 132 columns per page (DECCOLM).
    ///
    /// CSI ? 3 h -> set 132 column font.
    /// CSI ? 3 l -> reset 80 column font.
    ///
    /// Additionally,
    ///
    /// * set margins to default positions
    /// * erases all data in page memory
    /// * resets DECLRMM to unavailable
    /// * clears data from the status line (if set to host-writable)
    Column = 3,
    /// IRM Insert Mode.
    ///
    /// NB should be part of non-private mode enum.
    ///
    /// * `CSI 4 h` change to insert mode
    /// * `CSI 4 l` reset to replacement mode
    Insert = 4,
    /// ?6
    Origin = 6,
    /// ?7
    LineWrap = 7,
    /// ?12
    BlinkingCursor = 12,
    /// 20
    ///
    /// NB This is actually a private mode. We should consider adding a second
    /// enumeration for public/private modesets.
    LineFeedNewLine = 20,
    /// ?25
    ShowCursor = 25,
    /// ?80
    SixelDisplay = 80,
    /// ?1000
    ReportMouseClicks = 1000,
    /// ?1002
    ReportSquareMouseMotion = 1002,
    /// ?1003
    ReportAllMouseMotion = 1003,
    /// ?1004
    ReportFocusInOut = 1004,
    /// ?1005
    Utf8Mouse = 1005,
    /// ?1006
    SgrMouse = 1006,
    /// ?1007
    AlternateScroll = 1007,
    /// ?1042
    UrgencyHints = 1042,
    /// ?1049
    SwapScreenAndSetRestoreCursor = 1049,
    /// Use a private palette for each new graphic.
    SixelPrivateColorRegisters = 1070,
    /// ?2004
    BracketedPaste = 2004,
    /// Sixel scrolling leaves cursor to right of graphic.
    SixelCursorToTheRight = 8452,
}

impl Mode {
    /// Create mode from a primitive.
    pub fn from_primitive(intermediate: Option<&u8>, num: u16) -> Option<Mode> {
        let private = match intermediate {
            Some(b'?') => true,
            None => false,
            _ => return None,
        };

        if private {
            Some(match num {
                1 => Mode::CursorKeys,
                3 => Mode::Column,
                6 => Mode::Origin,
                7 => Mode::LineWrap,
                12 => Mode::BlinkingCursor,
                25 => Mode::ShowCursor,
                80 => Mode::SixelDisplay,
                1000 => Mode::ReportMouseClicks,
                1002 => Mode::ReportSquareMouseMotion,
                1003 => Mode::ReportAllMouseMotion,
                1004 => Mode::ReportFocusInOut,
                1005 => Mode::Utf8Mouse,
                1006 => Mode::SgrMouse,
                1007 => Mode::AlternateScroll,
                1042 => Mode::UrgencyHints,
                1049 => Mode::SwapScreenAndSetRestoreCursor,
                1070 => Mode::SixelPrivateColorRegisters,
                2004 => Mode::BracketedPaste,
                8452 => Mode::SixelCursorToTheRight,
                _ => {
                    warn!("[unimplemented] primitive mode: {}", num);
                    return None;
                }
            })
        } else {
            Some(match num {
                4 => Mode::Insert,
                20 => Mode::LineFeedNewLine,
                _ => return None,
            })
        }
    }
}
