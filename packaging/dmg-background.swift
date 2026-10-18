// Draws the .dmg window background (660x400, plus @2x) with an arrow from the app to Applications.
// Regenerate packaging/dmg-background.tiff with:
//   swift packaging/dmg-background.swift && tiffutil -cathidpicheck bg.png bg@2x.png -out packaging/dmg-background.tiff && rm bg.png bg@2x.png
// Icon positions must match packaging/dmg-settings.py.
import AppKit

let size = NSSize(width: 660, height: 400)
let sand = NSColor(srgbRed: 0.965, green: 0.937, blue: 0.894, alpha: 1)
let ink = NSColor(srgbRed: 0.122, green: 0.141, blue: 0.2, alpha: 1) // navy from the app icon
let coral = NSColor(srgbRed: 0.878, green: 0.478, blue: 0.373, alpha: 1)

func render(scale: CGFloat, to path: String) {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(size.width * scale), pixelsHigh: Int(size.height * scale),
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = size
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

    sand.setFill()
    NSRect(origin: .zero, size: size).fill()

    // Dashed arrow between the icons (Finder places them at y = 150 from the top). Everything stays in
    // the top ~320pt: Finder's title bar and (on some macOS versions) status bar cover the rest.
    let y = size.height - 150
    let line = NSBezierPath()
    line.move(to: NSPoint(x: 262, y: y))
    line.line(to: NSPoint(x: 386, y: y))
    line.lineWidth = 4
    line.lineCapStyle = .round
    line.setLineDash([10, 9], count: 2, phase: 0)
    coral.setStroke()
    line.stroke()
    let head = NSBezierPath()
    head.move(to: NSPoint(x: 382, y: y + 13))
    head.line(to: NSPoint(x: 400, y: y))
    head.line(to: NSPoint(x: 382, y: y - 13))
    head.lineWidth = 4
    head.lineCapStyle = .round
    head.lineJoinStyle = .round
    head.stroke()

    let center = NSMutableParagraphStyle()
    center.alignment = .center
    let title: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: 20, weight: .semibold), .foregroundColor: ink, .paragraphStyle: center,
    ]
    let note: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: 12), .foregroundColor: ink.withAlphaComponent(0.6), .paragraphStyle: center,
    ]
    ("Drag cctwin to Applications" as NSString).draw(in: NSRect(x: 0, y: 110, width: size.width, height: 28), withAttributes: title)
    ("Then open it from Applications. It lives in the menu bar." as NSString)
        .draw(in: NSRect(x: 0, y: 88, width: size.width, height: 20), withAttributes: note)

    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

render(scale: 1, to: "bg.png")
render(scale: 2, to: "bg@2x.png")
