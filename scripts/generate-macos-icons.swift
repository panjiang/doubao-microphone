// Original project vector artwork, rendered with system frameworks; no downloads.
import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

let destination = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "assets/icons")
try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
let temporary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
let iconset = temporary.appendingPathComponent("DoubaoRemote.iconset")
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: temporary) }
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let size = points * scale
        guard let c = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8,
                                bytesPerRow: size * 4, space: CGColorSpaceCreateDeviceRGB(),
                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { fatalError("CGContext") }
        c.translateBy(x: 0, y: CGFloat(size)); c.scaleBy(x: CGFloat(size)/64, y: -CGFloat(size)/64)
        c.setFillColor(CGColor(red: 35/255, green: 75/255, blue: 198/255, alpha: 1))
        c.fillEllipse(in: CGRect(x: 0, y: 0, width: 64, height: 64))
        c.translateBy(x: 0, y: -2.5)
        c.setFillColor(CGColor(gray: 1, alpha: 1))
        c.addPath(CGPath(roundedRect: CGRect(x: 24, y: 13, width: 16, height: 27), cornerWidth: 8, cornerHeight: 8, transform: nil)); c.fillPath()
        c.setStrokeColor(CGColor(gray: 1, alpha: 1)); c.setLineWidth(4); c.setLineCap(.round); c.setLineJoin(.round)
        c.move(to: CGPoint(x: 19, y: 30)); c.addLine(to: CGPoint(x: 19, y: 34))
        c.addCurve(to: CGPoint(x: 32, y: 47), control1: CGPoint(x: 19, y: 41.18), control2: CGPoint(x: 24.82, y: 47))
        c.addCurve(to: CGPoint(x: 45, y: 34), control1: CGPoint(x: 39.18, y: 47), control2: CGPoint(x: 45, y: 41.18))
        c.addLine(to: CGPoint(x: 45, y: 30)); c.strokePath()
        c.move(to: CGPoint(x: 32, y: 47)); c.addLine(to: CGPoint(x: 32, y: 54)); c.strokePath()
        c.move(to: CGPoint(x: 25, y: 54)); c.addLine(to: CGPoint(x: 39, y: 54)); c.strokePath()
        let name = "icon_\(points)x\(points)\(scale == 2 ? "@2x" : "").png"
        guard let image = c.makeImage(), let output = CGImageDestinationCreateWithURL(iconset.appendingPathComponent(name) as CFURL, UTType.png.identifier as CFString, 1, nil) else { fatalError("PNG") }
        CGImageDestinationAddImage(output, image, nil)
        guard CGImageDestinationFinalize(output) else { fatalError("PNG finalize") }
    }
}
let task = Process(); task.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
task.arguments = ["-c", "icns", iconset.path, "-o", destination.appendingPathComponent("DoubaoRemote.icns").path]
try task.run(); task.waitUntilExit()
guard task.terminationStatus == 0 else { fatalError("iconutil") }
print("Generated DoubaoRemote.icns (16–1024px)")
