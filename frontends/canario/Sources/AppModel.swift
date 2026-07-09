import Foundation
import Observation

struct Panel: Identifiable {
    let id = UUID()
}

enum SplitDirection {
    case right
    case down
}

indirect enum PanelNode {
    case leaf(Panel)
    case split(SplitDirection, PanelNode, PanelNode)
}

@Observable
final class TerminalItem: Identifiable {
    let id = UUID()
    var name: String
    let createdAt = Date()
    var panelRoot: PanelNode
    var focusedPanelID: UUID

    init(name: String) {
        self.name = name
        let panel = Panel()
        self.panelRoot = .leaf(panel)
        self.focusedPanelID = panel.id
    }

    var panelIDs: [UUID] {
        Self.leafIDs(panelRoot)
    }

    var panelCount: Int {
        panelIDs.count
    }

    func split(_ direction: SplitDirection) {
        let newPanel = Panel()
        panelRoot = Self.splitting(
            panelRoot, at: focusedPanelID, direction: direction, newPanel: newPanel)
        focusedPanelID = newPanel.id
    }

    private static func leafIDs(_ node: PanelNode) -> [UUID] {
        switch node {
        case .leaf(let panel):
            [panel.id]
        case .split(_, let first, let second):
            leafIDs(first) + leafIDs(second)
        }
    }

    private static func splitting(
        _ node: PanelNode, at id: UUID, direction: SplitDirection, newPanel: Panel
    ) -> PanelNode {
        switch node {
        case .leaf(let panel):
            return panel.id == id ? .split(direction, node, .leaf(newPanel)) : node
        case .split(let existing, let first, let second):
            return .split(
                existing,
                splitting(first, at: id, direction: direction, newPanel: newPanel),
                splitting(second, at: id, direction: direction, newPanel: newPanel))
        }
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

    @ObservationIgnored
    let surfaces = SurfaceRegistry()

    private var nextTerminalIndex = 1
    private var nextFolderIndex = 1

    init() {
        createTerminal()
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

    func splitSelected(_ direction: SplitDirection) {
        selectedTerminal?.split(direction)
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
