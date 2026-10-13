import AppKit

let destination = URL(fileURLWithPath: CommandLine.arguments[1])
try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
let arrow = [(3,1),(4,1),(3,2),(4,2),(1,3),(2,3),(3,3),(4,3),(5,3),(6,3),(2,4),(3,4),(4,4),(5,4),(3,5),(4,5)]
for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = size * scale
        let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
                                      bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
                                      isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
        NSColor(srgbRed: 0.063, green: 0.063, blue: 0.06, alpha: 1).setFill()
        NSBezierPath(roundedRect: NSRect(x: 0, y: 0, width: pixels, height: pixels),
                     xRadius: CGFloat(pixels) * 0.19, yRadius: CGFloat(pixels) * 0.19).fill()
        let cell = CGFloat(pixels) / 8
        NSColor(srgbRed: 0.94, green: 0.25, blue: 0.23, alpha: 1).setFill()
        for (x, y) in arrow {
            NSRect(x: CGFloat(x) * cell + cell / 8, y: CGFloat(7-y) * cell + cell / 8,
                   width: cell * 0.75, height: cell * 0.75).fill()
        }
        NSColor(srgbRed: 0.91, green: 0.90, blue: 0.85, alpha: 1).setFill()
        for x in 1...6 { NSRect(x: CGFloat(x) * cell + cell / 8, y: cell + cell / 8, width: cell * 0.75, height: cell * 0.75).fill() }
        NSGraphicsContext.restoreGraphicsState()
        let suffix = scale == 2 ? "@2x" : ""
        try bitmap.representation(using: .png, properties: [:])!.write(to: destination.appendingPathComponent("icon_\(size)x\(size)\(suffix).png"))
    }
}
