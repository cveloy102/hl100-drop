# DROP для Linux

Нативный клиент на Rust + GTK 4. Ядро `core` подключается как обычный крейт: тот же код обнаружения, TCP и докачки, что на Windows, Mac и Android.

| Модуль | Назначение |
| --- | --- |
| `src/ui.rs` | Окно, карточки устройств, передачи, настройки, drag and drop, уведомления |
| `src/engine.rs` | Вызовы ядра в рабочем потоке, форматирование размеров и времени |
| `src/tray.rs` | Значок StatusNotifierItem (KDE, Xfce, Cinnamon, MATE, LXQt, Budgie, waybar; GNOME с AppIndicator) |
| `src/autostart.rs` | Автозапуск по XDG autostart; Flatpak пишет ту же запись с `flatpak run` |
| `src/updates.rs` | Проверка GitHub Releases; AppImage заменяет себя после проверки SHA-256, пакеты — через страницу релиза |
| `src/settings.rs` | `~/.config/hl100-drop/settings.json` |
| `flatpak/` | Манифест Flatpak (GNOME 50): сеть, домашняя папка и съёмные диски, трей; сборка — `scripts/build-flatpak.sh` |
| `data/` | desktop-файл, иконка, AppStream, «Отправить» для Thunar и Dolphin, профили ufw и firewalld |

## Поведение

- Один экземпляр на пользователя (D-Bus). Повторный запуск показывает окно; `hl100-drop файлы…` открывает DROP с файлами к отправке, `--send-to "Имя" файлы…` отправляет сразу, когда устройство появится; `--login` — запуск в фон; `--quit` — выход.
- Закрытие окна оставляет приём работать. Выход: меню значка, Ctrl+Q или «Выход» в меню приложения.
- Если TCP 41000 занят другой программой, приём запускается на свободном порту; обнаружение объявляет фактический порт.
- Минимум: GTK 4.12, glibc сборочной машины. Без libadwaita, чтобы фирменный вид не зависел от GNOME.

## Разработка

```sh
sudo apt install cargo libgtk-4-dev     # Debian / Ubuntu / Kali
cargo run -p hl100-drop
cargo test -p hl100-drop
```

Два устройства на одной машине:

```sh
HL100_SETTINGS_PATH=/tmp/a.json cargo run -p hl100-drop &
HL100_SETTINGS_PATH=/tmp/b.json cargo run -p hl100-drop &
```

Тестовый профиль не меняет настоящий автозапуск.
