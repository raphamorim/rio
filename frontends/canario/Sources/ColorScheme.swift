import AppKit
import RioKit

/// A terminal color scheme: the 16 ANSI slots + default foreground /
/// background the engine resolves cells against, plus the overlay colors
/// (cursor, selection) the CPU renderer draws itself. Values are "#rrggbb".
///
/// Prebuilt values are vendored from mbadolato/iTerm2-Color-Schemes' `rio/`
/// export (the upstream Ghostty and kitty galleries bundle from the same
/// source), so they match what every other terminal ships.
struct ColorScheme: Identifiable, Equatable {
    let name: String
    let isDark: Bool
    /// ANSI 0-7 normal, 8-15 bright.
    let ansi: [String]
    let foreground: String
    let background: String
    let cursor: String
    let selectionBackground: String
    let selectionForeground: String

    var id: String { name }

    var backgroundColor: NSColor { NSColor(scheme: background) }
    var foregroundColor: NSColor { NSColor(scheme: foreground) }
    var cursorColor: NSColor { NSColor(scheme: cursor) }
    var selectionBackgroundColor: NSColor { NSColor(scheme: selectionBackground) }
    var selectionForegroundColor: NSColor { NSColor(scheme: selectionForeground) }
    func ansiColor(_ slot: Int) -> NSColor { NSColor(scheme: ansi[slot]) }

    /// Push this scheme into librio: every named/indexed cell color
    /// re-resolves against it on the next draw. The caller owns repainting
    /// (`AppModel.applyColorScheme` walks the live sessions).
    func applyToEngine() {
        var colors = rio_colors_s()
        withUnsafeMutableBytes(of: &colors.ansi) { raw in
            let slots = raw.bindMemory(to: rio_rgb_s.self)
            for i in 0..<16 { slots[i] = Self.rgb(ansi[i]) }
        }
        colors.foreground = Self.rgb(foreground)
        colors.background = Self.rgb(background)
        colors.cursor = Self.rgb(cursor)
        rio_set_colors(&colors)
    }

    private static func rgb(_ hex: String) -> rio_rgb_s {
        let v = Self.parse(hex)
        return rio_rgb_s(
            r: UInt8((v >> 16) & 0xFF),
            g: UInt8((v >> 8) & 0xFF),
            b: UInt8(v & 0xFF))
    }

    fileprivate static func parse(_ hex: String) -> UInt32 {
        var value = hex.trimmingCharacters(in: .whitespaces)
        if value.hasPrefix("#") { value.removeFirst() }
        return UInt32(value, radix: 16) ?? 0
    }
}

extension NSColor {
    /// Scheme hex to sRGB color; black for malformed values, which prebuilt
    /// data never contains.
    convenience init(scheme hex: String) {
        let v = ColorScheme.parse(hex)
        self.init(
            srgbRed: CGFloat((v >> 16) & 0xFF) / 255,
            green: CGFloat((v >> 8) & 0xFF) / 255,
            blue: CGFloat(v & 0xFF) / 255,
            alpha: 1)
    }
}

extension ColorScheme {
    /// Rio's own default theme (rio-vt config/colors/defaults.rs). First in
    /// the gallery and what shipping builds render until the user picks
    /// something else.
    static let rio = ColorScheme(
        name: "Rio", isDark: true,
        ansi: [
            "#393a3d", "#ff1261", "#2ad947", "#fcba28", "#2d9aff", "#dd30ff", "#17d5df", "#e7e7e7",
            "#6b6b6b", "#c55555", "#aac474", "#feca88", "#82b8c8", "#c28cb8", "#93d3c3", "#f8f8f8",
        ],
        foreground: "#ffffff", background: "#0f0d0e",
        cursor: "#f712ff",
        selectionBackground: "#1c191a",
        selectionForeground: "#44c9f0")

    static let prebuilt: [ColorScheme] = [
        .rio,
        // raphamorim/lucario (rio/lucario.toml + the README's selection color).
        ColorScheme(
            name: "Lucario", isDark: true,
            ansi: [
                "#19242f", "#e94b35", "#199c4b", "#f0cc04", "#5c98cd", "#ca94ff", "#8be0fd", "#f8f8f2",
                "#2f3943", "#ff6541", "#72cc5a", "#ffffa5", "#d6acff", "#d4a9ff", "#b9ecfd", "#ffffff",
            ],
            foreground: "#f8f8f2", background: "#2b3e50",
            cursor: "#f0cc04",
            selectionBackground: "#19242f",
            selectionForeground: "#f8f8f2"),
        ColorScheme(
            name: "Dracula", isDark: true,
            ansi: [
                "#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2",
                "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff",
            ],
            foreground: "#f8f8f2", background: "#282a36",
            cursor: "#f8f8f2",
            selectionBackground: "#44475a",
            selectionForeground: "#ffffff"),
        ColorScheme(
            name: "Catppuccin Mocha", isDark: true,
            ansi: [
                "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de",
                "#585b70", "#f7aec2", "#c2ecbf", "#fcd682", "#aeccfc", "#f398da", "#b1eae1", "#a6adc8",
            ],
            foreground: "#cdd6f4", background: "#1e1e2e",
            cursor: "#f5e0dc",
            selectionBackground: "#f5e0dc",
            selectionForeground: "#1e1e2e"),
        ColorScheme(
            name: "Catppuccin Latte", isDark: false,
            ansi: [
                "#bcc0cc", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#5c5f77",
                "#acb0be", "#e7103f", "#46b02f", "#e49931", "#3878f6", "#ef95d7", "#19a1a8", "#6c6f85",
            ],
            foreground: "#4c4f69", background: "#eff1f5",
            cursor: "#dc8a78",
            selectionBackground: "#dc8a78",
            selectionForeground: "#eff1f5"),
        ColorScheme(
            name: "Nord", isDark: true,
            ansi: [
                "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
                "#596377", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4",
            ],
            foreground: "#d8dee9", background: "#2e3440",
            cursor: "#eceff4",
            selectionBackground: "#eceff4",
            selectionForeground: "#4c566a"),
        ColorScheme(
            name: "Solarized Dark", isDark: true,
            ansi: [
                "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
                "#335e69", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
            ],
            foreground: "#839496", background: "#002b36",
            cursor: "#839496",
            selectionBackground: "#073642",
            selectionForeground: "#93a1a1"),
        ColorScheme(
            name: "Solarized Light", isDark: false,
            ansi: [
                "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#bbb5a2",
                "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
            ],
            foreground: "#657b83", background: "#fdf6e3",
            cursor: "#657b83",
            selectionBackground: "#eee8d5",
            selectionForeground: "#586e75"),
        ColorScheme(
            name: "Gruvbox Dark", isDark: true,
            ansi: [
                "#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984",
                "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2",
            ],
            foreground: "#ebdbb2", background: "#282828",
            cursor: "#ebdbb2",
            selectionBackground: "#665c54",
            selectionForeground: "#ebdbb2"),
        ColorScheme(
            name: "Tokyo Night", isDark: true,
            ansi: [
                "#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6",
                "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5",
            ],
            foreground: "#c0caf5", background: "#1a1b26",
            cursor: "#c0caf5",
            selectionBackground: "#283457",
            selectionForeground: "#c0caf5"),
        ColorScheme(
            name: "One Half Dark", isDark: true,
            ansi: [
                "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#dcdfe4",
                "#5d677a", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#dcdfe4",
            ],
            foreground: "#dcdfe4", background: "#282c34",
            cursor: "#a3b3cc",
            selectionBackground: "#474e5d",
            selectionForeground: "#dcdfe4"),
        ColorScheme(
            name: "One Half Light", isDark: false,
            ansi: [
                "#383a42", "#e45649", "#50a14f", "#c18401", "#0184bc", "#a626a4", "#0997b3", "#bababa",
                "#4f525e", "#e06c75", "#98c379", "#d8b36e", "#61afef", "#c678dd", "#56b6c2", "#ffffff",
            ],
            foreground: "#383a42", background: "#fafafa",
            cursor: "#a5b4e5",
            selectionBackground: "#bfceff",
            selectionForeground: "#383a42"),
        ColorScheme(
            name: "Rosé Pine", isDark: true,
            ansi: [
                "#26233a", "#eb6f92", "#31748f", "#f6c177", "#9ccfd8", "#c4a7e7", "#ebbcba", "#e0def4",
                "#6e6a86", "#eb6f92", "#31748f", "#f6c177", "#9ccfd8", "#c4a7e7", "#ebbcba", "#e0def4",
            ],
            foreground: "#e0def4", background: "#191724",
            cursor: "#e0def4",
            selectionBackground: "#403d52",
            selectionForeground: "#e0def4"),
        ColorScheme(
            name: "GitHub Dark", isDark: true,
            ansi: [
                "#484f58", "#ff7b72", "#3fb950", "#d29922", "#58a6ff", "#bc8cff", "#39c5cf", "#b1bac4",
                "#6e7681", "#ffa198", "#56d364", "#e3b341", "#79c0ff", "#d2a8ff", "#56d4dd", "#ffffff",
            ],
            foreground: "#e6edf3", background: "#0d1117",
            cursor: "#2f81f7",
            selectionBackground: "#e6edf3",
            selectionForeground: "#0d1117"),
        ColorScheme(
            name: "GitHub Light", isDark: false,
            ansi: [
                "#24292f", "#cf222e", "#116329", "#4d2d00", "#0969da", "#8250df", "#1b7c83", "#6e7781",
                "#57606a", "#a40e26", "#1a7f37", "#633c01", "#218bff", "#a475f9", "#3192aa", "#8c959f",
            ],
            foreground: "#1f2328", background: "#ffffff",
            cursor: "#0969da",
            selectionBackground: "#1f2328",
            selectionForeground: "#ffffff"),
        ColorScheme(
            name: "Monokai", isDark: true,
            ansi: [
                "#1a1a1a", "#f4005f", "#98e024", "#fd971f", "#9d65ff", "#f4005f", "#58d1eb", "#c4c5b5",
                "#625e4c", "#f4005f", "#98e024", "#e0d561", "#9d65ff", "#f4005f", "#58d1eb", "#f6f6ef",
            ],
            foreground: "#d9d9d9", background: "#0c0c0c",
            cursor: "#fc971f",
            selectionBackground: "#343434",
            selectionForeground: "#ffffff"),
        ColorScheme(
            name: "Snazzy", isDark: true,
            ansi: [
                "#000000", "#fc4346", "#50fb7c", "#f0fb8c", "#49baff", "#fc4cb4", "#8be9fe", "#ededec",
                "#555555", "#fc4346", "#50fb7c", "#f0fb8c", "#49baff", "#fc4cb4", "#8be9fe", "#ededec",
            ],
            foreground: "#ebece6", background: "#1e1f29",
            cursor: "#e4e4e4",
            selectionBackground: "#81aec6",
            selectionForeground: "#000000"),
        ColorScheme(
            name: "Everforest Dark", isDark: true,
            ansi: [
                "#7a8478", "#e67e80", "#a7c080", "#dbbc7f", "#7fbbb3", "#d699b6", "#83c092", "#f2efdf",
                "#a6b0a0", "#f85552", "#8da101", "#dfa000", "#3a94c5", "#df69ba", "#35a77c", "#fffbef",
            ],
            foreground: "#d3c6aa", background: "#232a2e",
            cursor: "#e69875",
            selectionBackground: "#543a48",
            selectionForeground: "#d3c6aa"),
        ColorScheme(
            name: "Kanagawa Wave", isDark: true,
            ansi: [
                "#090618", "#c34043", "#76946a", "#c0a36e", "#7e9cd8", "#957fb8", "#6a9589", "#c8c093",
                "#727169", "#e82424", "#98bb6c", "#e6c384", "#7fb4ca", "#938aa9", "#7aa89f", "#dcd7ba",
            ],
            foreground: "#dcd7ba", background: "#1f1f28",
            cursor: "#dcd7ba",
            selectionBackground: "#dcd7ba",
            selectionForeground: "#1f1f28"),
    ]

    static func named(_ name: String) -> ColorScheme? {
        prebuilt.first { $0.name == name }
    }
}
