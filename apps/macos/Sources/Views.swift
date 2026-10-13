import AppKit

enum Style {
    static let background = NSColor(srgbRed: 0.063, green: 0.063, blue: 0.060, alpha: 1)
    static let card = NSColor(srgbRed: 0.095, green: 0.095, blue: 0.090, alpha: 1)
    static let paper = NSColor(srgbRed: 0.91, green: 0.90, blue: 0.85, alpha: 1)
    static let muted = NSColor(srgbRed: 0.62, green: 0.62, blue: 0.57, alpha: 1)
    static let red = NSColor(srgbRed: 0.94, green: 0.25, blue: 0.23, alpha: 1)
    static let border = NSColor(srgbRed: 0.24, green: 0.24, blue: 0.21, alpha: 1)
    static func label(_ text: String, size: CGFloat = 12, color: NSColor = paper) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: text)
        field.font = .monospacedSystemFont(ofSize: size, weight: .regular)
        field.textColor = color
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return field
    }
    static func stack(_ views: [NSView] = [], spacing: CGFloat = 10) -> NSStackView {
        let stack = NSStackView(views: views)
        stack.orientation = .vertical; stack.alignment = .leading; stack.spacing = spacing
        return stack
    }
    static func pin(_ child: NSView, in parent: NSView, inset: CGFloat = 0) {
        child.translatesAutoresizingMaskIntoConstraints = false; parent.addSubview(child)
        NSLayoutConstraint.activate([
            child.leadingAnchor.constraint(equalTo: parent.leadingAnchor, constant: inset),
            child.trailingAnchor.constraint(equalTo: parent.trailingAnchor, constant: -inset),
            child.topAnchor.constraint(equalTo: parent.topAnchor, constant: inset),
            child.bottomAnchor.constraint(equalTo: parent.bottomAnchor, constant: -inset)
        ])
    }
}

final class ActionButton: NSButton {
    var actionHandler: (() -> Void)?
    init(_ title: String, accent: Bool = false, action: @escaping () -> Void) {
        super.init(frame: .zero)
        self.title = title; actionHandler = action; target = self; self.action = #selector(invoke)
        bezelStyle = .regularSquare; font = .monospacedSystemFont(ofSize: 11, weight: .medium)
        contentTintColor = accent ? Style.red : Style.paper
        heightAnchor.constraint(greaterThanOrEqualToConstant: 28).isActive = true
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
    func useAccentFill() {
        isBordered = false; wantsLayer = true
        layer?.backgroundColor = Style.red.cgColor; layer?.cornerRadius = 4
        contentTintColor = Style.paper
    }
    @objc private func invoke() { actionHandler?() }
}

func fileURLs(from pasteboard: NSPasteboard) -> [URL] {
    (pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? [])
        .filter { $0.isFileURL }
}

// Used both by the status-bar hit area and the panel's drop zone. No activation
// or synthetic mouse events: AppKit keeps ownership of the Finder drag session.
class FileDropTarget: NSView {
    var onEnter: (() -> Void)?
    var onDrop: (([URL]) -> Void)?
    var onClick: (() -> Void)?
    override init(frame: NSRect) {
        super.init(frame: frame); registerForDraggedTypes([.fileURL])
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
    override func mouseDown(with event: NSEvent) { onClick?() }
    func acceptDrag(from pasteboard: NSPasteboard) -> NSDragOperation {
        guard !fileURLs(from: pasteboard).isEmpty else { return [] }
        onEnter?(); return .copy
    }
    func acceptDrop(from pasteboard: NSPasteboard) -> Bool {
        let urls = fileURLs(from: pasteboard)
        guard !urls.isEmpty else { return false }
        onDrop?(urls); return true
    }
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard sender.draggingSourceOperationMask.contains(.copy) else { return [] }
        return acceptDrag(from: sender.draggingPasteboard)
    }
    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        !fileURLs(from: sender.draggingPasteboard).isEmpty && sender.draggingSourceOperationMask.contains(.copy) ? .copy : []
    }
    override func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool {
        !fileURLs(from: sender.draggingPasteboard).isEmpty && sender.draggingSourceOperationMask.contains(.copy)
    }
    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool { acceptDrop(from: sender.draggingPasteboard) }
}

final class DropZone: FileDropTarget {
    let label = Style.label("Перетащи файлы или папки сюда", size: 11, color: Style.muted)
    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true; layer?.borderWidth = 1; layer?.borderColor = Style.border.cgColor
        layer?.cornerRadius = 5
        label.alignment = .center
        Style.pin(label, in: self, inset: 12)
        heightAnchor.constraint(greaterThanOrEqualToConstant: 46).isActive = true
        setAccessibilityLabel("Область для файлов и папок")
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
    override func hitTest(_ point: NSPoint) -> NSView? {
        super.hitTest(point) == nil ? nil : self
    }
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        let operation = super.draggingEntered(sender)
        if operation == .copy { layer?.borderColor = Style.red.cgColor }
        return operation
    }
    override func draggingExited(_ sender: NSDraggingInfo?) { layer?.borderColor = Style.border.cgColor }
    override func draggingEnded(_ sender: NSDraggingInfo) { layer?.borderColor = Style.border.cgColor }
}

final class PeerCard: FileDropTarget {
    let peer: Peer
    init(peer: Peer, selected: Bool, select: @escaping (Peer) -> Void, send: @escaping (Peer, [URL]) -> Void) {
        self.peer = peer
        super.init(frame: .zero)
        wantsLayer = true; layer?.backgroundColor = Style.card.cgColor
        layer?.borderWidth = 1; layer?.borderColor = (selected ? Style.red : Style.border).cgColor
        layer?.cornerRadius = 5
        let button = ActionButton(peer.name + (selected ? "  ↗" : ""), accent: selected) { select(peer) }
        button.isBordered = false; button.alignment = .left
        let meta = Style.label("\(peer.platform.uppercased()) / \(peer.address)", size: 11, color: Style.muted)
        let content = Style.stack([button, meta], spacing: 2)
        Style.pin(content, in: self, inset: 7)
        for view in [button, meta] as [NSView] { view.widthAnchor.constraint(equalTo: content.widthAnchor).isActive = true }
        onClick = { select(peer) }; onDrop = { urls in send(peer, urls) }
        setAccessibilityLabel("Получатель \(peer.name)\(selected ? ", выбран" : "")")
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
}

final class TransferProgress: NSView {
    var value: Double = 0 { didSet { needsDisplay = true; setAccessibilityValue(NSNumber(value: value)) } }
    override init(frame: NSRect) {
        super.init(frame: frame)
        heightAnchor.constraint(equalToConstant: 4).isActive = true
        setAccessibilityElement(true); setAccessibilityRole(.progressIndicator)
        setAccessibilityLabel("Прогресс передачи, проценты")
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
    override func draw(_ dirtyRect: NSRect) {
        Style.border.setFill(); bounds.fill()
        Style.red.setFill()
        NSRect(x: 0, y: 0, width: bounds.width * CGFloat(min(100, max(0, value))) / 100, height: bounds.height).fill()
    }
}

final class TransferFile: NSView {
    let path: String
    private var url: URL?
    private let button: ActionButton
    private let reveal: (URL) -> Void
    init(entry: [String: Any], reveal: @escaping (URL) -> Void) {
        path = entry["path"] as? String ?? "Файл"
        self.reveal = reveal
        button = ActionButton((path as NSString).lastPathComponent) {}
        super.init(frame: .zero)
        button.isBordered = false; button.alignment = .left; button.isEnabled = false
        button.lineBreakMode = .byTruncatingMiddle
        button.image = NSImage(systemSymbolName: entry["directory"] as? Bool == true ? "folder" : "doc", accessibilityDescription: nil)
        button.imagePosition = .imageLeading
        button.toolTip = path
        button.actionHandler = { [weak self] in self?.revealFile() }
        let size = Style.label((entry["directory"] as? Bool == true ? "Папка · " : "") + bytes(entry["size"] as? Double ?? 0), size: 10, color: Style.muted)
        size.alignment = .right
        size.setContentHuggingPriority(.required, for: .horizontal)
        size.setContentCompressionResistancePriority(.required, for: .horizontal)
        let line = NSStackView(views: [button, size]); line.spacing = 8
        Style.pin(line, in: self)
        button.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
    func complete(_ event: [String: Any]) {
        guard let path = event["local_path"] as? String else { return }
        url = URL(fileURLWithPath: path); button.title = url!.lastPathComponent; button.isEnabled = true
        button.contentTintColor = Style.paper
        button.toolTip = "Показать в Finder: \(path)"
        button.setAccessibilityLabel("Показать \(url!.lastPathComponent) в Finder")
    }
    @discardableResult func revealFile() -> Bool {
        guard let url = url else { return false }; reveal(url); return true
    }
}

final class TransferRow: NSView {
    let id: String
    var finished = false
    let initialEvent: [String: Any]
    private var progressEvent: [String: Any]?
    private var completionEvent: [String: Any]?
    private var fileEvents: [[String: Any]] = []
    var replayEvents: [[String: Any]] { fileEvents + [progressEvent, completionEvent].compactMap { $0 } }
    private var fileViews: [TransferFile] = []
    private let total: Double
    private let detail = Style.label("Подготовка…", size: 11, color: Style.muted)
    private let progress = TransferProgress(frame: .zero)
    private let cancel: ActionButton
    init(event: [String: Any], reveal: @escaping (URL) -> Void, cancelAction: @escaping (String) -> Void) {
        initialEvent = event; id = event["transfer"] as? String ?? ""
        total = event["total"] as? Double ?? 0
        let transferID = id
        cancel = ActionButton("Отмена") { cancelAction(transferID) }
        cancel.toolTip = "Отменить передачу"; cancel.setAccessibilityLabel("Отменить передачу")
        cancel.isBordered = false
        super.init(frame: .zero)
        wantsLayer = true; layer?.backgroundColor = Style.card.cgColor; layer?.cornerRadius = 5
        let direction = event["direction"] as? String == "send" ? "↑" : "↓"
        let title = Style.label("\(direction) \(event["name"] as? String ?? "Устройство")", size: 11, color: Style.muted)
        let header = NSStackView(views: [title, NSView(), cancel]); header.spacing = 4
        let count = event["display_count"] as? Int ?? event["files"] as? Int ?? 0
        detail.stringValue = "\(count) объектов · \(bytes(total))"
        let entries = event["display_entries"] as? [[String: Any]] ?? event["entries"] as? [[String: Any]] ?? []
        let files = Style.stack(spacing: 1)
        for entry in entries {
            let file = TransferFile(entry: entry, reveal: reveal)
            if entry["local_path"] is String { file.complete(entry) }
            fileViews.append(file); files.addArrangedSubview(file)
            file.widthAnchor.constraint(equalTo: files.widthAnchor).isActive = true
        }
        if count > entries.count {
            let more = Style.label("Ещё \(count - entries.count) объектов · см. папку полученных", size: 10, color: Style.muted)
            files.addArrangedSubview(more); more.widthAnchor.constraint(equalTo: files.widthAnchor).isActive = true
        }
        let content = Style.stack([header, files, detail, progress], spacing: 5)
        Style.pin(content, in: self, inset: 8)
        for view in [header, files, detail, progress] as [NSView] { view.widthAnchor.constraint(equalTo: content.widthAnchor).isActive = true }
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
    @discardableResult func revealFile(at index: Int) -> Bool {
        guard fileViews.indices.contains(index) else { return false }
        return fileViews[index].revealFile()
    }
    func process(_ event: [String: Any]) {
        switch event["type"] as? String {
        case "file":
            fileEvents.append(event)
            fileViews.first { $0.path == event["path"] as? String }?.complete(event)
        case "progress":
            progressEvent = event
            let done = event["done"] as? Double ?? 0, total = event["total"] as? Double ?? 0
            progress.value = total > 0 ? min(100, done / total * 100) : 100
            let remaining = min(864000, max(0, event["remaining"] as? Double ?? 0))
            detail.stringValue = "\(bytes(done)) / \(bytes(total))\n\(bytes(event["speed"] as? Double ?? 0))/с · осталось \(Int(remaining)) с"
        case "complete":
            completionEvent = event; finished = true; cancel.isHidden = true
            switch event["status"] as? String {
            case "done": detail.stringValue = "Готово ✓ · \(bytes(total))"; progress.value = 100
            case "cancelled": detail.stringValue = "Отменено · \(bytes(total))"
            default: detail.stringValue = "Ошибка: \(event["message"] as? String ?? "Неизвестная ошибка")"
            }
        default: break
        }
    }
}

private final class FlippedView: NSView { override var isFlipped: Bool { true } }
func scrollingColumn(_ stack: NSStackView) -> NSScrollView {
    let scroll = NSScrollView(); scroll.hasVerticalScroller = true; scroll.autohidesScrollers = true; scroll.drawsBackground = false
    let document = FlippedView(); scroll.documentView = document; document.translatesAutoresizingMaskIntoConstraints = false
    Style.pin(stack, in: document, inset: 1)
    document.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor).isActive = true
    for view in stack.arrangedSubviews { view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true }
    return scroll
}
