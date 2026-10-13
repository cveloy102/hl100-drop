import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate, NSMenuDelegate {
    private let engine = Engine()
    private let networkActivity = NetworkActivity()
    private var discoveryState = "starting"
    private var settings = Settings()
    private let popover = NSPopover()
    private let panelController = NSViewController()
    private let mainBody = Style.stack()
    private let bodyHost = NSView()
    private var settingsView: SettingsView?
    private var statusDrop: FileDropTarget?
    private var escapeMonitor: Any?
    private var localMouseMonitor: Any?, globalMouseMonitor: Any?
    private var outsideDown = false, mouseDragged = false, menuTracking = false
    private var pinned = false
    private var statusIcon: StatusIcon = .bolt
    private var branding: Branding = .lightning
    private var brandHeader: BrandHeader!
    private var desktopBrand: BrandHeader?
    private var loginLaunch = false
    private lazy var login: LoginLaunchManaging = CommandLine.arguments.contains("--ui-smoke") ? PreviewLoginLaunch() : LoginLaunch()
    private var pinButton: ActionButton!
    private var desktopWindow: NSWindow?
    private let desktopTransfers = Style.stack()
    private var desktopRows: [TransferRow] = []
    private var smokeRevealedURL: URL?
    private var selectedPeerID: String?
    private var pendingURLs: [URL] = []
    private let dropZone = DropZone(frame: .zero)
    private var sendButton: ActionButton!
    private var window: NSWindow? { panelController.view.window }
    private var statusItem: NSStatusItem!
    private let peerList = Style.stack()
    private let transferList = Style.stack()
    private let identity = Style.label("", color: Style.muted)
    private let status = Style.label("Запуск…", color: Style.muted)
    private let updateText = Style.label("", color: Style.red)
    private var updateBanner: NSStackView!
    private var updateButton: ActionButton!
    private var rows: [TransferRow] = []
    private var peers: [Peer] = []
    private var timer: Timer?
    private var polling = false, running = false, busy = false, starting = false
    private var checking = false, downloading = false, terminating = false
    private var release: Release?

    func applicationWillFinishLaunching(_ notification: Notification) {
        loginLaunch = LoginLaunch.isLoginLaunch(arguments: CommandLine.arguments, event: NSAppleEventManager.shared().currentAppleEvent)
    }
    func applicationDidFinishLaunching(_ notification: Notification) {
        loginLaunch = loginLaunch || LoginLaunch.isLoginLaunch(arguments: CommandLine.arguments, event: NSAppleEventManager.shared().currentAppleEvent)
        if !CommandLine.arguments.contains("--ui-smoke") {
            pinned = UserDefaults.standard.bool(forKey: "panelPinned")
            statusIcon = StatusIcon.restored(from: .standard)
            branding = Branding.restored(from: .standard)
        }
        if CommandLine.arguments.contains("--ui-brand-large") { branding = .large }
        if CommandLine.arguments.contains("--ui-brand-terminal") { branding = .terminal }
        createMenu(); createPanel(); createStatusItem()
        if !CommandLine.arguments.contains("--ui-smoke") { installMouseMonitors() }
        if CommandLine.arguments.contains("--ui-smoke") { renderSmoke(); return }
        do { settings = try Settings.load() }
        catch { alert("Не удалось прочитать настройки", detail: "\(error.localizedDescription)\nИсходный файл сохранён. Выберите настройки заново.") }
        if !loginLaunch { showWindow() }
        if settings.configured {
            start(settings) { [weak self] failure in
                if let failure = failure { self?.alert("Не удалось запустить приём", detail: failure.localizedDescription); self?.showSettings() }
            }
        } else { showSettings() }
        if settings.checkUpdates { checkUpdates(manual: false) }
        timer = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in self?.poll() }
        if let timer = timer { RunLoop.main.add(timer, forMode: .common) }
    }

    private func createMenu() {
        let root = NSMenu()
        let appItem = NSMenuItem(); root.addItem(appItem)
        let app = NSMenu(); appItem.submenu = app
        app.addItem(withTitle: "О HL100 Drop", action: #selector(about), keyEquivalent: "")
        app.addItem(withTitle: "Открыть окно передач", action: #selector(showDesktopWindow), keyEquivalent: "1")
        app.addItem(withTitle: "Настройки…", action: #selector(showSettings), keyEquivalent: ",")
        app.addItem(withTitle: "Проверить обновления…", action: #selector(manualUpdate), keyEquivalent: "")
        app.addItem(.separator())
        app.addItem(withTitle: "Скрыть HL100 Drop", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        app.addItem(withTitle: "Завершить HL100 Drop", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        for item in app.items where item.action != #selector(NSApplication.terminate(_:)) && item.action != #selector(NSApplication.hide(_:)) {
            item.target = self
        }
        let editItem = NSMenuItem(); root.addItem(editItem)
        let edit = NSMenu(title: "Правка"); editItem.submenu = edit
        edit.addItem(withTitle: "Вырезать", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Копировать", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Вставить", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Выбрать всё", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        NSApp.mainMenu = root
    }

    private func createPanel() {
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 430, height: 680))
        root.wantsLayer = true; root.layer?.backgroundColor = Style.background.cgColor
        root.appearance = NSAppearance(named: .darkAqua)
        panelController.view = root
        popover.contentViewController = panelController
        // App-controlled dismissal preserves Finder drag sessions and the Pin mode.
        popover.behavior = .applicationDefined; popover.animates = false
        popover.contentSize = NSSize(width: 430, height: 680)
        brandHeader = BrandHeader(style: branding)
        let more = ActionButton("•••") {}
        more.isBordered = false; more.toolTip = "Меню HL100 Drop"
        more.setAccessibilityLabel("Меню: настройки, окно передач и обновления")
        more.actionHandler = { [weak self, weak more] in
            guard let self = self, let more = more else { return }
            self.menuTracking = true
            defer { self.menuTracking = false }
            self.panelMenu().popUp(positioning: nil, at: NSPoint(x: 0, y: more.bounds.minY), in: more)
        }
        let header = NSStackView(views: [brandHeader!, NSView(), more]); header.spacing = 6; header.alignment = .centerY
        header.heightAnchor.constraint(equalToConstant: 68).isActive = true
        let greeting = Style.label("Hello, nearby_", size: 22)
        identity.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        identity.isSelectable = true
        let peerScroll = scrollingColumn(peerList)
        peerScroll.heightAnchor.constraint(equalToConstant: 134).isActive = true
        let transferScroll = scrollingColumn(transferList)
        transferScroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 65).isActive = true
        transferScroll.setContentHuggingPriority(.defaultLow, for: .vertical)
        sendButton = ActionButton("ВЫБЕРИ ПОЛУЧАТЕЛЯ", accent: true) { [weak self] in self?.chooseOrSend() }
        sendButton.useAccentFill()
        dropZone.onClick = { [weak self] in self?.chooseOrSend() }
        dropZone.onDrop = { [weak self] urls in self?.dropIntoPanel(urls) }
        updateButton = ActionButton("ЗАГРУЗИТЬ ОБНОВЛЕНИЕ →", accent: true) { [weak self] in self?.installUpdate() }
        updateBanner = Style.stack([updateText, updateButton], spacing: 4); updateBanner.isHidden = true
        for view in [updateText, updateButton!] as [NSView] { view.widthAnchor.constraint(equalTo: updateBanner.widthAnchor).isActive = true }
        for view in [greeting, identity, updateBanner!, Style.label("РЯДОМ / УСТРОЙСТВА", size: 11, color: Style.red), peerScroll,
                     dropZone, sendButton!, Style.label("ПЕРЕДАЧИ", size: 11, color: Style.red), transferScroll, status] as [NSView] {
            mainBody.addArrangedSubview(view); view.widthAnchor.constraint(equalTo: mainBody.widthAnchor).isActive = true
        }
        mainBody.spacing = 10
        Style.pin(mainBody, in: bodyHost)
        let folder = ActionButton("ПОЛУЧЕННЫЕ ФАЙЛЫ ↗") { [weak self] in self?.openReceiveFolder() }
        folder.isBordered = false
        pinButton = ActionButton("Pin") { [weak self] in self?.togglePin() }
        pinButton.isBordered = false; refreshPinButton()
        let footer = NSStackView(views: [pinButton!, NSView(), folder])
        footer.spacing = 8
        footer.heightAnchor.constraint(equalToConstant: 28).isActive = true
        let content = Style.stack([header, bodyHost, footer], spacing: 14)
        Style.pin(content, in: root, inset: 20)
        for view in [header, bodyHost, footer] as [NSView] { view.widthAnchor.constraint(equalTo: content.widthAnchor).isActive = true }
        drawPeers(); drawEmptyTransfers(); refreshSendButton()
        if CommandLine.arguments.contains("--ui-smoke") { return }
        escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            if event.keyCode == 53, let self = self, self.popover.isShown, !self.pinned, self.window?.attachedSheet == nil {
                self.closePanel(); return nil
            }
            return event
        }
    }

    private func createStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        guard let button = statusItem.button else { return }
        button.image = statusIcon.image()
        button.toolTip = "HL100 Drop — приём в фоне. Перетащи сюда файлы."
        let target = FileDropTarget(frame: button.bounds)
        target.autoresizingMask = [.width, .height]
        target.onClick = { [weak self] in self?.togglePanel() }
        target.onEnter = { [weak self] in self?.showPanel(activate: false) }
        target.onDrop = { [weak self] urls in self?.stageFiles(urls) }
        target.setAccessibilityElement(true); target.setAccessibilityRole(.button)
        target.setAccessibilityLabel("Открыть HL100 Drop или перетащить файлы")
        button.addSubview(target); statusDrop = target
        let menu = panelMenu()
        menu.insertItem(withTitle: "Открыть шторку", action: #selector(showWindow), keyEquivalent: "", at: 0)
        menu.items.first?.target = self
        target.menu = menu
    }

    private func panelMenu() -> NSMenu {
        let menu = NSMenu(); menu.delegate = self
        menu.addItem(withTitle: "Открыть окно передач", action: #selector(showDesktopWindow), keyEquivalent: "")
        menu.addItem(withTitle: "Полученные файлы", action: #selector(openReceiveFolder), keyEquivalent: "")
        menu.addItem(.separator())
        let iconItem = NSMenuItem(title: "Значок", action: nil, keyEquivalent: "")
        let icons = NSMenu(title: "Значок"); icons.delegate = self
        for icon in StatusIcon.allCases {
            let item = NSMenuItem(title: icon.title, action: #selector(selectStatusIcon(_:)), keyEquivalent: "")
            item.target = self; item.representedObject = icon.rawValue; item.image = icon.image()
            item.state = icon == statusIcon ? .on : .off; icons.addItem(item)
        }
        iconItem.submenu = icons; menu.addItem(iconItem)
        menu.addItem(withTitle: "Настройки…", action: #selector(showSettings), keyEquivalent: "")
        menu.addItem(withTitle: "Проверить обновления…", action: #selector(manualUpdate), keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(withTitle: "Завершить", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "")
        for item in menu.items where item.action != #selector(NSApplication.terminate(_:)) { item.target = self }
        return menu
    }
    @objc private func selectStatusIcon(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String, let icon = StatusIcon(rawValue: raw) else { return }
        statusIcon = icon; statusItem.button?.image = icon.image()
        if !CommandLine.arguments.contains("--ui-smoke") { UserDefaults.standard.set(icon.rawValue, forKey: StatusIcon.preferenceKey) }
    }
    func menuWillOpen(_ menu: NSMenu) {
        menuTracking = true; outsideDown = false
        for item in menu.items {
            if let raw = item.representedObject as? String, let icon = StatusIcon(rawValue: raw) {
                item.state = icon == statusIcon ? .on : .off
            }
        }
    }
    func menuDidClose(_ menu: NSMenu) {
        outsideDown = false
        DispatchQueue.main.async { [weak self] in self?.menuTracking = false }
    }
    private func refreshPinButton() {
        pinButton.title = pinned ? "Pinned" : "Pin"
        pinButton.image = NSImage(systemSymbolName: pinned ? "pin.fill" : "pin", accessibilityDescription: nil)
        pinButton.imagePosition = .imageLeading; pinButton.contentTintColor = pinned ? Style.red : Style.muted
        pinButton.toolTip = pinned ? "Закреплено: закрыть повторным нажатием на значок Drop" : "Закрепить шторку. Сейчас она закрывается по клику снаружи"
        pinButton.setAccessibilityValue(pinned ? "Закреплено" : "Не закреплено")
    }
    private func togglePin() {
        pinned.toggle(); refreshPinButton()
        if !CommandLine.arguments.contains("--ui-smoke") { UserDefaults.standard.set(pinned, forKey: "panelPinned") }
    }
    private func installMouseMonitors() {
        let mask: NSEvent.EventTypeMask = [.leftMouseDown, .rightMouseDown, .leftMouseUp, .rightMouseUp, .leftMouseDragged, .rightMouseDragged]
        localMouseMonitor = NSEvent.addLocalMonitorForEvents(matching: mask) { [weak self] event in
            self?.handleMouse(event); return event
        }
        globalMouseMonitor = NSEvent.addGlobalMonitorForEvents(matching: mask) { [weak self] event in self?.handleMouse(event) }
    }
    private func handleMouse(_ event: NSEvent) {
        guard popover.isShown, !menuTracking, event.window?.level != .popUpMenu else { outsideDown = false; return }
        let point = NSEvent.mouseLocation
        let inPanel = window?.frame.contains(point) == true
        let inIcon = statusItem.button.flatMap { button in button.window?.convertToScreen(button.convert(button.bounds, to: nil)).contains(point) } == true
        if event.type == .leftMouseDown || event.type == .rightMouseDown {
            outsideDown = !inPanel && !inIcon; mouseDragged = false
        } else if event.type == .leftMouseDragged || event.type == .rightMouseDragged {
            mouseDragged = true
        } else {
            dismissAfterOutsideClick(outside: outsideDown && !inPanel && !inIcon, dragged: mouseDragged)
            outsideDown = false
        }
    }
    private func dismissAfterOutsideClick(outside: Bool, dragged: Bool) {
        guard outside, !dragged, !pinned, !menuTracking, NSApp.modalWindow == nil else { return }
        closePanel()
    }
    private func revealFile(_ url: URL) {
        if CommandLine.arguments.contains("--ui-smoke") { smokeRevealedURL = url; return }
        guard FileManager.default.fileExists(atPath: url.path) else {
            alert("Файл перемещён или удалён", detail: url.path); return
        }
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }
    private func makeTransferRow(_ event: [String: Any]) -> TransferRow {
        TransferRow(event: event, reveal: { [weak self] url in self?.revealFile(url) },
                    cancelAction: { [weak self] id in self?.cancel(id) })
    }
    private func addDesktopRow(from row: TransferRow) {
        let copy = makeTransferRow(row.initialEvent)
        row.replayEvents.forEach { copy.process($0) }
        desktopRows.insert(copy, at: 0); desktopTransfers.insertArrangedSubview(copy, at: 0)
        copy.widthAnchor.constraint(equalTo: desktopTransfers.widthAnchor).isActive = true
    }
    @objc private func showDesktopWindow() {
        if desktopWindow == nil {
            let desktop = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 700, height: 650),
                                   styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
            desktop.title = "HL100 Drop — Передачи"; desktop.isReleasedWhenClosed = false
            desktop.minSize = NSSize(width: 480, height: 400)
            desktop.appearance = NSAppearance(named: .darkAqua); desktop.backgroundColor = Style.background
            let folder = ActionButton("Полученные файлы ↗") { [weak self] in self?.openReceiveFolder() }
            folder.isBordered = false
            let scroll = scrollingColumn(desktopTransfers)
            let brand = BrandHeader(style: branding); desktopBrand = brand
            let content = Style.stack([brand, Style.label("Hello, nearby_", size: 22),
                                       Style.label("Файлы и папки · нажми, чтобы выделить в Finder", size: 11, color: Style.muted), scroll, folder], spacing: 14)
            Style.pin(content, in: desktop.contentView!, inset: 24)
            scroll.widthAnchor.constraint(equalTo: content.widthAnchor).isActive = true
            desktopWindow = desktop; desktop.center()
            for row in rows.reversed() { addDesktopRow(from: row) }
        }
        desktopWindow?.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func showWindow() { showPanel(activate: true) }
    private func showPanel(activate: Bool) {
        guard let button = statusItem.button else { return }
        if !popover.isShown { popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY) }
        window?.level = .floating; window?.hidesOnDeactivate = false
        if activate { NSApp.activate(ignoringOtherApps: true); window?.makeKey() }
    }
    private func closePanel() {
        guard !starting, window?.attachedSheet == nil else { return }
        popover.performClose(nil)
    }
    private func togglePanel() { if popover.isShown { closePanel() } else { showPanel(activate: true) } }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool { showWindow(); return true }
    @objc private func openReceiveFolder() { NSWorkspace.shared.open(URL(fileURLWithPath: settings.folder)) }
    @objc private func about() { alert("HL100 Drop \(Updates.current)", detail: "Нативная передача файлов по локальной сети.\nmacOS 12+ / Intel + Apple Silicon") }

    private func start(_ next: Settings, completion: @escaping (Error?) -> Void) {
        guard !starting else { completion(DropError(message: "Дождитесь запуска")); return }
        starting = true
        networkActivity.setListening(true)
        DispatchQueue.global(qos: .userInitiated).async {
            let folderResult = Result { try next.validateFolder() }
            DispatchQueue.main.async { [weak self] in
                guard let self = self else { return }
                if case .failure(let error) = folderResult { self.starting = false; self.networkActivity.setListening(self.running); completion(error); return }
                self.engine.perform(["op": "start", "config": ["id": next.id, "name": next.name, "folder": next.folder]]) { [weak self] result in
                    guard let self = self else { return }
                    self.starting = false
                    switch result {
                    case .success:
                        self.running = true; self.settings = next
                        self.identity.stringValue = "• \(next.name) / приём включён"
                        self.status.stringValue = "Готов к отправке и автоматическому приёму"; self.refreshSendButton()
                        completion(nil)
                    case .failure(let error): self.running = false; self.networkActivity.setListening(false); self.status.stringValue = error.localizedDescription; completion(error)
                    }
                }
            }
        }
    }

    @objc private func showSettings() {
        showWindow()
        guard settingsView == nil, !starting else { return }
        if busy { alert("Сначала завершите передачи", detail: "Настройки можно изменить после завершения отправки и приёма."); return }
        let view = SettingsView(settings: settings, login: login, branding: branding, changeBranding: { [weak self] style in self?.setBranding(style) }, save: { [weak self] next, completion in
            guard let self = self else { return }
            self.start(next) { failure in
                if let failure = failure { completion(failure); return }
                do { try next.save(); completion(nil) } catch { completion(error) }
            }
        }, back: { [weak self] in self?.showMainBody() }, check: { [weak self] in self?.manualUpdate() },
        quit: { NSApp.terminate(nil) })
        mainBody.isHidden = true; settingsView = view; Style.pin(view, in: bodyHost)
    }
    private func setBranding(_ style: Branding) {
        branding = style; brandHeader.update(style); desktopBrand?.update(style)
        if !CommandLine.arguments.contains("--ui-smoke") { UserDefaults.standard.set(style.rawValue, forKey: Branding.preferenceKey) }
    }
    private func showMainBody() {
        guard !starting else { return }
        settingsView?.removeFromSuperview(); settingsView = nil; mainBody.isHidden = false
        if !running { status.stringValue = "Открой настройки, чтобы запустить приём" }
    }

    private func poll() {
        guard running, !polling, !starting, !terminating else { return }
        polling = true
        engine.perform(["op": "poll"]) { [weak self] result in
            guard let self = self else { return }
            self.polling = false
            switch result {
            case .success(let value): self.consume(value)
            case .failure(let error): self.status.stringValue = error.localizedDescription
            }
        }
    }

    private func consume(_ value: [String: Any]) {
        busy = value["busy"] as? Bool ?? false
        networkActivity.setTransferring(busy)
        let newPeers = (value["peers"] as? [[String: Any]] ?? []).compactMap(Peer.init)
            .sorted { $0.name == $1.name ? $0.id < $1.id : $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        if newPeers != peers { peers = newPeers; drawPeers() }
        for event in value["events"] as? [[String: Any]] ?? [] {
            if event["type"] as? String == "discovery" {
                let next = event["status"] as? String ?? "retrying"
                if next != "active" || discoveryState != "active" {
                    status.stringValue = event["message"] as? String ?? "Восстанавливаю поиск устройств…"
                }
                discoveryState = next
            }
            if event["type"] as? String == "error" { status.stringValue = event["message"] as? String ?? "Ошибка" }
            guard let id = event["transfer"] as? String else { continue }
            if event["type"] as? String == "transfer", !rows.contains(where: { $0.id == id }) {
                if rows.isEmpty { clear(transferList) }
                let row = makeTransferRow(event)
                rows.insert(row, at: 0); transferList.insertArrangedSubview(row, at: 0)
                row.widthAnchor.constraint(equalTo: transferList.widthAnchor).isActive = true
                if desktopWindow != nil { addDesktopRow(from: row) }
            }
            rows.first(where: { $0.id == id })?.process(event)
            desktopRows.first(where: { $0.id == id })?.process(event)
        }
        while rows.count > 100, let index = rows.lastIndex(where: { $0.finished }) {
            let row = rows.remove(at: index); transferList.removeArrangedSubview(row); row.removeFromSuperview()
            if let index = desktopRows.firstIndex(where: { $0.id == row.id }) {
                let copy = desktopRows.remove(at: index); desktopTransfers.removeArrangedSubview(copy); copy.removeFromSuperview()
            }
        }
        statusItem.button?.toolTip = busy ? "HL100 Drop — передача файлов" : "HL100 Drop — приём в фоне"
    }

    private func clear(_ stack: NSStackView) {
        for view in stack.arrangedSubviews { stack.removeArrangedSubview(view); view.removeFromSuperview() }
    }
    private var selectedPeer: Peer? { peers.first { $0.id == selectedPeerID } }
    private func selectPeer(_ peer: Peer) { selectedPeerID = peer.id; drawPeers() }
    private func refreshSendButton() {
        sendButton.isEnabled = running && selectedPeer != nil && !starting
        if let peer = selectedPeer {
            sendButton.title = pendingURLs.isEmpty ? "ОТПРАВИТЬ НА \(peer.name) →" : "ОТПРАВИТЬ \(pendingURLs.count) ОБЪЕКТОВ →"
        } else { sendButton.title = "ВЫБЕРИ ПОЛУЧАТЕЛЯ" }
        dropZone.label.stringValue = pendingURLs.isEmpty ? "Перетащи файлы или папки сюда" : "\(pendingURLs.count) объектов готовы к отправке"
    }
    private func stageFiles(_ urls: [URL]) {
        showPanel(activate: false)
        if settingsView != nil { showMainBody() }
        pendingURLs = urls; refreshSendButton()
        status.stringValue = selectedPeer == nil ? "Файлы добавлены. Выбери получателя." : "Файлы добавлены. Нажми «Отправить»."
    }
    private func dropIntoPanel(_ urls: [URL]) {
        if let peer = selectedPeer { send(peer, urls: urls) } else { stageFiles(urls) }
    }
    private func chooseOrSend() {
        guard let peer = selectedPeer, running, !starting else {
            status.stringValue = running ? "Сначала выбери устройство" : "Открой настройки, чтобы запустить приём"
            return
        }
        if !pendingURLs.isEmpty { send(peer, urls: pendingURLs); return }
        guard let window = window else { return }
        let picker = NSOpenPanel(); picker.canChooseFiles = true; picker.canChooseDirectories = true
        picker.allowsMultipleSelection = true; picker.prompt = "Отправить"; picker.message = "Отправить на \(peer.name)"
        picker.beginSheetModal(for: window) { [weak self] result in
            guard result == .OK else { return }; self?.send(peer, urls: picker.urls)
        }
    }
    private func drawPeers() {
        clear(peerList)
        if selectedPeer == nil { selectedPeerID = nil }
        if peers.isEmpty {
            let label = Style.label("Пока никого рядом.\nОткрой Drop на устройстве в той же сети.", size: 11, color: Style.muted)
            peerList.addArrangedSubview(label); label.widthAnchor.constraint(equalTo: peerList.widthAnchor).isActive = true
        }
        for peer in peers {
            let card = PeerCard(peer: peer, selected: peer.id == selectedPeerID,
                                select: { [weak self] peer in self?.selectPeer(peer) },
                                send: { [weak self] peer, urls in self?.send(peer, urls: urls) })
            peerList.addArrangedSubview(card); card.widthAnchor.constraint(equalTo: peerList.widthAnchor).isActive = true
        }
        if sendButton != nil { refreshSendButton() }
    }
    private func drawEmptyTransfers() {
        let label = Style.label("Твои передачи появятся здесь.", size: 11, color: Style.muted)
        transferList.addArrangedSubview(label); label.widthAnchor.constraint(equalTo: transferList.widthAnchor).isActive = true
    }

    private func send(_ peer: Peer, urls: [URL]) {
        guard running, !starting, !terminating, !urls.isEmpty else { return }
        status.stringValue = "Подготовка файлов…"
        networkActivity.setTransferring(true)
        engine.perform(["op": "send", "address": peer.address, "port": peer.port, "name": peer.name, "paths": urls.map(\.path)]) { [weak self] result in
            switch result {
            case .success:
                self?.busy = true
                if self?.pendingURLs == urls { self?.pendingURLs = [] }
                self?.refreshSendButton(); self?.status.stringValue = "Передача началась"
            case .failure(let error):
                self?.networkActivity.setTransferring(self?.busy ?? false)
                self?.status.stringValue = error.localizedDescription
            }
        }
    }
    private func cancel(_ id: String) {
        engine.perform(["op": "cancel", "transfer": id]) { [weak self] result in
            if case .failure(let error) = result { self?.status.stringValue = error.localizedDescription }
        }
    }

    @objc private func manualUpdate() { checkUpdates(manual: true) }
    private func checkUpdates(manual: Bool) {
        guard !checking else { return }; checking = true
        Updates.check { [weak self] result in
            guard let self = self else { return }; self.checking = false
            switch result {
            case .success(let release):
                self.release = release; self.updateBanner.isHidden = release == nil
                if let release = release { self.updateText.stringValue = "Доступна версия \(release.version)" }
                if manual { self.alert("Обновления", detail: release.map { "Доступна версия \($0.version) — кнопка загрузки в шторке." } ?? "Новых стабильных версий для macOS нет.") }
            case .failure(let error): if manual { self.alert("Не удалось проверить обновления", detail: error.localizedDescription) }
            }
        }
    }
    private func installUpdate() {
        guard let release = release, !downloading else { return }
        downloading = true; updateButton.isEnabled = false; updateText.stringValue = "Загрузка и проверка SHA-256…"
        Updates.download(release) { [weak self] result in
            guard let self = self else { return }
            self.downloading = false; self.updateButton.isEnabled = true
            self.updateText.stringValue = "Доступна версия \(release.version)"
            switch result {
            case .success(let url):
                NSWorkspace.shared.open(url)
                self.alert("Обновление проверено", detail: "Завершите передачи и выйдите из HL100 Drop через меню. Затем перетащите приложение из открытого образа в Applications с заменой. Настройки и полученные файлы сохранятся.")
            case .failure(let error): self.alert("Не удалось загрузить обновление", detail: error.localizedDescription)
            }
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard !terminating else { return .terminateLater }
        terminating = true
        engine.perform(["op": "poll"]) { [weak self] result in
            guard let self = self else { return }
            if case .success(let value) = result { self.consume(value) }
            if self.busy {
                let alert = NSAlert(); alert.messageText = "Завершить HL100 Drop?"
                alert.informativeText = "Текущие передачи будут прерваны."
                alert.addButton(withTitle: "Продолжить передачи"); alert.addButton(withTitle: "Завершить")
                if alert.runModal() != .alertSecondButtonReturn {
                    self.terminating = false; NSApp.reply(toApplicationShouldTerminate: false); return
                }
            }
            self.timer?.invalidate()
            self.engine.perform(["op": "stop"]) { _ in self.networkActivity.stop(); NSApp.reply(toApplicationShouldTerminate: true) }
        }
        return .terminateLater
    }

    private func alert(_ title: String, detail: String) {
        let alert = NSAlert(); alert.messageText = title; alert.informativeText = detail
        alert.addButton(withTitle: "OK"); alert.runModal()
    }

    // Render the actual AppKit view hierarchy with fixture data, without starting
    // discovery, touching user preferences, or checking releases.
    private func renderSmoke() {
        identity.stringValue = "• MacBook Pro / приём включён"
        status.stringValue = "Готов к отправке и автоматическому приёму"
        consume(["busy": true, "peers": [
            ["id": "pc", "name": "Рабочий компьютер", "address": "192.168.1.20", "port": 41000, "platform": "windows"],
            ["id": "phone", "name": "Poco F7", "address": "192.168.1.32", "port": 41000, "platform": "android"]
        ], "events": [
            ["type": "transfer", "transfer": "demo", "name": "Рабочий компьютер", "direction": "send", "files": 1, "total": 104857600.0, "entries": [["path": "Archive.zip", "size": 104857600.0]]],
            ["type": "progress", "transfer": "demo", "done": 67108864.0, "total": 104857600.0, "speed": 12582912.0, "remaining": 3.0],
            ["type": "transfer", "transfer": "done", "name": "Poco F7", "direction": "receive", "files": 1, "total": 1024.0, "entries": [["path": "photo.jpg", "size": 1024.0]]],
            ["type": "file", "transfer": "done", "path": "photo.jpg", "size": 1024.0, "local_path": "/tmp/photo (1).jpg"],
            ["type": "complete", "transfer": "done", "status": "done"]
        ]])
        running = true
        if let peer = peers.first(where: { $0.id == "pc" }) { selectPeer(peer) }
        showWindow()
        if CommandLine.arguments.contains("--interaction-smoke") {
            let iconMenu = panelMenu().items.first { $0.title == "Значок" }!.submenu!
            guard iconMenu.items.count == 4 else { exit(1) }
            for item in iconMenu.items {
                selectStatusIcon(item); menuWillOpen(iconMenu)
                guard item.state == .on, iconMenu.items.filter({ $0.state == .on }).count == 1,
                      statusItem.button?.image?.isTemplate == true, statusItem.button?.image?.size == NSSize(width: 18, height: 18) else { exit(1) }
            }
            menuTracking = false; statusIcon = .bolt; statusItem.button?.image = statusIcon.image()
            let suite = "hl100-icon-smoke-\(UUID().uuidString)"
            let defaults = UserDefaults(suiteName: suite)!
            defer { defaults.removePersistentDomain(forName: suite) }
            defaults.set("nodes", forKey: StatusIcon.preferenceKey)
            guard StatusIcon.restored(from: defaults) == .nodes else { exit(1) }
            defaults.set("obsolete", forKey: StatusIcon.preferenceKey)
            guard StatusIcon.restored(from: defaults) == .bolt else { exit(1) }
            print("PASS: four template icons, menu selection/checkmarks, persisted choice and fallback")
            let pasteboard = NSPasteboard.withUniqueName()
            defer { pasteboard.releaseGlobally() }
            pasteboard.writeObjects([NSURL(fileURLWithPath: "/tmp/fixture.txt")])
            closePanel()
            guard statusDrop?.acceptDrag(from: pasteboard) == .copy, popover.isShown,
                  popover.behavior == .applicationDefined, statusDrop?.acceptDrop(from: pasteboard) == true,
                  pendingURLs.count == 1, busy else { fputs("Drag interaction smoke failed\n", stderr); exit(1) }
            let textBoard = NSPasteboard.withUniqueName()
            defer { textBoard.releaseGlobally() }
            textBoard.setString("not a file", forType: .string)
            guard statusDrop?.acceptDrag(from: textBoard) == [], statusDrop?.acceptDrop(from: textBoard) == false else { exit(1) }
            // Exercise dismissal rules without injecting events into other apps.
            dismissAfterOutsideClick(outside: true, dragged: true)
            guard popover.isShown else { exit(1) }
            togglePin(); dismissAfterOutsideClick(outside: true, dragged: false)
            guard popover.isShown else { exit(1) }
            togglePanel(); guard !popover.isShown else { exit(1) }; togglePanel()
            togglePin(); dismissAfterOutsideClick(outside: true, dragged: false)
            guard !popover.isShown else { exit(1) }; showWindow()
            guard rows.first(where: { $0.id == "demo" })?.revealFile(at: 0) == false else { exit(1) }
            guard rows.first(where: { $0.id == "done" })?.revealFile(at: 0) == true,
                  smokeRevealedURL?.path == "/tmp/photo (1).jpg" else { exit(1) }
            showDesktopWindow(); smokeRevealedURL = nil
            guard desktopRows.first(where: { $0.id == "done" })?.revealFile(at: 0) == true,
                  smokeRevealedURL?.path == "/tmp/photo (1).jpg" else { exit(1) }
            desktopWindow?.orderOut(nil)
            NSApp.deactivate()
            guard popover.isShown else { fputs("Panel closed on deactivation\n", stderr); exit(1) }
            print("PASS: drag staging, text rejection, Pin/icon/outside-click rules, exact received file reveal in panel and desktop window")
            pendingURLs = []; refreshSendButton()
            let wasBusy = busy; busy = false; showSettings()
            guard settingsView?.verifyPreviewInteractions() == true, branding == .lightning else { exit(1) }
            showMainBody(); busy = wasBusy
            print("PASS: login checkbox enable/disable, error rollback, OS approval state, three title styles")
        }
        if CommandLine.arguments.contains("--ui-settings-smoke") {
            busy = false; showSettings()
        }
        if CommandLine.arguments.contains("--ui-desktop-smoke") { showDesktopWindow() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            guard let self = self else { exit(1) }
            let view = CommandLine.arguments.contains("--ui-desktop-smoke") ? self.desktopWindow!.contentView! : self.panelController.view
            if CommandLine.arguments.contains("--interaction-smoke"), !self.popover.isShown {
                fputs("Panel disappeared after deactivation\n", stderr); exit(1)
            }
            view.layoutSubtreeIfNeeded()
            if ProcessInfo.processInfo.environment["HL100_UI_TRACE"] != nil {
                func trace(_ item: NSView) {
                    if let text = item as? NSTextField {
                        print("LABEL \(text.stringValue): \(item.convert(item.bounds, to: view)) hidden=\(item.isHiddenOrHasHiddenAncestor)")
                    } else if let button = item as? NSButton {
                        print("BUTTON \(button.title): \(item.convert(item.bounds, to: view)) hidden=\(item.isHiddenOrHasHiddenAncestor)")
                    }
                    item.subviews.forEach(trace)
                }
                trace(view)
            }
            guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { exit(1) }
            view.cacheDisplay(in: view.bounds, to: bitmap)
            let path = ProcessInfo.processInfo.environment["HL100_UI_SMOKE_PATH"] ?? "/tmp/hl100-macos-ui.png"
            do {
                try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
                print("Rendered AppKit UI: \(path)")
                exit(0)
            } catch { fputs("UI render failed: \(error)\n", stderr); exit(1) }
        }
    }
}
