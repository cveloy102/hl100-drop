import AppKit

// Monochrome vector marks adapted from Lucide (ISC, see Resources/Lucide-LICENSE).
// Template images let macOS apply the menu bar's tint on light/dark wallpapers.
enum StatusIcon: String, CaseIterable {
    case bolt, radar, nodes, packet
    static let preferenceKey = "statusIcon"
    static func restored(from defaults: UserDefaults) -> StatusIcon {
        defaults.string(forKey: preferenceKey).flatMap(StatusIcon.init(rawValue:)) ?? .bolt
    }
    var title: String {
        switch self {
        case .bolt: return "Молния (Bolt)"
        case .radar: return "Радар (Radar)"
        case .nodes: return "Узлы (Nodes)"
        case .packet: return "Пакет (Packet)"
        }
    }
    func image(size: CGFloat = 18) -> NSImage {
        let image = NSImage(size: NSSize(width: size, height: size), flipped: false) { _ in
            NSGraphicsContext.saveGraphicsState()
            defer { NSGraphicsContext.restoreGraphicsState() }
            let transform = NSAffineTransform()
            transform.translateX(by: 0, yBy: size); transform.scaleX(by: size / 24, yBy: -size / 24); transform.concat()
            NSColor.black.setStroke()
            func line(_ points: [NSPoint], close: Bool = false) {
                let path = NSBezierPath(); path.lineWidth = 1.9; path.lineCapStyle = .round; path.lineJoinStyle = .round
                path.move(to: points[0]); points.dropFirst().forEach { path.line(to: $0) }
                if close { path.close() }; path.stroke()
            }
            func circle(_ x: CGFloat, _ y: CGFloat, _ radius: CGFloat) {
                let path = NSBezierPath(ovalIn: NSRect(x: x-radius, y: y-radius, width: radius*2, height: radius*2))
                path.lineWidth = 1.9; path.stroke()
            }
            func arc(_ radius: CGFloat, _ from: CGFloat, _ to: CGFloat) {
                let path = NSBezierPath(); path.lineWidth = 1.9; path.lineCapStyle = .round
                path.appendArc(withCenter: NSPoint(x: 12, y: 12), radius: radius, startAngle: from, endAngle: to, clockwise: false)
                path.stroke()
            }
            switch self {
            case .bolt:
                line([NSPoint(x: 15, y: 2), NSPoint(x: 3, y: 14), NSPoint(x: 10, y: 14),
                      NSPoint(x: 9, y: 22), NSPoint(x: 21, y: 10), NSPoint(x: 14, y: 10)], close: true)
            case .radar:
                arc(10, 339, 584); arc(10, 240, 315)
                arc(6, 57, 315); circle(12, 12, 2)
                line([NSPoint(x: 13.41, y: 10.59), NSPoint(x: 19.07, y: 4.93)])
            case .nodes:
                line([NSPoint(x: 10.586, y: 5.414), NSPoint(x: 5.414, y: 10.586)])
                line([NSPoint(x: 18.586, y: 13.414), NSPoint(x: 13.414, y: 18.586)])
                line([NSPoint(x: 6, y: 12), NSPoint(x: 18, y: 12)])
                for point in [NSPoint(x: 12, y: 4), NSPoint(x: 4, y: 12), NSPoint(x: 20, y: 12), NSPoint(x: 12, y: 20)] {
                    circle(point.x, point.y, 2)
                }
            case .packet:
                line([NSPoint(x: 12, y: 2), NSPoint(x: 21, y: 7), NSPoint(x: 21, y: 17),
                      NSPoint(x: 12, y: 22), NSPoint(x: 3, y: 17), NSPoint(x: 3, y: 7)], close: true)
                line([NSPoint(x: 3, y: 7), NSPoint(x: 12, y: 12), NSPoint(x: 21, y: 7)])
                line([NSPoint(x: 12, y: 12), NSPoint(x: 12, y: 22)])
            }
            return true
        }
        image.isTemplate = true; image.accessibilityDescription = title
        return image
    }
}
