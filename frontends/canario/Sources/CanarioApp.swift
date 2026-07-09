import AppKit
import SwiftUI

@main
struct CanarioApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        Window("Canario", id: "main") {
            ContentView()
                .environment(model)
                .frame(minWidth: 640, minHeight: 400)
        }
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(after: .newItem) {
                Button("New Terminal") { model.createTerminal() }
                    .keyboardShortcut("t", modifiers: .command)
                Button("New Folder") { model.createFolder() }
                    .keyboardShortcut("n", modifiers: [.command, .shift])
                Divider()
                Button("Split Right") {
                    withAnimation(.spring(duration: 0.25)) {
                        model.splitSelected(.right)
                    }
                }
                .keyboardShortcut("d", modifiers: .command)
                .disabled(model.selectedTerminalID == nil)
                Button("Split Down") {
                    withAnimation(.spring(duration: 0.25)) {
                        model.splitSelected(.down)
                    }
                }
                .keyboardShortcut("d", modifiers: [.command, .shift])
                .disabled(model.selectedTerminalID == nil)
                Button(model.selectedTerminalID == nil ? "Close Window" : "Close Terminal") {
                    if model.selectedTerminalID != nil {
                        model.closeSelectedTerminal()
                    } else {
                        NSApp.keyWindow?.performClose(nil)
                    }
                }
                .keyboardShortcut("w", modifiers: .command)
            }
            CommandGroup(after: .sidebar) {
                Button(model.isSidebarCollapsed ? "Show Sidebar" : "Hide Sidebar") {
                    withAnimation(.spring(duration: 0.28)) {
                        model.isSidebarCollapsed.toggle()
                    }
                }
                .keyboardShortcut("s", modifiers: .command)
            }
        }
    }
}
