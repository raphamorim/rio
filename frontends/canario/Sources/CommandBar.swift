import SwiftUI

// Arc-style command bar (⌘K): one input that finds terminals and panes by
// title, or runs an app action, without touching the sidebar. Visual
// language follows Arc's palette: dark floating panel, large input row,
// icon chips, and a selected row tinted with the current space's color.

private struct CommandEntry: Identifiable {
    let id: String
    let icon: String
    let title: String
    let subtitle: String?
    let run: () -> Void
}

struct CommandBarView: View {
    @Environment(AppModel.self) private var model

    @State private var query = ""
    @State private var selectedIndex = 0
    @FocusState private var fieldFocused: Bool

    var body: some View {
        ZStack(alignment: .top) {
            Color.black.opacity(0.30)
                .ignoresSafeArea()
                .onTapGesture { close() }

            VStack(spacing: 0) {
                HStack(spacing: 12) {
                    RoundedRectangle(cornerRadius: 7)
                        .fill(accentGradient)
                        .frame(width: 26, height: 26)
                        .overlay {
                            Image(systemName: "terminal")
                                .font(.system(size: 12, weight: .semibold))
                                .foregroundStyle(.white)
                        }
                    TextField("Search terminals, run commands…", text: $query)
                        .textFieldStyle(.plain)
                        .font(.system(size: 19, weight: .medium))
                        .foregroundStyle(.white)
                        .focused($fieldFocused)
                        .onSubmit { runSelected() }
                        .onKeyPress(.downArrow) {
                            selectedIndex = min(selectedIndex + 1, max(results.count - 1, 0))
                            return .handled
                        }
                        .onKeyPress(.upArrow) {
                            selectedIndex = max(selectedIndex - 1, 0)
                            return .handled
                        }
                }
                .padding(.horizontal, 18)
                .frame(height: 62)

                if !results.isEmpty {
                    Rectangle()
                        .fill(.white.opacity(0.08))
                        .frame(height: 1)

                    ScrollViewReader { proxy in
                        ScrollView(.vertical, showsIndicators: false) {
                            VStack(spacing: 4) {
                                ForEach(Array(results.enumerated()), id: \.element.entry.id) {
                                    index, result in
                                    CommandRowView(
                                        entry: result.entry,
                                        highlights: result.match.titleHighlights,
                                        isSelected: index == selectedIndex,
                                        accent: accentColor
                                    ) {
                                        selectedIndex = index
                                        runSelected()
                                    }
                                    .id(result.entry.id)
                                }
                            }
                            .padding(10)
                        }
                        .frame(maxHeight: 380)
                        .onChange(of: selectedIndex) { _, index in
                            if results.indices.contains(index) {
                                proxy.scrollTo(results[index].entry.id)
                            }
                        }
                    }
                }
            }
            .frame(width: 640)
            .background(
                RoundedRectangle(cornerRadius: 18)
                    .fill(Color(red: 0.14, green: 0.13, blue: 0.15).opacity(0.98))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 18)
                    .strokeBorder(.white.opacity(0.09), lineWidth: 1)
            )
            .shadow(color: .black.opacity(0.45), radius: 38, y: 16)
            .padding(.top, 96)
            .environment(\.colorScheme, .dark)
            .transition(.scale(scale: 0.97).combined(with: .opacity))
        }
        .onAppear { fieldFocused = true }
        .onExitCommand { close() }
        .onChange(of: query) { _, _ in selectedIndex = 0 }
    }

    /// Selected-row tint follows the current space, like Arc; a deep salmon
    /// mauve when the selected terminal lives outside any space.
    private var accentColor: Color {
        if let id = model.selectedTerminalID,
            let folder = model.rootFolder(containing: id)
        {
            return Theme.spaceGradients[
                folder.colorIndex % Theme.spaceGradients.count
            ].bottom
        }
        return Color(red: 0.71, green: 0.36, blue: 0.46)
    }

    private var accentGradient: LinearGradient {
        if let id = model.selectedTerminalID,
            let folder = model.rootFolder(containing: id)
        {
            return Theme.spaceGradient(folder.colorIndex)
        }
        return LinearGradient(
            colors: [Theme.chrome, Color(red: 0.71, green: 0.36, blue: 0.46)],
            startPoint: .topLeading, endPoint: .bottomTrailing)
    }

    private var results: [(entry: CommandEntry, match: FuzzyMatch)] {
        let scored = allEntries.compactMap {
            entry -> (entry: CommandEntry, match: FuzzyMatch)? in
            guard
                let match = FuzzyMatch.match(
                    query: query, title: entry.title, subtitle: entry.subtitle)
            else { return nil }
            return (entry, match)
        }
        return scored.sorted { $0.match.score > $1.match.score }
    }

    private var allEntries: [CommandEntry] {
        var entries: [CommandEntry] = []

        for terminal in model.flattenedTerminals {
            let folder = model.rootFolder(containing: terminal.id)
            entries.append(
                CommandEntry(
                    id: terminal.id.uuidString,
                    icon: "terminal",
                    title: terminal.displayTitle,
                    subtitle: folder?.name
                ) {
                    model.selectedTerminalID = terminal.id
                })
            guard terminal.panelCount > 1 else { continue }
            for (index, panel) in terminal.panels.enumerated() {
                entries.append(
                    CommandEntry(
                        id: panel.id.uuidString,
                        icon: "rectangle.on.rectangle",
                        title: terminal.panelTitles[panel.id] ?? "Panel \(index + 1)",
                        subtitle: terminal.displayTitle
                    ) {
                        model.selectedTerminalID = terminal.id
                        terminal.focusedPanelID = panel.id
                    })
            }
        }

        let actions: [(String, String, () -> Void)] = [
            ("plus", "New Terminal", { model.createTerminal() }),
            ("folder.badge.plus", "New Folder", { model.createFolder() }),
            ("rectangle.split.2x1", "Split Right", { model.splitRightInSelected() }),
            ("rectangle.split.1x2", "Split Down", { model.splitDownInSelected() }),
            ("xmark.rectangle", "Close Panel", { model.closeFocusedPanel() }),
            ("sidebar.left", "Toggle Sidebar", { model.isSidebarCollapsed.toggle() }),
            ("bolt", "Quick Terminal", { model.quickTerminal.toggle() }),
            ("textformat.size.larger", "Increase Font Size", { model.adjustFontSize(by: 1) }),
            ("textformat.size.smaller", "Decrease Font Size", { model.adjustFontSize(by: -1) }),
            ("textformat.size", "Reset Font Size", { model.resetFontSize() }),
        ]
        for (icon, title, action) in actions {
            entries.append(
                CommandEntry(id: "action.\(title)", icon: icon, title: title, subtitle: "Action") {
                    action()
                })
        }
        return entries
    }

    private func runSelected() {
        guard results.indices.contains(selectedIndex) else { return }
        let entry = results[selectedIndex].entry
        close()
        withAnimation(.spring(duration: 0.25)) {
            entry.run()
        }
    }

    private func close() {
        model.isCommandBarVisible = false
        model.refocusTerminal()
    }
}

private struct CommandRowView: View {
    let entry: CommandEntry
    let highlights: Set<Int>
    let isSelected: Bool
    let accent: Color
    let action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                RoundedRectangle(cornerRadius: 7)
                    .fill(.white.opacity(isSelected ? 0.22 : 0.10))
                    .frame(width: 28, height: 28)
                    .overlay {
                        Image(systemName: entry.icon)
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(.white.opacity(isSelected ? 1.0 : 0.75))
                    }

                // Arc-style single line: bold title — dimmed subtitle.
                // Matched characters carry the accent so the fuzzy hit
                // is visible, fzf-style.
                HStack(spacing: 6) {
                    highlightedTitle
                        .font(.system(size: 14, weight: .semibold))
                        .lineLimit(1)
                    if let subtitle = entry.subtitle {
                        Text("— \(subtitle)")
                            .font(.system(size: 13, weight: .regular))
                            .foregroundStyle(.white.opacity(isSelected ? 0.75 : 0.40))
                            .lineLimit(1)
                    }
                }

                Spacer(minLength: 0)

                if isSelected {
                    Image(systemName: "return")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.white.opacity(0.55))
                }
            }
            .padding(.horizontal, 12)
            .frame(height: 46)
            .background(
                RoundedRectangle(cornerRadius: 11)
                    .fill(
                        isSelected
                            ? AnyShapeStyle(accent.opacity(0.55))
                            : AnyShapeStyle(Color.white.opacity(isHovered ? 0.06 : 0.0001)))
            )
            .contentShape(RoundedRectangle(cornerRadius: 11))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }

    private var highlightedTitle: Text {
        var text = Text("")
        for (index, character) in entry.title.enumerated() {
            var piece = Text(String(character))
            if highlights.contains(index) {
                piece = piece
                    .foregroundColor(Color(red: 1.0, green: 0.62, blue: 0.66))
                    .bold()
            } else {
                piece = piece.foregroundColor(.white.opacity(0.95))
            }
            text = text + piece
        }
        return text
    }
}

/// fzf-style fuzzy matcher: every whitespace-separated query token must
/// subsequence-match the "title subtitle" haystack. Scoring rewards word
/// starts, consecutive runs and prefix hits; title matches report their
/// character positions so rows can light them up.
struct FuzzyMatch {
    let score: Int
    let titleHighlights: Set<Int>

    static func match(
        query: String, title: String, subtitle: String?
    ) -> FuzzyMatch? {
        let tokens = query.lowercased().split(whereSeparator: \.isWhitespace)
        if tokens.isEmpty { return FuzzyMatch(score: 0, titleHighlights: []) }

        // "\u{1}" separates title from subtitle so a match can never
        // straddle the boundary as one consecutive run.
        let haystack = Array(
            (title + "\u{1}" + (subtitle ?? "")).lowercased())
        let titleCount = title.count

        var total = 0
        var highlights: Set<Int> = []
        for token in tokens {
            guard
                let result = matchToken(Array(token), in: haystack)
            else { return nil }
            total += result.score
            for index in result.positions where index < titleCount {
                highlights.insert(index)
            }
        }
        return FuzzyMatch(score: total, titleHighlights: highlights)
    }

    private static func matchToken(
        _ token: [Character], in haystack: [Character]
    ) -> (score: Int, positions: [Int])? {
        var positions: [Int] = []
        var score = 0
        var streak = 0
        var ti = 0
        for (i, ch) in haystack.enumerated() {
            guard ti < token.count else { break }
            if ch == token[ti] {
                let boundary =
                    i == 0 || haystack[i - 1] == " " || haystack[i - 1] == "\u{1}"
                    || haystack[i - 1] == "-" || haystack[i - 1] == "_"
                    || haystack[i - 1] == "/" || haystack[i - 1] == "."
                streak += 1
                score += 10 + streak * 5
                if boundary { score += 30 }
                if i == 0 { score += 20 }
                positions.append(i)
                ti += 1
            } else {
                streak = 0
                score -= 1
            }
        }
        return ti == token.count ? (score, positions) : nil
    }
}
