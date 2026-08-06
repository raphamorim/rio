import AppKit
import RioKit

final class RioEngine {
    static let shared = RioEngine()

    private var engine: OpaquePointer?
    private let lock = NSLock()
    private var sessions: [Int: PanelSession] = [:]

    var onTitle: ((PanelSession, String) -> Void)?
    var onCloseSurface: ((PanelSession) -> Void)?
    /// OSC 9;4 progress per session: state (0 remove, 1 set, 2 error,
    /// 3 indeterminate, 4 paused) and 0-100 value.
    var onProgress: ((PanelSession, Int, Int) -> Void)?

    static var fontSize: Float = 13.0
    static var fontFamily: String = "Menlo"

    private init() {
        var config = rio_runtime_config_s()
        config.userdata = Unmanaged.passUnretained(self).toOpaque()
        config.wakeup_cb = { userdata, surfaceID in
            guard let userdata else { return }
            let engine = Unmanaged<RioEngine>.fromOpaque(userdata).takeUnretainedValue()
            engine.wakeup(surfaceID)
        }
        config.action_cb = { userdata, surfaceID, action in
            guard let userdata else { return }
            let engine = Unmanaged<RioEngine>.fromOpaque(userdata).takeUnretainedValue()
            if action.tag == RIO_ACTION_SET_TITLE, let titlePtr = action.title {
                engine.title(surfaceID, String(cString: titlePtr))
            } else if action.tag == RIO_ACTION_PROGRESS {
                engine.progress(
                    surfaceID, Int(action.data_a), Int(action.data_b))
            }
        }
        config.clipboard_write_cb = { _, _, _, textPtr in
            guard let textPtr else { return }
            let text = String(cString: textPtr)
            DispatchQueue.main.async {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(text, forType: .string)
            }
        }
        config.close_surface_cb = { userdata, surfaceID in
            guard let userdata else { return }
            let engine = Unmanaged<RioEngine>.fromOpaque(userdata).takeUnretainedValue()
            engine.closeSurface(surfaceID)
        }
        engine = withUnsafePointer(to: &config) { rio_engine_new($0) }
    }

    func register(_ session: PanelSession, id: Int) {
        lock.lock()
        sessions[id] = session
        lock.unlock()
    }

    func unregister(id: Int) {
        lock.lock()
        sessions.removeValue(forKey: id)
        lock.unlock()
    }

    func handle() -> OpaquePointer? {
        engine
    }

    private func session(for id: Int) -> PanelSession? {
        lock.lock()
        defer { lock.unlock() }
        return sessions[id]
    }

    private func wakeup(_ id: Int) {
        guard let session = session(for: id) else { return }
        DispatchQueue.main.async {
            session.render()
        }
    }

    private func title(_ id: Int, _ title: String) {
        guard let session = session(for: id) else { return }
        DispatchQueue.main.async {
            self.onTitle?(session, title)
        }
    }

    private func progress(_ id: Int, _ state: Int, _ value: Int) {
        guard let session = session(for: id) else { return }
        DispatchQueue.main.async {
            self.onProgress?(session, state, value)
        }
    }

    private func closeSurface(_ id: Int) {
        guard let session = session(for: id) else { return }
        DispatchQueue.main.async {
            self.onCloseSurface?(session)
        }
    }
}

final class PanelSession {
    let panelID: UUID
    let terminal: TerminalItem
    let hostView: PanelHostView
    /// Frame-paced rendering: see `render()`.
    private var displayLink: CADisplayLink?
    private var needsRender = false

    private(set) var surface: OpaquePointer?
    private(set) var renderState: OpaquePointer?
    private(set) var cpuRenderer: CPURenderer?
    private var surfaceID: Int = 0
    private var lastCols: UInt16 = 0
    private var lastRows: UInt16 = 0

    init(panelID: UUID, terminal: TerminalItem) {
        self.panelID = panelID
        self.terminal = terminal
        self.hostView = PanelHostView()
        self.hostView.session = self
    }

    var isStarted: Bool {
        surface != nil
    }

    func startIfNeeded() {
        guard surface == nil else { return }
        let surfaceView = hostView.surfaceView
        let bounds = surfaceView.bounds
        guard bounds.width > 1, bounds.height > 1, surfaceView.window != nil else {
            return
        }
        let pixelSize = surfaceView.convertToBacking(bounds).size

        // All drawing happens on the Swift side now (CPU). The renderer
        // owns the font + cell geometry that the grid size is derived from.
        let renderer = CPURenderer(
            fontSize: CGFloat(RioEngine.fontSize),
            family: RioEngine.fontFamily)
        self.cpuRenderer = renderer
        surfaceView.session = self

        let cellWidth = Float(renderer.metrics.cellWidth)
        let cellHeight = Float(renderer.metrics.cellHeight)
        let pad = Float(renderer.metrics.padding)
        let cols = gridCols(
            logical: Float(bounds.width), cell: cellWidth, padding: pad)
        let rows = gridCols(
            logical: Float(bounds.height), cell: cellHeight, padding: pad)

        var config = rio_surface_config_s()
        config.cols = cols
        config.rows = rows
        config.pixel_width = UInt16(clamping: Int(pixelSize.width))
        config.pixel_height = UInt16(clamping: Int(pixelSize.height))
        config.scrollback = 10_000

        // Session restore: start the shell in the saved working directory.
        let savedCwd = terminal.panelWorkingDirs[panelID]
        let created: OpaquePointer? = {
            if let cwd = savedCwd {
                return cwd.withCString { cwdPtr in
                    config.working_dir = cwdPtr
                    return withUnsafePointer(to: &config) {
                        rio_surface_new(RioEngine.shared.handle(), $0)
                    }
                }
            }
            return withUnsafePointer(to: &config) {
                rio_surface_new(RioEngine.shared.handle(), $0)
            }
        }()
        guard let surface = created else { return }
        self.surface = surface
        self.lastCols = cols
        self.lastRows = rows
        self.surfaceID = rio_surface_id(surface)
        self.renderState = rio_render_state_new(surface)
        RioEngine.shared.register(self, id: surfaceID)

        // Replay saved scrollback once, into the DISPLAY (not the shell).
        if let text = terminal.panelScrollback[panelID], !text.isEmpty {
            injectOutput(text.replacingOccurrences(of: "\n", with: "\r\n"))
            terminal.panelScrollback[panelID] = nil
        }
        // Deep-link seed: type the command once the shell has settled.
        if let command = terminal.pendingCommand {
            terminal.pendingCommand = nil
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) {
                [weak self] in
                self?.sendText(command + "\r")
            }
        }
        render()
    }

    /// Write bytes to the terminal display without sending them to the PTY,
    /// so the shell never executes replayed scrollback.
    func injectOutput(_ text: String) {
        guard let surface else { return }
        text.withCString { pointer in
            rio_surface_inject_output(surface, pointer, strlen(pointer))
        }
    }

    /// The visible screen as text, one string per row, trailing blanks
    /// trimmed. Drives watcher scanning; placeholder and unset cells read
    /// as spaces.
    func visibleTextRows() -> [String] {
        guard let renderState else { return [] }
        let lines = Int(rio_render_state_lines(renderState))
        let cols = Int(rio_render_state_columns(renderState))
        var rows: [String] = []
        rows.reserveCapacity(lines)
        for line in 0..<lines {
            var text = ""
            text.reserveCapacity(cols)
            for col in 0..<cols {
                let cell = rio_render_state_cell(
                    renderState, UInt16(line), UInt16(col))
                if cell.codepoint == 0 || cell.codepoint == 0x10EEEE {
                    text.append(" ")
                } else if let scalar = Unicode.Scalar(cell.codepoint) {
                    text.append(Character(scalar))
                } else {
                    text.append(" ")
                }
            }
            while text.hasSuffix(" ") { text.removeLast() }
            rows.append(text)
        }
        return rows
    }

    /// Kitty images currently on screen, resolved to view-space rects for
    /// hit-testing (peek, context menu, drag-out). Uses the last render
    /// state snapshot; no update, so damage tracking stays untouched.
    func kittyImages() -> [CPURenderer.ResolvedKittyImage] {
        guard let renderState, let renderer = cpuRenderer else { return [] }
        let scale = hostView.window?.backingScaleFactor ?? 2
        return renderer.resolvedKittyImages(state: renderState, scale: scale)
    }

    /// Topmost kitty image under `point` (surface-view coordinates).
    func kittyImage(at point: NSPoint) -> CPURenderer.ResolvedKittyImage? {
        // Later in the list = higher z; reversed finds the topmost first.
        kittyImages().reversed().first { $0.rect.contains(point) }
    }

    /// Bitmap of the pane's current contents for sidebar peek previews.
    /// Works for offscreen views too: `cacheDisplay` runs `draw(_:)`, which
    /// paints from the render state whether or not the view is in a window.
    func peekSnapshot() -> NSImage? {
        guard let renderState else { return nil }
        rio_render_state_update(renderState)
        let view = hostView.surfaceView
        let bounds = view.bounds
        guard bounds.width > 1, bounds.height > 1,
            let rep = view.bitmapImageRepForCachingDisplay(in: bounds)
        else { return nil }
        view.cacheDisplay(in: bounds, to: rep)
        let image = NSImage(size: bounds.size)
        image.addRepresentation(rep)
        return image
    }

    /// Snapshot for session persistence: the live cwd (OSC 7) and the
    /// whole buffer as text.
    func snapshot() -> (cwd: String?, scrollback: String?) {
        guard let surface else { return (nil, nil) }
        var cwd: String?
        if let ptr = rio_surface_working_dir(surface) {
            cwd = String(cString: ptr)
            rio_text_free(ptr)
        }
        var scrollback: String?
        if let ptr = rio_surface_dump(surface) {
            let text = String(cString: ptr)
            rio_text_free(ptr)
            scrollback = text.isEmpty ? nil : text
        }
        return (cwd, scrollback)
    }

    func syncSize() {
        guard let surface, let renderer = cpuRenderer else {
            startIfNeeded()
            return
        }
        let surfaceView = hostView.surfaceView
        let bounds = surfaceView.bounds
        guard bounds.width > 1, bounds.height > 1 else { return }
        let pixelSize = surfaceView.convertToBacking(bounds).size

        let cellWidth = Float(renderer.metrics.cellWidth)
        let cellHeight = Float(renderer.metrics.cellHeight)
        let pad = Float(renderer.metrics.padding)
        let cols = gridCols(logical: Float(bounds.width), cell: cellWidth, padding: pad)
        let rows = gridCols(logical: Float(bounds.height), cell: cellHeight, padding: pad)
        if cols != lastCols || rows != lastRows {
            lastCols = cols
            lastRows = rows
            rio_surface_resize(
                surface,
                cols,
                rows,
                UInt16(clamping: Int(pixelSize.width)),
                UInt16(clamping: Int(pixelSize.height)))
        }
        render()
    }

    func rescale(_ scale: Float) {
        // CPU rendering draws in points; AppKit maps to the backing store,
        // so a scale change just needs a re-layout + redraw.
        syncSize()
    }

    /// Draw the current render state into the surface view. Called from
    /// `RioSurfaceNSView.draw(_:)` with the AppKit graphics context active.
    func drawSurface() {
        guard let renderState, let renderer = cpuRenderer else { return }
        let view = hostView.surfaceView
        let focused =
            (view.window?.isKeyWindow ?? false)
            && (view.window?.firstResponder === hostView)
        renderer.render(
            state: renderState, bounds: view.bounds, focused: focused)
    }

    /// Render requests coalesce onto the display's refresh (ghostty's
    /// renderer pacing): a flood of PTY wakeups marks the session dirty
    /// and the display link performs one snapshot + draw per frame, so
    /// high-volume output can't outpace the screen or tear mid-burst.
    /// (Applications that wrap updates in synchronized-output [?2026
    /// are already buffered whole by rio-vt's parser.)
    func render() {
        needsRender = true
        if displayLink == nil {
            let link = hostView.displayLink(
                target: self, selector: #selector(displayTick))
            link.add(to: .main, forMode: .common)
            displayLink = link
        }
        displayLink?.isPaused = false
    }

    @objc private func displayTick() {
        guard needsRender else {
            // Nothing new since the last frame: sleep until woken.
            displayLink?.isPaused = true
            return
        }
        needsRender = false
        guard let renderState else { return }
        rio_render_state_update(renderState)
        WatcherScanner.shared.scan(session: self)
        hostView.surfaceView.setNeedsDisplay(hostView.surfaceView.bounds)
    }

    func sendText(_ text: String) {
        guard let surface else { return }
        text.withCString { pointer in
            rio_surface_text(surface, pointer, strlen(pointer))
        }
        // Input snaps a scrolled view back to the live screen; redraw now
        // rather than waiting for the shell's echo to wake us.
        render()
    }

    /// Report a key to librio, which decides what the terminal receives. The
    /// text is what the platform produced for this key, already composed;
    /// `consumedMods` are the modifiers it spent doing so.
    @discardableResult
    func sendKey(
        _ tag: UInt32,
        mods: UInt8 = 0,
        codepoint: UInt32 = 0,
        functionKey: UInt8 = 0,
        action: UInt32 = UInt32(RIO_KEY_ACTION_PRESS),
        consumedMods: UInt8 = 0,
        composing: Bool = false,
        text: String? = nil
    ) -> Bool {
        guard let surface else { return false }

        var event = rio_key_event_s()
        event.action = action
        event.tag = tag
        event.codepoint = codepoint
        event.function_key = functionKey
        event.mods = mods
        event.consumed_mods = consumedMods
        event.composing = composing

        let handled: Bool
        if let text, !text.isEmpty {
            // The pointer only has to outlive the call, and the byte count
            // comes from the UTF-8 view rather than strlen so text containing
            // a NUL is passed whole instead of being cut short.
            var utf8 = Array(text.utf8)
            handled = utf8.withUnsafeMutableBufferPointer { buffer in
                event.text = UnsafeRawPointer(buffer.baseAddress!)
                    .assumingMemoryBound(to: CChar.self)
                event.text_len = buffer.count
                return rio_surface_key(surface, &event)
            }
        } else {
            event.text = nil
            event.text_len = 0
            handled = rio_surface_key(surface, &event)
        }
        // See sendText: reflect the scroll-to-bottom without waiting on echo.
        if handled { render() }
        return handled
    }

    func setAltIsMeta(_ enabled: Bool) {
        guard let surface else { return }
        rio_surface_set_alt_is_meta(surface, enabled)
    }

    func scroll(deltaLines: Int32) {
        guard let surface else { return }
        rio_surface_scroll(surface, deltaLines)
    }

    func cellAt(_ point: NSPoint) -> (line: Int32, col: UInt16)? {
        guard let renderer = cpuRenderer else { return nil }
        let cellWidth = renderer.metrics.cellWidth
        let cellHeight = renderer.metrics.cellHeight
        guard cellWidth > 0, cellHeight > 0 else { return nil }
        let pad = renderer.metrics.padding
        let col = Int((point.x - pad) / cellWidth)
        let line = Int((point.y - pad) / cellHeight)
        let maxCol = max(Int(lastCols) - 1, 0)
        let maxLine = max(Int(lastRows) - 1, 0)
        return (
            Int32(min(max(line, 0), maxLine)),
            UInt16(min(max(col, 0), maxCol))
        )
    }

    func selectionBegin(at point: NSPoint, kind: UInt8) {
        guard let surface, let cell = cellAt(point) else { return }
        rio_surface_selection_begin(surface, cell.line, cell.col, kind)
        render()
    }

    func selectionUpdate(at point: NSPoint) {
        guard let surface, let cell = cellAt(point) else { return }
        rio_surface_selection_update(surface, cell.line, cell.col)
        render()
    }

    func selectionClear() {
        guard let surface else { return }
        rio_surface_selection_clear(surface)
        render()
    }

    func selectionText() -> String? {
        guard let surface else { return nil }
        guard let pointer = rio_surface_selection_text(surface) else { return nil }
        defer { rio_text_free(pointer) }
        return String(cString: pointer)
    }

    func setPreedit(_ text: String?) {
        guard let renderer = cpuRenderer else { return }
        renderer.preedit = (text?.isEmpty ?? true) ? nil : text
        render()
    }

    func setFont(size: Float, family: String) {
        RioEngine.fontSize = size
        RioEngine.fontFamily = family
        cpuRenderer?.setFont(size: CGFloat(size), family: family)
        syncSize()
    }

    func cursorRect() -> NSRect? {
        guard let renderState, let renderer = cpuRenderer else { return nil }
        let cursor = rio_render_state_cursor(renderState)
        let cellWidth = renderer.metrics.cellWidth
        let cellHeight = renderer.metrics.cellHeight
        let pad = renderer.metrics.padding
        return NSRect(
            x: pad + CGFloat(cursor.column) * cellWidth,
            y: pad + CGFloat(cursor.line) * cellHeight,
            width: cellWidth,
            height: cellHeight)
    }

    func shutdown() {
        displayLink?.invalidate()
        displayLink = nil
        if surfaceID != 0 {
            RioEngine.shared.unregister(id: surfaceID)
        }
        if let renderState {
            rio_render_state_free(renderState)
        }
        cpuRenderer = nil
        if let surface {
            rio_surface_free(surface)
        }
        renderState = nil
        cpuRenderer = nil
        surface = nil
    }
}

private func gridCols(logical: Float, cell: Float, padding: Float) -> UInt16 {
    guard cell > 0 else { return 2 }
    let count = Int(((logical - padding * 2) / cell).rounded(.down))
    return UInt16(clamping: max(count, 2))
}
