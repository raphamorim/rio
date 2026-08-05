import AppKit
import RioKit

// CPU terminal renderer.
//
// canario draws entirely on the Swift side: it pulls cell data from
// librio's render state (`rio_render_state_cell`, which returns fully
// resolved RGB colors) and paints glyphs + backgrounds with AppKit /
// CoreText. No GPU renderer from Rio is involved.
//
// NOTE: this is drawn from within a flipped `NSView.draw(_:)`, so AppKit
// text drawing (`NSAttributedString.draw`) lands right-side up without any
// manual matrix flipping.

// Style flag bits, mirrored from `rio-vt`'s `StyleFlags`.
private enum StyleFlag {
    static let inverse: UInt16 = 1 << 0
    static let bold: UInt16 = 1 << 1
    static let italic: UInt16 = 1 << 2
    static let dim: UInt16 = 1 << 3
    static let hidden: UInt16 = 1 << 4
    static let strikeout: UInt16 = 1 << 5
    static let underline: UInt16 = 1 << 6
}

/// Font faces and cell geometry derived from a single point size.
struct TerminalMetrics {
    let regular: NSFont
    let bold: NSFont
    let italic: NSFont
    let boldItalic: NSFont
    let cellWidth: CGFloat
    let cellHeight: CGFloat
    let padding: CGFloat

    init(fontSize: CGFloat) {
        let base =
            NSFont(name: "Menlo", size: fontSize)
            ?? .monospacedSystemFont(ofSize: fontSize, weight: .regular)
        let manager = NSFontManager.shared
        regular = base
        bold = manager.convert(base, toHaveTrait: .boldFontMask)
        italic = manager.convert(base, toHaveTrait: .italicFontMask)
        boldItalic = manager.convert(bold, toHaveTrait: .italicFontMask)

        // Monospace: every glyph shares the advance of "M".
        let advance = ("M" as NSString).size(withAttributes: [.font: base]).width
        cellWidth = ceil(advance)

        let ct = base as CTFont
        let lineHeight =
            CTFontGetAscent(ct) + CTFontGetDescent(ct) + CTFontGetLeading(ct)
        cellHeight = ceil(lineHeight)
        padding = 6
    }

    func font(bold isBold: Bool, italic isItalic: Bool) -> NSFont {
        switch (isBold, isItalic) {
        case (true, true): return boldItalic
        case (true, false): return bold
        case (false, true): return italic
        case (false, false): return regular
        }
    }
}

final class CPURenderer {
    private(set) var metrics: TerminalMetrics
    var preedit: String?

    // Rio's default theme (rio-vt config/colors/defaults.rs); librio resolves
    // cell colors from the same source, these cover what the renderer draws
    // itself. Replace with theme-file values once config loading lands.
    private let defaultBackground = NSColor(
        srgbRed: 0x0f / 255, green: 0x0d / 255, blue: 0x0e / 255, alpha: 1)
    private let selectionBackground = NSColor(
        srgbRed: 0x1c / 255, green: 0x19 / 255, blue: 0x1a / 255, alpha: 1)
    private let selectionForeground = NSColor(
        srgbRed: 0x44 / 255, green: 0xc9 / 255, blue: 0xf0 / 255, alpha: 1)
    private let cursorColor = NSColor(
        srgbRed: 0xf7 / 255, green: 0x12 / 255, blue: 0xff / 255, alpha: 1)

    init(fontSize: CGFloat) {
        metrics = TerminalMetrics(fontSize: fontSize)
    }

    func setFontSize(_ size: CGFloat) {
        metrics = TerminalMetrics(fontSize: size)
    }

    private func color(_ c: rio_color_s) -> NSColor {
        NSColor(
            srgbRed: CGFloat(c.r) / 255,
            green: CGFloat(c.g) / 255,
            blue: CGFloat(c.b) / 255,
            alpha: 1)
    }

    /// Paint the whole surface. Called from `RioSurfaceNSView.draw(_:)` with
    /// the current AppKit graphics context active. `focused` draws a solid
    /// block cursor; unfocused draws a hollow outline.
    func render(state: OpaquePointer, bounds: CGRect, focused: Bool) {
        defaultBackground.setFill()
        bounds.fill()

        let cols = Int(rio_render_state_columns(state))
        let lines = Int(rio_render_state_lines(state))
        guard cols > 0, lines > 0 else { return }

        let cw = metrics.cellWidth
        let ch = metrics.cellHeight
        let pad = metrics.padding

        let selection = rio_render_state_selection(state)

        for line in 0..<lines {
            let y = pad + CGFloat(line) * ch
            for col in 0..<cols {
                let x = pad + CGFloat(col) * cw
                let rect = NSRect(x: x, y: y, width: cw, height: ch)
                let cell = rio_render_state_cell(state, UInt16(line), UInt16(col))

                let inverse = cell.style_flags & StyleFlag.inverse != 0
                var fg = color(inverse ? cell.bg : cell.fg)
                var bg = color(inverse ? cell.fg : cell.bg)

                // Selection recolors the cell (rio-style), rather than
                // painting a translucent overlay on top of it.
                if selection.active
                    && cellSelected(selection, line: line, col: col, cols: cols)
                {
                    bg = selectionBackground
                    fg = selectionForeground
                }

                // Background (skip the default to avoid overdraw).
                if bg != defaultBackground {
                    bg.setFill()
                    rect.fill()
                }

                if cell.style_flags & StyleFlag.hidden != 0 { continue }
                if cell.style_flags & StyleFlag.dim != 0 {
                    fg = fg.withAlphaComponent(0.6)
                }

                guard let scalar = Unicode.Scalar(cell.codepoint),
                    cell.codepoint != 0x20, cell.codepoint != 0
                else { continue }

                let font = metrics.font(
                    bold: cell.style_flags & StyleFlag.bold != 0,
                    italic: cell.style_flags & StyleFlag.italic != 0)
                var attrs: [NSAttributedString.Key: Any] = [
                    .font: font, .foregroundColor: fg,
                ]
                if cell.style_flags & StyleFlag.underline != 0 {
                    attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue
                }
                if cell.style_flags & StyleFlag.strikeout != 0 {
                    attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
                }
                (String(scalar) as NSString).draw(
                    at: NSPoint(x: x, y: y), withAttributes: attrs)
            }
        }

        drawCursor(state: state, cols: cols, lines: lines, focused: focused)
    }

    private func drawCursor(
        state: OpaquePointer, cols: Int, lines: Int, focused: Bool
    ) {
        // The cursor belongs to the live screen. Scrolling up shifts it down
        // in viewport coordinates; once it leaves the viewport, don't draw
        // it (same behavior as ghostty's viewport-relative cursor).
        let cursor = rio_render_state_cursor(state)
        let offset = rio_render_state_display_offset(state)
        let line = Int(cursor.line) + offset
        guard line < lines, Int(cursor.column) < cols else { return }
        let cw = metrics.cellWidth
        let ch = metrics.cellHeight
        let pad = metrics.padding
        let x = pad + CGFloat(cursor.column) * cw
        let y = pad + CGFloat(line) * ch
        let rect = NSRect(x: x, y: y, width: cw, height: ch)

        guard focused else {
            // Unfocused: hollow outline so the glyph stays readable.
            cursorColor.withAlphaComponent(0.6).setStroke()
            let path = NSBezierPath(rect: rect.insetBy(dx: 0.5, dy: 0.5))
            path.lineWidth = 1
            path.stroke()
            return
        }

        // Focused: solid block, with the glyph under it redrawn in the
        // background color so it reads as inverted.
        cursorColor.setFill()
        rect.fill()

        let cell = rio_render_state_cell(state, UInt16(line), cursor.column)
        guard let scalar = Unicode.Scalar(cell.codepoint),
            cell.codepoint != 0x20, cell.codepoint != 0
        else { return }
        let font = metrics.font(
            bold: cell.style_flags & StyleFlag.bold != 0,
            italic: cell.style_flags & StyleFlag.italic != 0)
        (String(scalar) as NSString).draw(
            at: NSPoint(x: x, y: y),
            withAttributes: [.font: font, .foregroundColor: defaultBackground])
    }

    private func cellSelected(
        _ sel: rio_selection_s, line: Int, col: Int, cols: Int
    ) -> Bool {
        let startLine = Int(sel.start_line)
        let startCol = Int(sel.start_col)
        let endLine = Int(sel.end_line)
        let endCol = Int(sel.end_col)

        if sel.is_block {
            let loLine = min(startLine, endLine)
            let hiLine = max(startLine, endLine)
            let loCol = min(startCol, endCol)
            let hiCol = max(startCol, endCol)
            return line >= loLine && line <= hiLine && col >= loCol && col <= hiCol
        }

        // Linear selection: order the two endpoints by (line, col), then
        // include the span (partial first/last row, full rows in between).
        let forward =
            (startLine, startCol) <= (endLine, endCol)
        let loLine = forward ? startLine : endLine
        let loCol = forward ? startCol : endCol
        let hiLine = forward ? endLine : startLine
        let hiCol = forward ? endCol : startCol

        if line < loLine || line > hiLine { return false }
        let lineStart = line == loLine ? loCol : 0
        let lineEnd = line == hiLine ? hiCol : cols - 1
        return col >= lineStart && col <= lineEnd
    }
}
