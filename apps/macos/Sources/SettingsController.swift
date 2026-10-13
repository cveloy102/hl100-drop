import AppKit

final class SettingsView: NSView {
    private var settings: Settings
    private let login: LoginLaunchManaging
    private let changeBranding: (Branding) -> Void
    private let name = NSTextField()
    private let folder = Style.label("", size: 11, color: Style.muted)
    private let error = Style.label("", size: 11, color: Style.red)
    private let updates = NSButton(checkboxWithTitle: "Проверять обновления при запуске", target: nil, action: nil)
    private let launch = NSButton(checkboxWithTitle: "Запускать DROP при входе в macOS", target: nil, action: nil)
    private let launchMessage = Style.label("Разреши запуск в системных настройках macOS.", size: 11, color: Style.red)
    private let stylePicker = NSPopUpButton(frame: .zero, pullsDown: false)
    private var approval: ActionButton!
    private var saving = false
    private var observer: NSObjectProtocol?
    init(settings: Settings, login: LoginLaunchManaging, branding: Branding, changeBranding: @escaping (Branding) -> Void,
         save: @escaping (Settings, @escaping (Error?) -> Void) -> Void,
         back: @escaping () -> Void, check: @escaping () -> Void, quit: @escaping () -> Void) {
        self.settings = settings; self.login = login; self.changeBranding = changeBranding
        super.init(frame: .zero)
        name.stringValue = settings.name; name.font = .monospacedSystemFont(ofSize: 13, weight: .regular)
        folder.stringValue = settings.folder; folder.isSelectable = true
        updates.state = settings.checkUpdates ? .on : .off
        for checkbox in [updates, launch] { checkbox.font = .monospacedSystemFont(ofSize: 11, weight: .regular) }
        launch.allowsMixedState = true; launch.target = self; launch.action = #selector(toggleLogin)
        launch.toolTip = "Автозапуск работает в фоне, без открытия шторки. Изменение применяется сразу."
        approval = ActionButton("РАЗРЕШИТЬ В macOS →") { login.openApprovalSettings() }
        refreshLogin()
        stylePicker.addItems(withTitles: Branding.allCases.map(\.title)); stylePicker.selectItem(at: Branding.allCases.firstIndex(of: branding) ?? 0)
        stylePicker.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        stylePicker.target = self; stylePicker.action = #selector(selectBranding)
        stylePicker.setAccessibilityLabel("Оформление названия DROP")
        let choose = ActionButton("ВЫБРАТЬ ПАПКУ ↗") { [weak self] in self?.chooseFolder() }
        let saveButton = ActionButton(settings.configured ? "СОХРАНИТЬ →" : "НАЧАТЬ →", accent: true) { }
        saveButton.useAccentFill()
        saveButton.actionHandler = { [weak self, weak saveButton] in
            guard let self = self, !self.saving else { return }
            let trimmed = self.name.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { self.error.stringValue = "Укажите имя устройства"; return }
            var next = self.settings
            next.name = trimmed; next.checkUpdates = self.updates.state == .on; next.configured = true
            self.saving = true; saveButton?.isEnabled = false
            save(next) { [weak self, weak saveButton] failure in
                guard let self = self else { return }
                self.saving = false; saveButton?.isEnabled = true
                if let failure = failure { self.error.stringValue = failure.localizedDescription } else { back() }
            }
        }
        saveButton.keyEquivalent = "\r"
        let form = Style.stack([
            Style.label(settings.configured ? "Настройки_" : "Hello, nearby_", size: 22),
            Style.label(settings.configured ? "[ SETTINGS ]" : "[ ПЕРВОЕ ВКЛЮЧЕНИЕ ]", size: 11, color: Style.red),
            Style.label("Имя устройства"), name, Style.label("Папка приёма"), folder, choose,
            Style.label("Оформление DROP"), stylePicker, launch, launchMessage, approval!, updates
        ], spacing: 10)
        let scroll = scrollingColumn(form)
        scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 160).isActive = true
        scroll.setContentHuggingPriority(.defaultLow, for: .vertical)
        let actions = NSStackView(views: [
            ActionButton("← Назад") { [weak self] in if self?.saving == false { back() } },
            ActionButton("Обновления") { check() },
            ActionButton("Выйти") { [weak self] in if self?.saving == false { quit() } }
        ]); actions.spacing = 8; actions.distribution = .fillEqually
        let content = Style.stack([scroll, error, saveButton, actions], spacing: 10)
        Style.pin(content, in: self)
        for view in content.arrangedSubviews { view.widthAnchor.constraint(equalTo: content.widthAnchor).isActive = true }
        observer = NotificationCenter.default.addObserver(forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in self?.refreshLogin() }
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
    deinit { if let observer = observer { NotificationCenter.default.removeObserver(observer) } }
    private func refreshLogin() {
        let state = login.state
        launch.state = state == .enabled ? .on : state == .requiresApproval ? .mixed : .off
        launchMessage.isHidden = state != .requiresApproval; approval.isHidden = state != .requiresApproval
    }
    @objc private func toggleLogin() {
        guard !saving else { refreshLogin(); return }
        do { try login.setEnabled(login.state == .disabled); error.stringValue = "" }
        catch { self.error.stringValue = error.localizedDescription }
        refreshLogin()
    }
    @objc private func selectBranding() {
        guard Branding.allCases.indices.contains(stylePicker.indexOfSelectedItem) else { return }
        changeBranding(Branding.allCases[stylePicker.indexOfSelectedItem])
    }
    func verifyPreviewInteractions() -> Bool {
        guard let preview = login as? PreviewLoginLaunch, stylePicker.numberOfItems == 3 else { return false }
        preview.failChange = true; launch.performClick(nil)
        guard launch.state == .off, !error.stringValue.isEmpty else { return false }
        preview.failChange = false; launch.performClick(nil)
        guard preview.state == .enabled, launch.state == .on else { return false }
        launch.performClick(nil)
        guard preview.state == .disabled, launch.state == .off else { return false }
        preview.state = .requiresApproval; refreshLogin()
        guard launch.state == .mixed, !approval.isHidden, !launchMessage.isHidden else { return false }
        launch.performClick(nil)
        guard preview.state == .disabled, launch.state == .off, approval.isHidden else { return false }
        for index in 0..<stylePicker.numberOfItems { stylePicker.selectItem(at: index); selectBranding() }
        stylePicker.selectItem(at: 0); selectBranding()
        return true
    }
    private func chooseFolder() {
        guard !saving, let window = window else { return }
        let panel = NSOpenPanel(); panel.canChooseDirectories = true; panel.canChooseFiles = false
        panel.canCreateDirectories = true; panel.prompt = "Выбрать"
        panel.directoryURL = URL(fileURLWithPath: settings.folder)
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            self?.settings.folder = url.path; self?.folder.stringValue = url.path
        }
    }
}
