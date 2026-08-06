import AppKit
import SwiftUI
import UniformTypeIdentifiers
import VisionKit

// Image Peek: Arc-style lightbox for kitty images. Click an image in a
// terminal and it zooms out of its cell rect into a dimmed overlay with
// pinch-zoom, Live Text (select/copy text inside the image, QR codes),
// arrow-key navigation between the buffer's images, and quick actions.

/// One image in the peek gallery. `sourceRect` is the placement's rect in
/// window coordinates, so the lightbox can animate from where it sits.
struct PeekImage: Identifiable {
    let id = UUID()
    let cgImage: CGImage
    let sourceRect: CGRect

    var nsImage: NSImage {
        NSImage(cgImage: cgImage, size: sourceRect.size)
    }
}

struct ImagePeekState {
    var images: [PeekImage]
    var index: Int

    var current: PeekImage { images[index] }
}

/// Clipboard / file actions shared by the peek's toolbar and the terminal's
/// image context menu.
enum ImageActions {
    static func pngData(_ image: CGImage) -> Data? {
        let rep = NSBitmapImageRep(cgImage: image)
        return rep.representation(using: .png, properties: [:])
    }

    static func copy(_ image: CGImage) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.writeObjects([
            NSImage(
                cgImage: image,
                size: NSSize(width: image.width, height: image.height))
        ])
    }

    private static func suggestedName() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd 'at' HH.mm.ss"
        return "Canario Image \(formatter.string(from: Date())).png"
    }

    /// Writes into ~/Downloads and returns the file, nil on failure.
    @discardableResult
    static func saveToDownloads(_ image: CGImage) -> URL? {
        guard let data = pngData(image),
            let downloads = FileManager.default.urls(
                for: .downloadsDirectory, in: .userDomainMask
            ).first
        else { return nil }
        let url = downloads.appendingPathComponent(suggestedName())
        return (try? data.write(to: url)) != nil ? url : nil
    }

    /// Writes a temp PNG for handoff to other apps (Preview, drag-out).
    static func tempFile(_ image: CGImage) -> URL? {
        guard let data = pngData(image) else { return nil }
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(suggestedName())
        return (try? data.write(to: url)) != nil ? url : nil
    }

    static func openInPreview(_ image: CGImage) {
        guard let url = tempFile(image) else { return }
        NSWorkspace.shared.open(url)
    }
}

struct ImagePeekOverlay: View {
    @Environment(AppModel.self) private var model
    @State private var appeared = false
    @State private var keyMonitor: Any?

    var body: some View {
        GeometryReader { geo in
            if let peek = model.imagePeek {
                let current = peek.current
                let target = targetRect(for: current, in: geo.size)
                let source = sourceRect(for: current, in: geo.size)
                let frame = appeared ? target : source

                ZStack {
                    Color.black.opacity(appeared ? 0.62 : 0)
                        .contentShape(Rectangle())
                        .onTapGesture { close() }

                    ZoomableImageView(cgImage: current.cgImage)
                        .id(current.id)
                        .clipShape(
                            RoundedRectangle(cornerRadius: appeared ? 10 : 2)
                        )
                        .shadow(
                            color: .black.opacity(appeared ? 0.5 : 0),
                            radius: 36, y: 12
                        )
                        .frame(width: frame.width, height: frame.height)
                        .position(x: frame.midX, y: frame.midY)

                    actionBar(peek)
                        .opacity(appeared ? 1 : 0)
                        .frame(
                            maxWidth: .infinity, maxHeight: .infinity,
                            alignment: .bottom)
                        .padding(.bottom, 18)
                }
            }
        }
        .ignoresSafeArea()
        .onAppear {
            withAnimation(.spring(response: 0.32, dampingFraction: 0.86)) {
                appeared = true
            }
            installKeyMonitor()
        }
        .onDisappear { removeKeyMonitor() }
    }

    /// Aspect-fit inside the window, leaving air for the dim border and
    /// the action bar.
    private func targetRect(for image: PeekImage, in size: CGSize) -> CGRect {
        let insetX: CGFloat = 64
        let insetTop: CGFloat = 48
        let insetBottom: CGFloat = 96
        let available = CGSize(
            width: max(size.width - insetX * 2, 80),
            height: max(size.height - insetTop - insetBottom, 80))
        let imageSize = CGSize(
            width: CGFloat(image.cgImage.width),
            height: CGFloat(image.cgImage.height))
        // Never upscale beyond 2x the terminal cell footprint's pixels;
        // small glyphs stay sharp instead of ballooning into mush.
        let fit = min(
            available.width / imageSize.width,
            available.height / imageSize.height, 2)
        let w = imageSize.width * fit
        let h = imageSize.height * fit
        return CGRect(
            x: (size.width - w) / 2,
            y: insetTop + (available.height - h) / 2,
            width: w, height: h)
    }

    /// The placement's on-screen rect, converted from window coordinates
    /// (bottom-left origin) into the overlay's top-left space.
    private func sourceRect(for image: PeekImage, in size: CGSize) -> CGRect {
        let rect = image.sourceRect
        return CGRect(
            x: rect.minX,
            y: size.height - rect.maxY,
            width: max(rect.width, 1),
            height: max(rect.height, 1))
    }

    private func actionBar(_ peek: ImagePeekState) -> some View {
        HStack(spacing: 14) {
            if peek.images.count > 1 {
                Text("\(peek.index + 1) of \(peek.images.count)")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.55))
                Divider().frame(height: 14)
            }
            barButton("doc.on.doc", "Copy") {
                ImageActions.copy(peek.current.cgImage)
            }
            barButton("arrow.down.circle", "Save") {
                ImageActions.saveToDownloads(peek.current.cgImage)
            }
            barButton("eye", "Preview") {
                ImageActions.openInPreview(peek.current.cgImage)
                close()
            }
            ShareButton(cgImage: peek.current.cgImage)
                .frame(width: 22, height: 20)
        }
        .fixedSize()
        .padding(.horizontal, 16)
        .padding(.vertical, 9)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(red: 0.11, green: 0.10, blue: 0.11))
                .overlay(
                    RoundedRectangle(cornerRadius: 12)
                        .strokeBorder(.white.opacity(0.12), lineWidth: 1))
        )
    }

    private func barButton(
        _ symbol: String, _ label: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Image(systemName: symbol).font(.system(size: 11, weight: .medium))
                Text(label).font(.system(size: 11, weight: .semibold))
            }
            .foregroundStyle(.white.opacity(0.85))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func step(_ delta: Int) {
        guard var peek = model.imagePeek, peek.images.count > 1 else { return }
        peek.index = (peek.index + delta + peek.images.count) % peek.images.count
        model.imagePeek = peek
    }

    private func close() {
        withAnimation(.easeOut(duration: 0.18)) { appeared = false }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.18) {
            model.imagePeek = nil
        }
    }

    private func installKeyMonitor() {
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) {
            event in
            switch event.keyCode {
            case 0x35, 0x31:  // Escape, Space (Quick Look muscle memory)
                close()
                return nil
            case 0x7B:  // Left arrow
                step(-1)
                return nil
            case 0x7C:  // Right arrow
                step(1)
                return nil
            default:
                return event
            }
        }
    }

    private func removeKeyMonitor() {
        if let keyMonitor {
            NSEvent.removeMonitor(keyMonitor)
        }
        keyMonitor = nil
    }
}

/// Pinch-zoomable image with Live Text: an NSScrollView with magnification
/// hosting the image, plus VisionKit's overlay so text inside the image is
/// selectable and QR codes are tappable (the same interface Photos uses).
private struct ZoomableImageView: NSViewRepresentable {
    let cgImage: CGImage

    static let analyzer = ImageAnalyzer()

    /// Keeps the document view matched to the scroll view's size; an
    /// NSScrollView never resizes its document on its own, and the
    /// representable's updateNSView runs before layout, when bounds are
    /// still zero. `frame` (not the magnified bounds) so zoom is stable.
    final class FittingScrollView: NSScrollView {
        override func layout() {
            super.layout()
            documentView?.frame = CGRect(origin: .zero, size: frame.size)
        }
    }

    func makeNSView(context: Context) -> NSScrollView {
        let image = NSImage(
            cgImage: cgImage,
            size: NSSize(width: cgImage.width, height: cgImage.height))

        let imageView = NSImageView(image: image)
        imageView.imageScaling = .scaleProportionallyUpOrDown
        imageView.autoresizingMask = [.width, .height]

        let overlay = ImageAnalysisOverlayView()
        overlay.autoresizingMask = [.width, .height]
        overlay.frame = imageView.bounds
        overlay.trackingImageView = imageView
        overlay.preferredInteractionTypes = [.textSelection, .dataDetectors]
        imageView.addSubview(overlay)

        let scroll = FittingScrollView()
        scroll.drawsBackground = false
        scroll.hasHorizontalScroller = false
        scroll.hasVerticalScroller = false
        scroll.allowsMagnification = true
        scroll.minMagnification = 1
        scroll.maxMagnification = 12
        scroll.documentView = imageView
        imageView.frame = scroll.bounds
        scroll.autoresizesSubviews = true

        analyze(image: image, overlay: overlay)
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {}

    private func analyze(image: NSImage, overlay: ImageAnalysisOverlayView) {
        Task { @MainActor in
            let configuration = ImageAnalyzer.Configuration([
                .text, .machineReadableCode,
            ])
            guard
                let analysis = try? await Self.analyzer.analyze(
                    image, orientation: .up, configuration: configuration)
            else { return }
            overlay.analysis = analysis
        }
    }
}

/// AppKit-backed share button: NSSharingServicePicker needs a concrete
/// NSView anchor, which SwiftUI buttons can't provide.
private struct ShareButton: NSViewRepresentable {
    let cgImage: CGImage

    func makeNSView(context: Context) -> NSButton {
        let button = NSButton(
            image: NSImage(
                systemSymbolName: "square.and.arrow.up",
                accessibilityDescription: "Share")!,
            target: context.coordinator,
            action: #selector(Coordinator.share(_:)))
        button.isBordered = false
        button.contentTintColor = NSColor.white.withAlphaComponent(0.85)
        return button
    }

    func updateNSView(_ button: NSButton, context: Context) {
        context.coordinator.cgImage = cgImage
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(cgImage: cgImage)
    }

    final class Coordinator: NSObject {
        var cgImage: CGImage

        init(cgImage: CGImage) {
            self.cgImage = cgImage
        }

        @objc func share(_ sender: NSButton) {
            let image = NSImage(
                cgImage: cgImage,
                size: NSSize(width: cgImage.width, height: cgImage.height))
            let picker = NSSharingServicePicker(items: [image])
            picker.show(
                relativeTo: sender.bounds, of: sender, preferredEdge: .minY)
        }
    }
}
