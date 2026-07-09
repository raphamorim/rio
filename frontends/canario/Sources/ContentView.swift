import SwiftUI

struct ContentView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
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
            .padding(EdgeInsets(top: 10, leading: 2, bottom: 10, trailing: 10))
            .ignoresSafeArea(.container, edges: .top)
        }
        .background(ChromeBackground().ignoresSafeArea())
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
