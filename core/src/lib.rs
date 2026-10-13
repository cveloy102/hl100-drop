//! HL100 Drop v1: JSON control frames and uncompressed TCP byte streams.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    collections::{HashMap, VecDeque},
    ffi::{CStr, CString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    os::raw::c_char,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 100, 100);
const DISCOVERY_PORT: u16 = 41001;
const MAX_FRAME: usize = 4 * 1024 * 1024;
const BUFFER: usize = 1024 * 1024;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn retry_interrupted<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match operation() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub id: String,
    pub name: String,
    pub folder: String,
    #[serde(default = "default_port")]
    pub port: u16,
}
fn default_port() -> u16 {
    41000
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub size: u64,
    pub directory: bool,
}
#[derive(Serialize, Deserialize)]
struct Offer {
    protocol: u32,
    id: String,
    name: String,
    transfer: String,
    entries: Vec<Entry>,
}
#[derive(Clone, Serialize)]
struct Peer {
    id: String,
    name: String,
    address: String,
    port: u16,
    platform: String,
}
pub struct Source {
    pub entry: Entry,
    pub local_path: Option<PathBuf>,
    pub open: Option<Box<dyn FnOnce() -> io::Result<File> + Send>>,
}

// UI metadata is separate from the full wire manifest. A tree is one item.
fn display_entries(entries: &[Entry]) -> Vec<Entry> {
    let mut result: Vec<Entry> = Vec::new();
    let mut indices = HashMap::new();
    for entry in entries {
        let top = entry.path.split('/').next().unwrap_or(&entry.path);
        let index = *indices.entry(top.to_string()).or_insert_with(|| {
            result.push(Entry {
                path: top.into(),
                size: 0,
                directory: entry.directory || entry.path.contains('/'),
            });
            result.len() - 1
        });
        result[index].size += entry.size;
    }
    result
}

pub trait Storage: Send + Sync {
    fn open(&self, transfer: &str, entry: &Entry) -> io::Result<Option<File>>;
    fn finish(&self, transfer: &str, entry: &Entry) -> io::Result<()>;
    fn abort(&self, transfer: &str);
    fn local_path(&self, _transfer: &str, _entry: &Entry) -> Option<PathBuf> {
        None
    }
}
pub struct DiskStorage {
    root: PathBuf,
    paths: Mutex<HashMap<String, (PathBuf, PathBuf)>>,
}
impl DiskStorage {
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            root: folder.into(),
            paths: Mutex::new(HashMap::new()),
        }
    }
}
fn key(transfer: &str, path: &str) -> String {
    format!("{transfer}:{path}")
}
fn valid_path(path: &str) -> io::Result<PathBuf> {
    if path.is_empty() || path.contains('\\') || path.contains(':') || path.contains('\0') {
        return Err(io::Error::other("Недопустимое имя файла"));
    }
    let p = Path::new(path);
    if p.components().any(|c| !matches!(c, Component::Normal(_)))
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(io::Error::other("Недопустимый путь"));
    }
    Ok(p.to_path_buf())
}
impl Storage for DiskStorage {
    fn open(&self, transfer: &str, entry: &Entry) -> io::Result<Option<File>> {
        let relative = valid_path(&entry.path)?;
        fs::create_dir_all(&self.root)?;
        // Resolve one collision-free top-level name for the entire offered tree.
        let mut paths = self.paths.lock().unwrap();
        let top = relative
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_string_lossy()
            .to_string();
        let root_key = key(transfer, &format!("@/{top}"));
        let chosen = if let Some((_, chosen)) = paths.get(&root_key) {
            chosen.clone()
        } else {
            let base = self.root.join(&top);
            let mut chosen = base.clone();
            let mut n = 1;
            while chosen.exists() || paths.values().any(|(_, p)| p == &chosen) {
                let p = Path::new(&top);
                let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                let ext = p
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                chosen = self.root.join(format!("{stem} ({n}){ext}"));
                n += 1;
            }
            paths.insert(root_key, (PathBuf::new(), chosen.clone()));
            chosen
        };
        let tail: PathBuf = relative.components().skip(1).collect();
        let target = if tail.as_os_str().is_empty() {
            chosen
        } else {
            chosen.join(tail)
        };
        if entry.directory {
            fs::create_dir_all(target)?;
            return Ok(None);
        }
        fs::create_dir_all(target.parent().unwrap())?;
        let temp = target.with_file_name(format!(
            "{}.{}.part",
            target.file_name().unwrap().to_string_lossy(),
            transfer
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        paths.insert(key(transfer, &entry.path), (temp, target));
        Ok(Some(file))
    }
    fn local_path(&self, transfer: &str, entry: &Entry) -> Option<PathBuf> {
        let relative = valid_path(&entry.path).ok()?;
        let top = relative.components().next()?.as_os_str().to_string_lossy();
        let paths = self.paths.lock().unwrap();
        let (_, chosen) = paths.get(&key(transfer, &format!("@/{top}")))?;
        let tail = relative.components().skip(1).collect::<PathBuf>();
        Some(if tail.as_os_str().is_empty() {
            chosen.clone()
        } else {
            chosen.join(tail)
        })
    }
    fn finish(&self, transfer: &str, entry: &Entry) -> io::Result<()> {
        if entry.directory {
            return Ok(());
        }
        let mut paths = self.paths.lock().unwrap();
        if let Some((temp, target)) = paths.get(&key(transfer, &entry.path)) {
            fs::rename(temp, target)?;
        }
        paths.remove(&key(transfer, &entry.path));
        Ok(())
    }
    fn abort(&self, transfer: &str) {
        let mut paths = self.paths.lock().unwrap();
        let prefix = format!("{transfer}:");
        paths.retain(|k, (temp, _)| {
            if k.starts_with(&prefix) {
                if !temp.as_os_str().is_empty() {
                    let _ = fs::remove_file(temp);
                }
                false
            } else {
                true
            }
        });
    }
}

pub struct Engine {
    config: Mutex<Config>,
    storage: Arc<dyn Storage>,
    stopped: AtomicBool,
    events: Mutex<VecDeque<Value>>,
    peers: Mutex<HashMap<String, (Peer, Instant)>>,
    transfers: Mutex<HashMap<String, Arc<AtomicBool>>>,
    workers: Mutex<Vec<thread::JoinHandle<()>>>,
    listener_worker: Mutex<Option<thread::JoinHandle<()>>>,
    listener_port: u16,
}
fn frame_write(stream: &mut TcpStream, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::other("Слишком много файлов в одной передаче"));
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)
}
fn frame_read<T: for<'a> Deserialize<'a>>(stream: &mut TcpStream) -> io::Result<T> {
    let mut len = [0; 4];
    stream.read_exact(&mut len)?;
    let size = u32::from_be_bytes(len) as usize;
    if size > MAX_FRAME {
        return Err(io::Error::other("Слишком большой заголовок"));
    }
    let mut data = vec![0; size];
    stream.read_exact(&mut data)?;
    Ok(serde_json::from_slice(&data)?)
}
fn local_address(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            ip.is_loopback()
                || if_addrs::get_if_addrs()
                    .unwrap_or_default()
                    .iter()
                    .any(|iface| {
                        if let if_addrs::IfAddr::V4(v4) = &iface.addr {
                            (u32::from(ip) & u32::from(v4.netmask))
                                == (u32::from(v4.ip) & u32::from(v4.netmask))
                        } else {
                            false
                        }
                    })
        }
        _ => false,
    }
}
impl Engine {
    pub fn start(config: Config, storage: Arc<dyn Storage>) -> io::Result<Arc<Self>> {
        if config.name.trim().is_empty() || config.id.is_empty() {
            return Err(io::Error::other("Укажите имя устройства"));
        }
        #[cfg(unix)]
        let listener: TcpListener = {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
            socket.set_reuse_address(true)?;
            socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, config.port)).into())?;
            socket.listen(128)?;
            socket.into()
        };
        #[cfg(not(unix))]
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, config.port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let e = Arc::new(Self {
            config: Mutex::new(config),
            storage,
            stopped: AtomicBool::new(false),
            events: Mutex::new(VecDeque::new()),
            peers: Mutex::new(HashMap::new()),
            transfers: Mutex::new(HashMap::new()),
            workers: Mutex::new(Vec::new()),
            listener_worker: Mutex::new(None),
            listener_port: port,
        });
        let worker = e.clone();
        let handle = thread::spawn(move || {
            while !worker.stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, addr))
                        if !worker.stopped.load(Ordering::Relaxed) && local_address(addr.ip()) =>
                    {
                        let w = worker.clone();
                        worker.spawn(move || w.receive(stream));
                    }
                    Ok(_) => {}
                    Err(err)
                        if matches!(
                            err.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        thread::sleep(Duration::from_millis(50))
                    }
                    Err(err) => {
                        worker.event(json!({"type":"error","message":err.to_string()}));
                        break;
                    }
                }
            }
        });
        *e.listener_worker.lock().unwrap() = Some(handle);
        let w = e.clone();
        e.spawn(move || {
            let mut previous_error = String::new();
            while !w.stopped.load(Ordering::Relaxed) {
                match w.discovery() {
                    Ok(()) => break,
                    Err(err) => {
                        let message = format!("Обнаружение устройств: {err}");
                        if message != previous_error {
                            w.event(json!({"type":"error","message":message}));
                            previous_error = message;
                        }
                        for _ in 0..10 {
                            if w.stopped.load(Ordering::Relaxed) {
                                break;
                            }
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            }
        });
        e.event(json!({"type":"started","port":port}));
        Ok(e)
    }
    fn spawn(&self, f: impl FnOnce() + Send + 'static) {
        let mut workers = self.workers.lock().unwrap();
        workers.retain(|w| !w.is_finished());
        workers.push(thread::spawn(f));
    }
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        for c in self.transfers.lock().unwrap().values() {
            c.store(true, Ordering::Relaxed);
        }
        // Returning from stop guarantees the listening port is available again.
        if let Some(listener) = self.listener_worker.lock().unwrap().take() {
            let _ = listener.join();
        }
    }
    pub fn busy(&self) -> bool {
        !self.transfers.lock().unwrap().is_empty()
    }
    pub fn cancel(&self, id: &str) {
        if let Some(c) = self.transfers.lock().unwrap().get(id) {
            c.store(true, Ordering::Relaxed);
        }
    }
    fn event(&self, v: Value) {
        let mut events = self.events.lock().unwrap();
        if v["type"] == "progress" {
            events.retain(|old| !(old["type"] == "progress" && old["transfer"] == v["transfer"]));
        }
        if events.len() >= 2048 {
            events.pop_front();
        }
        events.push_back(v);
    }
    pub fn poll(&self) -> Value {
        let events: Vec<_> = self.events.lock().unwrap().drain(..).collect();
        let mut peers = self.peers.lock().unwrap();
        peers.retain(|_, (_, seen)| seen.elapsed() < Duration::from_secs(7));
        json!({"events":events,"peers":peers.values().map(|(p,_)|p).collect::<Vec<_>>(),"busy":self.busy()})
    }
    fn discovery(&self) -> io::Result<()> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)).into())?;
        let socket: UdpSocket = socket.into();
        socket.set_read_timeout(Some(Duration::from_millis(250)))?;
        socket.set_broadcast(true)?;
        socket.set_multicast_ttl_v4(1)?;
        let interfaces: Vec<_> = if_addrs::get_if_addrs()?
            .into_iter()
            .filter_map(|i| match i.addr {
                if_addrs::IfAddr::V4(v) if !v.ip.is_loopback() => Some(v),
                _ => None,
            })
            .collect();
        for iface in &interfaces {
            let _ = socket.join_multicast_v4(&GROUP, &iface.ip);
        }
        let mut announced = Instant::now() - Duration::from_secs(3);
        let mut buffer = [0; 4096];
        while !self.stopped.load(Ordering::Relaxed) {
            if announced.elapsed() >= Duration::from_secs(2) {
                let c = self.config.lock().unwrap().clone();
                let data = serde_json::to_vec(
                    &json!({"app":"hl100-drop","protocol":1,"id":c.id,"name":c.name,"port":self.listener_port,"platform":std::env::consts::OS}),
                )?;
                let _ = socket.send_to(&data, (GROUP, DISCOVERY_PORT));
                for iface in &interfaces {
                    if let Some(ip) = iface.broadcast {
                        let _ = socket.send_to(&data, (ip, DISCOVERY_PORT));
                    }
                }
                announced = Instant::now();
            }
            match retry_interrupted(|| socket.recv_from(&mut buffer)) {
                Ok((n, addr)) if local_address(addr.ip()) => {
                    if let Ok(v) = serde_json::from_slice::<Value>(&buffer[..n]) {
                        let c = self.config.lock().unwrap();
                        if v["app"] == "hl100-drop"
                            && v["protocol"] == 1
                            && v["id"].as_str().is_some_and(|id| id != c.id)
                        {
                            if let (Some(id), Some(name), Some(port)) = (
                                v["id"].as_str(),
                                v["name"].as_str(),
                                v["port"].as_u64().filter(|p| *p > 0 && *p <= 65535),
                            ) {
                                self.peers.lock().unwrap().insert(
                                    id.into(),
                                    (
                                        Peer {
                                            id: id.into(),
                                            name: name.into(),
                                            address: addr.ip().to_string(),
                                            port: port as u16,
                                            platform: v["platform"]
                                                .as_str()
                                                .unwrap_or("unknown")
                                                .into(),
                                        },
                                        Instant::now(),
                                    ),
                                );
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    pub fn send(
        self: &Arc<Self>,
        addr: SocketAddr,
        name: String,
        sources: Vec<Source>,
    ) -> io::Result<String> {
        if !local_address(addr.ip()) {
            return Err(io::Error::other("Устройство должно быть в локальной сети"));
        }
        if sources.is_empty() {
            return Err(io::Error::other("Выберите файлы"));
        }
        validate_entries(&sources.iter().map(|s| s.entry.clone()).collect::<Vec<_>>())?;
        let id = format!(
            "{}-{}",
            self.config.lock().unwrap().id,
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        );
        let cancel = Arc::new(AtomicBool::new(false));
        self.transfers
            .lock()
            .unwrap()
            .insert(id.clone(), cancel.clone());
        let w = self.clone();
        let transfer = id.clone();
        self.spawn(move || {
            let total = sources.iter().map(|s| s.entry.size).sum::<u64>();
            let manifest = sources.iter().map(|s| s.entry.clone()).collect::<Vec<_>>();
            let display = display_entries(&manifest);
            let display_values = display.iter().take(200).map(|entry| {
                let mut value = serde_json::to_value(entry).unwrap();
                if let Some(path) = sources.iter().find(|s| s.entry.path == entry.path).and_then(|s| s.local_path.as_ref()) {
                    value["local_path"] = json!(path);
                }
                value
            }).collect::<Vec<_>>();
            w.event(json!({"type":"transfer","transfer":transfer,"direction":"send","name":name,"total":total,"files":sources.len(),"entries":manifest.iter().take(200).collect::<Vec<_>>(),"display_entries":display_values,"display_count":display.len()}));
            let result = (|| -> io::Result<()> {
                let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?; setup_stream(&stream)?;
                let c = w.config.lock().unwrap().clone();
                frame_write(&mut stream, &Offer { protocol:1,id:c.id,name:c.name,transfer:transfer.clone(),entries:sources.iter().map(|s|s.entry.clone()).collect() })?;
                let reply: Value = frame_read(&mut stream)?;
                if reply["ok"] != true { return Err(io::Error::other(reply["error"].as_str().unwrap_or("Получатель отказал"))); }
                let mut done=0; let started=Instant::now(); let mut update=Instant::now(); let mut buffer=vec![0;BUFFER];
                for mut source in sources {
                    if source.entry.directory { continue; }
                    let mut file=source.open.take().ok_or_else(||io::Error::other("Файл недоступен"))?()?; let mut remaining=source.entry.size;
                    while remaining>0 {
                        check_cancel(&cancel)?;
                        let n=file.read(&mut buffer[..remaining.min(BUFFER as u64) as usize])?;
                        if n==0 { return Err(io::Error::other("Исходный файл изменился или стал недоступен")); }
                        stream.write_all(&buffer[..n])?; remaining-=n as u64; done+=n as u64;
                        if update.elapsed()>=Duration::from_millis(150) { w.progress(&transfer,done,total,started); update=Instant::now(); }
                    }
                    let ack:Value=frame_read(&mut stream)?; if ack["ok"]!=true { return Err(io::Error::other(ack["error"].as_str().unwrap_or("Ошибка сохранения"))); }
                }
                let final_ack:Value=frame_read(&mut stream)?;
                if final_ack["ok"]!=true{return Err(io::Error::other("Передача не завершена получателем"));}
                w.progress(&transfer,total,total,started); Ok(())
            })();
            w.complete(&transfer,result,cancel.load(Ordering::Relaxed));
        });
        Ok(id)
    }
    fn receive(self: Arc<Self>, mut stream: TcpStream) {
        let result = (|| -> io::Result<(String, Vec<Entry>)> {
            setup_stream(&stream)?;
            let offer: Offer = frame_read(&mut stream)?;
            if offer.protocol != 1 || offer.entries.is_empty() || offer.entries.len() > 100000 {
                return Err(io::Error::other("Неподдерживаемая передача"));
            }
            validate_entries(&offer.entries)?;
            let total = offer
                .entries
                .iter()
                .try_fold(0u64, |sum, e| sum.checked_add(e.size))
                .ok_or_else(|| io::Error::other("Слишком большой размер"))?;
            // Allocate a local transfer ID; sender IDs are only informational.
            let id = format!("receive-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
            let cancel = Arc::new(AtomicBool::new(false));
            self.transfers
                .lock()
                .unwrap()
                .insert(id.clone(), cancel.clone());
            let display = display_entries(&offer.entries);
            self.event(json!({"type":"transfer","transfer":id,"direction":"receive","name":offer.name,"total":total,"files":offer.entries.len(),"entries":offer.entries.iter().take(200).collect::<Vec<_>>(),"display_entries":display.iter().take(200).collect::<Vec<_>>(),"display_count":display.len()}));
            let receive_result = (|| -> io::Result<()> {
                frame_write(&mut stream, &json!({"ok":true}))?;
                let mut done = 0;
                let started = Instant::now();
                let mut update = Instant::now();
                let mut buffer = vec![0; BUFFER];
                for (index, entry) in offer.entries.iter().enumerate() {
                    check_cancel(&cancel)?;
                    let mut file = self.storage.open(&id, entry)?;
                    let local_path = self.storage.local_path(&id, entry);
                    if entry.directory {
                        continue;
                    }
                    let out = file
                        .as_mut()
                        .ok_or_else(|| io::Error::other("Не удалось открыть файл"))?;
                    let mut remaining = entry.size;
                    while remaining > 0 {
                        check_cancel(&cancel)?;
                        let n =
                            stream.read(&mut buffer[..remaining.min(BUFFER as u64) as usize])?;
                        if n == 0 {
                            return Err(io::Error::other("Соединение прервано"));
                        }
                        out.write_all(&buffer[..n])?;
                        remaining -= n as u64;
                        done += n as u64;
                        if update.elapsed() >= Duration::from_millis(150) {
                            self.progress(&id, done, total, started);
                            update = Instant::now();
                        }
                    }
                    out.flush()?;
                    drop(file);
                    self.storage.finish(&id, entry)?;
                    if index < 200 {
                        self.event(json!({"type":"file","transfer":id,"path":entry.path,"size":entry.size,"directory":false,"local_path":local_path}));
                    }
                    frame_write(&mut stream, &json!({"ok":true}))?;
                }
                frame_write(&mut stream, &json!({"ok":true}))?;
                self.progress(&id, total, total, started);
                for entry in display.iter().filter(|e| e.directory).take(200) {
                    self.event(json!({"type":"file","transfer":id,"path":entry.path,"size":entry.size,"directory":true,"local_path":self.storage.local_path(&id,entry)}));
                }
                Ok(())
            })();
            if let Err(err) = &receive_result {
                let _ = frame_write(&mut stream, &json!({"ok":false,"error":err.to_string()}));
            }
            self.storage.abort(&id);
            self.complete(&id, receive_result, cancel.load(Ordering::Relaxed));
            Ok((id, offer.entries))
        })();
        if let Err(err) = result {
            let _ = frame_write(&mut stream, &json!({"ok":false,"error":err.to_string()}));
            self.event(json!({"type":"error","message":err.to_string()}));
        }
    }
    fn progress(&self, id: &str, done: u64, total: u64, started: Instant) {
        let speed = done as f64 / started.elapsed().as_secs_f64().max(0.001);
        self.event(json!({"type":"progress","transfer":id,"done":done,"total":total,"speed":speed,"remaining":(total-done) as f64/speed.max(1.)}));
    }
    fn complete(&self, id: &str, result: io::Result<()>, cancelled: bool) {
        self.transfers.lock().unwrap().remove(id);
        self.event(json!({"type":"complete","transfer":id,"status":if cancelled {"cancelled"} else if result.is_ok() {"done"} else {"error"},"message":result.err().map(|e|e.to_string()).unwrap_or_default()}));
    }
}
fn setup_stream(stream: &TcpStream) -> io::Result<()> {
    // Accepted sockets can inherit the listener's nonblocking mode on Windows.
    // Each transfer has its own worker and must wait for the sender's data.
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))
}
fn validate_entries(entries: &[Entry]) -> io::Result<()> {
    let mut names = std::collections::HashSet::new();
    for e in entries {
        valid_path(&e.path)?;
        if !names.insert(e.path.to_lowercase()) {
            return Err(io::Error::other(
                "В одной передаче есть одинаковые имена. Отправьте их отдельно",
            ));
        }
        if e.directory && e.size != 0 {
            return Err(io::Error::other("Неверный размер папки"));
        }
    }
    entries
        .iter()
        .try_fold(0u64, |sum, e| sum.checked_add(e.size))
        .ok_or_else(|| io::Error::other("Слишком большой размер"))?;
    Ok(())
}
fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::other("Передача отменена"))
    } else {
        Ok(())
    }
}
pub fn collect_paths(paths: &[String]) -> io::Result<Vec<Source>> {
    fn walk(path: &Path, relative: String, out: &mut Vec<Source>) -> io::Result<()> {
        valid_path(&relative)?;
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() {
            return Err(io::Error::other("Символические ссылки не передаются"));
        }
        let directory = meta.is_dir();
        let source_path = path.to_path_buf();
        out.push(Source {
            local_path: Some(fs::canonicalize(path)?),
            entry: Entry {
                path: relative.clone(),
                size: if directory { 0 } else { meta.len() },
                directory,
            },
            open: if directory {
                None
            } else {
                Some(Box::new(move || File::open(source_path)))
            },
        });
        if directory {
            for item in fs::read_dir(path)? {
                let item = item?;
                walk(
                    &item.path(),
                    format!("{relative}/{}", item.file_name().to_string_lossy()),
                    out,
                )?;
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for path in paths {
        let p = Path::new(path);
        walk(
            p,
            p.file_name()
                .ok_or_else(|| io::Error::other("Выберите папку, а не корень диска"))?
                .to_string_lossy()
                .into(),
            &mut out,
        )?;
    }
    Ok(out)
}
static ENGINE: OnceLock<Mutex<Option<Arc<Engine>>>> = OnceLock::new();
fn registry() -> &'static Mutex<Option<Arc<Engine>>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}
pub fn dispatch(command: &str) -> Value {
    let result = (|| -> io::Result<Value> {
        let v: Value = serde_json::from_str(command)?;
        let mut guard = registry().lock().unwrap();
        match v["op"].as_str().unwrap_or("") {
            "start" => {
                if let Some(e) = guard.as_ref() {
                    if e.busy() {
                        return Err(io::Error::other("Дождитесь завершения передач"));
                    }
                    e.stop();
                    thread::sleep(Duration::from_millis(300));
                }
                let c: Config = serde_json::from_value(v["config"].clone())?;
                let storage = Arc::new(DiskStorage::new(&c.folder));
                *guard = Some(Engine::start(c, storage)?);
                Ok(json!({"ok":true}))
            }
            "stop" => {
                if let Some(e) = guard.take() {
                    e.stop();
                }
                Ok(json!({"ok":true}))
            }
            op => {
                let e = guard
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Устройство ещё не настроено"))?;
                match op {
                    "poll" => Ok(e.poll()),
                    "cancel" => {
                        e.cancel(v["transfer"].as_str().unwrap_or(""));
                        Ok(json!({"ok":true}))
                    }
                    "send" => {
                        let addr: SocketAddr = format!(
                            "{}:{}",
                            v["address"].as_str().unwrap_or(""),
                            v["port"].as_u64().unwrap_or(41000)
                        )
                        .parse()
                        .map_err(io::Error::other)?;
                        let paths: Vec<String> = serde_json::from_value(v["paths"].clone())?;
                        let id = e.send(
                            addr,
                            v["name"].as_str().unwrap_or("Устройство").into(),
                            collect_paths(&paths)?,
                        )?;
                        Ok(json!({"ok":true,"transfer":id}))
                    }
                    _ => Err(io::Error::other("Неизвестная команда")),
                }
            }
        }
    })();
    result.unwrap_or_else(|err| json!({"ok":false,"error":err.to_string()}))
}
/// Execute one UTF-8 JSON command and return an owned JSON response.
///
/// # Safety
/// `command` must be null or point to a valid NUL-terminated string for the duration of the call.
/// Release the returned pointer exactly once using `hl100_free`.
#[no_mangle]
pub unsafe extern "C" fn hl100_command(command: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if command.is_null() {
            return json!({"ok":false,"error":"null command"});
        }
        let input = unsafe { CStr::from_ptr(command) }.to_string_lossy();
        dispatch(&input)
    })
    .unwrap_or_else(|_| json!({"ok":false,"error":"Ошибка ядра"}));
    CString::new(result.to_string()).unwrap().into_raw()
}
/// Release a response returned by `hl100_command`.
///
/// # Safety
/// `value` must be null or an unfreed pointer returned by `hl100_command`.
#[no_mangle]
pub unsafe extern "C" fn hl100_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

#[cfg(target_os = "android")]
mod android;

#[cfg(test)]
mod tests {
    #[test]
    fn folder_display_keeps_one_root_with_aggregate_size() {
        let entries = vec![
            Entry {
                path: "Фото".into(),
                size: 0,
                directory: true,
            },
            Entry {
                path: "Фото/2026".into(),
                size: 0,
                directory: true,
            },
            Entry {
                path: "Фото/2026/a.jpg".into(),
                size: 12,
                directory: false,
            },
            Entry {
                path: "Фото/b.jpg".into(),
                size: 25,
                directory: false,
            },
            Entry {
                path: "notes.txt".into(),
                size: 7,
                directory: false,
            },
        ];
        let display = display_entries(&entries);
        assert_eq!(display.len(), 2);
        assert_eq!(display[0].path, "Фото");
        assert!(display[0].directory);
        assert_eq!(display[0].size, 37);
        assert_eq!(display[1].size, 7);
        assert_eq!(entries.len(), 5);
    }
    #[test]
    fn stopped_listener_can_restart_immediately_on_same_port() {
        let root = std::env::temp_dir().join(format!("hl100-restart-{}", std::process::id()));
        let mut port = 0;
        for _ in 0..16 {
            let receiver = Engine::start(
                Config {
                    id: "restart-test".into(),
                    name: "Receiver".into(),
                    folder: root.to_string_lossy().into(),
                    port,
                },
                Arc::new(DiskStorage::new(&root)),
            )
            .unwrap();
            port = receiver.listener_port;
            receiver.stop();
        }
    }
    #[cfg(unix)]
    #[test]
    fn receiver_restarts_after_closed_tcp_connection() {
        let root = std::env::temp_dir().join(format!("hl100-time-wait-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut port = 0;
        for n in 0..4 {
            let receiver = Engine::start(
                Config {
                    id: "restart-test".into(),
                    name: "Receiver".into(),
                    folder: root.to_string_lossy().into(),
                    port,
                },
                Arc::new(DiskStorage::new(&root)),
            )
            .unwrap();
            port = receiver.listener_port;
            let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            setup_stream(&stream).unwrap();
            frame_write(
                &mut stream,
                &Offer {
                    protocol: 1,
                    id: "test".into(),
                    name: "Test".into(),
                    transfer: format!("restart-{n}"),
                    entries: vec![Entry {
                        path: format!("empty-{n}"),
                        size: 0,
                        directory: true,
                    }],
                },
            )
            .unwrap();
            assert_eq!(frame_read::<Value>(&mut stream).unwrap()["ok"], true);
            assert_eq!(frame_read::<Value>(&mut stream).unwrap()["ok"], true);
            assert_eq!(stream.read(&mut [0]).unwrap(), 0);
            drop(stream);
            receiver.stop();
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn saved_file_path_is_exact_after_collision() {
        let root = std::env::temp_dir().join(format!("hl100-reveal-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("name.txt"), b"original").unwrap();
        let storage = DiskStorage::new(&root);
        let entry = Entry {
            path: "name.txt".into(),
            size: 4,
            directory: false,
        };
        let mut file = storage.open("reveal", &entry).unwrap().unwrap();
        let saved = storage.local_path("reveal", &entry).unwrap();
        assert_eq!(saved.as_os_str(), root.join("name (1).txt").as_os_str());
        file.write_all(b"data").unwrap();
        drop(file);
        storage.finish("reveal", &entry).unwrap();
        assert!(saved.is_file());
        assert_eq!(fs::read(saved).unwrap(), b"data");
        storage.abort("reveal");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_discovery_receive_retries_without_stopping() {
        let mut calls = 0;
        let result = retry_interrupted(|| {
            calls += 1;
            if calls < 3 {
                Err(io::Error::from(io::ErrorKind::Interrupted))
            } else {
                Ok(42)
            }
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls, 3);
        let timeout: io::Result<()> =
            retry_interrupted(|| Err(io::Error::from(io::ErrorKind::TimedOut)));
        assert_eq!(timeout.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }
    use super::*;
    #[test]
    fn rejects_unsafe_paths() {
        for p in ["../a", "/etc/x", "a/../x", "a\\b", "C:/x", "a//b", "a/./b"] {
            assert!(valid_path(p).is_err(), "{p}");
        }
        assert!(valid_path("папка/фото.jpg").is_ok());
    }
    #[test]
    fn rejects_colliding_manifest_names() {
        let entries = vec![
            Entry {
                path: "Photo.jpg".into(),
                size: 10,
                directory: false,
            },
            Entry {
                path: "photo.jpg".into(),
                size: 10,
                directory: false,
            },
        ];
        assert!(validate_entries(&entries).is_err());
    }
    #[test]
    fn delayed_sender_is_received_on_nonblocking_listener() {
        let root = std::env::temp_dir().join(format!("hl100-delayed-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let receiver = Engine::start(
            Config {
                id: "delayed-test".into(),
                name: "Receiver".into(),
                folder: root.to_string_lossy().into(),
                port: 0,
            },
            Arc::new(DiskStorage::new(&root)),
        )
        .unwrap();
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, receiver.listener_port)).unwrap();
        setup_stream(&stream).unwrap();
        thread::sleep(Duration::from_millis(200));
        frame_write(
            &mut stream,
            &Offer {
                protocol: 1,
                id: "test".into(),
                name: "Test".into(),
                transfer: "delayed".into(),
                entries: vec![Entry {
                    path: "delayed.txt".into(),
                    size: 4,
                    directory: false,
                }],
            },
        )
        .unwrap();
        let reply: Value = frame_read(&mut stream).unwrap();
        assert_eq!(reply["ok"], true);
        thread::sleep(Duration::from_millis(200));
        stream.write_all(b"data").unwrap();
        let ack: Value = frame_read(&mut stream).unwrap();
        assert_eq!(ack["ok"], true);
        let final_ack: Value = frame_read(&mut stream).unwrap();
        assert_eq!(final_ack["ok"], true);
        assert_eq!(fs::read(root.join("delayed.txt")).unwrap(), b"data");
        receiver.stop();
        drop(stream);
        thread::sleep(Duration::from_millis(300));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn broken_connection_removes_partial_file() {
        let root = std::env::temp_dir().join(format!("hl100-disconnect-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let receiver = Engine::start(
            Config {
                id: "disconnect-test".into(),
                name: "Receiver".into(),
                folder: root.to_string_lossy().into(),
                port: 0,
            },
            Arc::new(DiskStorage::new(&root)),
        )
        .unwrap();
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, receiver.listener_port)).unwrap();
        frame_write(
            &mut stream,
            &Offer {
                protocol: 1,
                id: "test".into(),
                name: "Test".into(),
                transfer: "test".into(),
                entries: vec![Entry {
                    path: "unfinished.bin".into(),
                    size: 10000,
                    directory: false,
                }],
            },
        )
        .unwrap();
        let _: Value = frame_read(&mut stream).unwrap();
        stream.write_all(b"incomplete").unwrap();
        drop(stream);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut completed = false;
        while Instant::now() < deadline {
            let state = receiver.poll();
            if state["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["type"] == "complete" && e["status"] == "error")
            {
                completed = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(completed);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        receiver.stop();
        thread::sleep(Duration::from_millis(300));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancel_stops_sender_and_cleans_receiver_partial() {
        let root = std::env::temp_dir().join(format!("hl100-cancel-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.bin");
        fs::write(&source, vec![42; 10000]).unwrap();
        let dest = root.join("dest");
        let receiver = Engine::start(
            Config {
                id: "cancel-r".into(),
                name: "Receiver".into(),
                folder: dest.to_string_lossy().into(),
                port: 0,
            },
            Arc::new(DiskStorage::new(&dest)),
        )
        .unwrap();
        let sender = Engine::start(
            Config {
                id: "cancel-s".into(),
                name: "Sender".into(),
                folder: root.to_string_lossy().into(),
                port: 0,
            },
            Arc::new(DiskStorage::new(&root)),
        )
        .unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let sources = vec![Source {
            local_path: None,
            entry: Entry {
                path: "cancelled.bin".into(),
                size: 10000,
                directory: false,
            },
            open: Some(Box::new(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                File::open(source)
            })),
        }];
        let id = sender
            .send(
                SocketAddr::from((Ipv4Addr::LOCALHOST, receiver.listener_port)),
                "Receiver".into(),
                sources,
            )
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        sender.cancel(&id);
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while (sender.busy() || receiver.busy()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!sender.busy() && !receiver.busy());
        let state = sender.poll();
        assert!(state["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["status"] == "cancelled"));
        assert!(!dest.join("cancelled.bin").exists());
        if dest.exists() {
            assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
        }
        sender.stop();
        receiver.stop();
        thread::sleep(Duration::from_millis(300));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn transfers_tree_and_preserves_existing_file() {
        let root = std::env::temp_dir().join(format!("hl100-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let source = root.join("source");
        let dest = root.join("dest");
        fs::create_dir_all(source.join("папка/empty")).unwrap();
        fs::create_dir_all(&dest).unwrap();
        let bytes: Vec<u8> = (0..3_100_000).map(|i| (i % 251) as u8).collect();
        fs::write(source.join("папка/данные.bin"), &bytes).unwrap();
        fs::write(dest.join("папка"), b"existing").unwrap();
        let receiver = Engine::start(
            Config {
                id: "r".into(),
                name: "Receiver".into(),
                folder: dest.to_string_lossy().into(),
                port: 0,
            },
            Arc::new(DiskStorage::new(&dest)),
        )
        .unwrap();
        let sender = Engine::start(
            Config {
                id: "s".into(),
                name: "Sender".into(),
                folder: source.to_string_lossy().into(),
                port: 0,
            },
            Arc::new(DiskStorage::new(&source)),
        )
        .unwrap();
        sender
            .send(
                SocketAddr::from(([127, 0, 0, 1], receiver.listener_port)),
                "Receiver".into(),
                collect_paths(&[source.join("папка").to_string_lossy().into()]).unwrap(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while sender.busy() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(!sender.busy());
        let result = sender.poll();
        assert!(
            result["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["status"] == "done"),
            "{result}"
        );
        assert_eq!(fs::read(dest.join("папка (1)/данные.bin")).unwrap(), bytes);
        assert!(dest.join("папка (1)/empty").is_dir());
        assert_eq!(fs::read(dest.join("папка")).unwrap(), b"existing");
        sender.stop();
        receiver.stop();
        thread::sleep(Duration::from_millis(400));
        fs::remove_dir_all(root).unwrap();
    }
}
