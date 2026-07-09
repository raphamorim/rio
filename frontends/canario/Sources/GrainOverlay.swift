import AppKit
import SwiftUI

enum NoiseTexture {
    static let image: NSImage = {
        let pixels = 256
        var data = [UInt8](repeating: 0, count: pixels * pixels)
        for i in 0..<data.count {
            data[i] = UInt8.random(in: 0...255)
        }
        let provider = CGDataProvider(
            data: Data(data) as CFData)!
        let cgImage = CGImage(
            width: pixels, height: pixels,
            bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: pixels,
            space: CGColorSpaceCreateDeviceGray(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false,
            intent: .defaultIntent)!
        let image = NSImage(cgImage: cgImage, size: NSSize(width: pixels / 2, height: pixels / 2))
        return image
    }()
}

struct GrainOverlay: View {
    var opacity: Double = 0.05

    var body: some View {
        Image(nsImage: NoiseTexture.image)
            .resizable(resizingMode: .tile)
            .interpolation(.none)
            .opacity(opacity)
            .blendMode(.overlay)
            .allowsHitTesting(false)
    }
}
