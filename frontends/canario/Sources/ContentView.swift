import SwiftUI

struct ContentView: View {
    @Environment(AppModel.self) private var model

    @State private var isSidebarPeeking = false
    @State private var window: NSWindow?

    var body: some View {
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                if !model.isSidebarCollapsed {
                    SidebarView()
                        .transition(.move(edge: .leading).combined(with: .opacity))
                } else {
                    CollapsedSidebarStrip()
                        .onHover { hovering in
                            if hovering {
                                withAnimation(.spring(duration: 0.25)) {
                                    isSidebarPeeking = true
                                }
                            }
                        }
                }

                ZStack {
                    if let terminal = model.selectedTerminal {
                        PanelGridView(terminal: terminal)
                            .id(terminal.id)
                    } else {
                        EmptyStateView()
                    }
                }
                .padding(
                    EdgeInsets(
                        top: 10,
                        leading: model.isSidebarCollapsed ? 8 : 2,
                        bottom: 10,
                        trailing: 10))
            }

            // Arc-style: while collapsed the toggle lives inside the floating
            // peek panel instead of hovering over the terminal content.
            if !model.isSidebarCollapsed {
                HStack(spacing: 8) {
                    SidebarToggleButton()
                    UpdatePillView()
                }
                .padding(.leading, 88)
                .padding(.top, 4)
            }
        }
        .ignoresSafeArea(.container, edges: .top)
        .background(
            ChromeBackground(base: model.chromeColor, spaceIndex: selectedSpaceIndex)
                .overlay(GrainOverlay())
                .ignoresSafeArea()
        )
        // Arc-style: collapsing hides the window's traffic lights; they come
        // back inside the floating peek panel.
        .background(
            WindowChrome(hidesTrafficLights: model.isSidebarCollapsed) {
                window = $0
            })
        .overlay(alignment: .topLeading) { sidebarPeekOverlay }
        .overlayPreferenceValue(PeekAnchorKey.self) { anchor in
            panePeekOverlay(anchor)
        }
        .overlay {
            if model.isCommandBarVisible {
                CommandBarView()
                    .transition(.opacity)
            }
        }
        .onChange(of: model.isSidebarCollapsed) { _, _ in
            isSidebarPeeking = false
        }
    }

    /// Space tint for the chrome: the gradient of the selected terminal's
    /// root folder, if it lives in one.
    private var selectedSpaceIndex: Int? {
        guard let id = model.selectedTerminalID else { return nil }
        return model.rootFolder(containing: id)?.colorIndex
    }

    /// Arc-style hover peek: the full sidebar floats over the content while
    /// the pointer stays on it.
    @ViewBuilder
    private var sidebarPeekOverlay: some View {
        if model.isSidebarCollapsed && isSidebarPeeking {
            SidebarView()
                .background(
                    RoundedRectangle(cornerRadius: 14)
                        .fill(model.chromeColor)
                        .overlay(
                            GrainOverlay()
                                .clipShape(RoundedRectangle(cornerRadius: 14)))
                        .shadow(color: .black.opacity(0.35), radius: 22, x: 6)
                )
                .overlay(alignment: .topLeading) {
                    // The panel's own header, Arc-style: the window's traffic
                    // lights live here while collapsed, next to the expand
                    // toggle (SidebarView's top spacer leaves this room free).
                    HStack(spacing: 10) {
                        PanelTrafficLights(window: window)
                        SidebarToggleButton()
                    }
                    .padding(.leading, 14)
                    .padding(.top, 12)
                }
                .padding(.top, 8)
                .padding(.leading, 6)
                .padding(.bottom, 10)
                .onHover { hovering in
                    if !hovering {
                        withAnimation(.spring(duration: 0.25)) {
                            isSidebarPeeking = false
                        }
                    }
                }
                .transition(.move(edge: .leading).combined(with: .opacity))
                .zIndex(5)
        }
    }

    @ViewBuilder
    private func panePeekOverlay(_ anchor: Anchor<CGRect>?) -> some View {
        if let anchor,
            let panelID = model.peekedPanelID,
            let session = model.surfaces.existingSession(for: panelID)
        {
            GeometryReader { geo in
                let rect = geo[anchor]
                let y = min(max(rect.midY, 120), max(geo.size.height - 120, 120))
                PanePeekView(session: session)
                    .position(x: rect.maxX + 12 + 150, y: y)
            }
            .allowsHitTesting(false)
        }
    }
}

/// Live thumbnail of a pane, refreshed while the peek is up.
private struct PanePeekView: View {
    @Environment(AppModel.self) private var model
    let session: PanelSession

    @State private var image: NSImage?
    private let refresh = Timer.publish(every: 0.5, on: .main, in: .common)
        .autoconnect()

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                VStack(spacing: 6) {
                    Image(systemName: "terminal")
                        .font(.system(size: 22, weight: .light))
                    Text("No output yet")
                        .font(.system(size: 11, weight: .medium))
                }
                .foregroundStyle(.white.opacity(0.4))
            }
        }
        .frame(width: 300, height: 200)
        .background(Color(red: 0x0f / 255, green: 0x0d / 255, blue: 0x0e / 255))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(model.borderColor.opacity(0.9), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.4), radius: 20, y: 6)
        .onAppear { image = session.peekSnapshot() }
        .onReceive(refresh) { _ in image = session.peekSnapshot() }
    }
}

/// Quiet update affordance: a small pill that only exists when the updater
/// has something to say; clicking it drops the options down.
struct UpdatePillView: View {
    @Environment(Updater.self) private var updater

    var body: some View {
        if updater.state != .idle, updater.state != .checking {
            Menu {
                menuItems
            } label: {
                pillLabel
            }
            .buttonStyle(.plain)
            .fixedSize()
            .transition(.opacity)
        }
    }

    @ViewBuilder
    private var menuItems: some View {
        switch updater.state {
        case .available(let version, _):
            Button("Install \(version) and relaunch") { updater.install() }
            Button("Skip this version") { updater.skip() }
        case .failed(let message):
            Text(message)
            Button("Try again") { updater.checkNow() }
            Button("Dismiss") { updater.dismiss() }
        case .upToDate:
            Button("You're up to date") { updater.dismiss() }
        default:
            Button("Cancel") { updater.dismiss() }
        }
    }

    private var pillLabel: some View {
        HStack(spacing: 5) {
            Image(systemName: icon)
                .font(.system(size: 10, weight: .semibold))
            Text(text)
                .font(.system(size: 11, weight: .semibold))
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 9)
        .frame(height: 22)
        .background(Capsule().fill(.black.opacity(0.72)))
        .contentShape(Capsule())
        .help("Canario update")
    }

    private var icon: String {
        switch updater.state {
        case .available: "arrow.down.circle.fill"
        case .downloading, .installing: "arrow.down.circle"
        case .upToDate: "checkmark.circle.fill"
        default: "exclamationmark.triangle.fill"
        }
    }

    private var text: String {
        switch updater.state {
        case .available(let version, _): version
        case .downloading(let progress): "\(Int(progress * 100))%"
        case .installing: "installing…"
        case .upToDate: "up to date"
        default: "update failed"
        }
    }
}

struct SidebarToggleButton: View {
    @Environment(AppModel.self) private var model

    @State private var isHovered = false

    var body: some View {
        Button {
            withAnimation(.spring(duration: 0.28)) {
                model.isSidebarCollapsed.toggle()
            }
        } label: {
            Image(systemName: "sidebar.left")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(
                    model.isSidebarCollapsed
                        ? Color.white.opacity(isHovered ? 1.0 : 0.65)
                        : model.textPrimary.opacity(isHovered ? 1.0 : 0.6)
                )
                .frame(width: 26, height: 26)
                .background(
                    RoundedRectangle(cornerRadius: 7)
                        .fill(
                            model.isSidebarCollapsed
                                ? Color.white.opacity(isHovered ? 0.16 : 0.0001)
                                : Color.black.opacity(isHovered ? 0.10 : 0.0001))
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Toggle Sidebar")
        .onHover { isHovered = $0 }
    }
}

private struct PanelGridView: View {
    let terminal: TerminalItem

    var body: some View {
        PanelGridLayout(spacing: 8) {
            ForEach(Array(terminal.columns.enumerated()), id: \.element.id) {
                columnIndex, column in
                ForEach(column.panels) { panel in
                    PanelTileView(terminal: terminal, panel: panel)
                        .layoutValue(key: PanelGridLayout.ColumnKey.self, value: columnIndex)
                        .layoutValue(
                            key: PanelGridLayout.WeightKey.self,
                            value: terminal.weight(for: panel.id))
                }
            }
        }
        .animation(.spring(duration: 0.25), value: terminal.panelCount)
    }
}

private struct PanelTileView: View {
    @Environment(AppModel.self) private var model
    let terminal: TerminalItem
    let panel: Panel

    @State private var weightAtDragStart: CGFloat?
    @State private var isHandleHovered = false

    var body: some View {
        let session = model.surfaces.session(for: panel.id, terminal: terminal)
        TerminalSurface(hostView: session.hostView)
            .clipShape(RoundedRectangle(cornerRadius: Theme.cardRadius))
            .overlay {
                if terminal.panelCount > 1 {
                    if terminal.focusedPanelID == panel.id {
                        RoundedRectangle(cornerRadius: Theme.cardRadius)
                            .strokeBorder(model.borderColor, lineWidth: 2.5)
                    } else {
                        RoundedRectangle(cornerRadius: Theme.cardRadius)
                            .strokeBorder(.white.opacity(0.10), lineWidth: 1)
                    }
                }
            }
            .overlay(alignment: .bottom) {
                if terminal.panelCount > 1 {
                    resizeHandle
                }
            }
            .shadow(color: .black.opacity(0.22), radius: 10, y: 3)
            .contentShape(Rectangle())
            .onTapGesture {
                terminal.focusedPanelID = panel.id
            }
            .onChange(of: terminal.focusedPanelID, initial: true) { _, focused in
                if focused == panel.id {
                    session.hostView.window?.makeFirstResponder(session.hostView)
                }
            }
    }

    private var resizeHandle: some View {
        ZStack {
            Color.clear
            RoundedRectangle(cornerRadius: 2)
                .fill(.white.opacity(isHandleHovered ? 0.35 : 0.0001))
                .frame(width: 28, height: 3)
        }
        .frame(height: 9)
        .contentShape(Rectangle())
        .onHover { hovering in
            isHandleHovered = hovering
            if hovering {
                NSCursor.resizeUpDown.push()
            } else {
                NSCursor.pop()
            }
        }
        .gesture(
            DragGesture(minimumDistance: 1)
                .onChanged { value in
                    if weightAtDragStart == nil {
                        weightAtDragStart = terminal.weight(for: panel.id)
                    }
                    if let start = weightAtDragStart {
                        terminal.setWeight(
                            start + value.translation.height / 200, for: panel.id)
                    }
                }
                .onEnded { _ in
                    weightAtDragStart = nil
                }
        )
    }
}

/// Grabs the hosting NSWindow and keeps the standard traffic lights hidden
/// while the sidebar is collapsed (they reappear in the peek panel).
private struct WindowChrome: NSViewRepresentable {
    let hidesTrafficLights: Bool
    let onWindow: (NSWindow) -> Void

    func makeNSView(context: Context) -> NSView {
        NSView()
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        let hidden = hidesTrafficLights
        DispatchQueue.main.async {
            guard let window = nsView.window else { return }
            onWindow(window)
            for button: NSWindow.ButtonType in [
                .closeButton, .miniaturizeButton, .zoomButton,
            ] {
                window.standardWindowButton(button)?.isHidden = hidden
            }
        }
    }
}

/// Working close/minimize/zoom controls for the peek panel, styled like the
/// system traffic lights.
struct PanelTrafficLights: View {
    let window: NSWindow?

    var body: some View {
        HStack(spacing: 8) {
            light(color: Color(red: 1.00, green: 0.37, blue: 0.34), label: "Close") {
                window?.performClose(nil)
            }
            light(color: Color(red: 1.00, green: 0.74, blue: 0.18), label: "Minimize") {
                window?.performMiniaturize(nil)
            }
            light(color: Color(red: 0.16, green: 0.78, blue: 0.25), label: "Zoom") {
                window?.performZoom(nil)
            }
        }
    }

    private func light(
        color: Color, label: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Circle()
                .fill(color)
                .overlay(Circle().strokeBorder(.black.opacity(0.15), lineWidth: 0.5))
                .frame(width: 12, height: 12)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

private struct EmptyStateView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "terminal")
                .font(.system(size: 34, weight: .light))
                .opacity(0.5)
            Text("No terminal open")
                .font(.system(size: 14, weight: .medium))
                .opacity(0.6)
            Button {
                model.createTerminal()
            } label: {
                Text("New Terminal  ⌘T")
                    .font(.system(size: 12, weight: .medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 7)
                    .background(Capsule().fill(.white.opacity(0.16)))
            }
            .buttonStyle(.plain)
        }
        .foregroundStyle(.white)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            RoundedRectangle(cornerRadius: Theme.cardRadius)
                .fill(.black.opacity(0.82))
        )
        .shadow(color: .black.opacity(0.22), radius: 10, y: 3)
    }
}
