import AppKit
import Metal
import QuartzCore
import SwiftUI

final class SurfaceRegistry {
    private var views: [UUID: MetalHostView] = [:]

    func view(for id: UUID) -> MetalHostView {
        if let existing = views[id] {
            return existing
        }
        let view = MetalHostView()
        views[id] = view
        return view
    }

    func remove(_ id: UUID) {
        views.removeValue(forKey: id)
    }
}

final class MetalHostView: NSView {
    private static let device = MTLCreateSystemDefaultDevice()
    private static let commandQueue = device?.makeCommandQueue()

    private let metalLayer: CAMetalLayer

    override init(frame frameRect: NSRect) {
        let metalLayer = CAMetalLayer()
        metalLayer.device = Self.device
        metalLayer.pixelFormat = .bgra8Unorm
        metalLayer.colorspace = CGColorSpace(name: CGColorSpace.displayP3)
        metalLayer.isOpaque = true
        metalLayer.maximumDrawableCount = 3
        metalLayer.allowsNextDrawableTimeout = false
        self.metalLayer = metalLayer

        super.init(frame: frameRect)
        wantsLayer = true
        layerContentsRedrawPolicy = .duringViewResize
        // Rounding an opaque CAMetalLayer directly is unreliable, so the
        // metal layer lives as a sublayer clipped by the backing layer.
        layer?.cornerRadius = 12
        layer?.masksToBounds = true
        layer?.addSublayer(metalLayer)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override var acceptsFirstResponder: Bool { true }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        metalLayer.frame = bounds
        CATransaction.commit()
        updateDrawableSize()
        render(synchronized: true)
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        guard let window else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        metalLayer.contentsScale = window.backingScaleFactor
        CATransaction.commit()
        updateDrawableSize()
        render(synchronized: true)
    }

    private func updateDrawableSize() {
        let size = convertToBacking(bounds).size
        guard size.width > 0, size.height > 0 else { return }
        if metalLayer.drawableSize != size {
            metalLayer.drawableSize = size
        }
    }

    func render(synchronized: Bool = false) {
        guard let queue = Self.commandQueue,
            metalLayer.drawableSize.width > 0,
            metalLayer.drawableSize.height > 0
        else { return }

        let sync = synchronized || inLiveResize
        if metalLayer.presentsWithTransaction != sync {
            metalLayer.presentsWithTransaction = sync
        }

        guard let drawable = metalLayer.nextDrawable(),
            let commandBuffer = queue.makeCommandBuffer()
        else { return }

        let descriptor = MTLRenderPassDescriptor()
        let attachment = descriptor.colorAttachments[0]!
        attachment.texture = drawable.texture
        attachment.loadAction = .clear
        attachment.storeAction = .store
        attachment.clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)

        guard let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: descriptor)
        else { return }
        encoder.endEncoding()

        if sync {
            commandBuffer.commit()
            commandBuffer.waitUntilScheduled()
            drawable.present()
        } else {
            commandBuffer.present(drawable)
            commandBuffer.commit()
        }
    }
}

struct TerminalSurface: NSViewRepresentable {
    let hostView: MetalHostView

    func makeNSView(context: Context) -> MetalHostView {
        hostView
    }

    func updateNSView(_ nsView: MetalHostView, context: Context) {}
}
