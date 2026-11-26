//! StatusNotifierItem tray (KDE, Xfce, Cinnamon, MATE, LXQt, Budgie, sway/waybar;
//! GNOME with the AppIndicator extension). Works on X11 and Wayland alike.

use gtk::cairo;

pub enum TrayEvent {
    Show,
    Quit,
}

/// The DROP lightning mark from docs/images/drop-mark.svg (viewBox 32×40).
pub fn draw_mark(cr: &cairo::Context, width: f64, height: f64) {
    let scale = (width / 32.0).min(height / 40.0);
    cr.save().ok();
    cr.translate((width - 32.0 * scale) / 2.0, (height - 40.0 * scale) / 2.0);
    cr.scale(scale, scale);
    for (i, (x, y)) in [
        (18.0, 0.0),
        (2.0, 22.0),
        (14.0, 22.0),
        (9.0, 40.0),
        (31.0, 14.0),
        (19.0, 14.0),
        (24.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        if i == 0 {
            cr.move_to(x, y)
        } else {
            cr.line_to(x, y)
        }
    }
    cr.close_path();
    cr.set_source_rgb(
        0xef as f64 / 255.0,
        0x3f as f64 / 255.0,
        0x3b as f64 / 255.0,
    );
    cr.fill().ok();
    cr.restore().ok();
}

/// Render the mark into SNI pixmaps, so the tray never depends on an installed
/// icon theme (AppImage, portable runs).
fn pixmaps() -> Vec<ksni::Icon> {
    [22, 32, 48, 64]
        .into_iter()
        .filter_map(|size| {
            let mut surface =
                cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
            {
                let cr = cairo::Context::new(&surface).ok()?;
                draw_mark(&cr, size as f64, size as f64);
            }
            surface.flush();
            let stride = surface.stride() as usize;
            let data = surface.data().ok()?;
            let mut out = Vec::with_capacity((size * size * 4) as usize);
            for row in 0..size as usize {
                for col in 0..size as usize {
                    let o = row * stride + col * 4;
                    let px = u32::from_ne_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
                    let a = px >> 24;
                    // Cairo stores premultiplied alpha; SNI expects straight ARGB.
                    let un = |c: u32| (c * 255).checked_div(a).unwrap_or(0).min(255);
                    let straight = (a << 24)
                        | (un((px >> 16) & 0xff) << 16)
                        | (un((px >> 8) & 0xff) << 8)
                        | un(px & 0xff);
                    out.extend_from_slice(&straight.to_be_bytes());
                }
            }
            Some(ksni::Icon {
                width: size,
                height: size,
                data: out,
            })
        })
        .collect()
}

struct DropTray {
    sender: async_channel::Sender<TrayEvent>,
    icons: Vec<ksni::Icon>,
}

impl ksni::Tray for DropTray {
    fn id(&self) -> String {
        crate::APP_ID.into()
    }
    fn title(&self) -> String {
        "DROP".into()
    }
    fn icon_name(&self) -> String {
        crate::APP_ID.into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "DROP".into(),
            description: "Передача файлов по локальной сети".into(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send_blocking(TrayEvent::Show);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Открыть DROP".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.sender.send_blocking(TrayEvent::Show);
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Выход".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.sender.send_blocking(TrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub struct TrayHandle(ksni::blocking::Handle<DropTray>);

impl TrayHandle {
    pub fn shutdown(&self) {
        self.0.shutdown().wait();
    }
}

/// Start the tray. `None` when the desktop has no StatusNotifier host
/// (plain GNOME): the window then stays reachable from the app launcher.
pub fn spawn(sender: async_channel::Sender<TrayEvent>) -> Option<TrayHandle> {
    use ksni::blocking::TrayMethods;
    DropTray {
        sender,
        icons: pixmaps(),
    }
    .disable_dbus_name(crate::in_flatpak())
    .spawn()
    .map_err(|e| eprintln!("DROP: трей недоступен: {e}"))
    .ok()
    .map(TrayHandle)
}
