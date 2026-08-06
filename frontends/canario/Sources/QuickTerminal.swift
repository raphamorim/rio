import AppKit
import Carbon.HIToolbox
import SwiftUI

// Little-Arc-style quick terminal: a floating panel with a single shell,
// summoned from anywhere with a global ⌥⌘T, dismissed by clicking away or
// pressing the hotkey again. Lives outside the sidebar tree; not persisted.

private final class KeyablePanel: NSPanel {
    override var canBecomeKey: Bool { true }
}

final class QuickTerminalController: NSObject {
    private weak var model: AppModel?
    private var panel: NSPanel?
    private var terminal: TerminalItem?
    private var hotKeyRef: EventHotKeyRef?

    init(model: AppModel) {
        self.model = model
        super.init()
        registerGlobalHotKey()
    }

    func toggle() {
        if let panel, panel.isVisible {
            panel.orderOut(nil)
            return
        }
        show()
    }

    /// The quick shell exited (close_surface_cb): tear the panel down so the
    /// next summon starts fresh. Returns false when the session isn't ours.
    func handleClosedSession(_ session: PanelSession) -> Bool {
        guard let terminal, session.terminal === terminal else { return false }
        model?.surfaces.remove(session.panelID)
        panel?.orderOut(nil)
        panel = nil
        self.terminal = nil
        return true
    }

    private func show() {
        guard let panel = ensurePanel() else { return }
        position(panel)
        NSApp.activate(ignoringOtherApps: true)
        panel.makeKeyAndOrderFront(nil)
        // Transparent windows keep the shadow of their square frame unless
        // it's recomputed after the rounded content has drawn.
        DispatchQueue.main.async { panel.invalidateShadow() }
    }

    private func ensurePanel() -> NSPanel? {
        if let panel { return panel }
        guard let model else { return nil }

        let terminal = TerminalItem(name: "Quick Terminal")
        self.terminal = terminal
        let session = model.surfaces.session(
            for: terminal.focusedPanelID, terminal: terminal)

        let content = QuickTerminalView(
            hostView: session.hostView,
            chrome: model.chromeColor,
            text: model.textPrimary,
            border: model.borderColor
        ) { [weak self] in
            self?.panel?.orderOut(nil)
        }
        let panel = KeyablePanel(
            contentRect: NSRect(x: 0, y: 0, width: 760, height: 420),
            styleMask: [.borderless, .fullSizeContentView],
            backing: .buffered, defer: false)
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.isFloatingPanel = true
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        panel.contentView = NSHostingView(rootView: content)

        // Little Arc behavior: clicking anywhere else dismisses it.
        NotificationCenter.default.addObserver(
            self, selector: #selector(panelResignedKey),
            name: NSWindow.didResignKeyNotification, object: panel)

        self.panel = panel
        return panel
    }

    @objc private func panelResignedKey(_ note: Notification) {
        (note.object as? NSPanel)?.orderOut(nil)
    }

    private func position(_ panel: NSPanel) {
        let mouse = NSEvent.mouseLocation
        let screen =
            NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) }
            ?? NSScreen.main
        guard let frame = screen?.visibleFrame else { return }
        let size = panel.frame.size
        panel.setFrameOrigin(
            NSPoint(
                x: frame.midX - size.width / 2,
                y: frame.maxY - size.height - frame.height * 0.12))
    }

    // MARK: Global hotkey (⌥⌘T) — Carbon so no accessibility prompt is needed.

    private func registerGlobalHotKey() {
        var eventType = EventTypeSpec(
            eventClass: OSType(kEventClassKeyboard),
            eventKind: UInt32(kEventHotKeyPressed))
        InstallEventHandler(
            GetApplicationEventTarget(),
            { _, _, userData in
                guard let userData else { return noErr }
                let controller = Unmanaged<QuickTerminalController>
                    .fromOpaque(userData).takeUnretainedValue()
                DispatchQueue.main.async { controller.toggle() }
                return noErr
            },
            1, &eventType,
            Unmanaged.passUnretained(self).toOpaque(), nil)

        let hotKeyID = EventHotKeyID(signature: OSType(0x434E_5254), id: 1)  // 'CNRT'
        RegisterEventHotKey(
            UInt32(kVK_ANSI_T), UInt32(cmdKey | optionKey),
            hotKeyID, GetApplicationEventTarget(), 0, &hotKeyRef)
    }
}

private struct QuickTerminalView: View {
    let hostView: PanelHostView
    let chrome: Color
    let text: Color
    let border: Color
    let onClose: () -> Void

    @State private var isCloseHovered = false

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "bolt.fill")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(text.opacity(0.55))
                Text("Quick Terminal")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(text.opacity(0.75))
                Spacer(minLength: 0)
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(text.opacity(isCloseHovered ? 0.95 : 0.5))
                        .frame(width: 18, height: 18)
                        .background(
                            RoundedRectangle(cornerRadius: 5)
                                .fill(Color.black.opacity(isCloseHovered ? 0.12 : 0.0001))
                        )
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Close Quick Terminal")
                .onHover { isCloseHovered = $0 }
            }
            .padding(.horizontal, 12)
            .frame(height: 32)
            .background(chrome.overlay(GrainOverlay()))

            TerminalSurface(hostView: hostView)
        }
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .overlay(
            RoundedRectangle(cornerRadius: 14)
                .strokeBorder(border.opacity(0.9), lineWidth: 1)
        )
    }
}
