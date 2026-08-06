import AppKit
import SwiftUI

// canario:// deep links, the Raycast pattern: other apps, launchers and
// docs can open terminals, jump to sessions and run commands.
//
//   canario://quick                          toggle the quick terminal
//   canario://new?space=Work&cwd=~/api       new terminal (space + cwd optional)
//   canario://terminal?title=htop            focus a terminal by fuzzy title
//   canario://run?cmd=npm+test&space=Work    new terminal + run (confirmed first)
//
// `run` always shows a confirmation with the exact command: URLs arrive
// from browsers and other apps, and a shell one-liner should never
// execute on a click alone.
//
// That confirmation is only worth something if the command cannot read
// differently than it runs, so any parameter carrying a Unicode control
// or format character (category Cc or Cf) is dropped, and a link whose
// `cmd` carries one never opens. Two ways they lie: the shell's line
// editor acts on C0 bytes rather than printing them, so `echo Foo^Uls`
// shows as `echo Fools` in an alert and runs `ls` (^U kills the line,
// ^P recalls history); and the bidi overrides reorder the rendered line,
// so what is read is not the order that reaches the shell. Both are
// invisible in an NSAlert. Reported as #1824.

enum DeepLink {
    case quick
    case newTerminal(space: String?, cwd: String?)
    case focusTerminal(query: String)
    case run(command: String, space: String?)

    static func parse(_ url: URL) -> DeepLink? {
        guard url.scheme == "canario" else { return nil }
        let action = url.host ?? url.path.trimmingCharacters(
            in: CharacterSet(charactersIn: "/"))
        let components = URLComponents(url: url, resolvingAgainstBaseURL: false)
        func query(_ name: String) -> String? {
            // Form-encoding convention: `+` is a space. URLComponents
            // only undoes percent-escapes, so launchers writing
            // cmd=npm+test get what they meant.
            guard
                let value = components?.queryItems?.first(where: {
                    $0.name == name
                })?.value?.replacingOccurrences(of: "+", with: " ")
            else { return nil }
            return value.rangeOfCharacter(from: .controlCharacters) == nil
                ? value : nil
        }

        switch action {
        case "quick":
            return .quick
        case "new":
            return .newTerminal(space: query("space"), cwd: query("cwd"))
        case "terminal":
            guard let title = query("title"), !title.isEmpty else { return nil }
            return .focusTerminal(query: title)
        case "run":
            guard let command = query("cmd"), !command.isEmpty else {
                return nil
            }
            return .run(command: command, space: query("space"))
        default:
            return nil
        }
    }

    @MainActor
    static func handle(_ url: URL, model: AppModel) {
        guard let link = parse(url) else { return }
        switch link {
        case .quick:
            model.quickTerminal.toggle()
        case .newTerminal(let space, let cwd):
            NSApp.activate(ignoringOtherApps: true)
            openTerminal(model: model, space: space, cwd: cwd, command: nil)
        case .focusTerminal(let query):
            NSApp.activate(ignoringOtherApps: true)
            let needle = query.lowercased()
            if let terminal = model.flattenedTerminals.first(where: {
                $0.displayTitle.lowercased().contains(needle)
                    || $0.name.lowercased().contains(needle)
            }) {
                model.selectedTerminalID = terminal.id
            }
        case .run(let command, let space):
            NSApp.activate(ignoringOtherApps: true)
            confirmRun(command: command) {
                openTerminal(
                    model: model, space: space, cwd: nil, command: command)
            }
        }
    }

    /// New terminal, optionally filed into the named space and seeded
    /// with a working directory and a command to type once the shell is
    /// up.
    @MainActor
    private static func openTerminal(
        model: AppModel, space: String?, cwd: String?, command: String?
    ) {
        let folder = space.flatMap { name -> Folder? in
            let needle = name.lowercased()
            for item in model.items {
                if case .folder(let folder) = item,
                    folder.name.lowercased() == needle
                {
                    return folder
                }
            }
            return nil
        }
        model.createTerminal(in: folder)
        guard let terminal = model.selectedTerminal else { return }
        if let cwd {
            let expanded = NSString(string: cwd).expandingTildeInPath
            terminal.panelWorkingDirs[terminal.focusedPanelID] = expanded
        }
        if let command {
            terminal.pendingCommand = command
        }
    }

    /// A command from a URL runs only after the user reads it. Plain
    /// NSAlert: this is a security prompt, not chrome.
    @MainActor
    private static func confirmRun(
        command: String, proceed: @escaping () -> Void
    ) {
        let alert = NSAlert()
        alert.messageText = "Run command from link?"
        alert.informativeText =
            "A link asked to run this command. It goes to a new terminal "
            + "exactly as shown."
        alert.alertStyle = .warning
        alert.accessoryView = commandView(command)
        alert.addButton(withTitle: "Run")
        alert.addButton(withTitle: "Cancel")
        // Return activates nothing: a security prompt must not execute on
        // a keystroke aimed at the window behind it. Cancel keeps the
        // Escape equivalent NSAlert gives it, so assigning Return here
        // instead would cost the dismissal every dialog is expected to have.
        alert.buttons[0].keyEquivalent = ""
        if alert.runModal() == .alertFirstButtonReturn {
            proceed()
        }
    }

    /// The command in a bounded monospaced box, so a long one scrolls
    /// instead of pushing its tail out of a dialog the user thinks they
    /// have read to the end.
    @MainActor
    private static func commandView(_ command: String) -> NSView {
        let frame = NSRect(x: 0, y: 0, width: 380, height: 68)
        let text = NSTextView(frame: frame)
        text.string = command
        text.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        text.isEditable = false
        text.textContainerInset = NSSize(width: 5, height: 5)
        text.isVerticallyResizable = true
        text.isHorizontallyResizable = false
        text.autoresizingMask = [.width]
        text.textContainer?.widthTracksTextView = true

        let scroll = NSScrollView(frame: frame)
        scroll.documentView = text
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .bezelBorder
        return scroll
    }
}
