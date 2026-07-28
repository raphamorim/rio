import AppKit
import Foundation
import Observation

struct Panel: Identifiable {
    let id = UUID()
}

struct PanelColumn: Identifiable {
    let id = UUID()
    var panels: [Panel]
}

@Observable
final class TerminalItem: Identifiable {
    let id = UUID()
    var name: String
    let createdAt = Date()
    var columns: [PanelColumn]
    var focusedPanelID: UUID
    var panelWeights: [UUID: CGFloat] = [:]
    var panelTitles: [UUID: String] = [:]
    // Session restore: seed per-pane working dir + scrollback (consumed by
    // `PanelSession.startIfNeeded`).
    var panelWorkingDirs: [UUID: String] = [:]
    var panelScrollback: [UUID: String] = [:]
    var isExpanded = false

    init(name: String) {
        self.name = name
        let panel = Panel()
        self.columns = [PanelColumn(panels: [panel])]
        self.focusedPanelID = panel.id
    }

    /// Rebuild a tab from a persisted layout. `grid[column][row]` is the
    /// split structure; each entry carries the pane's saved cwd / title /
    /// weight / scrollback, keyed onto freshly minted panel ids.
    init(
        name: String,
        isExpanded: Bool,
        focused: (column: Int, row: Int)?,
        grid: [[(cwd: String?, title: String?, weight: CGFloat, scrollback: String?)]]
    ) {
        // Build everything into locals first — Swift forbids touching `self`
        // (even the defaulted dicts) before all stored properties are set.
        var builtColumns: [PanelColumn] = []
        var weights: [UUID: CGFloat] = [:]
        var titles: [UUID: String] = [:]
        var workingDirs: [UUID: String] = [:]
        var scrollback: [UUID: String] = [:]
        var focusID: UUID?
        for (c, column) in grid.enumerated() where !column.isEmpty {
            var panels: [Panel] = []
            for (r, pane) in column.enumerated() {
                let panel = Panel()
                panels.append(panel)
                if let cwd = pane.cwd { workingDirs[panel.id] = cwd }
                if let title = pane.title { titles[panel.id] = title }
                if let sb = pane.scrollback { scrollback[panel.id] = sb }
                weights[panel.id] = pane.weight
                if let f = focused, f.column == c, f.row == r { focusID = panel.id }
            }
            builtColumns.append(PanelColumn(panels: panels))
        }
        if builtColumns.isEmpty { builtColumns = [PanelColumn(panels: [Panel()])] }

        self.name = name
        self.isExpanded = isExpanded
        self.columns = builtColumns
        self.focusedPanelID = focusID ?? builtColumns[0].panels[0].id
        self.panelWeights = weights
        self.panelTitles = titles
        self.panelWorkingDirs = workingDirs
        self.panelScrollback = scrollback
    }

    var panels: [Panel] {
        columns.flatMap(\.panels)
    }

    var panelIDs: [UUID] {
        panels.map(\.id)
    }

    var panelCount: Int {
        columns.reduce(0) { $0 + $1.panels.count }
    }

    var displayTitle: String {
        if let title = panelTitles[focusedPanelID]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !title.isEmpty
        {
            return title
        }
        return name
    }

    func weight(for id: UUID) -> CGFloat {
        panelWeights[id] ?? 1
    }

    func setWeight(_ weight: CGFloat, for id: UUID) {
        panelWeights[id] = min(max(weight, 0.35), 3.0)
    }

    func position(of panelID: UUID) -> (column: Int, row: Int)? {
        for (columnIndex, column) in columns.enumerated() {
            if let rowIndex = column.panels.firstIndex(where: { $0.id == panelID }) {
                return (columnIndex, rowIndex)
            }
        }
        return nil
    }

    func splitRight() {
        let panel = Panel()
        let columnIndex = position(of: focusedPanelID)?.column ?? columns.count - 1
        columns.insert(PanelColumn(panels: [panel]), at: columnIndex + 1)
        focusedPanelID = panel.id
    }

    func splitDown() {
        let panel = Panel()
        if let position = position(of: focusedPanelID) {
            columns[position.column].panels.insert(panel, at: position.row + 1)
        } else {
            columns[columns.count - 1].panels.append(panel)
        }
        focusedPanelID = panel.id
    }

    func removePanel(_ id: UUID) -> Bool {
        guard let position = position(of: id) else { return false }
        columns[position.column].panels.remove(at: position.row)
        panelWeights.removeValue(forKey: id)
        panelTitles.removeValue(forKey: id)
        if columns[position.column].panels.isEmpty {
            columns.remove(at: position.column)
        }
        if focusedPanelID == id {
            if columns.indices.contains(position.column),
                !columns[position.column].panels.isEmpty
            {
                let column = columns[position.column]
                focusedPanelID = column.panels[min(position.row, column.panels.count - 1)].id
            } else if let first = panels.first {
                focusedPanelID = first.id
            }
        }
        return true
    }
}

@Observable
final class Folder: Identifiable {
    let id = UUID()
    var name: String
    var isExpanded = true
    var children: [SidebarItem] = []

    init(name: String) {
        self.name = name
    }
}

enum SidebarItem: Identifiable {
    case terminal(TerminalItem)
    case folder(Folder)

    var id: UUID {
        switch self {
        case .terminal(let terminal): terminal.id
        case .folder(let folder): folder.id
        }
    }
}

@Observable
final class AppModel {
    var items: [SidebarItem] = []
    var selectedTerminalID: UUID?
    var isSidebarCollapsed = false
    var draggingTerminalID: UUID?
    var pendingRenameFolderID: UUID?
    var fontSize: Float = 13.0

    @ObservationIgnored
    let surfaces = SurfaceRegistry()

    private var nextTerminalIndex = 1
    private var nextFolderIndex = 1

    @ObservationIgnored
    private var saveWorkItem: DispatchWorkItem?

    init() {
        // Restore the previous session if there is one; otherwise start fresh.
        if let restored = SessionStore.load() {
            items = restored
            selectedTerminalID = flattenedTerminals.first?.id
        } else {
            createTerminal()
        }
        RioEngine.shared.onTitle = { [weak self] session, title in
            guard let self else { return }
            session.terminal.panelTitles[session.panelID] = title
            self.scheduleSave()
        }
        RioEngine.shared.onCloseSurface = { [weak self] session in
            self?.closePanel(session.panelID, in: session.terminal)
        }
        // Authoritative save on quit (captures live cwd + scrollback).
        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification,
            object: nil, queue: .main
        ) { [weak self] _ in
            guard let self else { return }
            SessionStore.save(self)
        }
    }

    /// Debounced save so a crash still leaves a recent session on disk.
    func scheduleSave() {
        saveWorkItem?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            SessionStore.save(self)
        }
        saveWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 2, execute: work)
    }

    var selectedTerminal: TerminalItem? {
        flattenedTerminals.first { $0.id == selectedTerminalID }
    }

    var flattenedTerminals: [TerminalItem] {
        Self.terminals(in: items)
    }

    private static func terminals(in items: [SidebarItem]) -> [TerminalItem] {
        var result: [TerminalItem] = []
        for item in items {
            switch item {
            case .terminal(let terminal):
                result.append(terminal)
            case .folder(let folder):
                result.append(contentsOf: terminals(in: folder.children))
            }
        }
        return result
    }

    func createTerminal(in folder: Folder? = nil) {
        let terminal = TerminalItem(name: "Terminal \(nextTerminalIndex)")
        nextTerminalIndex += 1
        if let folder {
            folder.children.append(.terminal(terminal))
            folder.isExpanded = true
        } else {
            items.append(.terminal(terminal))
        }
        selectedTerminalID = terminal.id
    }

    func createFolder() {
        let folder = Folder(name: "")
        items.append(.folder(folder))
        pendingRenameFolderID = folder.id
    }

    func commitFolderName(_ folder: Folder) {
        let trimmed = folder.name.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty {
            folder.name = "Folder \(nextFolderIndex)"
            nextFolderIndex += 1
        } else {
            folder.name = trimmed
        }
    }

    func closeTerminal(_ id: UUID) {
        let flattened = flattenedTerminals
        guard let index = flattened.firstIndex(where: { $0.id == id }) else {
            return
        }
        let panelIDs = flattened[index].panelIDs
        Self.removeTerminal(id, from: &items)
        for panelID in panelIDs {
            surfaces.remove(panelID)
        }
        if selectedTerminalID == id {
            let remaining = flattenedTerminals
            if remaining.isEmpty {
                selectedTerminalID = nil
            } else {
                selectedTerminalID = remaining[min(index, remaining.count - 1)].id
            }
        }
    }

    func closeSelectedTerminal() {
        if let id = selectedTerminalID {
            closeTerminal(id)
        }
    }

    func splitRightInSelected() {
        selectedTerminal?.splitRight()
    }

    func splitDownInSelected() {
        selectedTerminal?.splitDown()
    }

    var focusedSession: PanelSession? {
        guard let terminal = selectedTerminal else { return nil }
        return surfaces.existingSession(for: terminal.focusedPanelID)
    }

    func adjustFontSize(by delta: Float) {
        setFontSize(fontSize + delta)
    }

    func resetFontSize() {
        setFontSize(13.0)
    }

    private func setFontSize(_ size: Float) {
        fontSize = min(max(size, 6.0), 72.0)
        RioEngine.fontSize = fontSize
        for session in surfaces.allSessions {
            session.setFontSize(fontSize)
        }
    }

    func copySelection() {
        guard let session = focusedSession, let text = session.selectionText(),
            !text.isEmpty
        else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    func pasteClipboard() {
        guard let session = focusedSession,
            let text = NSPasteboard.general.string(forType: .string)
        else { return }
        session.sendText(text)
    }

    func closeFocusedPanel() {
        guard let terminal = selectedTerminal else { return }
        closePanel(terminal.focusedPanelID, in: terminal)
    }

    func closePanel(_ id: UUID, in terminal: TerminalItem) {
        if terminal.panelCount <= 1 {
            closeTerminal(terminal.id)
            return
        }
        if terminal.removePanel(id) {
            surfaces.remove(id)
        }
    }

    func selectTerminal(at index: Int) {
        let flattened = flattenedTerminals
        guard flattened.indices.contains(index) else { return }
        selectedTerminalID = flattened[index].id
    }

    func moveDraggedTerminal(before targetID: UUID) {
        guard let dragging = draggingTerminalID, dragging != targetID else { return }
        if Self.isImmediatelyBefore(dragging, targetID, in: items) { return }
        guard let terminal = Self.extractTerminal(dragging, from: &items) else { return }
        if !Self.insert(terminal, before: targetID, in: &items) {
            items.append(.terminal(terminal))
        }
    }

    func moveDraggedIntoFolder(_ folder: Folder) {
        guard let dragging = draggingTerminalID else { return }
        if folder.children.contains(where: { $0.id == dragging }) { return }
        guard let terminal = Self.extractTerminal(dragging, from: &items) else { return }
        folder.children.append(.terminal(terminal))
        folder.isExpanded = true
    }

    func moveDraggedToRootEnd() {
        guard let dragging = draggingTerminalID else { return }
        if items.last?.id == dragging { return }
        guard let terminal = Self.extractTerminal(dragging, from: &items) else { return }
        items.append(.terminal(terminal))
    }

    private static func insert(
        _ terminal: TerminalItem, before targetID: UUID, in items: inout [SidebarItem]
    ) -> Bool {
        for (index, item) in items.enumerated() {
            if item.id == targetID {
                items.insert(.terminal(terminal), at: index)
                return true
            }
            if case .folder(let folder) = item {
                if insert(terminal, before: targetID, in: &folder.children) {
                    return true
                }
            }
        }
        return false
    }

    private static func isImmediatelyBefore(
        _ first: UUID, _ second: UUID, in items: [SidebarItem]
    ) -> Bool {
        for (index, item) in items.enumerated() {
            if item.id == second {
                return index > 0 && items[index - 1].id == first
            }
            if case .folder(let folder) = item {
                if isImmediatelyBefore(first, second, in: folder.children) {
                    return true
                }
            }
        }
        return false
    }

    private static func extractTerminal(_ id: UUID, from items: inout [SidebarItem]) -> TerminalItem? {
        for (index, item) in items.enumerated() {
            switch item {
            case .terminal(let terminal):
                if terminal.id == id {
                    items.remove(at: index)
                    return terminal
                }
            case .folder(let folder):
                if let found = extractTerminal(id, from: &folder.children) {
                    return found
                }
            }
        }
        return nil
    }

    func deleteFolder(_ folder: Folder) {
        for terminal in Self.terminals(in: folder.children) {
            for panelID in terminal.panelIDs {
                surfaces.remove(panelID)
            }
            if selectedTerminalID == terminal.id {
                selectedTerminalID = nil
            }
        }
        Self.removeFolder(folder.id, from: &items)
        if selectedTerminalID == nil, let first = flattenedTerminals.first {
            selectedTerminalID = first.id
        }
    }

    @discardableResult
    private static func removeTerminal(_ id: UUID, from items: inout [SidebarItem]) -> Bool {
        for (index, item) in items.enumerated() {
            switch item {
            case .terminal(let terminal):
                if terminal.id == id {
                    items.remove(at: index)
                    return true
                }
            case .folder(let folder):
                if removeTerminal(id, from: &folder.children) {
                    return true
                }
            }
        }
        return false
    }

    @discardableResult
    private static func removeFolder(_ id: UUID, from items: inout [SidebarItem]) -> Bool {
        for (index, item) in items.enumerated() {
            if case .folder(let folder) = item {
                if folder.id == id {
                    items.remove(at: index)
                    return true
                }
                if removeFolder(id, from: &folder.children) {
                    return true
                }
            }
        }
        return false
    }
}
