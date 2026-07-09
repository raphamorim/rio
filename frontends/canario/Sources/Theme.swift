import SwiftUI

enum Theme {
    static let chrome = Color(red: 0.973, green: 0.557, blue: 0.569)
    static let textPrimary = Color.black.opacity(0.78)
    static let textMuted = Color.black.opacity(0.45)
    static let textSelected = Color.black.opacity(0.78)
    static let inset = Color.black.opacity(0.08)
    static let insetHover = Color.black.opacity(0.14)
    static let selectedFill = Color.white.opacity(0.94)
    static let accentBorder = Color(red: 1.0, green: 0.78, blue: 0.80)
    static let cardRadius: CGFloat = 12
}

struct ChromeBackground: View {
    var body: some View {
        Theme.chrome
    }
}
