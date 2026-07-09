import SwiftUI

struct SidebarView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Spacer()
                .frame(height: 52)

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
