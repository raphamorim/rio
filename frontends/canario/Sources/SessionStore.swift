import CoreGraphics
import Foundation

// Tier A session persistence: serialize the sidebar tree (folders + tabs +
// split layout) plus each pane's cwd / title / scrollback, and rebuild it on
// launch. Shells are restarted; only state is restored. See
// SESSION_PERSISTENCE.md.

// MARK: - On-disk DTOs (the model types wrap @Observable classes and aren't
// Codable, so we mirror them here).

private struct SessionDTO: Codable {
    var items: [ItemDTO]
}

private enum ItemDTO: Codable {
    case folder(FolderDTO)
    case terminal(TerminalDTO)
}

private struct FolderDTO: Codable {
    var name: String
    var isExpanded: Bool
    var children: [ItemDTO]
}

private struct TerminalDTO: Codable {
    var name: String
    var isExpanded: Bool
    var focusedColumn: Int?
    var focusedRow: Int?
    var columns: [[PaneDTO]]
}

private struct PaneDTO: Codable {
    var cwd: String?
    var title: String?
    var weight: Double
    var scrollbackFile: String?
}

enum SessionStore {
    private static let dir: URL = {
        let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask
        ).first!
        return base.appendingPathComponent("canario", isDirectory: true)
    }()
    private static var sessionFile: URL { dir.appendingPathComponent("session.json") }
    private static var scrollbackDir: URL {
        dir.appendingPathComponent("scrollback", isDirectory: true)
    }

    // MARK: Save

    static func save(_ model: AppModel) {
        ensureDirs()
        // Rewrite scrollback fresh so removed panes don't leave orphans.
        try? FileManager.default.removeItem(at: scrollbackDir)
        try? FileManager.default.createDirectory(
            at: scrollbackDir, withIntermediateDirectories: true)

        let dto = SessionDTO(items: model.items.map { encode($0, model) })
        guard let data = try? JSONEncoder().encode(dto) else { return }
        // Atomic: write to a temp file then rename.
        let tmp = sessionFile.appendingPathExtension("tmp")
        do {
            try data.write(to: tmp)
            _ = try? FileManager.default.replaceItemAt(sessionFile, withItemAt: tmp)
        } catch {
            try? data.write(to: sessionFile)
        }
    }

    private static func encode(_ item: SidebarItem, _ model: AppModel) -> ItemDTO {
        switch item {
        case .folder(let folder):
            return .folder(
                FolderDTO(
                    name: folder.name,
                    isExpanded: folder.isExpanded,
                    children: folder.children.map { encode($0, model) }))
        case .terminal(let terminal):
            var columns: [[PaneDTO]] = []
            for column in terminal.columns {
                var panes: [PaneDTO] = []
                for panel in column.panels {
                    let snap = model.surfaces.existingSession(for: panel.id)?.snapshot()
                    var scrollbackFile: String?
                    if let text = snap?.scrollback, !text.isEmpty {
                        let name = "\(panel.id.uuidString).txt"
                        try? text.write(
                            to: scrollbackDir.appendingPathComponent(name),
                            atomically: true, encoding: .utf8)
                        scrollbackFile = name
                    }
                    panes.append(
                        PaneDTO(
                            cwd: snap?.cwd ?? terminal.panelWorkingDirs[panel.id],
                            title: terminal.panelTitles[panel.id],
                            weight: Double(terminal.weight(for: panel.id)),
                            scrollbackFile: scrollbackFile))
                }
                columns.append(panes)
            }
            let pos = terminal.position(of: terminal.focusedPanelID)
            return .terminal(
                TerminalDTO(
                    name: terminal.name,
                    isExpanded: terminal.isExpanded,
                    focusedColumn: pos?.column,
                    focusedRow: pos?.row,
                    columns: columns))
        }
    }

    // MARK: Load

    /// Rebuilt sidebar items, or nil on first run / unreadable / empty.
    static func load() -> [SidebarItem]? {
        guard let data = try? Data(contentsOf: sessionFile),
            let dto = try? JSONDecoder().decode(SessionDTO.self, from: data)
        else { return nil }
        let items = dto.items.map { decode($0) }
        return items.isEmpty ? nil : items
    }

    private static func decode(_ dto: ItemDTO) -> SidebarItem {
        switch dto {
        case .folder(let folder):
            let model = Folder(name: folder.name)
            model.isExpanded = folder.isExpanded
            model.children = folder.children.map { decode($0) }
            return .folder(model)
        case .terminal(let terminal):
            let grid = terminal.columns.map { column in
                column.map {
                    pane -> (cwd: String?, title: String?, weight: CGFloat, scrollback: String?) in
                    var scrollback: String?
                    if let file = pane.scrollbackFile {
                        scrollback = try? String(
                            contentsOf: scrollbackDir.appendingPathComponent(file),
                            encoding: .utf8)
                    }
                    return (pane.cwd, pane.title, CGFloat(pane.weight), scrollback)
                }
            }
            let focused: (column: Int, row: Int)? =
                (terminal.focusedColumn).flatMap { col in
                    (terminal.focusedRow).map { row in (col, row) }
                }
            return .terminal(
                TerminalItem(
                    name: terminal.name,
                    isExpanded: terminal.isExpanded,
                    focused: focused,
                    grid: grid))
        }
    }

    private static func ensureDirs() {
        try? FileManager.default.createDirectory(
            at: dir, withIntermediateDirectories: true)
    }
}
