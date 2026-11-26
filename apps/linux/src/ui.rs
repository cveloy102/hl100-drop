use crate::{
    autostart,
    engine::{self, bytes, eta, Peer},
    settings::{self, Settings},
    tray::{self, TrayEvent},
    updates::{self, Release},
};
use gtk::{gdk, gio, glib, prelude::*};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Duration,
};

const CSS: &str = include_str!("style.css");
const HISTORY_LIMIT: usize = 100;

thread_local! {
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

type FileButtons = HashMap<String, (gtk::Button, Rc<RefCell<Option<String>>>)>;

struct Row {
    bar: gtk::ProgressBar,
    text: gtk::Label,
    pause: gtk::Button,
    cancel: gtk::Button,
    state: Rc<RefCell<String>>,
    total: f64,
    receiving: bool,
    first_name: String,
    count: u64,
    saved_path: Option<String>,
    files: FileButtons,
}

#[derive(Default)]
struct State {
    settings: Settings,
    running: bool,
    polling: bool,
    busy: bool,
    exiting: bool,
    updating: bool,
    peers: Vec<Peer>,
    peer_signature: String,
    transfers: HashMap<String, Row>,
    history: Vec<(String, gtk::Widget)>,
    release: Option<Release>,
    told_background: bool,
    /// Files handed over on the command line ("Open with DROP"), waiting for a receiver.
    pending: Vec<String>,
    /// `--send-to`: receiver name and when to give up waiting for it.
    pending_target: Option<(String, std::time::Instant)>,
}

struct Ui {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    frame: gtk::Box,
    pages: gtk::Stack,
    columns: gtk::Box,
    settings_button: gtk::Button,
    connection: gtk::Box,
    identity: gtk::Label,
    status: gtk::Label,
    update_banner: gtk::Box,
    update_text: gtk::Label,
    peer_list: gtk::Box,
    pending_box: gtk::Box,
    pending_text: gtk::Label,
    empty_peers: gtk::Label,
    drop_hint: gtk::Label,
    transfer_list: gtk::Box,
    empty_transfers: gtk::Label,
    settings_page: gtk::Box,
    settings_feedback: RefCell<Option<gtk::Label>>,
    st: RefCell<State>,
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    tray: RefCell<Option<tray::TrayHandle>>,
    initialized: Cell<bool>,
}

pub fn startup(_app: &gtk::Application) {
    prefer_dark();
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    gtk::Window::set_default_icon_name(crate::APP_ID);
}

/// GTK 4.20 replaced the old dark-theme flag; older runtimes only know the flag.
fn prefer_dark() {
    let Some(settings) = gtk::Settings::default() else {
        return;
    };
    if let Some(pspec) = settings.find_property("gtk-interface-color-scheme") {
        if let Some(class) = glib::EnumClass::with_type(pspec.value_type()) {
            if let Some(dark) = class.to_value_by_nick("dark") {
                settings.set_property_from_value("gtk-interface-color-scheme", &dark);
                return;
            }
        }
    }
    settings.set_gtk_application_prefer_dark_theme(true);
}

/// What a launch asks for: `--login`, `--send-to NAME`, files to send.
#[derive(Default)]
pub struct Launch {
    pub login: bool,
    pub quit: bool,
    pub send_to: Option<String>,
    pub files: Vec<String>,
}

/// Every launch lands here: the first builds the window, later ones (a second
/// start from the menu, the autostart entry) only bring it forward.
pub fn open(app: &gtk::Application, launch: Launch) {
    let login = launch.login;
    if launch.quit {
        // Only meaningful for a running copy; never start one just to stop it.
        if let Some(ui) = UI.with(|cell| cell.borrow().clone()) {
            glib::spawn_future_local(ui.quit());
        }
        return;
    }
    let ui = UI.with(|cell| cell.borrow().clone()).unwrap_or_else(|| {
        let ui = Ui::build(app);
        UI.with(|cell| *cell.borrow_mut() = Some(ui.clone()));
        ui
    });
    let configured = ui.st.borrow().settings.configured;
    // A configured background login never shows a window.
    if !login || !configured {
        ui.window.present();
    }
    if !launch.files.is_empty() {
        ui.set_pending(launch.files, launch.send_to);
        ui.window.present();
    }
    if !ui.initialized.replace(true) {
        glib::spawn_future_local(ui.clone().initialize());
    }
}

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

fn button(text: &str, class: &str) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class(class);
    b.set_halign(gtk::Align::Start);
    b
}

fn mark(size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_valign(gtk::Align::Center);
    area.set_draw_func(|_, cr, w, h| tray::draw_mark(cr, w as f64, h as f64));
    area
}

fn file_paths(value: &glib::Value) -> Vec<String> {
    value
        .get::<gdk::FileList>()
        .map(|list| {
            list.files()
                .iter()
                .filter_map(|f| f.path())
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn list_paths(model: &gio::ListModel) -> Vec<String> {
    (0..model.n_items())
        .filter_map(|i| model.item(i).and_downcast::<gio::File>())
        .filter_map(|f| f.path())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

impl Ui {
    fn build(app: &gtk::Application) -> Rc<Self> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("DROP")
            .default_width(1080)
            .default_height(800)
            .build();
        window.add_css_class("drop");
        window.set_size_request(600, 520);

        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_margin_top(28);
        root.set_margin_bottom(20);
        root.set_margin_start(32);
        root.set_margin_end(32);
        frame.append(&root);
        window.set_child(Some(&frame));

        // Header: mark + wordmark, settings on the right.
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header.set_margin_bottom(22);
        header.append(&mark(30));
        let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
        words.append(&label("DROP", &["brand"]));
        words.append(&label("HL100_", &["muted", "small"]));
        words.set_hexpand(true);
        header.append(&words);
        let settings_button = button("НАСТРОЙКИ ↗", "secondary");
        settings_button.set_valign(gtk::Align::Center);
        header.append(&settings_button);
        root.append(&header);

        let update_banner = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        update_banner.add_css_class("banner");
        update_banner.set_margin_bottom(20);
        let update_text = label("", &[]);
        update_text.set_hexpand(true);
        update_banner.append(&update_text);
        let update_button = button("ОБНОВИТЬСЯ ↗", "primary");
        update_banner.append(&update_button);
        update_banner.set_visible(false);
        root.append(&update_banner);

        let connection = gtk::Box::new(gtk::Orientation::Vertical, 0);
        connection.set_margin_bottom(24);
        connection.append(&label("[ 01 / LOCAL CONNECTION ]", &["accent", "small"]));
        let hero = label("Hello, nearby_", &["hero"]);
        hero.set_margin_top(12);
        hero.set_margin_bottom(8);
        connection.append(&hero);
        let identity = label("", &["muted"]);
        identity.set_selectable(true);
        connection.append(&identity);
        let status = label("Запуск…", &["muted"]);
        status.set_margin_top(8);
        connection.append(&status);
        root.append(&connection);

        // Main page: devices on the left, transfers on the right.
        let column = |title: &str, content: &gtk::Box| {
            let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
            b.set_hexpand(true);
            b.set_vexpand(true);
            let t = label(title, &[]);
            t.set_margin_bottom(16);
            b.append(&t);
            let scroll = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vexpand(true)
                .child(content)
                .build();
            b.append(&scroll);
            b
        };
        let peers_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        peers_content.set_margin_end(12);
        let pending_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        pending_box.add_css_class("banner");
        pending_box.set_margin_bottom(16);
        let pending_text = label("", &[]);
        pending_text.set_hexpand(true);
        pending_box.append(&pending_text);
        let pending_cancel = button("ОТМЕНА", "secondary");
        pending_cancel.set_valign(gtk::Align::Center);
        pending_box.append(&pending_cancel);
        pending_box.set_visible(false);
        peers_content.append(&pending_box);
        let empty_peers = label(
            "Пока никого рядом.\n\nОткрой DROP на другом устройстве\nв той же локальной сети.",
            &["muted"],
        );
        empty_peers.set_margin_top(8);
        empty_peers.set_margin_bottom(24);
        peers_content.append(&empty_peers);
        let peer_list = gtk::Box::new(gtk::Orientation::Vertical, 12);
        peers_content.append(&peer_list);
        let drop_hint = label(
            "Перетащи файлы или папки\nна карточку получателя →",
            &["muted", "hint"],
        );
        drop_hint.set_margin_top(12);
        peers_content.append(&drop_hint);

        let transfers_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        transfers_content.set_margin_end(12);
        let empty_transfers = label("Твои передачи появятся здесь.", &["muted"]);
        empty_transfers.set_margin_top(8);
        empty_transfers.set_margin_bottom(20);
        transfers_content.append(&empty_transfers);
        let transfer_list = gtk::Box::new(gtk::Orientation::Vertical, 24);
        transfers_content.append(&transfer_list);

        let columns = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        columns.set_homogeneous(true);
        columns.append(&column("[ 02 / УСТРОЙСТВА ]", &peers_content));
        columns.append(&column("[ 03 / ПЕРЕДАЧИ ]", &transfers_content));

        let settings_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        settings_page.set_halign(gtk::Align::Start);
        settings_page.set_margin_end(16);
        settings_page.set_size_request(-1, -1);
        let settings_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&settings_page)
            .build();

        let pages = gtk::Stack::new();
        pages.set_vexpand(true);
        pages.set_transition_type(gtk::StackTransitionType::Crossfade);
        pages.add_named(&columns, Some("main"));
        pages.add_named(&settings_scroll, Some("settings"));
        root.append(&pages);

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        footer.add_css_class("footer");
        footer.set_margin_top(24);
        let tagline = label("DIRECT TRANSMISSION / LOCAL NETWORK", &["muted", "small"]);
        tagline.set_hexpand(true);
        footer.append(&tagline);
        let version = label(
            &format!("V {} / LINUX", updates::CURRENT),
            &["muted", "small"],
        );
        footer.append(&version);
        root.append(&footer);

        let ui = Rc::new(Ui {
            app: app.clone(),
            window,
            frame,
            pages,
            columns,
            settings_button,
            connection,
            identity,
            status,
            update_banner,
            update_text,
            peer_list,
            pending_box,
            pending_text,
            empty_peers,
            drop_hint,
            transfer_list,
            empty_transfers,
            settings_page,
            settings_feedback: RefCell::new(None),
            st: RefCell::new(State {
                settings: Settings::load(),
                ..Default::default()
            }),
            hold: RefCell::new(Some(app.hold())),
            tray: RefCell::new(None),
            initialized: Cell::new(false),
        });
        ui.connect(&update_button);
        let weak = Rc::downgrade(&ui);
        pending_cancel.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.set_pending(Vec::new(), None);
            }
        });
        ui
    }

    fn set_pending(self: &Rc<Self>, files: Vec<String>, target: Option<String>) {
        {
            let mut st = self.st.borrow_mut();
            st.pending_target = target
                .filter(|_| !files.is_empty())
                .map(|t| (t, std::time::Instant::now() + Duration::from_secs(15)));
            st.pending = files;
        }
        self.show_pending();
        self.draw_peers();
        self.send_pending_to_target();
    }

    fn show_pending(&self) {
        let st = self.st.borrow();
        self.pending_box.set_visible(!st.pending.is_empty());
        let first = st
            .pending
            .first()
            .and_then(|p| std::path::Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let more = st.pending.len().saturating_sub(1);
        let what = if more > 0 {
            format!("{first} и ещё {more}")
        } else {
            first
        };
        self.pending_text.set_text(&match &st.pending_target {
            Some((name, _)) => format!("К отправке: {what}\nЖдём устройство «{name}»…"),
            None => format!("К отправке: {what}\nВыбери получателя ↓"),
        });
    }

    /// `--send-to`: send as soon as the named receiver shows up, or give up.
    fn send_pending_to_target(self: &Rc<Self>) {
        let (peer, expired) = {
            let st = self.st.borrow();
            let Some((name, deadline)) = &st.pending_target else {
                return;
            };
            if !st.running {
                return;
            }
            (
                st.peers.iter().find(|p| &p.name == name).cloned(),
                std::time::Instant::now() > *deadline,
            )
        };
        if let Some(peer) = peer {
            self.take_pending_and_send(peer);
        } else if expired {
            let name = self.st.borrow_mut().pending_target.take().map(|t| t.0);
            self.show_pending();
            self.status.set_text(&format!(
                "Устройство «{}» не найдено в сети",
                name.unwrap_or_default()
            ));
        }
    }

    fn take_pending_and_send(self: &Rc<Self>, peer: Peer) {
        let files = {
            let mut st = self.st.borrow_mut();
            st.pending_target = None;
            std::mem::take(&mut st.pending)
        };
        self.show_pending();
        self.draw_peers();
        glib::spawn_future_local(self.clone().send(peer, files));
    }

    fn connect(self: &Rc<Self>, update_button: &gtk::Button) {
        let ui = self.clone();
        self.settings_button.connect_clicked(move |_| {
            let (visible, configured) = (
                ui.pages.visible_child_name().as_deref() == Some("settings"),
                ui.st.borrow().settings.configured,
            );
            if visible {
                if configured {
                    ui.close_settings();
                }
            } else {
                ui.show_settings(!configured);
            }
        });
        let ui = self.clone();
        update_button.connect_clicked(move |_| {
            glib::spawn_future_local(ui.clone().install_update());
        });

        // Closing hides to the background; reception keeps running.
        let ui = self.clone();
        self.window.connect_close_request(move |w| {
            let (exiting, running) = {
                let st = ui.st.borrow();
                (st.exiting, st.running)
            };
            if exiting {
                return glib::Propagation::Proceed;
            }
            // Nothing to keep alive before setup: closing the first-run window quits.
            if !running {
                glib::spawn_future_local(ui.clone().quit());
                return glib::Propagation::Stop;
            }
            w.set_visible(false);
            ui.tell_background();
            glib::Propagation::Stop
        });

        // Narrow windows stack the two columns.
        let ui = self.clone();
        self.window
            .connect_default_width_notify(move |w| ui.relayout(w.default_width()));

        // Window-wide drop: with a single receiver there is no need to aim.
        let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let ui = self.clone();
        target.connect_enter(move |_, _, _| {
            if !ui.can_drop() {
                return gdk::DragAction::empty();
            }
            ui.frame.add_css_class("frame-target");
            ui.drop_hint.add_css_class("target");
            let peers = ui.st.borrow().peers.clone();
            ui.drop_hint.set_text(&if peers.len() == 1 {
                format!("Отпусти файлы → {}", peers[0].name)
            } else {
                "Отпусти файлы на карточку получателя →".into()
            });
            gdk::DragAction::COPY
        });
        let ui = self.clone();
        target.connect_leave(move |_| ui.reset_drop());
        let ui = self.clone();
        target.connect_drop(move |_, value, _, _| {
            ui.reset_drop();
            if !ui.can_drop() {
                return false;
            }
            let paths = file_paths(value);
            let peers = ui.st.borrow().peers.clone();
            if peers.len() == 1 {
                glib::spawn_future_local(ui.clone().send(peers[0].clone(), paths));
            } else {
                ui.status
                    .set_text("Перетащи файлы на карточку нужного устройства");
            }
            true
        });
        self.pages.add_controller(target);

        let reveal = gio::SimpleAction::new("reveal", Some(glib::VariantTy::STRING));
        let ui = self.clone();
        reveal.connect_activate(move |_, param| {
            if let Some(path) = param.and_then(|p| p.get::<String>()) {
                ui.reveal(&path);
            }
        });
        self.app.add_action(&reveal);
        let show = gio::SimpleAction::new("show", None);
        let ui = self.clone();
        show.connect_activate(move |_, _| ui.window.present());
        self.app.add_action(&show);
        let quit = gio::SimpleAction::new("quit", None);
        let ui = self.clone();
        quit.connect_activate(move |_, _| {
            glib::spawn_future_local(ui.clone().quit());
        });
        self.app.add_action(&quit);
        self.app.set_accels_for_action("app.quit", &["<Control>q"]);
        self.app
            .set_accels_for_action("window.close", &["<Control>w"]);

        let (sender, receiver) = async_channel::unbounded();
        *self.tray.borrow_mut() = tray::spawn(sender);
        let ui = self.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = receiver.recv().await {
                match event {
                    TrayEvent::Show => ui.window.present(),
                    TrayEvent::Quit => ui.clone().quit().await,
                }
            }
        });

        let ui = self.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            if ui.st.borrow().exiting {
                return glib::ControlFlow::Break;
            }
            glib::spawn_future_local(ui.clone().poll());
            glib::ControlFlow::Continue
        });
    }

    fn relayout(&self, width: i32) {
        let narrow = width > 0 && width < 900;
        self.columns.set_orientation(if narrow {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
    }

    fn tell_background(&self) {
        let mut st = self.st.borrow_mut();
        if st.told_background || !st.settings.configured {
            return;
        }
        st.told_background = true;
        let body = if self.tray.borrow().is_some() {
            "DROP продолжает принимать файлы. Открыть или выйти — через значок в трее."
        } else {
            "DROP продолжает принимать файлы. Открыть снова — из меню приложений, выйти — Ctrl+Q."
        };
        let n = gio::Notification::new("DROP работает в фоне");
        n.set_body(Some(body));
        n.set_default_action("app.show");
        self.app.send_notification(Some("background"), &n);
    }

    fn can_drop(&self) -> bool {
        let st = self.st.borrow();
        self.pages.visible_child_name().as_deref() == Some("main")
            && !st.updating
            && !st.peers.is_empty()
    }

    fn reset_drop(&self) {
        self.frame.remove_css_class("frame-target");
        self.drop_hint.remove_css_class("target");
        self.drop_hint
            .set_text("Перетащи файлы или папки\nна карточку получателя →");
        let mut child = self.peer_list.first_child();
        while let Some(c) = child {
            c.remove_css_class("target");
            child = c.next_sibling();
        }
    }

    async fn initialize(self: Rc<Self>) {
        let settings = self.st.borrow().settings.clone();
        if !settings.configured {
            self.show_settings(true);
            return;
        }
        if let Err(e) = settings::validate_folder(&settings.folder) {
            // E.g. an unmounted drive: ask for another folder instead of failing silently.
            self.show_settings(false);
            if let Some(f) = self.settings_feedback.borrow().as_ref() {
                f.set_text(&format!(
                    "Папка приёма недоступна: {e}. Выберите другую и сохраните."
                ));
            }
            self.window.present();
            return;
        }
        let _ = self.clone().start(&settings.name, &settings.folder).await;
        if settings.check_updates {
            self.check_update(false).await;
        }
    }

    /// Returns the core's refusal instead of failing, so callers can keep the old settings.
    async fn start(self: Rc<Self>, name: &str, folder: &str) -> Result<(), String> {
        let id = self.st.borrow().settings.id.clone();
        // The core prefers TCP 41000 and falls back to a free port when it is taken.
        match engine::call(json!({"op":"start","config":{"id":id,"name":name,"folder":folder}}))
            .await
        {
            Ok(_) => {
                self.st.borrow_mut().running = true;
                self.identity
                    .set_text(&format!("{name} / ПРИЁМ: {}", tilde(folder)));
                self.status
                    .set_text("Готов к отправке и автоматическому приёму");
                Ok(())
            }
            Err(e) => {
                self.status
                    .set_text(&format!("Не удалось запустить: {e}{}", firewall_hint(&e)));
                Err(e)
            }
        }
    }

    async fn poll(self: Rc<Self>) {
        {
            let mut st = self.st.borrow_mut();
            if !st.running || st.polling || st.exiting {
                return;
            }
            st.polling = true;
        }
        let result = engine::call(json!({"op":"poll"})).await;
        self.st.borrow_mut().polling = false;
        let state = match result {
            Ok(v) => v,
            Err(e) => {
                self.status.set_text(&e);
                return;
            }
        };
        let mut peers: Vec<Peer> = state["peers"]
            .as_array()
            .map(|a| a.iter().filter_map(Peer::parse).collect())
            .unwrap_or_default();
        peers.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        let signature = peers
            .iter()
            .map(|p| {
                format!(
                    "{}|{}|{}|{}|{}",
                    p.id, p.name, p.address, p.port, p.platform
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let changed = {
            let mut st = self.st.borrow_mut();
            st.busy = state["busy"].as_bool().unwrap_or(false);
            st.peers = peers;
            let changed = st.peer_signature != signature;
            st.peer_signature = signature;
            changed
        };
        if changed {
            self.draw_peers();
        }
        self.send_pending_to_target();
        if let Some(events) = state["events"].as_array() {
            for event in events {
                self.process_event(event);
            }
        }
    }

    fn draw_peers(self: &Rc<Self>) {
        while let Some(child) = self.peer_list.first_child() {
            self.peer_list.remove(&child);
        }
        let peers = self.st.borrow().peers.clone();
        self.empty_peers.set_visible(peers.is_empty());
        for peer in peers {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
            card.add_css_class("card");
            card.append(&label(&peer.name, &["peer-name"]));
            let meta = label(
                &format!("{} / {}", peer.platform.to_uppercase(), peer.address),
                &["muted"],
            );
            meta.set_margin_top(8);
            meta.set_margin_bottom(12);
            card.append(&meta);
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let has_pending = !self.st.borrow().pending.is_empty();
            let files = button(
                if has_pending {
                    "ОТПРАВИТЬ СЮДА →"
                } else {
                    "ОТПРАВИТЬ ФАЙЛЫ →"
                },
                "primary",
            );
            let folder = button("ПАПКУ →", "secondary");
            actions.append(&files);
            actions.append(&folder);
            card.append(&actions);

            let (ui, p) = (self.clone(), peer.clone());
            files.connect_clicked(move |_| {
                let (ui, p) = (ui.clone(), p.clone());
                if !ui.st.borrow().pending.is_empty() {
                    ui.take_pending_and_send(p);
                    return;
                }
                glib::spawn_future_local(async move {
                    let dialog = gtk::FileDialog::builder()
                        .title(format!("Отправить → {}", p.name))
                        .accept_label("Отправить")
                        .modal(true)
                        .build();
                    if let Ok(model) = dialog.open_multiple_future(Some(&ui.window)).await {
                        ui.clone().send(p, list_paths(&model)).await;
                    }
                });
            });
            let (ui, p) = (self.clone(), peer.clone());
            folder.connect_clicked(move |_| {
                let (ui, p) = (ui.clone(), p.clone());
                glib::spawn_future_local(async move {
                    let dialog = gtk::FileDialog::builder()
                        .title(format!("Отправить папку → {}", p.name))
                        .accept_label("Отправить")
                        .modal(true)
                        .build();
                    if let Ok(model) = dialog
                        .select_multiple_folders_future(Some(&ui.window))
                        .await
                    {
                        ui.clone().send(p, list_paths(&model)).await;
                    }
                });
            });

            let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
            let (ui, c, name) = (self.clone(), card.clone(), peer.name.clone());
            target.connect_enter(move |_, _, _| {
                if !ui.can_drop() {
                    return gdk::DragAction::empty();
                }
                c.add_css_class("target");
                ui.frame.add_css_class("frame-target");
                ui.drop_hint.add_css_class("target");
                ui.drop_hint.set_text(&format!("Отпустить → {name}"));
                gdk::DragAction::COPY
            });
            let c = card.clone();
            target.connect_leave(move |_| c.remove_css_class("target"));
            let (ui, p) = (self.clone(), peer.clone());
            target.connect_drop(move |_, value, _, _| {
                ui.reset_drop();
                if !ui.can_drop() {
                    return false;
                }
                glib::spawn_future_local(ui.clone().send(p.clone(), file_paths(value)));
                true
            });
            card.add_controller(target);
            self.peer_list.append(&card);
        }
    }

    async fn send(self: Rc<Self>, peer: Peer, paths: Vec<String>) {
        if self.st.borrow().updating {
            self.status.set_text("Дождитесь завершения обновления");
            return;
        }
        if paths.is_empty() {
            self.status
                .set_text("Нет локальных файлов для отправки (удалённые и виртуальные расположения не поддерживаются)");
            return;
        }
        self.status.set_text("Подготовка файлов…");
        match engine::call(json!({"op":"send","address":peer.address,"port":peer.port,"name":peer.name,"paths":paths})).await {
            Ok(_) => {
                self.st.borrow_mut().busy = true;
                self.status.set_text("Передача началась");
            }
            Err(e) => self.status.set_text(&e),
        }
    }

    fn reveal(&self, path: &str) {
        let file = gio::File::for_path(path);
        let launcher = gtk::FileLauncher::new(Some(&file));
        let status = self.status.clone();
        let parent = file.parent();
        launcher.open_containing_folder(Some(&self.window), gio::Cancellable::NONE, move |r| {
            if r.is_err() {
                // No FileManager1 service and no portal: just open the folder.
                let uri = parent.map(|p| p.uri().to_string()).unwrap_or_default();
                if let Err(e) =
                    gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE)
                {
                    status.set_text(&format!("Не удалось открыть папку: {e}"));
                }
            }
        });
    }

    fn process_event(self: &Rc<Self>, e: &Value) {
        let kind = e["type"].as_str().unwrap_or("");
        if kind == "error" {
            let message = e["message"].as_str().unwrap_or("Ошибка");
            self.status
                .set_text(&format!("{message}{}", firewall_hint(message)));
            return;
        }
        let Some(id) = e["transfer"].as_str().map(String::from) else {
            return;
        };
        if kind == "transfer" {
            self.add_transfer(&id, e);
        }
        let mut finished = None;
        {
            let mut st = self.st.borrow_mut();
            let notifications = st.settings.notifications;
            let folder = st.settings.folder.clone();
            let Some(row) = st.transfers.get_mut(&id) else {
                return;
            };
            match kind {
                "state" => {
                    let status = e["status"].as_str().unwrap_or("active").to_string();
                    row.pause
                        .set_sensitive(e["resumable"].as_bool().unwrap_or(false));
                    row.pause.set_label(if status == "paused" {
                        "ПРОДОЛЖИТЬ"
                    } else {
                        "ПАУЗА"
                    });
                    if status == "paused" {
                        row.text
                            .set_text(&format!("На паузе / {}", bytes(row.total)));
                    } else if status == "waiting" {
                        row.text
                            .set_text(&format!("Ожидание соединения / {}", bytes(row.total)));
                    }
                    *row.state.borrow_mut() = status;
                }
                "file" => {
                    if let Some(path) = e["local_path"].as_str() {
                        row.saved_path.get_or_insert_with(|| path.to_string());
                        if let Some((b, cell)) = e["path"].as_str().and_then(|p| row.files.get(p)) {
                            *cell.borrow_mut() = Some(path.to_string());
                            b.set_sensitive(true);
                            b.set_tooltip_text(Some(&format!("Показать в папке: {path}")));
                        }
                    }
                }
                "progress" => {
                    let done = e["done"].as_f64().unwrap_or(0.0);
                    let total = e["total"].as_f64().unwrap_or(0.0);
                    row.bar
                        .set_fraction(if total > 0.0 { done / total } else { 1.0 });
                    row.text.set_text(&format!(
                        "{} / {}\n{}/с · осталось {}",
                        bytes(done),
                        bytes(total),
                        bytes(e["speed"].as_f64().unwrap_or(0.0)),
                        eta(e["remaining"].as_f64().unwrap_or(0.0))
                    ));
                }
                "complete" => {
                    let status = e["status"].as_str().unwrap_or("error");
                    let head = match status {
                        "done" => "Готово ✓".to_string(),
                        "cancelled" => "Отменено".to_string(),
                        _ => format!("Ошибка: {}", e["message"].as_str().unwrap_or("")),
                    };
                    row.text.set_text(&format!("{head} / {}", bytes(row.total)));
                    row.cancel.set_visible(false);
                    row.pause.set_visible(false);
                    if status == "done" {
                        row.bar.set_fraction(1.0);
                        if row.receiving && notifications {
                            finished = Some((
                                if row.first_name.is_empty() {
                                    "Файлы получены".to_string()
                                } else {
                                    row.first_name.clone()
                                },
                                format!("{} объектов / {}", row.count, bytes(row.total)),
                                row.saved_path.clone().unwrap_or(folder),
                            ));
                        }
                    }
                    if status == "done" {
                        self.status.set_text("Передача завершена");
                    }
                }
                _ => {}
            }
            if kind == "complete" {
                st.transfers.remove(&id);
            }
        }
        if kind == "complete" {
            self.trim_history();
        }
        if let Some((title, detail, path)) = finished {
            self.notify_received(&title, &detail, &path);
        }
    }

    fn notify_received(&self, title: &str, detail: &str, path: &str) {
        let n = gio::Notification::new(&format!("Получено: {title}"));
        n.set_body(Some(detail));
        n.set_icon(&gio::ThemedIcon::new(crate::APP_ID));
        n.set_default_action_and_target_value("app.reveal", Some(&path.to_variant()));
        n.add_button_with_target_value("Показать в папке", "app.reveal", Some(&path.to_variant()));
        self.app.send_notification(Some("received"), &n);
    }

    fn add_transfer(self: &Rc<Self>, id: &str, e: &Value) {
        if self.st.borrow().transfers.contains_key(id) {
            return;
        }
        self.empty_transfers.set_visible(false);
        let receiving = e["direction"] == "receive";
        let total = e["total"].as_f64().unwrap_or(0.0);
        let count = e["display_count"]
            .as_u64()
            .or_else(|| e["files"].as_u64())
            .unwrap_or(0);
        let block = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let title = label(
            &format!(
                "{} {}",
                if receiving { "↓" } else { "↑" },
                e["name"].as_str().unwrap_or("")
            ),
            &["transfer-title"],
        );
        title.set_margin_bottom(4);
        block.append(&title);

        let mut files = HashMap::new();
        let mut first_name = String::new();
        let entries = e["display_entries"]
            .as_array()
            .or_else(|| e["entries"].as_array())
            .cloned()
            .unwrap_or_default();
        for entry in &entries {
            let path = entry["path"].as_str().unwrap_or("").to_string();
            if first_name.is_empty() {
                first_name = path.clone();
            }
            let text = format!(
                "{path} / {}{}",
                if entry["directory"].as_bool().unwrap_or(false) {
                    "ПАПКА · "
                } else {
                    ""
                },
                bytes(entry["size"].as_f64().unwrap_or(0.0))
            );
            let inner = label(&text, &[]);
            let b = gtk::Button::new();
            b.set_child(Some(&inner));
            b.add_css_class("file");
            b.set_tooltip_text(Some(&text));
            let cell = Rc::new(RefCell::new(entry["local_path"].as_str().map(String::from)));
            b.set_sensitive(cell.borrow().is_some());
            let (ui, c) = (self.clone(), cell.clone());
            b.connect_clicked(move |_| {
                if let Some(p) = c.borrow().as_deref() {
                    ui.reveal(p);
                }
            });
            block.append(&b);
            files.insert(path, (b, cell));
        }
        if count as usize > files.len() {
            block.append(&label(
                &format!("Ещё {} объектов", count as usize - files.len()),
                &["muted"],
            ));
        }
        let text = label(&format!("{count} объектов / {}", bytes(total)), &["muted"]);
        text.set_margin_top(8);
        text.set_margin_bottom(12);
        block.append(&text);
        let bar = gtk::ProgressBar::new();
        block.append(&bar);
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        controls.set_margin_top(12);
        let pause = button("ПАУЗА", "primary");
        pause.set_sensitive(e["resumable"].as_bool().unwrap_or(false));
        let cancel = button("ОТМЕНИТЬ", "secondary");
        controls.append(&pause);
        controls.append(&cancel);
        block.append(&controls);

        let state = Rc::new(RefCell::new("active".to_string()));
        let (ui, tid, s) = (self.clone(), id.to_string(), state.clone());
        pause.connect_clicked(move |_| {
            let op = if *s.borrow() == "paused" {
                "resume"
            } else {
                "pause"
            };
            let (ui, tid) = (ui.clone(), tid.clone());
            glib::spawn_future_local(async move {
                if let Err(e) = engine::call(json!({"op":op,"transfer":tid})).await {
                    ui.status.set_text(&e);
                }
            });
        });
        let tid = id.to_string();
        cancel.connect_clicked(move |_| {
            let tid = tid.clone();
            glib::spawn_future_local(async move {
                let _ = engine::call(json!({"op":"cancel","transfer":tid})).await;
            });
        });

        self.transfer_list.prepend(&block);
        let mut st = self.st.borrow_mut();
        st.history.push((id.to_string(), block.upcast()));
        st.transfers.insert(
            id.to_string(),
            Row {
                bar,
                text,
                pause,
                cancel,
                state,
                total,
                receiving,
                first_name,
                count,
                saved_path: None,
                files,
            },
        );
        drop(st);
        self.trim_history();
    }

    /// Keep the UI bounded during long sessions; active transfers are retained.
    fn trim_history(&self) {
        let mut st = self.st.borrow_mut();
        while st.history.len() > HISTORY_LIMIT {
            let Some(index) = st
                .history
                .iter()
                .position(|(id, _)| !st.transfers.contains_key(id))
            else {
                break;
            };
            let (_, widget) = st.history.remove(index);
            self.transfer_list.remove(&widget);
        }
    }

    fn close_settings(&self) {
        self.pages.set_visible_child_name("main");
        self.connection.set_visible(true);
        self.settings_button.set_label("НАСТРОЙКИ ↗");
    }

    fn show_settings(self: &Rc<Self>, first: bool) {
        self.pages.set_visible_child_name("settings");
        self.connection.set_visible(false);
        self.settings_button.set_label(if first {
            "ПЕРВОЕ ВКЛЮЧЕНИЕ"
        } else {
            "НАЗАД ←"
        });
        let page = &self.settings_page;
        while let Some(child) = page.first_child() {
            page.remove(&child);
        }
        let (settings, release) = {
            let st = self.st.borrow();
            (st.settings.clone(), st.release.clone())
        };
        let heading = label(
            &if first {
                "[ HELLO, FRIEND ]".to_string()
            } else {
                format!("[ SETTINGS / V {} ]", updates::CURRENT)
            },
            &["accent"],
        );
        heading.set_margin_bottom(20);
        page.append(&heading);
        if first {
            let intro = label(
                "Задай имя, под которым тебя увидят другие устройства, и папку для полученных файлов. Приём идёт автоматически.",
                &["muted"],
            );
            intro.set_margin_bottom(20);
            page.append(&intro);
        }

        page.append(&label("Имя устройства", &[]));
        let name = gtk::Entry::new();
        name.set_text(&settings.name);
        name.set_width_chars(40);
        name.set_max_length(64);
        name.set_margin_top(8);
        name.set_margin_bottom(16);
        page.append(&name);

        page.append(&label("Папка для полученных файлов", &[]));
        let folder = gtk::Entry::new();
        folder.set_text(&settings.folder);
        folder.set_editable(false);
        folder.set_can_focus(false);
        folder.set_margin_top(8);
        folder.set_margin_bottom(8);
        page.append(&folder);
        let choose = button("ВЫБРАТЬ ПАПКУ", "secondary");
        let (ui, f) = (self.clone(), folder.clone());
        choose.connect_clicked(move |_| {
            let (ui, f) = (ui.clone(), f.clone());
            glib::spawn_future_local(async move {
                let dialog = gtk::FileDialog::builder()
                    .title("Папка для полученных файлов")
                    .modal(true)
                    .initial_folder(&gio::File::for_path(f.text().as_str()))
                    .build();
                if let Ok(dir) = dialog.select_folder_future(Some(&ui.window)).await {
                    if let Some(path) = dir.path() {
                        f.set_text(&path.to_string_lossy());
                    }
                }
            });
        });
        page.append(&choose);

        let login = gtk::CheckButton::with_label("Запускать DROP при входе в систему");
        login.set_active(autostart::enabled());
        login.set_margin_top(24);
        let notifications = gtk::CheckButton::with_label("Уведомлять о полученных файлах");
        notifications.set_active(settings.notifications);
        notifications.set_margin_top(12);
        let auto = gtk::CheckButton::with_label("Проверять обновления при запуске");
        auto.set_active(settings.check_updates);
        auto.set_margin_top(12);
        auto.set_margin_bottom(20);
        if !first {
            page.append(&login);
            page.append(&notifications);
            page.append(&auto);
            let check = button(
                &match &release {
                    Some(r) => format!("ОБНОВИТЬСЯ ДО {}", r.version),
                    None => "ПРОВЕРИТЬ ОБНОВЛЕНИЯ".into(),
                },
                "secondary",
            );
            let ui = self.clone();
            check.connect_clicked(move |b| {
                let (ui, b) = (ui.clone(), b.clone());
                glib::spawn_future_local(async move {
                    if ui.st.borrow().release.is_none() {
                        ui.check_update(true).await;
                        if let Some(r) = ui.st.borrow().release.as_ref() {
                            b.set_label(&format!("ОБНОВИТЬСЯ ДО {}", r.version));
                        }
                    } else {
                        ui.clone().install_update().await;
                    }
                });
            });
            page.append(&check);
            let hint = label(
                "Если устройства не видят друг друга, разрешите в файрволе TCP 41000 и UDP 41001.\n\
                 ufw: sudo ufw allow DROP · firewalld: sudo firewall-cmd --permanent --add-service=hl100-drop",
                &["muted", "small"],
            );
            hint.set_selectable(true);
            hint.set_margin_top(20);
            page.append(&hint);
        }

        let feedback = label("", &["muted"]);
        feedback.set_margin_top(16);
        page.append(&feedback);
        *self.settings_feedback.borrow_mut() = Some(feedback.clone());

        let save = button(
            if first {
                "НАЧАТЬ →"
            } else {
                "СОХРАНИТЬ"
            },
            "primary",
        );
        save.set_margin_top(20);
        let ui = self.clone();
        let name_focus = name.clone();
        save.connect_clicked(move |save| {
            let ui = ui.clone();
            let (name, folder, login, notifications, auto, feedback, save) = (
                name.text().trim().to_string(),
                folder.text().to_string(),
                login.is_active(),
                notifications.is_active(),
                auto.is_active(),
                feedback.clone(),
                save.clone(),
            );
            glib::spawn_future_local(async move {
                if name.is_empty() || folder.is_empty() {
                    feedback.set_text("Укажите имя и папку");
                    return;
                }
                let (running, busy, old) = {
                    let st = ui.st.borrow();
                    (st.running, st.busy, st.settings.clone())
                };
                let restart = !running || old.name != name || old.folder != folder;
                if restart && busy {
                    feedback.set_text("Имя и папку можно изменить после завершения передач");
                    return;
                }
                if let Err(e) = settings::validate_folder(&folder) {
                    feedback.set_text(&format!("Нет доступа к папке: {e}"));
                    return;
                }
                save.set_sensitive(false);
                // Persist only what the core accepted; otherwise the next launch
                // would use settings that never ran.
                if restart {
                    if let Err(refusal) = ui.clone().start(&name, &folder).await {
                        if running {
                            let _ = ui.clone().start(&old.name, &old.folder).await;
                        }
                        feedback.set_text(&format!("Не удалось применить: {refusal}"));
                        save.set_sensitive(true);
                        return;
                    }
                }
                if !first && autostart::enabled() != login {
                    if let Err(e) = autostart::set(login) {
                        feedback.set_text(&format!("Не удалось изменить автозапуск: {e}"));
                    }
                }
                let saved = {
                    let mut st = ui.st.borrow_mut();
                    st.settings.name = name;
                    st.settings.folder = folder;
                    st.settings.configured = true;
                    if !first {
                        st.settings.check_updates = auto;
                        st.settings.notifications = notifications;
                    }
                    st.settings.save()
                };
                save.set_sensitive(true);
                if let Err(e) = saved {
                    feedback.set_text(&format!("Не удалось сохранить: {e}"));
                    return;
                }
                if !notifications {
                    ui.app.withdraw_notification("received");
                }
                ui.close_settings();
                if first {
                    // Same default as other platforms: start with the session.
                    let _ = autostart::set(true);
                    if ui.st.borrow().settings.check_updates {
                        ui.check_update(false).await;
                    }
                }
            });
        });
        page.append(&save);
        if first {
            name_focus.grab_focus();
        }
    }

    async fn check_update(&self, manual: bool) {
        let result = gio::spawn_blocking(updates::check)
            .await
            .unwrap_or_else(|_| Err("Ошибка проверки".into()));
        let feedback = self.settings_feedback.borrow().clone();
        match result {
            Ok(release) => {
                self.update_banner.set_visible(release.is_some());
                if let Some(r) = &release {
                    self.update_text
                        .set_text(&format!("Доступна новая версия {}", r.version));
                }
                if manual {
                    if let Some(f) = feedback {
                        f.set_text(&match &release {
                            None => "Установлена актуальная версия".to_string(),
                            Some(r) => format!("Доступна версия {}\n{}", r.version, r.notes),
                        });
                    }
                }
                self.st.borrow_mut().release = release;
            }
            Err(e) => {
                if manual {
                    if let Some(f) = feedback {
                        f.set_text(&format!("Не удалось проверить обновления: {e}"));
                    }
                }
            }
        }
    }

    async fn alert(&self, message: &str, detail: &str, buttons: &[&str]) -> i32 {
        let dialog = gtk::AlertDialog::builder()
            .message(message)
            .detail(detail)
            .modal(true)
            .buttons(buttons)
            .cancel_button(0)
            .default_button(buttons.len() as i32 - 1)
            .build();
        dialog.choose_future(Some(&self.window)).await.unwrap_or(0)
    }

    async fn install_update(self: Rc<Self>) {
        let (release, updating, busy) = {
            let st = self.st.borrow();
            (st.release.clone(), st.updating, st.busy)
        };
        let Some(release) = release else { return };
        if updating {
            return;
        }
        let image = updates::running_appimage();
        if image.is_none() || release.appimage.is_none() {
            // Package-manager installs: hand over to the release page.
            let launcher = gtk::UriLauncher::new(&release.page);
            launcher.launch(Some(&self.window), gio::Cancellable::NONE, |_| {});
            self.update_text.set_text(&format!(
                "Версия {}: скачайте пакет для своего дистрибутива на странице релиза",
                release.version
            ));
            return;
        }
        if busy {
            self.alert(
                "Обновление",
                "Сначала дождитесь завершения передач.",
                &["OK"],
            )
            .await;
            return;
        }
        self.st.borrow_mut().updating = true;
        let (tx, rx) = async_channel::unbounded::<f64>();
        let text = self.update_text.clone();
        glib::spawn_future_local(async move {
            while let Ok(p) = rx.recv().await {
                text.set_text(&format!("Загрузка обновления: {p:.0}%"));
            }
        });
        let r = release.clone();
        let result = gio::spawn_blocking(move || {
            updates::install_appimage(&r, |p| {
                let _ = tx.send_blocking(p);
            })
        })
        .await
        .unwrap_or_else(|_| Err("Ошибка загрузки".into()));
        let finish = |ui: &Rc<Self>| {
            ui.st.borrow_mut().updating = false;
            ui.update_text
                .set_text(&format!("Доступна новая версия {}", release.version));
        };
        let target = match result {
            Ok(t) => t,
            Err(e) => {
                finish(&self);
                self.alert(
                    "Обновление",
                    &format!("Не удалось обновиться: {e}"),
                    &["OK"],
                )
                .await;
                return;
            }
        };
        // A transfer may have started while downloading.
        if let Ok(state) = engine::call(json!({"op":"poll"})).await {
            if let Some(events) = state["events"].as_array() {
                for e in events {
                    self.process_event(e);
                }
            }
            if state["busy"].as_bool().unwrap_or(false) {
                finish(&self);
                self.alert(
                    "Обновление",
                    "Началась входящая передача. Новая версия уже загружена и запустится при следующем старте DROP.",
                    &["OK"],
                )
                .await;
                return;
            }
        }
        let spawned = std::process::Command::new(&target)
            .env("HL100_WAIT_PID", std::process::id().to_string())
            .env_remove("APPIMAGE")
            .env_remove("APPDIR")
            .spawn();
        if let Err(e) = spawned {
            finish(&self);
            self.alert(
                "Обновление",
                &format!("Новая версия загружена, но не запустилась: {e}"),
                &["OK"],
            )
            .await;
            return;
        }
        self.st.borrow_mut().busy = false;
        self.quit().await;
    }

    /// Stopping waits for network threads; keep the main loop responsive meanwhile.
    async fn quit(self: Rc<Self>) {
        let (exiting, busy) = {
            let st = self.st.borrow();
            (st.exiting, st.busy)
        };
        if exiting {
            return;
        }
        if busy {
            self.window.present();
            if self
                .alert(
                    "Выйти из DROP?",
                    "Передачи будут прерваны.",
                    &["Отмена", "Выйти"],
                )
                .await
                != 1
            {
                return;
            }
        }
        self.st.borrow_mut().exiting = true;
        self.window.set_visible(false);
        self.app.withdraw_notification("received");
        let tray = self.tray.borrow_mut().take();
        if let Some(tray) = tray {
            gio::spawn_blocking(move || tray.shutdown()).await.ok();
        }
        if self.st.borrow().running {
            gio::spawn_blocking(engine::stop_blocking).await.ok();
        }
        self.hold.borrow_mut().take();
        self.window.close();
        self.app.quit();
    }
}

/// Show home-relative paths the way Linux users read them: `~/Downloads/…`.
fn tilde(path: &str) -> String {
    let home = glib::home_dir();
    match std::path::Path::new(path).strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.into(),
    }
}

fn firewall_hint(message: &str) -> &'static str {
    let m = message.to_lowercase();
    if m.contains("permission denied") || m.contains("отказано в доступе") {
        " — проверьте файрвол (UDP 41001, TCP 41000)"
    } else {
        ""
    }
}
