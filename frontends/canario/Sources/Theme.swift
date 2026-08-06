import SwiftUI

enum Theme {
    static let chrome = Color(red: 0.973, green: 0.557, blue: 0.569)
    static let textPrimary = Color.black.opacity(0.92)
    static let textMuted = Color.black.opacity(0.58)
    static let textSelected = Color.black.opacity(0.92)
    static let inset = Color.black.opacity(0.08)
    static let insetHover = Color.black.opacity(0.14)
    static let selectedFill = Color.white.opacity(0.94)
    static let accentBorder = Color(red: 1.0, green: 0.78, blue: 0.80)
    static let cardRadius: CGFloat = 12

    /// Space identities: each root folder picks the next pair. Kept warm and
    /// low-saturation so they read as tints over the pink chrome, not as new
    /// chrome colors.
    static let spaceGradients: [(top: Color, bottom: Color)] = [
        (Color(red: 1.00, green: 0.72, blue: 0.42), Color(red: 0.95, green: 0.44, blue: 0.38)),  // ember
        (Color(red: 0.45, green: 0.78, blue: 0.98), Color(red: 0.35, green: 0.48, blue: 0.92)),  // tide
        (Color(red: 0.78, green: 0.56, blue: 0.98), Color(red: 0.52, green: 0.36, blue: 0.90)),  // iris
        (Color(red: 0.55, green: 0.88, blue: 0.62), Color(red: 0.25, green: 0.62, blue: 0.50)),  // moss
        (Color(red: 0.99, green: 0.83, blue: 0.45), Color(red: 0.93, green: 0.62, blue: 0.30)),  // dune
        (Color(red: 0.72, green: 0.78, blue: 0.86), Color(red: 0.45, green: 0.52, blue: 0.64)),  // slate
    ]

    static func spaceGradient(_ index: Int) -> LinearGradient {
        let pair = spaceGradients[((index % spaceGradients.count) + spaceGradients.count) % spaceGradients.count]
        return LinearGradient(
            colors: [pair.top, pair.bottom],
            startPoint: .topLeading, endPoint: .bottomTrailing)
    }
}

extension Color {
    /// "#RRGGBB" parsing/serialization for persisting user-picked colors.
    init?(hex: String) {
        var value = hex.trimmingCharacters(in: .whitespaces)
        if value.hasPrefix("#") { value.removeFirst() }
        guard value.count == 6, let rgb = UInt32(value, radix: 16) else {
            return nil
        }
        self.init(
            red: Double((rgb >> 16) & 0xFF) / 255,
            green: Double((rgb >> 8) & 0xFF) / 255,
            blue: Double(rgb & 0xFF) / 255)
    }

    var hexString: String? {
        guard let rgb = NSColor(self).usingColorSpace(.sRGB) else { return nil }
        return String(
            format: "#%02X%02X%02X",
            Int(rgb.redComponent * 255),
            Int(rgb.greenComponent * 255),
            Int(rgb.blueComponent * 255))
    }
}

struct ChromeBackground: View {
    /// The window color (user-configurable; Theme.chrome by default).
    var base: Color = Theme.chrome
    /// Gradient index of the selected terminal's space, if it lives in one.
    var spaceIndex: Int?

    var body: some View {
        ZStack {
            base
            if let spaceIndex {
                Theme.spaceGradient(spaceIndex)
                    .opacity(0.30)
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.35), value: spaceIndex)
    }
}
