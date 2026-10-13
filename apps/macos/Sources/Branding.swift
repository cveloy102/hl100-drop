import AppKit

enum Branding: String, CaseIterable {
    case lightning, large, terminal
    static let preferenceKey = "branding"
    static func restored(from defaults: UserDefaults) -> Branding {
        defaults.string(forKey: preferenceKey).flatMap(Branding.init(rawValue:)) ?? .lightning
    }
    var title: String {
        switch self {
        case .lightning: return "Молния + DROP"
        case .large: return "DROP крупно"
        case .terminal: return "Терминал"
        }
    }
}

final class BrandHeader: NSView {
    init(style: Branding) {
        super.init(frame: .zero)
        heightAnchor.constraint(equalToConstant: 68).isActive = true
        widthAnchor.constraint(greaterThanOrEqualToConstant: 180).isActive = true
        update(style)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
    func update(_ style: Branding) {
        subviews.forEach { $0.removeFromSuperview() }
        let word = Style.label("DROP", size: style == .large ? 44 : 32)
        word.font = .monospacedSystemFont(ofSize: style == .large ? 44 : 32, weight: .medium)
        let prefix = Style.label(style == .terminal ? "~/hl100/" : "HL100_", size: 11, color: Style.muted)
        let content: NSView
        switch style {
        case .lightning:
            let icon = NSImageView(); icon.image = StatusIcon.bolt.image(size: 30); icon.contentTintColor = Style.red
            icon.widthAnchor.constraint(equalToConstant: 30).isActive = true
            icon.heightAnchor.constraint(equalToConstant: 30).isActive = true
            let text = Style.stack([word, prefix], spacing: 0)
            let row = NSStackView(views: [icon, text]); row.spacing = 12; row.alignment = .centerY
            content = row
        case .large:
            let dot = Style.label(".", size: 44, color: Style.red)
            let row = NSStackView(views: [word, dot]); row.spacing = 0; row.alignment = .firstBaseline
            content = Style.stack([prefix, row], spacing: 0)
        case .terminal:
            let cursor = Style.label("_", size: 32, color: Style.red)
            let row = NSStackView(views: [word, cursor]); row.spacing = 0; row.alignment = .firstBaseline
            content = Style.stack([prefix, row], spacing: 0)
        }
        content.translatesAutoresizingMaskIntoConstraints = false; addSubview(content)
        NSLayoutConstraint.activate([content.leadingAnchor.constraint(equalTo: leadingAnchor),
                                     content.centerYAnchor.constraint(equalTo: centerYAnchor),
                                     content.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor)])
        setAccessibilityLabel("HL100 DROP · \(style.title)")
    }
}
