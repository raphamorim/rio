import AppKit
import RioKit
import SwiftUI

final class SurfaceRegistry {
    private var sessions: [UUID: PanelSession] = [:]

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
    override var isFlipped: Bool { true }
}

final class PanelHostView: NSView {
    weak var session: PanelSession?
    let surfaceView = RioSurfaceNSView()

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

    override func layout() {
        super.layout()
        surfaceView.frame = bounds
        session?.syncSize()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        session?.startIfNeeded()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        guard let window else { return }
        session?.rescale(Float(window.backingScaleFactor))
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        super.mouseDown(with: event)
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

    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {}

    func unmarkText() {}

    func selectedRange() -> NSRange {
        NSRange(location: NSNotFound, length: 0)
    }

    func markedRange() -> NSRange {
        NSRange(location: NSNotFound, length: 0)
    }

    func hasMarkedText() -> Bool {
        false
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
        let rect = convert(bounds, to: nil)
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
