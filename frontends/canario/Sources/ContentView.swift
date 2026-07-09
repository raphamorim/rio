import SwiftUI

struct ContentView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                if !model.isSidebarCollapsed {
                    SidebarView()
                        .transition(.move(edge: .leading).combined(with: .opacity))
                }

                ZStack {
                    if let terminal = model.selectedTerminal {
                        TerminalSurface(hostView: model.surfaces.view(for: terminal.id))
                            .id(terminal.id)
                            .clipShape(RoundedRectangle(cornerRadius: Theme.cardRadius))
                            .shadow(color: .black.opacity(0.22), radius: 10, y: 3)
                    } else {
                        EmptyStateView()
                    }
                }
                .padding(
                    EdgeInsets(
                        top: 10,
                        leading: model.isSidebarCollapsed ? 10 : 2,
                        bottom: 10,
                        trailing: 10))
            }

            SidebarToggleButton()
                .padding(.leading, 76)
                .padding(.top, 4)
        }
        .ignoresSafeArea(.container, edges: .top)
        .background(
            ChromeBackground()
                .overlay(GrainOverlay())
                .ignoresSafeArea()
        )
    }
}

struct SidebarToggleButton: View {
    @Environment(AppModel.self) private var model

    @State private var isHovered = false

    var body: some View {
        Button {
            withAnimation(.spring(duration: 0.28)) {
                model.isSidebarCollapsed.toggle()
            }
        } label: {
            Image(systemName: "sidebar.left")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(
                    model.isSidebarCollapsed
                        ? Color.white.opacity(isHovered ? 1.0 : 0.65)
                        : Theme.textPrimary.opacity(isHovered ? 1.0 : 0.6)
                )
                .frame(width: 26, height: 26)
                .background(
                    RoundedRectangle(cornerRadius: 7)
                        .fill(
                            model.isSidebarCollapsed
                                ? Color.white.opacity(isHovered ? 0.16 : 0.0001)
                                : Color.black.opacity(isHovered ? 0.10 : 0.0001))
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Toggle Sidebar")
        .onHover { isHovered = $0 }
    }
}

private struct EmptyStateView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "terminal")
                .font(.system(size: 34, weight: .light))
                .opacity(0.5)
            Text("No terminal open")
                .font(.system(size: 14, weight: .medium))
                .opacity(0.6)
            Button {
                model.createTerminal()
            } label: {
                Text("New Terminal  ⌘T")
                    .font(.system(size: 12, weight: .medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 7)
                    .background(Capsule().fill(.white.opacity(0.16)))
            }
            .buttonStyle(.plain)
        }
        .foregroundStyle(.white)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            RoundedRectangle(cornerRadius: Theme.cardRadius)
                .fill(.black.opacity(0.82))
        )
        .shadow(color: .black.opacity(0.22), radius: 10, y: 3)
    }
}
