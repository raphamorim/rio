import AppKit
import SwiftUI

@main
struct CanarioApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var model = AppModel()
    @State private var updater = Updater()

    init() {
        // Apps launched from Finder start with cwd `/`; shells spawned without
        // an explicit working_dir inherit it, so move to home before any spawn.
        FileManager.default.changeCurrentDirectoryPath(
            FileManager.default.homeDirectoryForCurrentUser.path)
        // Point terminfo lookups at the bundled database (ghostty does the
        // same); librio then resolves TERM=xterm-rio against it at spawn,
        // with no system-wide install needed. Lookups for other TERMs fall
        // through to the system paths as usual.
        if let terminfo = Bundle.main.resourceURL?
            .appendingPathComponent("terminfo").path,
            FileManager.default.fileExists(atPath: terminfo)
        {
            setenv("TERMINFO", terminfo, 1)
        }
        // Identify ourselves to shells (TERM itself is resolved by librio
        // against the installed terminfo at spawn time).
        setenv("TERM_PROGRAM", "canario", 1)
        if let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"]
            as? String, !version.hasPrefix("{{")
        {
            setenv("TERM_PROGRAM_VERSION", version, 1)
        }
    }

    var body: some Scene {
        Window("Canario", id: "main") {
            ContentView()
                .environment(model)
                .environment(updater)
                .frame(minWidth: 640, minHeight: 400)
                .task {
                    // Give launch (and session restore) a beat before
                    // touching the network.
                    try? await Task.sleep(for: .seconds(5))
                    updater.checkAutomatically()
                }
        }
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(after: .appInfo) {
                Button("Check for Updates…") { updater.checkNow() }
            }
            CommandGroup(after: .newItem) {
                Button("New Terminal") { model.createTerminal() }
                    .keyboardShortcut("t", modifiers: .command)
                Button("New Folder") { model.createFolder() }
                    .keyboardShortcut("n", modifiers: [.command, .shift])
                Button("Quick Terminal") { model.quickTerminal.toggle() }
                    .keyboardShortcut("t", modifiers: [.command, .option])
                Divider()
                Button("Command Bar…") {
                    withAnimation(.easeOut(duration: 0.15)) {
                        model.isCommandBarVisible.toggle()
                    }
                }
                .keyboardShortcut("k", modifiers: .command)
                Divider()
                Button("Split Right") {
                    withAnimation(.spring(duration: 0.25)) {
                        model.splitRightInSelected()
                    }
                }
                .keyboardShortcut("d", modifiers: .command)
                .disabled(model.selectedTerminalID == nil)
                Button("Split Down") {
                    withAnimation(.spring(duration: 0.25)) {
                        model.splitDownInSelected()
                    }
                }
                .keyboardShortcut("d", modifiers: [.command, .shift])
                .disabled(model.selectedTerminalID == nil)
                Divider()
                Button("Copy") { model.copySelection() }
                    .keyboardShortcut("c", modifiers: .command)
                    .disabled(model.selectedTerminalID == nil)
                Button("Paste") { model.pasteClipboard() }
                    .keyboardShortcut("v", modifiers: .command)
                    .disabled(model.selectedTerminalID == nil)
                Divider()
                Button("Increase Font Size") { model.adjustFontSize(by: 1) }
                    .keyboardShortcut("=", modifiers: .command)
                Button("Decrease Font Size") { model.adjustFontSize(by: -1) }
                    .keyboardShortcut("-", modifiers: .command)
                Button("Reset Font Size") { model.resetFontSize() }
                    .keyboardShortcut("0", modifiers: .command)
                Divider()
                // ⌘W closes the smallest thing in focus: the panel, which
                // itself falls back to closing the terminal when it's the
                // last one. ⇧⌘W takes the whole terminal.
                Button(model.selectedTerminalID == nil ? "Close Window" : "Close Panel") {
                    if model.selectedTerminalID != nil {
                        withAnimation(.spring(duration: 0.25)) {
                            model.closeFocusedPanel()
                        }
                    } else {
                        NSApp.keyWindow?.performClose(nil)
                    }
                }
                .keyboardShortcut("w", modifiers: .command)
                Button("Close Terminal") {
                    model.closeSelectedTerminal()
                }
                .keyboardShortcut("w", modifiers: [.command, .shift])
                .disabled(model.selectedTerminalID == nil)
            }
            CommandGroup(after: .sidebar) {
                Button(model.isSidebarCollapsed ? "Show Sidebar" : "Hide Sidebar") {
                    withAnimation(.spring(duration: 0.28)) {
                        model.isSidebarCollapsed.toggle()
                    }
                }
                .keyboardShortcut("s", modifiers: .command)
            }
            CommandMenu("Go") {
                ForEach(1..<10, id: \.self) { number in
                    Button("Space \(number)") {
                        withAnimation(.spring(duration: 0.25)) {
                            model.selectRootItem(at: number - 1)
                        }
                    }
                    .keyboardShortcut(
                        KeyEquivalent(Character("\(number)")), modifiers: .command)
                }
            }
        }

        MenuBarExtra("Canario", systemImage: "terminal.fill") {
            Button("Quick Terminal  ⌥⌘T") { model.quickTerminal.toggle() }
            Button("New Terminal") {
                model.createTerminal()
                NSApp.activate(ignoringOtherApps: true)
            }
            Divider()
            Button("Show Canario") { NSApp.activate(ignoringOtherApps: true) }
        }
    }
}
