import AppKit
import RioKit
import SwiftUI
import UniformTypeIdentifiers

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
    /// Drag-selection autoscroll (ghostty's selection scroll tick): dragging
    /// past the top or bottom edge keeps scrolling and extending.
    private var autoscrollTimer: Timer?
    private var lastDragPoint: NSPoint?
    /// Kitty image pressed on mouse-down; resolves to a peek on click or a
    /// file drag once the pointer travels. Cleared either way.
    private var pendingImageHit:
        (image: CPURenderer.ResolvedKittyImage, point: NSPoint)?
    /// The image under the last right-click, for menu actions.
    private var menuImageHit: CPURenderer.ResolvedKittyImage?
    /// Keeps the drag's file-promise delegate alive for the session.
    private var imageDragDelegate: ImageFilePromiseDelegate?

    /// Non-nil while a `keyDown` is in flight. `insertText` appends here
    /// instead of sending, so the key handler can tell text the input method
    /// committed from a key that still needs encoding.
    fileprivate var keyTextAccumulator: [String]?

    /// Whether alt acts as meta. Kept alongside librio's own copy so the host
    /// knows whether to let alt take part in text translation. Defaults to
    /// false, the macOS convention (Terminal.app, iTerm2, ghostty): option
    /// composes characters and dead keys, which international layouts and
    /// input methods depend on.
    fileprivate var altIsMeta = false

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layerContentsRedrawPolicy = .duringViewResize
        layer?.cornerRadius = Theme.cardRadius
        layer?.masksToBounds = true
        // Rio's default background, matching CPURenderer.defaultBackground.
        layer?.backgroundColor = CGColor(
            red: 0x0f / 255, green: 0x0d / 255, blue: 0x0e / 255, alpha: 1)
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
        session.setAltIsMeta(altIsMeta)
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
        // Ctrl+click means context menu. AppKit's translation lives in
        // NSView's default mouseDown, which this override replaces, so
        // it has to be done by hand.
        if event.modifierFlags.contains(.control) {
            if let menu = menu(for: event) {
                NSMenu.popUpContextMenu(menu, with: event, for: self)
            }
            return
        }
        let point = surfaceView.convert(event.locationInWindow, from: nil)
        // Kitty images act like objects, not cells: plain click peeks,
        // drag exports. Double/triple click falls through to selection so
        // text under an image stays reachable.
        if event.clickCount == 1, let hit = session.kittyImage(at: point) {
            pendingImageHit = (hit, point)
            return
        }
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
        if let pending = pendingImageHit {
            if hypot(point.x - pending.point.x, point.y - pending.point.y) > 4 {
                pendingImageHit = nil
                beginImageDrag(pending.image, event: event)
            }
            return
        }
        if !selectionActive {
            guard let anchor = selectionAnchor else { return }
            selectionActive = true
            session.selectionBegin(at: anchor, kind: UInt8(RIO_SELECTION_SIMPLE))
        }
        lastDragPoint = point
        // Outside the vertical bounds: hand off to the autoscroll tick,
        // which keeps scrolling and extending until the pointer returns.
        if point.y < 0 || point.y > surfaceView.bounds.height {
            startAutoscroll()
        } else {
            stopAutoscroll()
        }
        session.selectionUpdate(at: point)
    }

    override func mouseUp(with event: NSEvent) {
        if let pending = pendingImageHit {
            pendingImageHit = nil
            openImagePeek(clicked: pending.image)
        }
        selectionAnchor = nil
        stopAutoscroll()
        super.mouseUp(with: event)
    }

    // MARK: Kitty image interactions (peek, context menu, drag-out)

    /// Open the lightbox on the clicked image, with every image currently
    /// on this surface as the ←/→ gallery, ordered top-to-bottom.
    private func openImagePeek(clicked: CPURenderer.ResolvedKittyImage) {
        guard let session, let model = AppModel.shared else { return }
        let all = session.kittyImages().sorted {
            ($0.rect.minY, $0.rect.minX) < ($1.rect.minY, $1.rect.minX)
        }
        let images = all.map {
            PeekImage(
                cgImage: $0.image,
                sourceRect: surfaceView.convert($0.rect, to: nil))
        }
        guard !images.isEmpty else { return }
        let index = all.firstIndex { $0.rect == clicked.rect } ?? 0
        model.imagePeek = ImagePeekState(images: images, index: index)
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let point = surfaceView.convert(event.locationInWindow, from: nil)
        guard let session else { return super.menu(for: event) }
        guard let hit = session.kittyImage(at: point) else {
            return textMenu(for: point, session: session)
        }
        menuImageHit = hit
        let menu = NSMenu()
        let items: [(String, Selector)] = [
            ("Copy Image", #selector(copyImageFromMenu)),
            ("Save to Downloads", #selector(saveImageFromMenu)),
            ("Open in Preview", #selector(openImageInPreviewFromMenu)),
        ]
        for (title, action) in items {
            let item = menu.addItem(
                withTitle: title, action: action, keyEquivalent: "")
            item.target = self
        }
        menu.addItem(.separator())
        let share = menu.addItem(
            withTitle: "Share\u{2026}", action: #selector(shareImageFromMenu),
            keyEquivalent: "")
        share.target = self
        return menu
    }

    /// Menu for right-clicks that don't hit an image.
    private func textMenu(for point: NSPoint, session: PanelSession) -> NSMenu? {
        let menu = NSMenu()

        // Watchers: the selected text becomes the pattern.
        if let selection = session.selectionText()?
            .components(separatedBy: .newlines).first?
            .trimmingCharacters(in: .whitespaces),
            !selection.isEmpty
        {
            pendingWatchPattern = selection
            let display =
                selection.count > 24
                ? String(selection.prefix(24)) + "\u{2026}" : selection
            let watch = menu.addItem(
                withTitle: "Watch for \u{201C}\(display)\u{201D}",
                action: #selector(addWatcherFromMenu), keyEquivalent: "")
            watch.target = self
            menu.addItem(.separator())
        }

        let pip = menu.addItem(
            withTitle: "Pop Out Panel",
            action: #selector(popOutFromMenu), keyEquivalent: "")
        pip.target = self

        return menu
    }

    /// Selection captured while building the context menu.
    private var pendingWatchPattern: String?

    @objc private func addWatcherFromMenu() {
        guard let session, let pattern = pendingWatchPattern else { return }
        AppModel.shared?.addWatcher(
            pattern: pattern, terminalID: session.terminal.id)
    }

    @objc private func popOutFromMenu() {
        guard let session else { return }
        AppModel.shared?.pip.popOut(session)
    }

    @objc private func copyImageFromMenu() {
        guard let hit = menuImageHit else { return }
        ImageActions.copy(hit.image)
    }

    @objc private func saveImageFromMenu() {
        guard let hit = menuImageHit else { return }
        ImageActions.saveToDownloads(hit.image)
    }

    @objc private func openImageInPreviewFromMenu() {
        guard let hit = menuImageHit else { return }
        ImageActions.openInPreview(hit.image)
    }

    @objc private func shareImageFromMenu() {
        guard let hit = menuImageHit else { return }
        let image = NSImage(
            cgImage: hit.image,
            size: NSSize(width: hit.image.width, height: hit.image.height))
        let picker = NSSharingServicePicker(items: [image])
        let anchor = surfaceView.convert(hit.rect, to: self)
        picker.show(relativeTo: anchor, of: self, preferredEdge: .minY)
    }

    /// Drag the image out as a PNG file promise, the way Messages and
    /// Notes hand images to Finder / Slack / Figma.
    private func beginImageDrag(
        _ hit: CPURenderer.ResolvedKittyImage, event: NSEvent
    ) {
        let delegate = ImageFilePromiseDelegate(cgImage: hit.image)
        imageDragDelegate = delegate
        let provider = NSFilePromiseProvider(
            fileType: UTType.png.identifier, delegate: delegate)
        let item = NSDraggingItem(pasteboardWriter: provider)
        let dragFrame = surfaceView.convert(hit.rect, to: self)
        item.setDraggingFrame(
            dragFrame,
            contents: NSImage(cgImage: hit.image, size: hit.rect.size))
        beginDraggingSession(with: [item], event: event, source: self)
    }

    private func startAutoscroll() {
        guard autoscrollTimer == nil else { return }
        autoscrollTimer = Timer.scheduledTimer(
            withTimeInterval: 0.05, repeats: true
        ) { [weak self] _ in
            self?.autoscrollTick()
        }
    }

    private func stopAutoscroll() {
        autoscrollTimer?.invalidate()
        autoscrollTimer = nil
    }

    private func autoscrollTick() {
        guard let session, selectionActive, let point = lastDragPoint else {
            stopAutoscroll()
            return
        }
        let height = surfaceView.bounds.height
        let cell = session.cpuRenderer?.metrics.cellHeight ?? 16
        // Scroll speed grows with how far past the edge the pointer sits,
        // capped so a wild fling stays followable.
        let lines: Int32
        if point.y < 0 {
            lines = Int32(min(5, max(1, Int(-point.y / cell) + 1)))
        } else if point.y > height {
            lines = -Int32(min(5, max(1, Int((point.y - height) / cell) + 1)))
        } else {
            stopAutoscroll()
            return
        }
        session.scroll(deltaLines: lines)
        // Re-extend to the edge cell of the freshly revealed row.
        session.selectionUpdate(at: point)
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
        let action =
            event.isARepeat
            ? UInt32(RIO_KEY_ACTION_REPEAT) : UInt32(RIO_KEY_ACTION_PRESS)
        let wasComposing = !markedText.isEmpty

        // Run the input method for its text, but collect it rather than
        // sending: only after this returns is it clear whether the text is a
        // commit to forward, a preedit update to draw, or a key to encode.
        keyTextAccumulator = []
        // Control never contributes to text, and alt only does when alt is
        // acting as meta. Translating without them keeps the input method
        // from turning ctrl+a into an unrelated character.
        let translationEvent = event.strippingForTranslation(
            control: true,
            option: altIsMeta
        )
        interpretKeyEvents([translationEvent])
        let produced = keyTextAccumulator ?? []
        keyTextAccumulator = nil

        if !markedText.isEmpty {
            // Mid-composition: the preedit is already drawn, and nothing is
            // encoded until it commits.
            return
        }

        // Composition just ended, by commit or by cancel. Committed text is
        // not a keystroke to encode, it is the result of several; and the
        // key that ended the composition belongs to the input method, so a
        // backspace canceling romaji must not also delete from the
        // terminal. (Ghostty's keyDown draws the same line.)
        if wasComposing {
            for text in produced
            where !text.isEmpty && !Self.isBareControlCharacter(text) {
                session.sendKey(UInt32(RIO_KEY_NONE), text: text)
            }
            // Korean input methods commit on arrow keys that should then
            // still move the cursor; other enders (return, escape) are
            // consumed by the commit itself.
            if Self.replaysAfterCommit(event),
                let tag = Self.namedKeys[event.keyCode]
            {
                session.sendKey(tag, mods: mods, action: action)
            }
            return
        }

        if let tag = Self.namedKeys[event.keyCode] {
            session.sendKey(tag, mods: mods, action: action)
            return
        }

        if let number = Self.functionKeys[event.keyCode] {
            session.sendKey(
                UInt32(RIO_KEY_F), mods: mods, functionKey: number,
                action: action)
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

        // A bare control character as text (ctrl+a arriving as 0x01) is not
        // text to pass through: librio encodes control from the key + mods,
        // and passing both would encode it twice or wrongly.
        var text = produced.first ?? event.characters
        if let text_ = text, Self.isBareControlCharacter(text_) {
            text = nil
        }

        session.sendKey(
            UInt32(RIO_KEY_CHAR),
            mods: mods,
            codepoint: codepoint,
            action: action,
            text: text
        )
    }

    override func keyUp(with event: NSEvent) {
        guard let session else {
            super.keyUp(with: event)
            return
        }
        if event.modifierFlags.contains(.command) {
            super.keyUp(with: event)
            return
        }
        // Releases matter only to programs that asked for key event types
        // (the kitty keyboard protocol); librio filters accordingly.
        let mods = rioMods(event.modifierFlags)
        let release = UInt32(RIO_KEY_ACTION_RELEASE)
        if let tag = Self.namedKeys[event.keyCode] {
            session.sendKey(tag, mods: mods, action: release)
            return
        }
        if let number = Self.functionKeys[event.keyCode] {
            session.sendKey(
                UInt32(RIO_KEY_F), mods: mods, functionKey: number,
                action: release)
            return
        }
        guard
            let unshifted = event.charactersIgnoringModifiers?.unicodeScalars
                .first
        else { return }
        let codepoint =
            String(unshifted).lowercased().unicodeScalars.first?.value
            ?? unshifted.value
        session.sendKey(
            UInt32(RIO_KEY_CHAR), mods: mods, codepoint: codepoint,
            action: release)
    }

    /// A single scalar below 0x20: what ctrl+key or an input method's
    /// internal editing produces. Never forwarded as text.
    private static func isBareControlCharacter(_ text: String) -> Bool {
        let scalars = text.unicodeScalars
        guard let first = scalars.first,
            scalars.index(after: scalars.startIndex) == scalars.endIndex
        else { return false }
        return first.value < 0x20
    }

    /// Keys that ended a composition by committing but should still reach
    /// the terminal afterwards (mirrors ghostty): Korean input methods
    /// commit on arrows that the user also means as cursor movement. Plain
    /// left is excluded because AppKit already leaves the caret in place.
    private static func replaysAfterCommit(_ event: NSEvent) -> Bool {
        switch event.keyCode {
        case 0x7C, 0x7D, 0x7E:  // Right, Down, Up
            return true
        case 0x7B:  // Left, only with modifiers
            return !event.modifierFlags.isDisjoint(
                with: [.shift, .control, .option, .command])
        default:
            return false
        }
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
            characters: characters(byApplyingModifiers: flags)
                ?? charactersIgnoringModifiers ?? "",
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
        let hadMarkedText = !markedText.isEmpty
        if hadMarkedText {
            markedText = ""
            session?.setPreedit(nil)
        }

        // Inside a keyDown: collect and let the key handler decide. Outside
        // one, this is text arriving on its own and goes straight through.
        if keyTextAccumulator != nil {
            keyTextAccumulator?.append(text)
            return
        }

        if hadMarkedText {
            // An input method committing outside a keyDown (mouse-picked
            // candidate): a text-only key event, not raw bytes.
            session?.sendKey(UInt32(RIO_KEY_NONE), text: text)
        } else {
            session?.sendText(text)
        }
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


extension PanelHostView: NSDraggingSource {
    func draggingSession(
        _ session: NSDraggingSession,
        sourceOperationMaskFor context: NSDraggingContext
    ) -> NSDragOperation {
        .copy
    }

    func draggingSession(
        _ session: NSDraggingSession, endedAt screenPoint: NSPoint,
        operation: NSDragOperation
    ) {
        imageDragDelegate = nil
    }
}

/// Fulfills an image drag's file promise: the destination (Finder, Slack,
/// Figma) asks for the file only on drop, and gets a fresh PNG.
final class ImageFilePromiseDelegate: NSObject, NSFilePromiseProviderDelegate {
    private let cgImage: CGImage

    init(cgImage: CGImage) {
        self.cgImage = cgImage
    }

    func filePromiseProvider(
        _ filePromiseProvider: NSFilePromiseProvider, fileNameForType fileType: String
    ) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd 'at' HH.mm.ss"
        return "Canario Image \(formatter.string(from: Date())).png"
    }

    func filePromiseProvider(
        _ filePromiseProvider: NSFilePromiseProvider, writePromiseTo url: URL,
        completionHandler: @escaping (Error?) -> Void
    ) {
        guard let data = ImageActions.pngData(cgImage) else {
            completionHandler(CocoaError(.fileWriteUnknown))
            return
        }
        do {
            try data.write(to: url)
            completionHandler(nil)
        } catch {
            completionHandler(error)
        }
    }
}
