import AppKit
import SwiftUI

// OSC 9;4 progress (the ConEmu sequence cargo, winget, systemd and a
// growing list of CLIs emit) surfaced where macOS users actually look:
// a menu-bar pill, the Dock icon's badge, and a ring on the terminal's
// sidebar row. Ghostty draws these in-window; the point here is that
// progress follows you *out* of the app.

struct TerminalProgress: Equatable {
    /// ConEmu numbering: 1 set, 2 error, 3 indeterminate, 4 paused.
    /// (0 removes; removed entries leave the map.)
    var state: Int
    /// 0-100, meaningful for set/error/paused.
    var value: Int

    var isError: Bool { state == 2 }
    var isIndeterminate: Bool { state == 3 }
}

/// Owns the NSStatusItem and Dock badge. One shared instance; AppModel
/// calls `update` whenever the progress map changes.
@MainActor
final class ProgressCenter {
    static let shared = ProgressCenter()
    private var statusItem: NSStatusItem?

    func update(model: AppModel) {
        guard
            let terminalID = model.lastProgressTerminalID,
            let progress = model.terminalProgress[terminalID]
        else {
            clear()
            return
        }

        let item = statusItem ?? makeStatusItem()
        if let button = item.button {
            button.image = NSImage(
                systemSymbolName: progress.isError
                    ? "exclamationmark.triangle.fill" : "terminal.fill",
                accessibilityDescription: "Terminal progress")
            button.title =
                progress.isIndeterminate ? " …" : " \(progress.value)%"
            button.imagePosition = .imageLeft
        }

        NSApp.dockTile.badgeLabel =
            progress.isError
            ? "!" : progress.isIndeterminate ? "…" : "\(progress.value)%"
    }

    private func clear() {
        if let item = statusItem {
            NSStatusBar.system.removeStatusItem(item)
            statusItem = nil
        }
        NSApp.dockTile.badgeLabel = nil
    }

    private func makeStatusItem() -> NSStatusItem {
        let item = NSStatusBar.system.statusItem(
            withLength: NSStatusItem.variableLength)
        item.button?.target = self
        item.button?.action = #selector(statusItemClicked)
        statusItem = item
        return item
    }

    /// Jump to the terminal that's reporting progress.
    @objc private func statusItemClicked() {
        NSApp.activate(ignoringOtherApps: true)
        guard let model = AppModel.shared,
            let terminalID = model.lastProgressTerminalID
        else { return }
        model.selectRootItemContaining(terminalID: terminalID)
    }
}

/// Small determinate ring (or spinner for indeterminate) for sidebar rows.
struct ProgressRingView: View {
    let progress: TerminalProgress
    let tint: Color

    var body: some View {
        if progress.isIndeterminate {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.6)
                .frame(width: 16, height: 16)
        } else {
            ZStack {
                Circle()
                    .stroke(tint.opacity(0.25), lineWidth: 2)
                Circle()
                    .trim(from: 0, to: CGFloat(progress.value) / 100)
                    .stroke(
                        progress.isError ? Color.red : tint,
                        style: StrokeStyle(lineWidth: 2, lineCap: .round)
                    )
                    .rotationEffect(.degrees(-90))
            }
            .frame(width: 12, height: 12)
            .animation(.easeOut(duration: 0.2), value: progress.value)
        }
    }
}
