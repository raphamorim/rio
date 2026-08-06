import AppKit
import SwiftUI

// Picture-in-picture panes: pop a *running* pane out of the window into
// a small always-on-top panel, the way videos pop out of a browser.
// Watch the build from another app; close the panel and the pane slides
// back into its tab.

final class PiPController {
    private weak var model: AppModel?
    private var panels: [UUID: NSPanel] = [:]
    private var delegates: [UUID: PiPWindowDelegate] = [:]

    init(model: AppModel) {
        self.model = model
    }

    func isPoppedOut(_ panelID: UUID) -> Bool {
        panels[panelID] != nil
    }

    func popOut(_ session: PanelSession) {
        guard let model, panels[session.panelID] == nil else { return }
        model.pipPanelIDs.insert(session.panelID)

        // Let SwiftUI unmount the pane's representable first, so it does
        // not tear the view back out of the panel afterwards.
        DispatchQueue.main.async { [weak self] in
            self?.presentPanel(for: session)
        }
    }

    func bringBack(_ panelID: UUID) {
        guard let panel = panels[panelID] else {
            model?.pipPanelIDs.remove(panelID)
            return
        }
        panel.orderOut(nil)
        finishBringBack(panelID)
    }

    private func finishBringBack(_ panelID: UUID) {
        if let panel = panels[panelID] {
            panel.contentView = NSView()
        }
        panels.removeValue(forKey: panelID)
        delegates.removeValue(forKey: panelID)
        model?.pipPanelIDs.remove(panelID)
    }

    private func presentPanel(for session: PanelSession) {
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 460, height: 300),
            styleMask: [
                .titled, .closable, .resizable, .utilityWindow,
                .nonactivatingPanel,
            ],
            backing: .buffered, defer: false)
        panel.title = session.terminal.displayTitle
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.fullScreenAuxiliary, .managed]
        panel.minSize = NSSize(width: 280, height: 180)

        session.hostView.removeFromSuperview()
        panel.contentView = session.hostView

        // Bottom-right corner of the screen the main window sits on.
        let screen =
            NSApp.mainWindow?.screen ?? NSScreen.main ?? NSScreen.screens[0]
        let frame = screen.visibleFrame
        panel.setFrameOrigin(
            NSPoint(x: frame.maxX - 460 - 24, y: frame.minY + 24))

        let delegate = PiPWindowDelegate { [weak self] in
            self?.finishBringBack(session.panelID)
        }
        panel.delegate = delegate
        delegates[session.panelID] = delegate
        panels[session.panelID] = panel

        panel.orderFront(nil)
        panel.makeKey()
        panel.makeFirstResponder(session.hostView)
        session.syncSize()
    }
}

/// Closing the panel returns the pane to the main window.
private final class PiPWindowDelegate: NSObject, NSWindowDelegate {
    private let onClose: () -> Void

    init(onClose: @escaping () -> Void) {
        self.onClose = onClose
    }

    func windowWillClose(_ notification: Notification) {
        onClose()
    }
}
