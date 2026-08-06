import AppKit
import SwiftUI

// Settings window (⌘,), Arc-style: the native preferences toolbar tabs on
// top (Arc's own settings use exactly this chrome) with dark grouped-card
// sections below. One tab for now; the TabView is the growth point.
// Changes apply to live terminals instantly and persist in UserDefaults.

/// Monospace families installed on this machine, resolved once. Family
/// names aren't always valid font names, so membership goes through
/// NSFontManager's member lists.
enum FontCatalog {
    static let monospaceFamilies: [String] = {
        let manager = NSFontManager.shared
        return manager.availableFontFamilies
            .filter { !$0.hasPrefix(".") }
            .filter { family in
                guard
                    let members = manager.availableMembers(ofFontFamily: family),
                    let first = members.first,
                    let name = first[0] as? String,
                    let font = NSFont(name: name, size: 13)
                else { return false }
                return font.isFixedPitch
            }
            .sorted()
    }()
}

struct SettingsView: View {
    var body: some View {
        TabView {
            TextSettingsView()
                .tabItem {
                    Label("Text", systemImage: "character.cursor.ibeam")
                }
            AppearanceSettingsView()
                .tabItem {
                    Label("Appearance", systemImage: "paintpalette")
                }
        }
        .frame(width: 620)
        .preferredColorScheme(.dark)
    }
}

private struct TextSettingsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Form {
            Section {
                preview
                    .listRowInsets(EdgeInsets())
            }

            Section("Font") {
                LabeledContent("Size") {
                    HStack(spacing: 10) {
                        Slider(
                            value: Binding(
                                get: { Double(model.fontSize) },
                                set: { model.setFontSize(Float($0)) }
                            ),
                            in: 8...28, step: 1
                        )
                        .controlSize(.small)
                        .frame(width: 220)
                        Text("\(Int(model.fontSize)) pt")
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                            .frame(width: 40, alignment: .trailing)
                    }
                }

                familyList
            }
        }
        .formStyle(.grouped)
        .frame(height: 520)
    }

    /// Live sample rendered with the exact family + size the terminals use.
    private var preview: some View {
        VStack(alignment: .leading, spacing: 2) {
            let font = Font.custom(
                model.fontFamily, size: CGFloat(min(model.fontSize, 18)))
            HStack(spacing: 0) {
                Text("~ ").font(font).foregroundStyle(
                    Color(red: 0.18, green: 0.60, blue: 1.0))
                Text("make canario").font(font).foregroundStyle(.white)
            }
            HStack(spacing: 0) {
                Text("ok ").font(font).foregroundStyle(
                    Color(red: 0.16, green: 0.85, blue: 0.28))
                Text("0123456789 -> => != <=").font(font)
                    .foregroundStyle(.white.opacity(0.85))
            }
            Text("the quick brown fox jumps over the lazy dog")
                .font(font)
                .foregroundStyle(.white.opacity(0.55))
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            Color(red: 0x0f / 255, green: 0x0d / 255, blue: 0x0e / 255))
    }

    /// Every monospace family on the machine, each row rendered in itself.
    private var familyList: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical) {
                VStack(spacing: 2) {
                    ForEach(FontCatalog.monospaceFamilies, id: \.self) { family in
                        FontRowView(
                            family: family,
                            isSelected: family == model.fontFamily
                        ) {
                            model.setFontFamily(family)
                        }
                        .id(family)
                    }
                }
                .padding(.vertical, 4)
            }
            .frame(height: 240)
            .onAppear { proxy.scrollTo(model.fontFamily, anchor: .center) }
        }
    }
}

private struct AppearanceSettingsView: View {
    @Environment(AppModel.self) private var model

    /// Full theme presets: each carries a coherent window / text /
    /// selection / border palette (from the chrome color study), so one
    /// click can't leave black text on a dark window.
    private struct Preset {
        let name: String
        let chrome: String
        let text: String
        let selection: String
        let border: String

        var chromeColor: Color { Color(hex: chrome) ?? Theme.chrome }
    }

    private static let presets: [Preset] = [
        Preset(
            name: "Salmon", chrome: "#F88E91", text: "#000000",
            selection: "#FFFFFF", border: "#FFC7CC"),
        Preset(
            name: "Blush", chrome: "#FCE0D9", text: "#000000",
            selection: "#FFFFFF", border: "#EFA0A8"),
        Preset(
            name: "Rosewood", chrome: "#C85A64", text: "#FFFFFF",
            selection: "#8A3B44", border: "#FFC7CC"),
        Preset(
            name: "Cocoa", chrome: "#211A1B", text: "#F5EDEB",
            selection: "#3A3236", border: "#9AD4F0"),
        Preset(
            name: "Ultramarine", chrome: "#4B4DEE", text: "#FFFFFF",
            selection: "#2E30B8", border: "#C7C8FF"),
    ]

    var body: some View {
        Form {
            Section("Window") {
                ColorPicker(
                    "Window color",
                    selection: Binding(
                        get: { model.chromeColor },
                        set: { model.setChromeColor($0) }),
                    supportsOpacity: false)

                LabeledContent("Presets") {
                    HStack(spacing: 8) {
                        ForEach(Self.presets, id: \.name) { preset in
                            swatch(preset)
                        }
                    }
                }

                ColorPicker(
                    "Text color",
                    selection: Binding(
                        get: { model.textColor },
                        set: { model.setTextColor($0) }),
                    supportsOpacity: false)

                ColorPicker(
                    "Selection color",
                    selection: Binding(
                        get: { model.selectionColor },
                        set: { model.setSelectionColor($0) }),
                    supportsOpacity: false)

                ColorPicker(
                    "Border color",
                    selection: Binding(
                        get: { model.borderColor },
                        set: { model.setBorderColor($0) }),
                    supportsOpacity: false)

                LabeledContent("Reset") {
                    Button("Back to salmon") { model.resetAppearance() }
                }
            }
        }
        .formStyle(.grouped)
        .frame(height: 520)
    }

    private func swatch(_ preset: Preset) -> some View {
        let isCurrent = preset.chrome == model.chromeColor.hexString
        return Circle()
            .fill(preset.chromeColor)
            .frame(width: 24, height: 24)
            .overlay(
                Circle().strokeBorder(
                    isCurrent ? Color.white : .white.opacity(0.2),
                    lineWidth: isCurrent ? 2 : 1)
            )
            .contentShape(Circle())
            .onTapGesture { apply(preset) }
            .help(preset.name)
    }

    /// A preset is the whole look: all four colors move together.
    private func apply(_ preset: Preset) {
        if let color = Color(hex: preset.chrome) { model.setChromeColor(color) }
        if let color = Color(hex: preset.text) { model.setTextColor(color) }
        if let color = Color(hex: preset.selection) {
            model.setSelectionColor(color)
        }
        if let color = Color(hex: preset.border) { model.setBorderColor(color) }
    }
}

private struct FontRowView: View {
    let family: String
    let isSelected: Bool
    let action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack {
                Text(family)
                    .font(.custom(family, size: 14))
                    .foregroundStyle(.primary.opacity(isSelected ? 1.0 : 0.75))
                    .lineLimit(1)
                Spacer(minLength: 0)
                if isSelected {
                    Image(systemName: "checkmark")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(Color(red: 0.97, green: 0.56, blue: 0.57))
                }
            }
            .padding(.horizontal, 10)
            .frame(height: 32)
            .background(
                RoundedRectangle(cornerRadius: 7)
                    .fill(
                        isSelected
                            ? AnyShapeStyle(Color.white.opacity(0.10))
                            : AnyShapeStyle(
                                Color.white.opacity(isHovered ? 0.05 : 0.0001)))
            )
            .contentShape(RoundedRectangle(cornerRadius: 7))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }
}
