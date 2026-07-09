import SwiftUI

struct SidebarView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ToolbarRowView()
                .padding(.bottom, 10)

            CurrentTerminalPill()
                .padding(.bottom, 12)

            FavoritesGridView()
                .padding(.bottom, 16)

            Text("Terminals")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(Theme.textMuted)
                .padding(.horizontal, 6)
                .padding(.bottom, 6)

            VStack(spacing: 3) {
                ForEach(model.terminals) { terminal in
                    TerminalRowView(terminal: terminal)
                }
                NewTerminalRowView()
            }

            Spacer(minLength: 8)

            BottomBarView()
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 10)
        .frame(width: 276)
    }
}

private struct ToolbarRowView: View {
    @Environment(AppModel.self) private var model

    @State private var isToggleHovered = false

    var body: some View {
        HStack {
            Spacer()
                .frame(width: 70)
            Button {
                withAnimation(.spring(duration: 0.28)) {
                    model.isSidebarCollapsed.toggle()
                }
            } label: {
                Image(systemName: "sidebar.left")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Theme.textPrimary.opacity(isToggleHovered ? 1.0 : 0.6))
                    .frame(width: 26, height: 26)
                    .background(
                        RoundedRectangle(cornerRadius: 7)
                            .fill(Color.black.opacity(isToggleHovered ? 0.10 : 0.0001))
                    )
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Toggle Sidebar")
            .onHover { isToggleHovered = $0 }
            Spacer()
        }
        .frame(height: 40)
    }
}

private struct CurrentTerminalPill: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "terminal")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(Theme.textMuted)
            Text(model.selectedTerminal?.name ?? "canario")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Theme.textPrimary.opacity(0.7))
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .frame(height: 38)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Theme.inset)
        )
    }
}

private struct FavoritesGridView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 8) {
            ForEach(0..<3, id: \.self) { slot in
                FavoriteTileView(
                    terminal: slot < model.terminals.count ? model.terminals[slot] : nil
                )
            }
        }
    }
}

private struct FavoriteTileView: View {
    @Environment(AppModel.self) private var model
    let terminal: TerminalItem?

    @State private var isHovered = false

    private var isSelected: Bool {
        terminal != nil && model.selectedTerminalID == terminal?.id
    }

    var body: some View {
        Button {
            if let terminal {
                model.selectedTerminalID = terminal.id
            }
        } label: {
            RoundedRectangle(cornerRadius: 10)
                .fill(
                    isSelected
                        ? AnyShapeStyle(Theme.selectedFill)
                        : AnyShapeStyle(
                            Color.black.opacity(
                                terminal == nil ? 0.04 : (isHovered ? 0.14 : 0.08)))
                )
                .frame(height: 46)
                .frame(maxWidth: .infinity)
                .overlay {
                    if terminal != nil {
                        Image(systemName: "terminal.fill")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(
                                isSelected ? Theme.textSelected : Theme.textPrimary.opacity(0.55))
                    }
                }
        }
        .buttonStyle(.plain)
        .disabled(terminal == nil)
        .accessibilityLabel(terminal?.name ?? "Empty favorite slot")
        .onHover { isHovered = terminal != nil && $0 }
    }
}

private struct TerminalRowView: View {
    @Environment(AppModel.self) private var model
    let terminal: TerminalItem

    @State private var isHovered = false
    @State private var isCloseHovered = false

    private var isSelected: Bool {
        model.selectedTerminalID == terminal.id
    }

    var body: some View {
        Button {
            model.selectedTerminalID = terminal.id
        } label: {
            HStack(spacing: 9) {
                Image(systemName: "terminal")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(
                        isSelected ? Theme.textSelected : Theme.textPrimary.opacity(0.6))

                Text(terminal.name)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(
                        isSelected ? Theme.textSelected : Theme.textPrimary.opacity(0.75))
                    .lineLimit(1)

                Spacer(minLength: 0)

                if isHovered || isSelected {
                    Button {
                        model.closeTerminal(terminal.id)
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 9, weight: .bold))
                            .foregroundStyle(
                                (isSelected ? Theme.textSelected : Theme.textPrimary)
                                    .opacity(isCloseHovered ? 0.95 : 0.55)
                            )
                            .frame(width: 19, height: 19)
                            .background(
                                RoundedRectangle(cornerRadius: 6)
                                    .fill(
                                        Color.black.opacity(isCloseHovered ? 0.12 : 0.0001))
                            )
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Close \(terminal.name)")
                    .onHover { isCloseHovered = $0 }
                }
            }
            .padding(.horizontal, 11)
            .frame(height: 40)
            .background {
                if isSelected {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(Theme.selectedFill)
                        .shadow(color: .black.opacity(0.10), radius: 3, y: 1)
                } else {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(Color.black.opacity(isHovered ? 0.08 : 0.0001))
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: 10))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }
}

private struct NewTerminalRowView: View {
    @Environment(AppModel.self) private var model

    @State private var isHovered = false

    var body: some View {
        Button {
            model.createTerminal()
        } label: {
            HStack(spacing: 9) {
                Image(systemName: "plus")
                    .font(.system(size: 11, weight: .semibold))
                Text("New Terminal")
                    .font(.system(size: 13, weight: .medium))
                Spacer(minLength: 0)
                Text("⌘T")
                    .font(.system(size: 11, weight: .medium))
                    .opacity(isHovered ? 0.7 : 0.0)
            }
            .foregroundStyle(Theme.textPrimary.opacity(isHovered ? 0.85 : 0.5))
            .padding(.horizontal, 11)
            .frame(height: 40)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(Color.black.opacity(isHovered ? 0.08 : 0.0001))
            )
            .contentShape(RoundedRectangle(cornerRadius: 10))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }
}

private struct BottomBarView: View {
    @Environment(AppModel.self) private var model

    @State private var isPlusHovered = false

    var body: some View {
        HStack {
            Image(systemName: "bird.fill")
                .font(.system(size: 13))
                .foregroundStyle(Theme.textPrimary.opacity(0.55))

            Spacer()

            Circle()
                .fill(Theme.textPrimary.opacity(0.8))
                .frame(width: 6, height: 6)

            Spacer()

            Button {
                model.createTerminal()
            } label: {
                Image(systemName: "plus")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Theme.textPrimary.opacity(isPlusHovered ? 0.95 : 0.55))
                    .frame(width: 26, height: 26)
                    .background(
                        RoundedRectangle(cornerRadius: 7)
                            .fill(Color.black.opacity(isPlusHovered ? 0.10 : 0.0001))
                    )
            }
            .buttonStyle(.plain)
            .accessibilityLabel("New Terminal")
            .onHover { isPlusHovered = $0 }
        }
        .padding(.horizontal, 6)
    }
}
