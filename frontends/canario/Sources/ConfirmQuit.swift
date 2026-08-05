import AppKit
import SwiftUI

// Arc-style quit confirmation: ⌘Q raises a dark floating dialog instead of
// killing every shell instantly. "Quit, and don't ask again" persists the
// choice (UserDefaults `quitWithoutConfirming`); ESC cancels, ↵ quits.

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var quitPanel: NSPanel?

    func applicationShouldTerminate(
        _ sender: NSApplication
    ) -> NSApplication.TerminateReply {
        if UserDefaults.standard.bool(forKey: "quitWithoutConfirming") {
            return .terminateNow
        }
        showQuitPanel()
        return .terminateLater
    }

    private func finish(quit: Bool, dontAskAgain: Bool = false) {
        if dontAskAgain {
            UserDefaults.standard.set(true, forKey: "quitWithoutConfirming")
        }
        quitPanel?.orderOut(nil)
        quitPanel = nil
        NSApp.reply(toApplicationShouldTerminate: quit)
    }

    private func showQuitPanel() {
        if let quitPanel {
            quitPanel.makeKeyAndOrderFront(nil)
            return
        }
        let view = ConfirmQuitView(
            quit: { [weak self] in self?.finish(quit: true) },
            quitAndDontAsk: { [weak self] in
                self?.finish(quit: true, dontAskAgain: true)
            },
            cancel: { [weak self] in self?.finish(quit: false) })
        let panel = KeyableDialogPanel(
            contentRect: NSRect(x: 0, y: 0, width: 500, height: 260),
            styleMask: [.borderless, .fullSizeContentView],
            backing: .buffered, defer: false)
        panel.level = .modalPanel
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        panel.contentView = NSHostingView(rootView: view)
        panel.center()
        NSApp.activate(ignoringOtherApps: true)
        panel.makeKeyAndOrderFront(nil)
        DispatchQueue.main.async { panel.invalidateShadow() }
        quitPanel = panel
    }
}

private final class KeyableDialogPanel: NSPanel {
    override var canBecomeKey: Bool { true }
}

private struct ConfirmQuitView: View {
    let quit: () -> Void
    let quitAndDontAsk: () -> Void
    let cancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 64, height: 64)

            Text("Quit Canario?")
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(.white)
                .padding(.top, 18)

            Spacer(minLength: 24)

            HStack(spacing: 10) {
                DialogButton(fill: .white.opacity(0.07), action: quitAndDontAsk) {
                    Text("Quit, and don't ask again")
                }

                Spacer(minLength: 0)

                DialogButton(fill: .white.opacity(0.07), action: cancel) {
                    HStack(spacing: 8) {
                        Text("Cancel")
                        KeycapLabel(text: "ESC")
                    }
                }
                .keyboardShortcut(.cancelAction)

                DialogButton(
                    fill: Color(red: 0.94, green: 0.42, blue: 0.46),
                    action: quit
                ) {
                    HStack(spacing: 8) {
                        Text("Quit")
                        KeycapLabel(systemName: "return")
                    }
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(28)
        .frame(width: 500, height: 260, alignment: .topLeading)
        .background(
            RoundedRectangle(cornerRadius: 22)
                .fill(Color(red: 0.13, green: 0.11, blue: 0.12))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 22)
                .strokeBorder(.white.opacity(0.08), lineWidth: 1)
        )
        .environment(\.colorScheme, .dark)
    }
}

private struct DialogButton<Label: View>: View {
    let fill: Color
    let action: () -> Void
    @ViewBuilder let label: () -> Label

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            label()
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 16)
                .frame(height: 44)
                .background(
                    RoundedRectangle(cornerRadius: 11)
                        .fill(fill)
                        .brightness(isHovered ? 0.06 : 0)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 11)
                        .strokeBorder(.white.opacity(0.10), lineWidth: 1)
                )
                .contentShape(RoundedRectangle(cornerRadius: 11))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }
}

private struct KeycapLabel: View {
    var text: String?
    var systemName: String?

    var body: some View {
        Group {
            if let text {
                Text(text)
                    .font(.system(size: 11, weight: .bold))
            } else if let systemName {
                Image(systemName: systemName)
                    .font(.system(size: 10, weight: .bold))
            }
        }
        .foregroundStyle(.white.opacity(0.8))
        .padding(.horizontal, 6)
        .frame(height: 20)
        .background(
            RoundedRectangle(cornerRadius: 5)
                .fill(.white.opacity(0.14))
        )
    }
}
