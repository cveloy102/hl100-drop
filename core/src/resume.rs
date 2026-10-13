//! Resumption is negotiated by optional v1 fields; legacy clients still stream v1.
use super::*;
use std::net::Shutdown;

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    id: String,
    wire_id: String,
    direction: String,
    name: String,
    peer_id: String,
    address: SocketAddr,
    entries: Vec<Entry>,
    sources: Vec<Value>,
    status: String,
    negotiated: bool,
    cancel_pending: bool,
}
pub(super) struct Control {
    record: Mutex<Record>,
    paused: AtomicBool,
    cancelled: AtomicBool,
    pub(super) active: AtomicBool,
    socket: Mutex<Option<TcpStream>>,
}
impl Control {
    fn new(record: Record) -> Arc<Self> {
        Arc::new(Self {
            paused: AtomicBool::new(record.status == "paused"),
            cancelled: AtomicBool::new(record.status == "cancelled"),
            record: Mutex::new(record),
            active: AtomicBool::new(false),
            socket: Mutex::new(None),
        })
    }
    pub(super) fn disconnect(&self) {
        if let Some(socket) = self.socket.lock().unwrap().as_ref() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
    fn attach(&self, socket: &TcpStream) -> io::Result<()> {
        *self.socket.lock().unwrap() = Some(socket.try_clone()?);
        Ok(())
    }
    fn detach(&self) {
        self.socket.lock().unwrap().take();
    }
}

pub(super) fn save_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    let temp = path.with_extension("tmp");
    let mut file = File::create(&temp)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.flush()?;
    drop(file);
    fs::rename(temp, path)
}
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
impl Engine {
    fn spawn_resume(&self, work: impl FnOnce() + Send + 'static) {
        let mut workers = self.resume_workers.lock().unwrap();
        workers.retain(|w| !w.is_finished());
        workers.push(thread::spawn(work));
    }
    fn record_path(&self, id: &str) -> io::Result<PathBuf> {
        if !safe_id(id) {
            return Err(io::Error::other("Недопустимый идентификатор передачи"));
        }
        Ok(self
            .storage
            .state_root()
            .ok_or_else(|| io::Error::other("Состояние передачи недоступно"))?
            .join("jobs")
            .join(format!("{id}.json")))
    }
    fn save_control(&self, control: &Control) -> io::Result<()> {
        let record = control.record.lock().unwrap();
        save_json(&self.record_path(&record.id)?, &*record)
    }
    fn announce(&self, control: &Control, sources: Option<&[Source]>) {
        let record = control.record.lock().unwrap();
        let display = display_entries(&record.entries);
        let values = display
            .iter()
            .take(200)
            .map(|entry| {
                let mut value = json!(entry);
                if let Some(path) = sources
                    .and_then(|s| s.iter().find(|s| s.entry.path == entry.path))
                    .and_then(|s| s.local_path.as_ref())
                {
                    value["local_path"] = json!(path);
                }
                value
            })
            .collect::<Vec<_>>();
        self.event(json!({"type":"transfer","transfer":record.id,"direction":record.direction,"name":record.name,"total":record.entries.iter().map(|e|e.size).sum::<u64>(),"files":record.entries.len(),"entries":record.entries.iter().take(200).collect::<Vec<_>>(),"display_entries":values,"display_count":display.len(),"resumable":record.negotiated}));
    }
    fn state(&self, control: &Control, status: &str, message: &str) {
        let mut record = control.record.lock().unwrap();
        let changed = record.status != status;
        record.status = status.into();
        if changed || !message.is_empty() {
            self.event(json!({"type":"state","transfer":record.id,"status":status,"message":message,"resumable":record.negotiated}));
            if let Ok(path) = self.record_path(&record.id) {
                if let Err(error) = save_json(&path, &*record) {
                    self.event(json!({"type":"error","message":format!("Не удалось сохранить передачу: {error}")}));
                }
            }
        }
    }
    fn transfer_end(&self, control: &Control, status: &str, message: &str) {
        self.state(control, status, message);
        let record = control.record.lock().unwrap();
        self.event(
            json!({"type":"complete","transfer":record.id,"status":status,"message":message}),
        );
    }
    fn check_control(&self, control: &Control) -> io::Result<()> {
        if self.stopped.load(Ordering::Relaxed)
            || control.paused.load(Ordering::Relaxed)
            || control.cancelled.load(Ordering::Relaxed)
        {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Передача остановлена",
            ));
        }
        Ok(())
    }
    pub(super) fn control_transfer(&self, id: &str, action: &str) -> io::Result<()> {
        let control = self
            .resumed
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| io::Error::other("Передача не поддерживает возобновление"))?;
        let record = control.record.lock().unwrap().clone();
        if matches!(record.status.as_str(), "done" | "cancelled" | "error") {
            return Err(io::Error::other("Передача уже завершена"));
        }
        if action != "cancel" && !record.negotiated {
            return Err(io::Error::other(
                "Обновите приложение получателя для паузы и возобновления",
            ));
        }
        match action {
            "pause" => {
                control.paused.store(true, Ordering::Relaxed);
                control.disconnect();
                self.state(&control, "paused", "");
            }
            "resume" => {
                control.paused.store(false, Ordering::Relaxed);
                self.state(&control, "waiting", "");
            }
            "cancel" => {
                control.cancelled.store(true, Ordering::Relaxed);
                control.disconnect();
                control.record.lock().unwrap().cancel_pending =
                    record.direction == "send" && record.negotiated;
                if record.direction == "receive" && !control.active.load(Ordering::Relaxed) {
                    self.storage.abort(id);
                }
                self.transfer_end(&control, "cancelled", "");
            }
            _ => return Err(io::Error::other("Неизвестное действие")),
        }
        self.save_control(&control)
    }
    pub(super) fn restore_transfers(self: &Arc<Self>) {
        let Some(root) = self.storage.state_root() else {
            return;
        };
        let Ok(files) = fs::read_dir(root.join("jobs")) else {
            return;
        };
        for item in files.flatten() {
            if item.path().extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(record) = fs::read(item.path())
                .ok()
                .and_then(|data| serde_json::from_slice::<Record>(&data).ok())
            else {
                continue;
            };
            if !safe_id(&record.id)
                || !safe_id(&record.wire_id)
                || validate_entries(&record.entries).is_err()
            {
                continue;
            }
            if record.status == "done" || (record.status == "cancelled" && !record.cancel_pending) {
                continue;
            }
            let sender = record.direction == "send";
            let mut record = record;
            if record.status == "active" {
                record.status = "waiting".into();
            }
            let sources = record
                .entries
                .iter()
                .zip(&record.sources)
                .map(|(entry, locator)| {
                    let storage = self.storage.clone();
                    let value = locator.clone();
                    Source {
                        entry: entry.clone(),
                        local_path: locator["path"].as_str().map(PathBuf::from),
                        locator: Some(locator.clone()),
                        open: if entry.directory {
                            None
                        } else {
                            Some(Box::new(move || storage.reopen_source(&value))
                                as Box<dyn FnMut() -> io::Result<File> + Send>)
                        },
                    }
                })
                .collect::<Vec<_>>();
            let control = Control::new(record);
            self.resumed
                .lock()
                .unwrap()
                .insert(control.record.lock().unwrap().id.clone(), control.clone());
            if control.record.lock().unwrap().status != "cancelled" {
                self.announce(&control, if sender { Some(&sources) } else { None });
                let record = control.record.lock().unwrap();
                self.event(json!({"type":"state","transfer":record.id,"status":record.status,"resumable":record.negotiated}));
            }
            if sender && sources.len() == control.record.lock().unwrap().entries.len() {
                let engine = self.clone();
                if !control.paused.load(Ordering::Relaxed)
                    && !control.cancelled.load(Ordering::Relaxed)
                {
                    control.active.store(true, Ordering::Relaxed);
                }
                self.spawn_resume(move || engine.send_loop(control, sources));
            }
        }
    }
    pub(super) fn send_resumable(
        self: &Arc<Self>,
        address: SocketAddr,
        name: String,
        sources: Vec<Source>,
    ) -> io::Result<String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let id = format!("tx-{stamp:x}-{:x}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let peer_id = self
            .peers
            .lock()
            .unwrap()
            .values()
            .find(|(p, _)| p.address == address.ip().to_string() && p.port == address.port())
            .map(|(p, _)| p.id.clone())
            .unwrap_or_default();
        let record = Record {
            id: id.clone(),
            wire_id: id.clone(),
            direction: "send".into(),
            name,
            peer_id,
            address,
            entries: sources.iter().map(|s| s.entry.clone()).collect(),
            sources: sources.iter().map(|s| s.locator.clone().unwrap()).collect(),
            status: "waiting".into(),
            negotiated: false,
            cancel_pending: false,
        };
        let control = Control::new(record);
        self.save_control(&control)?;
        self.announce(&control, Some(&sources));
        self.resumed
            .lock()
            .unwrap()
            .insert(id.clone(), control.clone());
        control.active.store(true, Ordering::Relaxed);
        let engine = self.clone();
        self.spawn_resume(move || engine.send_loop(control, sources));
        Ok(id)
    }
    fn send_loop(self: Arc<Self>, control: Arc<Control>, mut sources: Vec<Source>) {
        while !self.stopped.load(Ordering::Relaxed) {
            if control.cancelled.load(Ordering::Relaxed) {
                if !control.record.lock().unwrap().cancel_pending {
                    break;
                }
                if self.withdraw(&control).is_ok() {
                    control.record.lock().unwrap().cancel_pending = false;
                    let _ = self.save_control(&control);
                    break;
                }
            } else if !control.paused.load(Ordering::Relaxed) {
                control.active.store(true, Ordering::Relaxed);
                let result = self.send_attempt(&control, &mut sources);
                control.detach();
                if self.stopped.load(Ordering::Relaxed) {
                    break;
                }
                if control.cancelled.load(Ordering::Relaxed) {
                    control.active.store(false, Ordering::Relaxed);
                    continue;
                }
                if control.paused.load(Ordering::Relaxed) {
                    self.state(&control, "paused", "");
                } else {
                    match result {
                        Ok(()) => {
                            self.transfer_end(&control, "done", "");
                            break;
                        }
                        Err(error)
                            if error.kind() == io::ErrorKind::InvalidData
                                || (!control.record.lock().unwrap().negotiated
                                    && error.kind() != io::ErrorKind::ConnectionRefused
                                    && error.kind() != io::ErrorKind::TimedOut
                                    && error.kind() != io::ErrorKind::NotConnected) =>
                        {
                            self.transfer_end(&control, "error", &error.to_string());
                            break;
                        }
                        Err(_) => self.state(
                            &control,
                            "waiting",
                            "Соединение потеряно. Продолжим после восстановления связи",
                        ),
                    }
                }
            }
            control.active.store(false, Ordering::Relaxed);
            for _ in 0..20 {
                if self.stopped.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
        control.detach();
        if self.stopped.load(Ordering::Relaxed) && !control.cancelled.load(Ordering::Relaxed) {
            self.state(
                &control,
                if control.paused.load(Ordering::Relaxed) {
                    "paused"
                } else {
                    "waiting"
                },
                "",
            );
        }
        control.active.store(false, Ordering::Relaxed);
    }
    fn address(&self, record: &Record) -> SocketAddr {
        self.peers
            .lock()
            .unwrap()
            .get(&record.peer_id)
            .and_then(|(p, _)| format!("{}:{}", p.address, p.port).parse().ok())
            .unwrap_or(record.address)
    }
    fn offer(&self, record: &Record, action: &str) -> Offer {
        let config = self.config.lock().unwrap();
        Offer {
            protocol: 1,
            id: config.id.clone(),
            name: config.name.clone(),
            transfer: record.wire_id.clone(),
            entries: record.entries.clone(),
            resume: true,
            action: action.into(),
        }
    }
    fn withdraw(&self, control: &Control) -> io::Result<()> {
        let record = control.record.lock().unwrap().clone();
        let mut stream =
            TcpStream::connect_timeout(&self.address(&record), Duration::from_secs(2))?;
        setup_stream(&stream)?;
        frame_write(&mut stream, &self.offer(&record, "cancel"))?;
        let reply: Value = frame_read(&mut stream)?;
        if reply["ok"] == true {
            Ok(())
        } else {
            Err(io::Error::other("Получатель недоступен"))
        }
    }
    fn send_attempt(&self, control: &Control, sources: &mut [Source]) -> io::Result<()> {
        self.check_control(control)?;
        let record = control.record.lock().unwrap().clone();
        let mut stream =
            TcpStream::connect_timeout(&self.address(&record), Duration::from_secs(2))?;
        setup_stream(&stream)?;
        control.attach(&stream)?;
        self.check_control(control)?;
        frame_write(&mut stream, &self.offer(&record, ""))?;
        let reply: Value = frame_read(&mut stream)?;
        if reply["status"] == "cancelled" {
            control.cancelled.store(true, Ordering::Relaxed);
            self.transfer_end(control, "cancelled", "");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Получатель отменил передачу",
            ));
        }
        if reply["status"] == "paused" || reply["status"] == "busy" {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Получатель приостановил передачу",
            ));
        }
        if reply["ok"] != true {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                reply["error"].as_str().unwrap_or("Получатель отказал"),
            ));
        }
        let negotiated = reply["resume"] == true;
        if record.negotiated && !negotiated {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Для продолжения обновите приложение получателя",
            ));
        }
        if negotiated {
            if let Some(id) = reply["id"].as_str() {
                if !record.peer_id.is_empty() && record.peer_id != id {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "По этому адресу появилось другое устройство. Выберите получателя заново",
                    ));
                }
                control.record.lock().unwrap().peer_id = id.into();
            }
        }
        control.record.lock().unwrap().negotiated = negotiated;
        self.check_control(control)?;
        self.state(control, "active", "");
        self.save_control(control)?;
        // One offset per file at connection setup; no block acknowledgements.
        let offsets = if negotiated {
            serde_json::from_value::<Vec<(u64, bool)>>(reply["offsets"].clone())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            vec![(0, false); sources.len()]
        };
        if offsets.len() != sources.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Неверное состояние получателя",
            ));
        }
        let total = record.entries.iter().map(|e| e.size).sum();
        let mut done: u64 = offsets.iter().map(|(offset, _)| *offset).sum();
        let base = done;
        let started = Instant::now();
        self.resume_progress(&record.id, done, total, base, started);
        let mut update = Instant::now();
        let mut buffer = vec![0; BUFFER];
        for (source, (offset, finished)) in sources.iter_mut().zip(&offsets) {
            if *offset > source.entry.size || (*finished && *offset != source.entry.size) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Неверная позиция возобновления",
                ));
            }
            if source.entry.directory || *finished {
                continue;
            }
            self.check_control(control)?;
            let mut file = if let Some(locator) = &source.locator {
                self.storage.reopen_source(locator)
            } else {
                source
                    .open
                    .as_mut()
                    .ok_or_else(|| io::Error::other("Файл недоступен"))?()
            }
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            if *offset > 0 {
                file.seek(SeekFrom::Start(*offset)).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Этот источник не позволяет продолжить чтение. Отправьте файл заново",
                    )
                })?;
            }
            let mut remaining = source.entry.size - offset;
            while remaining > 0 {
                self.check_control(control)?;
                let n = file.read(&mut buffer[..remaining.min(BUFFER as u64) as usize])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Исходный файл изменился или недоступен",
                    ));
                }
                stream.write_all(&buffer[..n])?;
                remaining -= n as u64;
                done += n as u64;
                if update.elapsed() >= Duration::from_millis(150) {
                    self.resume_progress(&record.id, done, total, base, started);
                    update = Instant::now();
                }
            }
            let ack: Value = frame_read(&mut stream)?;
            if ack["ok"] != true {
                return Err(io::Error::other(
                    ack["error"].as_str().unwrap_or("Ошибка сохранения"),
                ));
            }
        }
        let ack: Value = frame_read(&mut stream)?;
        if ack["ok"] != true {
            return Err(io::Error::other("Получатель не завершил передачу"));
        }
        self.resume_progress(&record.id, total, total, base, started);
        Ok(())
    }
    pub(super) fn receive_resumable(&self, mut stream: TcpStream, offer: Offer) -> io::Result<()> {
        if !safe_id(&offer.transfer) {
            return Err(io::Error::other("Недопустимый идентификатор передачи"));
        }
        if offer.transfer.len() > 90 {
            return Err(io::Error::other("Слишком длинный идентификатор передачи"));
        }
        let id = format!("rx-{}", offer.transfer);
        let control = {
            let mut jobs = self.resumed.lock().unwrap();
            if let Some(control) = jobs.get(&id) {
                control.clone()
            } else {
                let path = self.record_path(&id)?;
                let record = if path.exists() {
                    serde_json::from_slice::<Record>(&fs::read(path)?)?
                } else {
                    Record {
                        id: id.clone(),
                        wire_id: offer.transfer.clone(),
                        direction: "receive".into(),
                        name: offer.name.clone(),
                        peer_id: offer.id.clone(),
                        address: stream.peer_addr()?,
                        entries: offer.entries.clone(),
                        sources: Vec::new(),
                        status: "waiting".into(),
                        negotiated: true,
                        cancel_pending: false,
                    }
                };
                let control = Control::new(record);
                self.save_control(&control)?;
                self.announce(&control, None);
                jobs.insert(id.clone(), control.clone());
                control
            }
        };
        let record = control.record.lock().unwrap().clone();
        if record.peer_id != offer.id || record.entries != offer.entries {
            return Err(io::Error::other("Содержимое передачи изменилось"));
        }
        if offer.action == "cancel" {
            control.cancelled.store(true, Ordering::Relaxed);
            control.disconnect();
            if !control.active.load(Ordering::Relaxed) {
                self.storage.abort(&id);
            }
            self.transfer_end(&control, "cancelled", "");
            return frame_write(&mut stream, &json!({"ok":true,"resume":true}));
        }
        if control.cancelled.load(Ordering::Relaxed) {
            return frame_write(
                &mut stream,
                &json!({"ok":false,"resume":true,"status":"cancelled"}),
            );
        }
        if record.status == "error" {
            return frame_write(
                &mut stream,
                &json!({"ok":false,"resume":true,"error":"Получатель не смог сохранить файл. Проверьте папку и отправьте заново"}),
            );
        }
        if control.paused.load(Ordering::Relaxed) {
            return frame_write(
                &mut stream,
                &json!({"ok":false,"resume":true,"status":"paused"}),
            );
        }
        if control
            .active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return frame_write(
                &mut stream,
                &json!({"ok":false,"resume":true,"status":"busy","error":"Предыдущее соединение ещё закрывается"}),
            );
        }
        let result = (|| {
            control.attach(&stream)?;
            self.check_control(&control)?;
            let offsets = offer
                .entries
                .iter()
                .map(|e| self.storage.resume_offset(&id, e))
                .collect::<io::Result<Vec<_>>>()?;
            frame_write(
                &mut stream,
                &json!({"ok":true,"resume":true,"id":self.config.lock().unwrap().id,"offsets":offsets}),
            )?;
            self.state(&control, "active", "");
            let total = offer.entries.iter().map(|e| e.size).sum();
            let mut done: u64 = offsets.iter().map(|(n, _)| *n).sum();
            let base = done;
            let started = Instant::now();
            let mut update = Instant::now();
            let mut buffer = vec![0; BUFFER];
            let visible_paths = display_entries(&offer.entries)
                .into_iter()
                .take(200)
                .map(|e| e.path)
                .collect::<std::collections::HashSet<_>>();
            for (entry, (offset, finished)) in offer.entries.iter().zip(&offsets) {
                self.check_control(&control)?;
                if *finished {
                    if visible_paths.contains(&entry.path) {
                        self.event(json!({"type":"file","transfer":id,"path":entry.path,"size":entry.size,"directory":false,"local_path":self.storage.local_path(&id,entry)}));
                    }
                    continue;
                }
                let mut file = self.storage.open(&id, entry)?;
                if entry.directory {
                    continue;
                }
                let out = file
                    .as_mut()
                    .ok_or_else(|| io::Error::other("Не удалось открыть файл"))?;
                out.seek(SeekFrom::Start(*offset))?;
                let mut remaining = entry.size - offset;
                while remaining > 0 {
                    self.check_control(&control)?;
                    let n = stream.read(&mut buffer[..remaining.min(BUFFER as u64) as usize])?;
                    if n == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "Соединение прервано",
                        ));
                    }
                    out.write_all(&buffer[..n])?;
                    remaining -= n as u64;
                    done += n as u64;
                    if update.elapsed() >= Duration::from_millis(150) {
                        self.resume_progress(&id, done, total, base, started);
                        update = Instant::now();
                    }
                }
                out.flush()?;
                drop(file);
                self.storage.finish(&id, entry)?;
                if visible_paths.contains(&entry.path) {
                    self.event(json!({"type":"file","transfer":id,"path":entry.path,"size":entry.size,"directory":false,"local_path":self.storage.local_path(&id,entry)}));
                }
                frame_write(&mut stream, &json!({"ok":true}))?;
            }
            for entry in display_entries(&offer.entries)
                .into_iter()
                .filter(|e| e.directory)
                .take(200)
            {
                self.event(json!({"type":"file","transfer":id,"path":entry.path,"size":entry.size,"directory":true,"local_path":self.storage.local_path(&id,&entry)}));
            }
            self.resume_progress(&id, total, total, base, started);
            self.transfer_end(&control, "done", "");
            frame_write(&mut stream, &json!({"ok":true}))
        })();
        if let Err(error) = &result {
            let transient = matches!(
                error.kind(),
                io::ErrorKind::Interrupted
                    | io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::NotConnected
                    | io::ErrorKind::TimedOut
                    | io::ErrorKind::WouldBlock
            );
            if !transient
                && !self.stopped.load(Ordering::Relaxed)
                && !control.cancelled.load(Ordering::Relaxed)
                && !control.paused.load(Ordering::Relaxed)
            {
                let _ = frame_write(&mut stream, &json!({"ok":false,"error":error.to_string()}));
                self.storage.abort(&id);
                self.transfer_end(&control, "error", &error.to_string());
                control.detach();
                control.active.store(false, Ordering::Relaxed);
                return Ok(());
            }
        }
        control.detach();
        if control.cancelled.load(Ordering::Relaxed) {
            self.storage.abort(&id);
        } else if result.is_err() {
            if control.paused.load(Ordering::Relaxed) {
                self.state(&control, "paused", "");
            } else {
                self.state(&control, "waiting", "Ожидание продолжения передачи");
            }
        }
        control.active.store(false, Ordering::Relaxed);
        // A broken transport is an unfinished transfer, not a discovery error.
        Ok(())
    }
    fn resume_progress(&self, id: &str, done: u64, total: u64, base: u64, started: Instant) {
        let speed = done.saturating_sub(base) as f64 / started.elapsed().as_secs_f64().max(0.001);
        self.event(json!({"type":"progress","transfer":id,"done":done,"total":total,"speed":speed,"remaining":total.saturating_sub(done) as f64/speed.max(1.)}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(root: &Path) -> Config {
        Config {
            id: "receiver".into(),
            name: "Receiver".into(),
            folder: root.to_string_lossy().into(),
            port: 0,
        }
    }
    fn offer(id: &str) -> Offer {
        Offer {
            protocol: 1,
            id: "sender".into(),
            name: "Sender".into(),
            transfer: id.into(),
            entries: vec![
                Entry {
                    path: "folder".into(),
                    size: 0,
                    directory: true,
                },
                Entry {
                    path: "folder/a.bin".into(),
                    size: 10,
                    directory: false,
                },
                Entry {
                    path: "folder/b.bin".into(),
                    size: 6,
                    directory: false,
                },
            ],
            resume: true,
            action: String::new(),
        }
    }
    fn connect(receiver: &Engine, offer: &Offer) -> (TcpStream, Value) {
        let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, receiver.listener_port)).unwrap();
        setup_stream(&socket).unwrap();
        frame_write(&mut socket, offer).unwrap();
        let reply = frame_read(&mut socket).unwrap();
        (socket, reply)
    }
    fn wait_idle(receiver: &Engine) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while receiver.busy() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!receiver.busy());
    }
    fn sender_case(manual_pause: bool) {
        let root =
            std::env::temp_dir().join(format!("drop-sender-{}-{manual_pause}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let size = 16 * 1024 * 1024;
        let bytes: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        fs::write(root.join("data.bin"), &bytes).unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (notice_tx, notice_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            setup_stream(&socket).unwrap();
            let first: Offer = frame_read(&mut socket).unwrap();
            assert!(first.resume);
            frame_write(
                &mut socket,
                &json!({"ok":true,"resume":true,"offsets":[[0,false]]}),
            )
            .unwrap();
            let mut received = vec![0; 1024];
            socket.read_exact(&mut received).unwrap();
            notice_tx.send(()).unwrap();
            if manual_pause {
                release_rx.recv().unwrap();
            }
            socket.shutdown(Shutdown::Both).unwrap();
            drop(socket);
            let (mut socket, _) = listener.accept().unwrap();
            setup_stream(&socket).unwrap();
            let again: Offer = frame_read(&mut socket).unwrap();
            assert_eq!(again.transfer, first.transfer);
            frame_write(
                &mut socket,
                &json!({"ok":true,"resume":true,"offsets":[[received.len(),false]]}),
            )
            .unwrap();
            let mut tail = vec![0; size - received.len()];
            socket.read_exact(&mut tail).unwrap();
            received.extend(tail);
            frame_write(&mut socket, &json!({"ok":true})).unwrap();
            frame_write(&mut socket, &json!({"ok":true})).unwrap();
            received
        });
        let sender = Engine::start(config(&root), Arc::new(DiskStorage::new(&root))).unwrap();
        let id = sender
            .send(
                address,
                "Fake peer".into(),
                collect_paths(&[root.join("data.bin").to_string_lossy().into()]).unwrap(),
            )
            .unwrap();
        notice_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let sender = if manual_pause {
            sender.control_transfer(&id, "pause").unwrap();
            release_tx.send(()).unwrap();
            sender.stop();
            drop(sender);
            let restored = Engine::start(config(&root), Arc::new(DiskStorage::new(&root))).unwrap();
            assert!(restored.resumed.lock().unwrap()[&id]
                .paused
                .load(Ordering::Relaxed));
            restored.control_transfer(&id, "resume").unwrap();
            restored
        } else {
            sender
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut done = false;
        while Instant::now() < deadline {
            let state = sender.poll();
            if state["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["type"] == "complete" && e["status"] == "done")
            {
                done = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(done, "sender never resumed");
        assert_eq!(server.join().unwrap(), bytes);
        sender.stop();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn sender_automatically_reconnects_from_receiver_offset() {
        sender_case(false);
    }
    #[test]
    fn sender_pause_and_sources_survive_restart() {
        sender_case(true);
    }
    #[test]
    fn legacy_peer_receives_normal_stream_without_resume() {
        let root = std::env::temp_dir().join(format!("drop-legacy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"legacy").unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            setup_stream(&socket).unwrap();
            let offer: Value = frame_read(&mut socket).unwrap();
            assert_eq!(offer["protocol"], 1);
            frame_write(&mut socket, &json!({"ok":true})).unwrap();
            let mut data = [0; 6];
            socket.read_exact(&mut data).unwrap();
            assert_eq!(&data, b"legacy");
            frame_write(&mut socket, &json!({"ok":true})).unwrap();
            frame_write(&mut socket, &json!({"ok":true})).unwrap();
        });
        let sender = Engine::start(config(&root), Arc::new(DiskStorage::new(&root))).unwrap();
        let id = sender
            .send(
                address,
                "Legacy".into(),
                collect_paths(&[root.join("a.txt").to_string_lossy().into()]).unwrap(),
            )
            .unwrap();
        server.join().unwrap();
        wait_idle(&sender);
        assert!(
            !sender.resumed.lock().unwrap()[&id]
                .record
                .lock()
                .unwrap()
                .negotiated
        );
        sender.stop();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn resumes_written_bytes_after_restart_and_skips_finished_files() {
        let root = std::env::temp_dir().join(format!("drop-resume-restart-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("folder/original"), b"keep").unwrap();
        let receiver = Engine::start(config(&root), Arc::new(DiskStorage::new(&root))).unwrap();
        let offered = offer("restart-test");
        let (mut socket, reply) = connect(&receiver, &offered);
        assert_eq!(reply["resume"], true);
        socket.write_all(b"0123456789").unwrap();
        assert_eq!(frame_read::<Value>(&mut socket).unwrap()["ok"], true);
        socket.write_all(b"abc").unwrap();
        socket.shutdown(Shutdown::Both).unwrap();
        drop(socket);
        wait_idle(&receiver);
        receiver.stop();
        drop(receiver);
        let receiver = Engine::start(config(&root), Arc::new(DiskStorage::new(&root))).unwrap();
        let (mut socket, reply) = connect(&receiver, &offered);
        assert_eq!(reply["offsets"][1], json!([10, true]));
        assert_eq!(reply["offsets"][2], json!([3, false]));
        socket.write_all(b"def").unwrap();
        assert_eq!(frame_read::<Value>(&mut socket).unwrap()["ok"], true);
        assert_eq!(frame_read::<Value>(&mut socket).unwrap()["ok"], true);
        drop(socket);
        wait_idle(&receiver);
        assert_eq!(
            fs::read(root.join("folder (1)/a.bin")).unwrap(),
            b"0123456789"
        );
        assert_eq!(fs::read(root.join("folder (1)/b.bin")).unwrap(), b"abcdef");
        assert_eq!(fs::read(root.join("folder/original")).unwrap(), b"keep");
        let (mut socket, reply) = connect(&receiver, &offered);
        assert_eq!(reply["offsets"][2], json!([6, true]));
        assert_eq!(frame_read::<Value>(&mut socket).unwrap()["ok"], true);
        drop(socket);
        wait_idle(&receiver);
        assert!(!root.join("folder (2)").exists());
        receiver.stop();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn receiving_pause_is_persistent_and_cancel_removes_only_partial_files() {
        let root = std::env::temp_dir().join(format!("drop-resume-pause-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let receiver = Engine::start(config(&root), Arc::new(DiskStorage::new(&root))).unwrap();
        let offered = offer("pause-test");
        let (mut socket, _) = connect(&receiver, &offered);
        socket.write_all(b"0123456789").unwrap();
        let _: Value = frame_read(&mut socket).unwrap();
        socket.write_all(b"abc").unwrap();
        receiver.control_transfer("rx-pause-test", "pause").unwrap();
        drop(socket);
        wait_idle(&receiver);
        receiver.stop();
        drop(receiver);
        let receiver = Engine::start(config(&root), Arc::new(DiskStorage::new(&root))).unwrap();
        let (socket, reply) = connect(&receiver, &offered);
        assert_eq!(reply["status"], "paused");
        drop(socket);
        receiver
            .control_transfer("rx-pause-test", "resume")
            .unwrap();
        let (socket, reply) = connect(&receiver, &offered);
        assert!(reply["offsets"][2][0].as_u64().unwrap() <= 3);
        receiver.cancel("rx-pause-test");
        drop(socket);
        wait_idle(&receiver);
        assert_eq!(fs::read(root.join("folder/a.bin")).unwrap(), b"0123456789");
        assert!(fs::read_dir(root.join("folder")).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".part")));
        let (socket, reply) = connect(&receiver, &offered);
        assert_eq!(reply["status"], "cancelled");
        drop(socket);
        receiver.stop();
        fs::remove_dir_all(root).unwrap();
    }
}
