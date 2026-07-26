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

    override func keyDown(with event: NSEvent) {
        guard let session else { return }
        let flags = event.modifierFlags

        if flags.contains(.command) {
            super.keyDown(with: event)
            return
        }

        if flags.contains(.control), let chars = event.charactersIgnoringModifiers,
            let ch = chars.unicodeScalars.first
        {
            var mods = UInt8(RIO_MOD_CTRL)
            if flags.contains(.option) {
                mods |= UInt8(RIO_MOD_ALT)
            }
            if flags.contains(.shift) {
                mods |= UInt8(RIO_MOD_SHIFT)
            }
            session.sendKey(UInt32(RIO_KEY_CHAR), mods: mods, codepoint: ch.value)
            return
        }

        interpretKeyEvents([event])
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
        session?.sendText(text)
    }

    override func doCommand(by selector: Selector) {
        guard let session else { return }
        switch selector {
        case #selector(insertNewline(_:)):
            session.sendKey(UInt32(RIO_KEY_ENTER))
        case #selector(deleteBackward(_:)):
            session.sendKey(UInt32(RIO_KEY_BACKSPACE))
        case #selector(insertTab(_:)):
            session.sendKey(UInt32(RIO_KEY_TAB))
        case #selector(insertBacktab(_:)):
            session.sendKey(UInt32(RIO_KEY_TAB), mods: UInt8(RIO_MOD_SHIFT))
        case #selector(cancelOperation(_:)):
            session.sendKey(UInt32(RIO_KEY_ESCAPE))
        case #selector(moveUp(_:)):
            session.sendKey(UInt32(RIO_KEY_UP))
        case #selector(moveDown(_:)):
            session.sendKey(UInt32(RIO_KEY_DOWN))
        case #selector(moveLeft(_:)):
            session.sendKey(UInt32(RIO_KEY_LEFT))
        case #selector(moveRight(_:)):
            session.sendKey(UInt32(RIO_KEY_RIGHT))
        case #selector(scrollToBeginningOfDocument(_:)):
            session.sendKey(UInt32(RIO_KEY_HOME))
        case #selector(scrollToEndOfDocument(_:)):
            session.sendKey(UInt32(RIO_KEY_END))
        case #selector(pageUp(_:)):
            session.sendKey(UInt32(RIO_KEY_PAGE_UP))
        case #selector(pageDown(_:)):
            session.sendKey(UInt32(RIO_KEY_PAGE_DOWN))
        case #selector(deleteForward(_:)):
            session.sendKey(UInt32(RIO_KEY_DELETE))
        default:
            break
        }
    }

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
