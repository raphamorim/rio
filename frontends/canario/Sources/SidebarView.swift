import SwiftUI
import UniformTypeIdentifiers

struct SidebarView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Spacer()
                .frame(height: 52)

            ScrollView(.vertical, showsIndicators: false) {
                VStack(spacing: 3) {
                    ForEach(model.items) { item in
                        SidebarItemView(item: item, depth: 0)
                    }
                    NewTerminalRowView()
                }
            }

            Spacer(minLength: 8)

            BottomBarView()
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 10)
        .frame(width: 276)
        .onDrop(
            of: [.text],
            delegate: ReorderDropDelegate(
                onEntered: {
                    withAnimation(.spring(duration: 0.25)) {
                        model.moveDraggedToRootEnd()
                    }
                },
                onPerform: { model.draggingTerminalID = nil }))
    }
}

private struct SidebarItemView: View {
    let item: SidebarItem
    let depth: Int

    var body: some View {
        switch item {
        case .terminal(let terminal):
            TerminalGroupView(terminal: terminal, depth: depth)
        case .folder(let folder):
            FolderGroupView(folder: folder, depth: depth)
        }
    }
}

private struct TerminalGroupView: View {
    let terminal: TerminalItem
    let depth: Int

    var body: some View {
        VStack(spacing: 3) {
            TerminalRowView(terminal: terminal)
                .padding(.leading, CGFloat(depth) * 18)
            if terminal.panelCount > 1 && terminal.isExpanded {
                ForEach(Array(terminal.panels.enumerated()), id: \.element.id) {
                    index, panel in
                    PanelRowView(terminal: terminal, panel: panel, index: index)
                        .padding(.leading, CGFloat(depth + 1) * 18)
                }
            }
        }
    }
}

private struct PanelRowView: View {
    @Environment(AppModel.self) private var model
    let terminal: TerminalItem
    let panel: Panel
    let index: Int

    @State private var isHovered = false
    @State private var isCloseHovered = false

    private var isActive: Bool {
        model.selectedTerminalID == terminal.id && terminal.focusedPanelID == panel.id
    }

    var body: some View {
        Button {
            model.selectedTerminalID = terminal.id
            terminal.focusedPanelID = panel.id
        } label: {
            HStack(spacing: 8) {
                Image(systemName: "rectangle.on.rectangle")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(
                        isActive ? Theme.textSelected : Theme.textPrimary.opacity(0.5))

                Text(terminal.panelTitles[panel.id] ?? "Panel \(index + 1)")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(
                        isActive ? Theme.textSelected : Theme.textPrimary.opacity(0.65))
                    .lineLimit(1)

                Spacer(minLength: 0)

                if isHovered {
                    Button {
                        model.closePanel(panel.id, in: terminal)
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 8, weight: .bold))
                            .foregroundStyle(
                                (isActive ? Theme.textSelected : Theme.textPrimary)
                                    .opacity(isCloseHovered ? 0.95 : 0.5)
                            )
                            .frame(width: 17, height: 17)
                            .background(
                                RoundedRectangle(cornerRadius: 5)
                                    .fill(
                                        Color.black.opacity(isCloseHovered ? 0.12 : 0.0001))
                            )
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Close Panel \(index + 1)")
                    .onHover { isCloseHovered = $0 }
                }
            }
            .padding(.horizontal, 10)
            .frame(height: 32)
            .background {
                if isActive {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Theme.selectedFill.opacity(0.75))
                } else {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.black.opacity(isHovered ? 0.07 : 0.0001))
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }
}

private struct FolderGroupView: View {
    let folder: Folder
    let depth: Int

    var body: some View {
        VStack(spacing: 3) {
            FolderRowView(folder: folder)
                .padding(.leading, CGFloat(depth) * 18)
            if folder.isExpanded {
                ForEach(folder.children) { child in
                    SidebarItemView(item: child, depth: depth + 1)
                }
            }
        }
    }
}

private struct FolderRowView: View {
    @Environment(AppModel.self) private var model
    @Bindable var folder: Folder

    @State private var isHovered = false
    @State private var isRenaming = false
    @State private var isDropTargeted = false
    @FocusState private var nameFieldFocused: Bool

    var body: some View {
        Button {
            withAnimation(.spring(duration: 0.25)) {
                folder.isExpanded.toggle()
            }
        } label: {
            HStack(spacing: 9) {
                Image(systemName: folder.isExpanded ? "folder" : "folder.fill")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(Theme.textPrimary.opacity(0.6))

                if isRenaming {
                    TextField("New Folder", text: $folder.name)
                        .textFieldStyle(.plain)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Theme.textPrimary)
                        .focused($nameFieldFocused)
                        .onSubmit { endRenaming() }
                        .onExitCommand { endRenaming() }
                } else {
                    Text(folder.name)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Theme.textPrimary.opacity(0.8))
                        .lineLimit(1)
                }

                Spacer(minLength: 0)

                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Theme.textPrimary.opacity(0.4))
                    .rotationEffect(.degrees(folder.isExpanded ? 90 : 0))
                    .opacity(isHovered ? 1.0 : 0.0)
            }
            .padding(.horizontal, 11)
            .frame(height: 40)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(
                        Color.black.opacity(
                            isDropTargeted ? 0.14 : (isHovered ? 0.08 : 0.0001)))
            )
            .contentShape(RoundedRectangle(cornerRadius: 10))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Folder \(folder.name)")
        .onAppear {
            if model.pendingRenameFolderID == folder.id {
                model.pendingRenameFolderID = nil
                isRenaming = true
                DispatchQueue.main.async {
                    nameFieldFocused = true
                }
            }
        }
        .onChange(of: nameFieldFocused) { _, focused in
            if !focused && isRenaming {
                endRenaming()
            }
        }
        .onHover { isHovered = $0 }
        .onDrop(
            of: [.text],
            delegate: ReorderDropDelegate(
                onEntered: {
                    isDropTargeted = true
                    withAnimation(.spring(duration: 0.25)) {
                        model.moveDraggedIntoFolder(folder)
                    }
                },
                onPerform: {
                    isDropTargeted = false
                    model.draggingTerminalID = nil
                },
                onExited: { isDropTargeted = false }))
        .contextMenu {
            Button("New Terminal in \(folder.name)") {
                model.createTerminal(in: folder)
            }
            Button("Rename") {
                isRenaming = true
                nameFieldFocused = true
            }
            Divider()
            Button("Delete Folder", role: .destructive) {
                model.deleteFolder(folder)
            }
        }
    }

    private func endRenaming() {
        guard isRenaming else { return }
        model.commitFolderName(folder)
        isRenaming = false
        nameFieldFocused = false
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

                if terminal.panelCount > 1 {
                    Button {
                        withAnimation(.spring(duration: 0.25)) {
                            terminal.isExpanded.toggle()
                        }
                    } label: {
                        ZStack {
                            Circle()
                                .fill(Color.black.opacity(terminal.isExpanded ? 0.20 : 0.12))
                            Text("\(terminal.panelCount)")
                                .font(.system(size: 10, weight: .semibold))
                                .foregroundStyle(
                                    isSelected
                                        ? Theme.textSelected
                                        : Theme.textPrimary.opacity(0.7))
                        }
                        .frame(width: 18, height: 18)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Show Panels")
                }

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
        .onDrag {
            model.draggingTerminalID = terminal.id
            return NSItemProvider(object: terminal.id.uuidString as NSString)
        } preview: {
            Color.black.opacity(0.001)
                .frame(width: 1, height: 1)
        }
        .onDrop(
            of: [.text],
            delegate: ReorderDropDelegate(
                onEntered: {
                    withAnimation(.spring(duration: 0.25)) {
                        model.moveDraggedTerminal(before: terminal.id)
                    }
                },
                onPerform: { model.draggingTerminalID = nil }))
        .onHover { isHovered = $0 }
    }
}

private struct ReorderDropDelegate: DropDelegate {
    let onEntered: () -> Void
    let onPerform: () -> Void
    var onExited: () -> Void = {}

    func dropEntered(info: DropInfo) {
        onEntered()
    }

    func dropExited(info: DropInfo) {
        onExited()
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        onPerform()
        return true
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

    var body: some View {
        HStack(spacing: 4) {
            Spacer()

            BottomBarButton(systemName: "folder.badge.plus", label: "New Folder") {
                model.createFolder()
            }

            BottomBarButton(systemName: "plus", label: "New Terminal") {
                model.createTerminal()
            }
        }
        .padding(.horizontal, 6)
    }
}

private struct BottomBarButton: View {
    let systemName: String
    let label: String
    let action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Theme.textPrimary.opacity(isHovered ? 0.95 : 0.55))
                .frame(width: 26, height: 26)
                .background(
                    RoundedRectangle(cornerRadius: 7)
                        .fill(Color.black.opacity(isHovered ? 0.10 : 0.0001))
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .onHover { isHovered = $0 }
    }
}
