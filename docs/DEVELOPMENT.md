# Сборка и выпуск

[← DROP](../README.md) · [Протокол](protocol.md) · [Проверки](validation.md)

## Устройство проекта

| Каталог | Назначение |
| --- | --- |
| `core` | Rust: обнаружение, TCP, манифест, докачка, C ABI и JNI |
| `apps/windows` | C# / WPF, трей, drag and drop, обновление |
| `apps/android` | Kotlin / Compose, SAF, фон, «Поделиться» |
| `apps/linux` | Rust / GTK 4, трей (StatusNotifierItem), уведомления, .deb и AppImage |
| `apps/macos` | Swift / AppKit, строка меню, Finder, universal DMG |
| `installer/windows` | Inno Setup, установка для текущего пользователя |
| `site` | Статический сайт продукта на GitHub Pages |
| `scripts` | Сборка, подпись, проверки, публикация |

## Windows

Rust с MSVC + Build Tools либо GNU + MinGW, .NET SDK 8+, Inno Setup 6.

```powershell
./scripts/build-windows.ps1 -Version 1.1.1
```

Результат: `artifacts/HL100DropSetup-1.1.1.exe`. Runtime .NET входит в приложение.

## Android

JDK, Android SDK 37, build-tools 36.0.0, NDK r27d; Rust targets `aarch64-linux-android`, `x86_64-linux-android`. Задайте `JAVA_HOME`, `ANDROID_HOME`, `ANDROID_NDK_HOME`.

```powershell
./scripts/build-android.ps1            # debug для разработки
./scripts/build-android.ps1 -Release   # релиз с постоянным ключом
```

Релиз требует `HL100_KEYSTORE`, `HL100_STORE_PASSWORD`, `HL100_KEY_ALIAS`, `HL100_KEY_PASSWORD` либо локальный `private/android-signing.json`. `scripts/initialize-signing.ps1` создаёт ключ один раз. **Не создавайте новый ключ для каждого релиза.** Ключ и пароли не хранятся в Git; резервную копию храните отдельно в защищённом месте.

```powershell
Copy-Item apps/android/app/build/outputs/apk/release/app-release.apk artifacts/HL100Drop-1.1.1.apk
```

## Mac

Apple Command Line Tools и Rust.

```sh
bash scripts/build-macos.sh
bash scripts/test-macos.sh
```

Результат: `artifacts/HL100Drop-1.1.1-universal.dmg`. App и dylib содержат x86_64 + arm64; проверяются lipo и codesign. По умолчанию подпись ad hoc. `MACOS_SIGN_IDENTITY` и `MACOS_NOTARY_PROFILE` позволяют использовать собственные Developer ID / нотариализацию. [Дополнительно](../apps/macos/README.md).

## Linux

Rust, `pkg-config`, GTK 4.12+ для разработки (Debian / Ubuntu / Kali: `sudo apt install cargo libgtk-4-dev`; Fedora: `gtk4-devel`; Arch: `gtk4`), `dpkg-deb`.

```sh
cargo run -p hl100-drop                 # запуск из исходников
bash scripts/build-linux.sh             # artifacts/hl100-drop_<версия>_amd64.deb
```

Flatpak (GNOME 50, ставится сам с Flathub):

```sh
sudo apt install flatpak flatpak-builder
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
bash scripts/build-flatpak.sh --install   # artifacts/HL100Drop-<версия>-x86_64.flatpak
flatpak run ru.hl100.drop
```

AppImage собирается скриптом `build-linux.sh`, если указан `LINUXDEPLOY` и в `PATH` есть `linuxdeploy-plugin-gtk.sh` (так делает CI). Приложение подключает `core` как обычный Rust-крейт, без C ABI. Два экземпляра на одной машине: задайте каждому свой `HL100_SETTINGS_PATH`; второй займёт свободный TCP-порт. [Дополнительно](../apps/linux/README.md).

## CI и деплой

**«Сборка и проверка»** (`.github/workflows/checks.yml`) проверяет Rust fmt / tests / clippy, строит Windows installer, Android debug APK с unit tests / lint и Mac universal DMG со Swift C ABI tests. Артефакты CI доступны в завершённом запуске; Android debug — только для QA, не релизный канал. Отдельный артефакт сохраняет скриншоты настоящего AppKit с фикстурами.

**«Сайт DROP»** (`.github/workflows/pages.yml`) публикует `site/` через GitHub Pages при изменениях сайта или изображений в основной ветке. Можно запустить вручную. У сайта нет сборщика и зависимостей; ссылки ведут на стабильные GitHub Releases.

## Выпуск стабильной версии

1. Обновите `core/Cargo.toml`, `apps/linux/Cargo.toml` и их записи в Cargo.lock, Windows Version, Android versionName / растущий versionCode, значения скриптов и установщика, `<release>` в `apps/linux/data/ru.hl100.drop.metainfo.xml`. Mac-версия берётся из Cargo.
2. Обновите `CHANGELOG.md` и русские `scripts/release-notes.md`.
3. Запустите CI и проверки на реальных устройствах для изменённого поведения.
4. Соберите Windows и **подписанный** Android. Из успешного CI нужного коммита скачайте DMG (`macos-universal-installer`), `.deb` и AppImage (`linux-packages`) и Flatpak (`HL100Drop-<версия>-x86_64.flatpak`). Все шесть файлов должны лежать в `artifacts/`:

```powershell
# run-id — успешный запуск «Сборка и проверка» на коммите релиза (gh run list --branch main)
gh run download <run-id> -n macos-universal-installer -n linux-packages -p "HL100Drop-*flatpak" -D artifacts/ci
Get-ChildItem artifacts/ci -Recurse -File -Include *.dmg,*.deb,*.AppImage,*.flatpak | Move-Item -Destination artifacts -Force
```
5. Опубликуйте:

```powershell
./scripts/publish-release.ps1 -Version 1.1.1 -Stable
```

Скрипт использует локальный Git Credential Manager, создаёт tag / Release, SHA-256 и загружает шесть установочных файлов. Релиз сначала создаётся как черновик; видимым становится после загрузки всех файлов. Ошибка оставляет черновик для исправления, а не частично заполненный канал обновлений. Без `-Stable` выпуск предварительный и не виден обновлятору.

6. Проверьте `/releases/latest`, имена и SHA-256 assets, затем обновление с предыдущей установленной версии на всех платформах.

Публикация Android остаётся локальной, чтобы не отправлять постоянный закрытый ключ подписи на сторонние сервисы. CI строит Android debug; он не заменяет релизный APK.

## Проверки ядра

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Докачка использует согласование смещений в начале подключения и исходный TCP-поток с буфером 1 МБ. Журналы `.drop-state` и Android private storage сохраняют незавершённые задания. Измерения `core/examples/throughput.rs` — loopback, не гарантия скорости Wi-Fi.
