import SwiftUI

// Arc/Zed-style theme picker (⌘⇧T): a floating panel with the live app as
// the preview surface, no scrim, no preview pane. Arrowing or hovering
// recolors every terminal instantly; Enter or a click commits; Esc or a
// click outside reverts to the scheme that was committed when it opened.

struct ThemePickerView: View {
    @Environment(AppModel.self) private var model

    @State private var query = ""
    @State private var selectedIndex = 0
    @State private var filter: Filter = .all
    /// The scheme to fall back to on Esc, captured at open.
    @State private var committed: ColorScheme = .rio
    @FocusState private var fieldFocused: Bool

    private enum Filter: String, CaseIterable {
        case all = "All"
        case dark = "Dark"
        case light = "Light"

        func admits(_ scheme: ColorScheme) -> Bool {
            switch self {
            case .all: true
            case .dark: scheme.isDark
            case .light: !scheme.isDark
            }
        }
    }

    var body: some View {
        ZStack(alignment: .top) {
            // Transparent click-catcher: the whole window must stay visible
            // (it IS the preview), but clicking away cancels.
            Color.black.opacity(0.001)
                .ignoresSafeArea()
                .onTapGesture { revertAndClose() }

            VStack(spacing: 0) {
                HStack(spacing: 12) {
                    RoundedRectangle(cornerRadius: 7)
                        .fill(
                            LinearGradient(
                                colors: [
                                    Color(scheme: model.colorScheme.ansi[5]),
                                    Color(scheme: model.colorScheme.ansi[4]),
                                ],
                                startPoint: .topLeading, endPoint: .bottomTrailing)
                        )
                        .frame(width: 26, height: 26)
                        .overlay {
                            Image(systemName: "paintpalette")
                                .font(.system(size: 12, weight: .semibold))
                                .foregroundStyle(.white)
                        }
                    TextField("Search themes…", text: $query)
                        .textFieldStyle(.plain)
                        .font(.system(size: 19, weight: .medium))
                        .foregroundStyle(.white)
                        .focused($fieldFocused)
                        .onSubmit { commitSelected() }
                        .onKeyPress(.downArrow) {
                            moveSelection(by: 1)
                            return .handled
                        }
                        .onKeyPress(.upArrow) {
                            moveSelection(by: -1)
                            return .handled
                        }

                    Picker("", selection: $filter) {
                        ForEach(Filter.allCases, id: \.self) { choice in
                            Text(choice.rawValue).tag(choice)
                        }
                    }
                    .pickerStyle(.segmented)
                    .controlSize(.small)
                    .frame(width: 150)
                }
                .padding(.horizontal, 18)
                .frame(height: 62)

                Rectangle()
                    .fill(.white.opacity(0.08))
                    .frame(height: 1)

                ScrollViewReader { proxy in
                    ScrollView(.vertical, showsIndicators: false) {
                        VStack(spacing: 6) {
                            ForEach(Array(results.enumerated()), id: \.element.id) {
                                index, scheme in
                                ThemeCardView(
                                    scheme: scheme,
                                    isSelected: index == selectedIndex,
                                    isCurrent: scheme.name == committed.name
                                ) {
                                    selectedIndex = index
                                    commitSelected()
                                } onHover: { hovering in
                                    guard hovering else { return }
                                    selectedIndex = index
                                    model.previewColorScheme(scheme)
                                }
                                .id(scheme.id)
                            }
                        }
                        .padding(10)
                    }
                    .frame(maxHeight: 440)
                    .onChange(of: selectedIndex) { _, index in
                        if results.indices.contains(index) {
                            proxy.scrollTo(results[index].id)
                        }
                    }
                }

                Rectangle()
                    .fill(.white.opacity(0.08))
                    .frame(height: 1)

                HStack(spacing: 14) {
                    hint("arrow.up.arrow.down", "preview")
                    hint("return", "apply")
                    hint("escape", "cancel")
                    Spacer(minLength: 0)
                    Text("\(results.count) themes")
                        .font(.system(size: 11))
                        .foregroundStyle(.white.opacity(0.35))
                }
                .padding(.horizontal, 16)
                .frame(height: 34)
            }
            .frame(width: 480)
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
        .onAppear {
            committed = model.colorScheme
            selectedIndex =
                results.firstIndex { $0.name == model.colorScheme.name } ?? 0
            fieldFocused = true
        }
        .onExitCommand { revertAndClose() }
        .onChange(of: query) { _, _ in selectedIndex = 0 }
        .onChange(of: filter) { _, _ in selectedIndex = 0 }
    }

    private func hint(_ symbol: String, _ label: String) -> some View {
        HStack(spacing: 5) {
            Image(systemName: symbol)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.white.opacity(0.5))
            Text(label)
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.35))
        }
    }

    private var results: [ColorScheme] {
        ColorScheme.prebuilt.filter { scheme in
            guard filter.admits(scheme) else { return false }
            return FuzzyMatch.match(query: query, title: scheme.name, subtitle: nil)
                != nil
        }
    }

    private func moveSelection(by delta: Int) {
        guard !results.isEmpty else { return }
        selectedIndex = min(max(selectedIndex + delta, 0), results.count - 1)
        model.previewColorScheme(results[selectedIndex])
    }

    private func commitSelected() {
        guard results.indices.contains(selectedIndex) else {
            revertAndClose()
            return
        }
        model.setColorScheme(results[selectedIndex])
        close()
    }

    private func revertAndClose() {
        model.setColorScheme(committed)
        close()
    }

    private func close() {
        model.isThemePickerVisible = false
        model.refocusTerminal()
    }
}

/// A mini terminal rendered from the scheme's own tokens: prompt line,
/// ls output, an error line with a selection run, then the 16-color strip
/// with the block cursor: the colors a swatch can't show in context.
private struct ThemeCardView: View {
    let scheme: ColorScheme
    let isSelected: Bool
    let isCurrent: Bool
    let action: () -> Void
    let onHover: (Bool) -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                preview
                    .frame(width: 210)

                VStack(alignment: .leading, spacing: 3) {
                    Text(scheme.name)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(.white.opacity(isSelected ? 1.0 : 0.85))
                        .lineLimit(1)
                    HStack(spacing: 6) {
                        Text(scheme.isDark ? "Dark" : "Light")
                            .font(.system(size: 10, weight: .bold))
                            .foregroundStyle(.white.opacity(0.45))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(
                                Capsule().fill(.white.opacity(0.10)))
                        if isCurrent {
                            Text("CURRENT")
                                .font(.system(size: 10, weight: .bold))
                                .foregroundStyle(
                                    Color(red: 0.97, green: 0.56, blue: 0.57))
                        }
                    }
                }

                Spacer(minLength: 0)

                if isSelected {
                    Image(systemName: "return")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.white.opacity(0.55))
                }
            }
            .padding(10)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(
                        isSelected
                            ? AnyShapeStyle(Color.white.opacity(0.10))
                            : AnyShapeStyle(Color.white.opacity(0.0001)))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(
                        isSelected ? .white.opacity(0.25) : .white.opacity(0.0),
                        lineWidth: 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
        .onHover(perform: onHover)
    }

    private var preview: some View {
        VStack(alignment: .leading, spacing: 2) {
            // ~/rio main ❯ make canario
            line(
                ("~/rio", 4), (" main", 5), (" ❯", 2),
                (" make canario", nil))
            // assets  bin  src  README
            line(("assets", 4), ("  bin", 2), ("  src", 6), ("  README", nil))
            // error: no tty  +  a selection run
            HStack(spacing: 0) {
                text("error:", 1)
                text(" no tty ", nil)
                Text(" cp -r ")
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundColor(Color(scheme: scheme.selectionForeground))
                    .background(Color(scheme: scheme.selectionBackground))
            }

            // The scheme fingerprint: 16 slots, cursor block at the end.
            HStack(spacing: 1.5) {
                ForEach(0..<16, id: \.self) { slot in
                    RoundedRectangle(cornerRadius: 1)
                        .fill(Color(scheme: scheme.ansi[slot]))
                        .frame(width: 8, height: 6)
                }
                RoundedRectangle(cornerRadius: 1)
                    .fill(Color(scheme: scheme.cursor))
                    .frame(width: 5, height: 9)
                    .padding(.leading, 3)
            }
            .padding(.top, 3)
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 7)
                .fill(Color(scheme: scheme.background))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 7)
                .strokeBorder(.white.opacity(0.08), lineWidth: 1)
        )
    }

    /// One rendered line; each piece is (text, ANSI slot or nil for the
    /// default foreground).
    private func line(_ pieces: (String, Int?)...) -> some View {
        HStack(spacing: 0) {
            ForEach(Array(pieces.enumerated()), id: \.offset) { _, piece in
                text(piece.0, piece.1)
            }
        }
    }

    private func text(_ string: String, _ slot: Int?) -> Text {
        Text(string)
            .font(.system(size: 9, design: .monospaced))
            .foregroundColor(
                slot.map { Color(scheme: scheme.ansi[$0]) }
                    ?? Color(scheme: scheme.foreground))
    }
}

extension Color {
    /// Scheme hex ("#rrggbb") to SwiftUI color; used by the preview cards.
    init(scheme hex: String) {
        self = Color(hex: hex) ?? .black
    }
}
