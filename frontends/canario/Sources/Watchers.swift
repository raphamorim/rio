import AppKit
import SwiftUI

// Watchers: breakpoints for terminal output. Select a string, right-click,
// "Watch for" — and when that text next appears in the terminal's output,
// the sidebar row badges with a hit count and the Dock asks for attention
// if Canario is in the background. iTerm2 buries this power in profile
// preferences as regex Triggers; here it's two clicks on the text itself.

struct Watcher: Identifiable {
    let id = UUID()
    let terminalID: UUID
    var pattern: String
    var hits = 0
    var lastFired: Date?
}

/// Diffs each render's visible rows against the previous ones and fires
/// watchers on lines that changed. Content-diff instead of damage flags,
/// so re-renders of an unchanged screen never refire.
final class WatcherScanner {
    static let shared = WatcherScanner()
    private var lastRows: [UUID: [String]] = [:]

    func scan(session: PanelSession) {
        guard let model = AppModel.shared else { return }
        let terminalID = session.terminal.id
        guard model.watchers.contains(where: { $0.terminalID == terminalID })
        else {
            lastRows.removeValue(forKey: session.panelID)
            return
        }

        let rows = session.visibleTextRows()
        defer { lastRows[session.panelID] = rows }
        // First scan is the baseline: text already on screen when the
        // watcher was created doesn't count as a new appearance.
        guard let previous = lastRows[session.panelID] else { return }

        for (index, row) in rows.enumerated() {
            if index < previous.count && previous[index] == row { continue }
            guard !row.isEmpty else { continue }
            let haystack = row.lowercased()
            for watcher in model.watchers
            where watcher.terminalID == terminalID
                && haystack.contains(watcher.pattern.lowercased())
            {
                model.watcherFired(watcher.id)
            }
        }
    }

    func forget(panelID: UUID) {
        lastRows.removeValue(forKey: panelID)
    }
}

/// Sidebar badge: an eye while armed, with the hit count once something
/// matched. The context menu clears or removes.
struct WatcherBadgeView: View {
    @Environment(AppModel.self) private var model
    let terminal: TerminalItem
    let tint: Color

    var body: some View {
        let watchers = model.watchers.filter { $0.terminalID == terminal.id }
        if !watchers.isEmpty {
            let hits = watchers.reduce(0) { $0 + $1.hits }
            HStack(spacing: 3) {
                Image(systemName: hits > 0 ? "eye.fill" : "eye")
                    .font(.system(size: 10, weight: .semibold))
                if hits > 0 {
                    Text("\(hits)")
                        .font(.system(size: 10, weight: .bold))
                        .monospacedDigit()
                }
            }
            .foregroundStyle(
                hits > 0 ? AnyShapeStyle(tint) : AnyShapeStyle(tint.opacity(0.5))
            )
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(
                Capsule().fill(Color.black.opacity(hits > 0 ? 0.22 : 0.10)))
            .contextMenu {
                ForEach(watchers) { watcher in
                    Button("Stop watching \u{201C}\(watcher.pattern)\u{201D}") {
                        model.removeWatcher(watcher.id)
                    }
                }
                if hits > 0 {
                    Divider()
                    Button("Clear Hits") {
                        model.clearWatcherHits(for: terminal.id)
                    }
                }
            }
        }
    }
}
