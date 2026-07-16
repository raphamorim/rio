import AppKit
import RioKit

final class RioEngine {
    static let shared = RioEngine()

    private var engine: OpaquePointer?
    private let lock = NSLock()
    private var sessions: [Int: PanelSession] = [:]

    var onTitle: ((PanelSession, String) -> Void)?
    var onCloseSurface: ((PanelSession) -> Void)?

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

    private(set) var surface: OpaquePointer?
    private(set) var renderState: OpaquePointer?
    private(set) var renderer: OpaquePointer?
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
        let scale = Float(surfaceView.window?.backingScaleFactor ?? 2.0)
        let pixelSize = surfaceView.convertToBacking(bounds).size

        let viewPtr = Unmanaged.passUnretained(surfaceView).toOpaque()
        guard
            let renderer = rio_renderer_new(
                viewPtr,
                Float(pixelSize.width),
                Float(pixelSize.height),
                scale,
                13.0)
        else { return }
        self.renderer = renderer

        var cellWidth: Float = 0
        var cellHeight: Float = 0
        rio_renderer_cell_size(renderer, &cellWidth, &cellHeight)
        let pad = rio_renderer_padding(renderer)
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
        guard
            let surface = withUnsafePointer(
                to: &config, { rio_surface_new(RioEngine.shared.handle(), $0) })
        else { return }
        self.surface = surface
        self.lastCols = cols
        self.lastRows = rows
        self.surfaceID = rio_surface_id(surface)
        self.renderState = rio_render_state_new(surface)
        RioEngine.shared.register(self, id: surfaceID)
        render()
    }

    func syncSize() {
        guard let surface, let renderer else {
            startIfNeeded()
            return
        }
        let surfaceView = hostView.surfaceView
        let bounds = surfaceView.bounds
        guard bounds.width > 1, bounds.height > 1 else { return }
        let pixelSize = surfaceView.convertToBacking(bounds).size
        rio_renderer_resize(
            renderer,
            UInt32(max(pixelSize.width, 1)),
            UInt32(max(pixelSize.height, 1)))

        var cellWidth: Float = 0
        var cellHeight: Float = 0
        rio_renderer_cell_size(renderer, &cellWidth, &cellHeight)
        let pad = rio_renderer_padding(renderer)
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
        guard let renderer else { return }
        rio_renderer_rescale(renderer, scale)
        syncSize()
    }

    func render() {
        guard let renderState, let renderer else { return }
        rio_render_state_update(renderState)
        rio_renderer_draw(renderer, renderState)
    }

    func sendText(_ text: String) {
        guard let surface else { return }
        text.withCString { pointer in
            rio_surface_text(surface, pointer, strlen(pointer))
        }
    }

    func sendKey(_ tag: UInt32, mods: UInt8 = 0, codepoint: UInt32 = 0) {
        guard let surface else { return }
        var event = rio_key_event_s()
        event.tag = tag
        event.codepoint = codepoint
        event.mods = mods
        _ = rio_surface_key(surface, event)
    }

    func scroll(deltaLines: Int32) {
        guard let surface else { return }
        rio_surface_scroll(surface, deltaLines)
    }

    func shutdown() {
        if surfaceID != 0 {
            RioEngine.shared.unregister(id: surfaceID)
        }
        if let renderState {
            rio_render_state_free(renderState)
        }
        if let renderer {
            rio_renderer_free(renderer)
        }
        if let surface {
            rio_surface_free(surface)
        }
        renderState = nil
        renderer = nil
        surface = nil
    }
}

private func gridCols(logical: Float, cell: Float, padding: Float) -> UInt16 {
    guard cell > 0 else { return 2 }
    let count = Int(((logical - padding * 2) / cell).rounded(.down))
    return UInt16(clamping: max(count, 2))
}
