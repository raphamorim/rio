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

    /// Distance from the cell top to the baseline; glyph drawing and the
    /// Nerd Font constraint transform both hang off it.
    let ascent: CGFloat
    /// `(2 * cap_height + face_height) / 3`, the Nerd Fonts patcher's
    /// single-cell icon height heuristic (same as ghostty's
    /// `icon_height_single`).
    let iconHeightSingle: CGFloat

    init(fontSize: CGFloat, family: String) {
        var base =
            NSFont(name: family, size: fontSize)
            ?? NSFont(name: "Menlo", size: fontSize)
            ?? .monospacedSystemFont(ofSize: fontSize, weight: .regular)

        // Cascade to the symbols-only Nerd Font embedded in librio (the
        // libghostty approach), so icon codepoints resolve with no font
        // file in the bundle and nothing installed on the machine.
        var fontLength = 0
        if let bytes = rio_symbols_nerd_font(&fontLength), fontLength > 0,
            let data = CFDataCreate(nil, bytes, fontLength),
            let descriptor = CTFontManagerCreateFontDescriptorFromData(data)
        {
            let cascaded = base.fontDescriptor.addingAttributes([
                .cascadeList: [descriptor as NSFontDescriptor]
            ])
            base = NSFont(descriptor: cascaded, size: fontSize) ?? base
        }

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
        ascent = CTFontGetAscent(ct)
        iconHeightSingle = (2 * CTFontGetCapHeight(ct) + cellHeight) / 3
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

    /// Resolved font + glyph + bounding box for icon codepoints, so the
    /// cascade walk and measurement run once per codepoint per font size.
    /// `nil` means "measured, nothing usable: draw the normal way".
    private struct IconGlyph {
        let font: CTFont
        let glyph: CGGlyph
        let bounds: CGRect
    }
    private var iconCache: [UInt32: IconGlyph?] = [:]

    /// Decoded kitty images keyed by image id; the stamp changes on
    /// retransmission, which is the only time pixels need re-decoding.
    private struct KittyBitmap {
        let stamp: UInt64
        let image: CGImage
    }
    private var kittyCache: [UInt32: KittyBitmap] = [:]

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

    init(fontSize: CGFloat, family: String) {
        metrics = TerminalMetrics(fontSize: fontSize, family: family)
    }

    func setFont(size: CGFloat, family: String) {
        metrics = TerminalMetrics(fontSize: size, family: family)
        iconCache.removeAll()
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

        // Kitty images interleave with the cell passes at the z bounds the
        // protocol defines (ghostty's renderer/image.zig splits the same
        // way): z < INT32_MIN/2 under cell backgrounds, z < 0 under text
        // (where virtual placements land), the rest above.
        let kitty = collectKittyImages(state: state)
        drawKittyLayer(kitty.belowBackground)

        // Background pass (skip the default to avoid overdraw). Runs
        // before any glyph so under-text images can sit between the two.
        for line in 0..<lines {
            let y = pad + CGFloat(line) * ch
            for col in 0..<cols {
                let cell = rio_render_state_cell(state, UInt16(line), UInt16(col))
                let inverse = cell.style_flags & StyleFlag.inverse != 0
                var bg = color(inverse ? cell.fg : cell.bg)
                if selection.active
                    && cellSelected(selection, line: line, col: col, cols: cols)
                {
                    bg = selectionBackground
                }
                if bg != defaultBackground {
                    bg.setFill()
                    NSRect(x: pad + CGFloat(col) * cw, y: y, width: cw, height: ch)
                        .fill()
                }
            }
        }

        drawKittyLayer(kitty.belowText)

        for line in 0..<lines {
            let y = pad + CGFloat(line) * ch
            for col in 0..<cols {
                let x = pad + CGFloat(col) * cw
                let cell = rio_render_state_cell(state, UInt16(line), UInt16(col))

                let inverse = cell.style_flags & StyleFlag.inverse != 0
                var fg = color(inverse ? cell.bg : cell.fg)

                // Selection recolors the cell (rio-style), rather than
                // painting a translucent overlay on top of it.
                if selection.active
                    && cellSelected(selection, line: line, col: col, cols: cols)
                {
                    fg = selectionForeground
                }

                if cell.style_flags & StyleFlag.hidden != 0 { continue }
                if cell.style_flags & StyleFlag.dim != 0 {
                    fg = fg.withAlphaComponent(0.6)
                }

                guard let scalar = Unicode.Scalar(cell.codepoint),
                    cell.codepoint != 0x20, cell.codepoint != 0,
                    cell.codepoint != 0x10EEEE
                else { continue }

                // Icon codepoints go through the Nerd Fonts constraint
                // pipeline (scaled + aligned per the patcher's rules, like
                // ghostty/rio). Everything else is plain string drawing.
                if cell.codepoint >= 0x2300 {
                    // A free neighbour cell lets wide icons span two cells,
                    // the same test ghostty applies.
                    let nextFree =
                        col + 1 < cols
                        && {
                            let next = rio_render_state_cell(
                                state, UInt16(line), UInt16(col + 1))
                            return next.codepoint == 0x20 || next.codepoint == 0
                        }()
                    if drawConstrainedGlyph(
                        scalar, at: NSPoint(x: x, y: y), color: fg,
                        constraintWidth: nextFree ? 2 : 1)
                    {
                        continue
                    }
                }

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

        drawKittyLayer(kitty.aboveText)
        drawCursor(state: state, cols: cols, lines: lines, focused: focused)
        drawPreedit(state: state, cols: cols, lines: lines)
    }

    /// The input method's in-progress composition, drawn at the cursor the
    /// way IME-aware apps draw it: inverted box, thick underline. Without
    /// this, dead keys and CJK composition are invisible until commit.
    private func drawPreedit(state: OpaquePointer, cols: Int, lines: Int) {
        guard let preedit, !preedit.isEmpty else { return }
        // The cursor is hidden while scrolled into history; so is preedit.
        guard rio_render_state_display_offset(state) == 0 else { return }
        let cursor = rio_render_state_cursor(state)
        guard Int(cursor.line) < lines else { return }

        let cw = metrics.cellWidth
        let ch = metrics.cellHeight
        let pad = metrics.padding
        let font = metrics.regular

        let attributes: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: defaultBackground,
            .underlineStyle: NSUnderlineStyle.thick.rawValue,
            .underlineColor: cursorColor,
        ]
        let size = (preedit as NSString).size(withAttributes: [.font: font])
        let width = max(size.width, cw)

        // Keep the box on screen: grow rightwards from the cursor, sliding
        // left when the composition outgrows the remaining columns.
        let gridRight = pad + CGFloat(cols) * cw
        var x = pad + CGFloat(cursor.column) * cw
        if x + width > gridRight {
            x = max(pad, gridRight - width)
        }
        let y = pad + CGFloat(cursor.line) * ch
        let rect = NSRect(x: x, y: y, width: width, height: ch)

        NSColor.white.setFill()
        rect.fill()
        (preedit as NSString).draw(
            at: NSPoint(x: x, y: y), withAttributes: attributes)
    }

    /// One kitty placement resolved for the host: decoded (cached) bitmap,
    /// already cropped to its source rect, with a point-space rect in the
    /// surface view's coordinates. Draw order = array order (z-sorted).
    struct ResolvedKittyImage {
        let imageID: UInt32
        let zIndex: Int32
        let rect: CGRect
        let image: CGImage
    }

    private struct KittyLayers {
        var belowBackground: [ResolvedKittyImage] = []
        var belowText: [ResolvedKittyImage] = []
        var aboveText: [ResolvedKittyImage] = []
    }

    private func collectKittyImages(state: OpaquePointer) -> KittyLayers {
        var layers = KittyLayers()
        // Drawing resolves against the active context's scale; hit-testing
        // callers pass the window's backing scale instead.
        let scale =
            NSGraphicsContext.current?.cgContext
            .userSpaceToDeviceSpaceTransform.a ?? 2
        let belowBackgroundLimit = Int32.min / 2
        for item in resolvedKittyImages(state: state, scale: scale) {
            if item.zIndex < belowBackgroundLimit {
                layers.belowBackground.append(item)
            } else if item.zIndex < 0 {
                layers.belowText.append(item)
            } else {
                layers.aboveText.append(item)
            }
        }
        return layers
    }

    /// Resolve every kitty placement (librio hands them z-sorted with
    /// viewport-relative pixel geometry) into a decoded + cropped bitmap
    /// and a point-space rect. Also the mouse hit-testing entry point for
    /// image interactions (peek, context menu, drag-out).
    func resolvedKittyImages(
        state: OpaquePointer, scale: CGFloat
    ) -> [ResolvedKittyImage] {
        var resolved: [ResolvedKittyImage] = []
        let count = rio_render_state_kitty_count(state)
        if count == 0 {
            if !kittyCache.isEmpty { kittyCache.removeAll() }
            return resolved
        }
        let pad = metrics.padding
        // The surface reports its size to librio in device pixels, so
        // placements are laid out in that space (images map 1:1 to device
        // pixels, per the kitty protocol); geometry queries must match it,
        // and the results scale back down to points.
        let scale = max(scale, 1)

        for index in 0..<count {
            var placement = rio_kitty_placement_s()
            guard
                rio_render_state_kitty_placement(
                    state, index, Float(metrics.cellWidth * scale),
                    Float(metrics.cellHeight * scale), &placement),
                var image = kittyBitmap(state: state, imageID: placement.image_id)
            else { continue }

            if placement.src_x > 0 || placement.src_y > 0
                || placement.src_w < 1 || placement.src_h < 1
            {
                let iw = CGFloat(image.width)
                let ih = CGFloat(image.height)
                let crop = CGRect(
                    x: CGFloat(placement.src_x) * iw,
                    y: CGFloat(placement.src_y) * ih,
                    width: CGFloat(placement.src_w) * iw,
                    height: CGFloat(placement.src_h) * ih)
                guard let cropped = image.cropping(to: crop.integral) else {
                    continue
                }
                image = cropped
            }

            let rect = CGRect(
                x: pad + CGFloat(placement.x) / scale,
                y: pad + CGFloat(placement.y) / scale,
                width: CGFloat(placement.width) / scale,
                height: CGFloat(placement.height) / scale)
            resolved.append(
                ResolvedKittyImage(
                    imageID: placement.image_id, zIndex: placement.z_index,
                    rect: rect, image: image))
        }
        return resolved
    }

    private func drawKittyLayer(_ draws: [ResolvedKittyImage]) {
        guard !draws.isEmpty,
            let context = NSGraphicsContext.current?.cgContext
        else { return }
        for draw in draws {
            // CGContext blits images y-up while the view is flipped; flip
            // locally around the rect so the image lands upright.
            context.saveGState()
            context.translateBy(x: 0, y: draw.rect.maxY)
            context.scaleBy(x: 1, y: -1)
            context.draw(
                draw.image,
                in: CGRect(
                    x: draw.rect.minX, y: 0,
                    width: draw.rect.width, height: draw.rect.height))
            context.restoreGState()
        }
    }

    private func kittyBitmap(state: OpaquePointer, imageID: UInt32) -> CGImage? {
        var width: UInt32 = 0
        var height: UInt32 = 0
        var stamp: UInt64 = 0
        guard
            rio_render_state_kitty_image_info(
                state, imageID, &width, &height, &stamp),
            width > 0, height > 0
        else { return nil }
        if let cached = kittyCache[imageID], cached.stamp == stamp {
            return cached.image
        }

        let byteCount = Int(width) * Int(height) * 4
        var pixels = Data(count: byteCount)
        let copied = pixels.withUnsafeMutableBytes { buffer in
            rio_render_state_kitty_image_rgba(
                state, imageID,
                buffer.baseAddress?.assumingMemoryBound(to: UInt8.self),
                byteCount)
        }
        guard copied == byteCount,
            let provider = CGDataProvider(data: pixels as CFData),
            let image = CGImage(
                width: Int(width), height: Int(height),
                bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: Int(width) * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
                provider: provider, decode: nil, shouldInterpolate: true,
                intent: .defaultIntent)
        else { return nil }
        kittyCache[imageID] = KittyBitmap(stamp: stamp, image: image)
        return image
    }

    /// Draw an icon glyph through librio's Nerd Fonts constraint math
    /// (the ghostty-derived table): resolve the glyph through the font
    /// cascade, measure it, let the table scale/align it, then paint the
    /// transformed outline. Returns false when the codepoint has no rule
    /// and generic PUA fitting doesn't apply, so the caller falls back to
    /// plain text drawing.
    private func drawConstrainedGlyph(
        _ scalar: Unicode.Scalar, at cellOrigin: NSPoint, color: NSColor,
        constraintWidth: Int32
    ) -> Bool {
        guard let icon = resolveIconGlyph(scalar) else { return false }
        let bounds = icon.bounds
        // The constraint math uses ghostty's space: y-up from the CELL
        // BOTTOM. CoreText boxes are y-up from the BASELINE; the descent
        // is the shift between the two.
        let descent = metrics.cellHeight - metrics.ascent

        var constrained = rio_glyph_box_s(
            x: bounds.minX, y: bounds.minY + descent,
            width: bounds.width, height: bounds.height)
        let hasRule = rio_nerd_constrain(
            scalar.value,
            constrained,
            Double(metrics.cellWidth), Double(metrics.cellHeight),
            Double(metrics.iconHeightSingle),
            UInt8(clamping: constraintWidth),
            &constrained)

        if !hasRule {
            // Untabled codepoint: only intervene for private-use icons
            // that overflow their cells; scale to fit, centered (in the
            // same cell-bottom space as the table output).
            let isPUA =
                (0xE000...0xF8FF).contains(scalar.value)
                || scalar.value >= 0xF0000
            let maxWidth = metrics.cellWidth * CGFloat(constraintWidth)
            guard
                isPUA,
                bounds.width > 0, bounds.height > 0,
                bounds.width > maxWidth || bounds.height > metrics.cellHeight
            else { return false }
            let scale = min(
                maxWidth / bounds.width, metrics.cellHeight / bounds.height)
            let width = bounds.width * scale
            let height = bounds.height * scale
            constrained = rio_glyph_box_s(
                x: (maxWidth - width) / 2,
                y: (metrics.cellHeight - height) / 2,
                width: width, height: height)
        }

        guard bounds.width > 0, bounds.height > 0,
            let context = NSGraphicsContext.current?.cgContext
        else { return false }

        // Map the measured box onto the constrained one. The output box is
        // cell-bottom based; shift by descent to get back to baseline
        // space, in which the glyph outlines are drawn. The view is
        // flipped, so the vertical axis negates.
        let sx = constrained.width / bounds.width
        let sy = constrained.height / bounds.height
        let tx = constrained.x - sx * bounds.minX
        let ty = (constrained.y - descent) - sy * bounds.minY
        let baselineY = cellOrigin.y + metrics.ascent

        context.saveGState()
        // CTFontDrawGlyphs multiplies in the text matrix, which AppKit's
        // flipped string drawing leaves flipped (and saveGState does NOT
        // cover it). Our CTM handles the flip; the text matrix must not.
        context.textMatrix = .identity
        context.translateBy(
            x: cellOrigin.x + CGFloat(tx), y: baselineY - CGFloat(ty))
        context.scaleBy(x: CGFloat(sx), y: -CGFloat(sy))
        context.setFillColor(color.cgColor)
        var glyph = icon.glyph
        var position = CGPoint.zero
        CTFontDrawGlyphs(icon.font, &glyph, &position, 1, context)
        context.restoreGState()
        return true
    }

    private func resolveIconGlyph(_ scalar: Unicode.Scalar) -> IconGlyph? {
        if let cached = iconCache[scalar.value] { return cached }

        let string = String(scalar)
        let base = metrics.regular as CTFont
        // Walks the cascade list (including the bundled symbols font) to
        // the face that actually covers the codepoint.
        let resolved = CTFontCreateForString(
            base, string as CFString,
            CFRange(location: 0, length: string.utf16.count))

        var units = Array(string.utf16)
        var glyphs = [CGGlyph](repeating: 0, count: units.count)
        var result: IconGlyph?
        if CTFontGetGlyphsForCharacters(resolved, &units, &glyphs, units.count),
            let glyph = glyphs.first, glyph != 0
        {
            var g = glyph
            var bounds = CGRect.zero
            bounds = CTFontGetBoundingRectsForGlyphs(
                resolved, .default, &g, nil, 1)
            if !bounds.isEmpty {
                result = IconGlyph(font: resolved, glyph: glyph, bounds: bounds)
            }
        }
        iconCache[scalar.value] = result
        return result
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
