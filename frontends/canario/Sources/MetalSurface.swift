import AppKit
import RioKit
import SwiftUI

final class SurfaceRegistry {
    private var sessions: [UUID: PanelSession] = [:]

    var allSessions: [PanelSession] {
        Array(sessions.values)
    }

    func session(for panelID: UUID, terminal: TerminalItem) -> PanelSession {
        if let existing = sessions[panelID] {
            return existing
        }
        let session = PanelSession(panelID: panelID, terminal: terminal)
        sessions[panelID] = session
        return session
    }

    func existingSession(for panelID: UUID) -> PanelSession? {
        sessions[panelID]
    }

    func remove(_ panelID: UUID) {
        if let session = sessions.removeValue(forKey: panelID) {
            session.shutdown()
        }
    }
}

final class RioSurfaceNSView: NSView {
    weak var session: PanelSession?

    override var isFlipped: Bool { true }

    // CPU rendering: the session paints the current render state straight
    // into this view's AppKit graphics context.
    override func draw(_ dirtyRect: NSRect) {
        session?.drawSurface()
    }
}

final class PanelHostView: NSView {
    weak var session: PanelSession?
    let surfaceView = RioSurfaceNSView()
    private var selectionAnchor: NSPoint?
    private var selectionActive = false
    private var markedText = ""

    /// Non-nil while a `keyDown` is in flight. `insertText` appends here
    /// instead of sending, so the key handler can tell text the input method
    /// committed from a key that still needs encoding.
    fileprivate var keyTextAccumulator: [String]?

    /// Whether alt acts as meta. Kept alongside librio's own copy so the host
    /// knows whether to let alt take part in text translation.
    fileprivate var altIsMeta = true

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layerContentsRedrawPolicy = .duringViewResize
        layer?.cornerRadius = Theme.cardRadius
        layer?.masksToBounds = true
        layer?.backgroundColor = CGColor(red: 0.06, green: 0.06, blue: 0.07, alpha: 1)
        addSubview(surfaceView)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override var acceptsFirstResponder: Bool { true }

    // Cursor style depends on focus, so redraw whenever it changes.
    private func redrawForFocusChange() {
        surfaceView.setNeedsDisplay(surfaceView.bounds)
    }

    override func becomeFirstResponder() -> Bool {
        redrawForFocusChange()
        return super.becomeFirstResponder()
    }

    override func resignFirstResponder() -> Bool {
        redrawForFocusChange()
        return super.resignFirstResponder()
    }

    @objc private func windowKeyChanged() {
        redrawForFocusChange()
    }

    override func layout() {
        super.layout()
        surfaceView.frame = bounds
        session?.syncSize()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        NotificationCenter.default.removeObserver(
            self, name: NSWindow.didBecomeKeyNotification, object: nil)
        NotificationCenter.default.removeObserver(
            self, name: NSWindow.didResignKeyNotification, object: nil)
        guard let session, let window else { return }
        for name in [
            NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification,
        ] {
            NotificationCenter.default.addObserver(
                self, selector: #selector(windowKeyChanged), name: name,
                object: window)
        }
        session.startIfNeeded()
        if session.terminal.focusedPanelID == session.panelID {
            window.makeFirstResponder(self)
        }
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        guard let window else { return }
        session?.rescale(Float(window.backingScaleFactor))
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        guard let session else { return }
        let point = surfaceView.convert(event.locationInWindow, from: nil)
        switch event.clickCount {
        case 2:
            selectionActive = true
            session.selectionBegin(at: point, kind: UInt8(RIO_SELECTION_WORD))
        case 3:
            selectionActive = true
            session.selectionBegin(at: point, kind: UInt8(RIO_SELECTION_LINE))
        default:
            selectionActive = false
            selectionAnchor = point
            session.selectionClear()
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let session else { return }
        let point = surfaceView.convert(event.locationInWindow, from: nil)
        if !selectionActive {
            guard let anchor = selectionAnchor else { return }
            selectionActive = true
            session.selectionBegin(at: anchor, kind: UInt8(RIO_SELECTION_SIMPLE))
        }
        session.selectionUpdate(at: point)
    }

    override func mouseUp(with event: NSEvent) {
        selectionAnchor = nil
        super.mouseUp(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        let delta = event.scrollingDeltaY
        if delta != 0 {
            let lines = event.hasPreciseScrollingDeltas ? delta / 12.0 : delta
            session?.scroll(deltaLines: Int32(lines.rounded()))
            session?.render()
        }
    }

    /// Named keys, by macOS virtual keycode. Identity has to come from the
    /// keycode rather than from `doCommand(by:)`, which reports a selector
    /// chosen by AppKit's key-binding dictionaries and carries no modifiers
    /// with it, so shift+arrow and alt+arrow are indistinguishable there.
    private static let namedKeys: [UInt16: UInt32] = [
        0x24: UInt32(RIO_KEY_ENTER),  // Return
        0x4C: UInt32(RIO_KEY_ENTER),  // Keypad Enter
        0x30: UInt32(RIO_KEY_TAB),
        0x33: UInt32(RIO_KEY_BACKSPACE),  // Delete, which is backspace here
        0x35: UInt32(RIO_KEY_ESCAPE),
        0x75: UInt32(RIO_KEY_DELETE),  // Forward delete
        0x73: UInt32(RIO_KEY_HOME),
        0x77: UInt32(RIO_KEY_END),
        0x74: UInt32(RIO_KEY_PAGE_UP),
        0x79: UInt32(RIO_KEY_PAGE_DOWN),
        0x7B: UInt32(RIO_KEY_LEFT),
        0x7C: UInt32(RIO_KEY_RIGHT),
        0x7D: UInt32(RIO_KEY_DOWN),
        0x7E: UInt32(RIO_KEY_UP),
    ]

    /// Function keys, by keycode. Their numbering is not contiguous.
    private static let functionKeys: [UInt16: UInt8] = [
        0x7A: 1, 0x78: 2, 0x63: 3, 0x76: 4, 0x60: 5, 0x61: 6,
        0x62: 7, 0x64: 8, 0x65: 9, 0x6D: 10, 0x67: 11, 0x6F: 12,
    ]

    private func rioMods(_ flags: NSEvent.ModifierFlags) -> UInt8 {
        var mods: UInt8 = 0
        if flags.contains(.shift) { mods |= UInt8(RIO_MOD_SHIFT) }
        if flags.contains(.control) { mods |= UInt8(RIO_MOD_CTRL) }
        if flags.contains(.option) { mods |= UInt8(RIO_MOD_ALT) }
        if flags.contains(.command) { mods |= UInt8(RIO_MOD_SUPER) }
        return mods
    }

    override func keyDown(with event: NSEvent) {
        guard let session else { return }
        let flags = event.modifierFlags

        // Command belongs to the app: menu equivalents and window commands.
        if flags.contains(.command) {
            super.keyDown(with: event)
            return
        }

        let mods = rioMods(flags)
        let wasComposing = !markedText.isEmpty

        // Run the input method for its text, but collect it rather than
        // sending: only after this returns is it clear whether the text is a
        // commit to forward, a preedit update to draw, or a key to encode.
        keyTextAccumulator = []
        // Control never contributes to text, and alt only does when alt is not
        // acting as meta. Translating without them keeps the input method from
        // turning ctrl+a into an unrelated character.
        let translationEvent = event.strippingForTranslation(
            control: true,
            option: altIsMeta
        )
        interpretKeyEvents([translationEvent])
        let produced = keyTextAccumulator ?? []
        keyTextAccumulator = nil

        let composing = !markedText.isEmpty
        if composing {
            // Mid-composition: the preedit is already drawn, and nothing is
            // encoded until it commits.
            return
        }

        // Text an input method committed after composing is not a keystroke to
        // encode, it is the result of several. Forward it and stop.
        if wasComposing && !produced.isEmpty {
            for text in produced where !text.isEmpty {
                session.sendText(text)
            }
            return
        }

        if let tag = Self.namedKeys[event.keyCode] {
            session.sendKey(tag, mods: mods)
            return
        }

        if let number = Self.functionKeys[event.keyCode] {
            session.sendKey(UInt32(RIO_KEY_F), mods: mods, functionKey: number)
            return
        }

        // A text key. The unshifted codepoint identifies it, and the text the
        // platform produced rides along so shift, dead keys and non-Latin
        // layouts need no special casing here.
        guard let unshifted = event.charactersIgnoringModifiers?.unicodeScalars.first
        else { return }

        // `charactersIgnoringModifiers` still applies shift, so lowercase it to
        // report the key rather than the character.
        let codepoint =
            String(unshifted).lowercased().unicodeScalars.first?.value ?? unshifted.value

        session.sendKey(
            UInt32(RIO_KEY_CHAR),
            mods: mods,
            codepoint: codepoint,
            text: produced.first ?? event.characters
        )
    }
}

extension NSEvent {
    /// A copy of this event with modifiers removed that must not take part in
    /// text translation, so the input method produces the character the key
    /// would produce on its own.
    func strippingForTranslation(control: Bool, option: Bool) -> NSEvent {
        var flags = modifierFlags
        if control { flags.remove(.control) }
        if option { flags.remove(.option) }
        if flags == modifierFlags { return self }

        return NSEvent.keyEvent(
            with: type,
            location: locationInWindow,
            modifierFlags: flags,
            timestamp: timestamp,
            windowNumber: windowNumber,
            context: nil,
            characters: charactersIgnoringModifiers ?? "",
            charactersIgnoringModifiers: charactersIgnoringModifiers ?? "",
            isARepeat: isARepeat,
            keyCode: keyCode
        ) ?? self
    }
}

extension PanelHostView: NSTextInputClient {
    func insertText(_ string: Any, replacementRange: NSRange) {
        let text: String
        if let value = string as? String {
            text = value
        } else if let value = string as? NSAttributedString {
            text = value.string
        } else {
            return
        }
        if !markedText.isEmpty {
            markedText = ""
            session?.setPreedit(nil)
        }

        // Inside a keyDown: collect and let the key handler decide. Outside
        // one, this is text arriving on its own and goes straight through.
        if keyTextAccumulator != nil {
            keyTextAccumulator?.append(text)
            return
        }

        session?.sendText(text)
    }

    /// Only here to keep AppKit from beeping at unhandled selectors. Key
    /// identity comes from the keycode in `keyDown`: a selector cannot carry
    /// the modifiers, and which selector arrives depends on the user's key
    /// bindings, neither of which a terminal can work with.
    override func doCommand(by selector: Selector) {}

    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        if let value = string as? String {
            markedText = value
        } else if let value = string as? NSAttributedString {
            markedText = value.string
        }
        session?.setPreedit(markedText)
    }

    func unmarkText() {
        markedText = ""
        session?.setPreedit(nil)
    }

    func selectedRange() -> NSRange {
        NSRange(location: NSNotFound, length: 0)
    }

    func markedRange() -> NSRange {
        if markedText.isEmpty {
            return NSRange(location: NSNotFound, length: 0)
        }
        return NSRange(location: 0, length: markedText.utf16.count)
    }

    func hasMarkedText() -> Bool {
        !markedText.isEmpty
    }

    func attributedSubstring(
        forProposedRange range: NSRange, actualRange: NSRangePointer?
    ) -> NSAttributedString? {
        nil
    }

    func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        []
    }

    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?)
        -> NSRect
    {
        guard let window else { return .zero }
        let local = session?.cursorRect() ?? bounds
        let rect = surfaceView.convert(local, to: nil)
        return window.convertToScreen(rect)
    }

    func characterIndex(for point: NSPoint) -> Int {
        0
    }
}

struct TerminalSurface: NSViewRepresentable {
    let hostView: PanelHostView

    func makeNSView(context: Context) -> PanelHostView {
        hostView
    }

    func updateNSView(_ nsView: PanelHostView, context: Context) {}
}
