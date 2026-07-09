import Foundation
import Observation

struct TerminalItem: Identifiable, Hashable {
    let id: UUID
    var name: String
    let createdAt: Date

    init(name: String) {
        self.id = UUID()
        self.name = name
        self.createdAt = Date()
    }
}

@Observable
final class AppModel {
    var terminals: [TerminalItem] = []
    var selectedTerminalID: UUID?
    var isSidebarCollapsed = false

    @ObservationIgnored
    let surfaces = SurfaceRegistry()

    private var nextIndex = 1

    init() {
        createTerminal()
    }

    var selectedTerminal: TerminalItem? {
        terminals.first { $0.id == selectedTerminalID }
    }

    func createTerminal() {
        let terminal = TerminalItem(name: "Terminal \(nextIndex)")
        nextIndex += 1
        terminals.append(terminal)
        selectedTerminalID = terminal.id
    }

    func closeTerminal(_ id: UUID) {
        guard let index = terminals.firstIndex(where: { $0.id == id }) else {
            return
        }
        terminals.remove(at: index)
        surfaces.remove(id)
        if selectedTerminalID == id {
            if terminals.isEmpty {
                selectedTerminalID = nil
            } else {
                selectedTerminalID = terminals[min(index, terminals.count - 1)].id
            }
        }
    }

    func closeSelectedTerminal() {
        if let id = selectedTerminalID {
            closeTerminal(id)
        }
    }

    func selectTerminal(at index: Int) {
        guard terminals.indices.contains(index) else { return }
        selectedTerminalID = terminals[index].id
    }
}
