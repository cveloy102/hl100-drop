use super::*;
use jni::{
    objects::{GlobalRef, JObject, JString, JValue},
    sys::jstring,
    JNIEnv, JavaVM,
};
use std::os::fd::FromRawFd;
struct AndroidStorage {
    vm: JavaVM,
    bridge: GlobalRef,
}
impl AndroidStorage {
    fn call_fd(&self, transfer: &str, path: &str, directory: bool) -> io::Result<i32> {
        let mut env = self.vm.attach_current_thread().map_err(io::Error::other)?;
        let t = env.new_string(transfer).map_err(io::Error::other)?;
        let p = env.new_string(path).map_err(io::Error::other)?;
        let result = env.call_method(
            self.bridge.as_obj(),
            "openReceive",
            "(Ljava/lang/String;Ljava/lang/String;Z)I",
            &[
                JValue::Object(&t),
                JValue::Object(&p),
                JValue::Bool(directory as u8),
            ],
        );
        java_result(&mut env, result)?.i().map_err(io::Error::other)
    }
    fn open_source(&self, uri: &str) -> io::Result<File> {
        let mut env = self.vm.attach_current_thread().map_err(io::Error::other)?;
        let u = env.new_string(uri).map_err(io::Error::other)?;
        let result = env.call_method(
            self.bridge.as_obj(),
            "openSource",
            "(Ljava/lang/String;)I",
            &[JValue::Object(&u)],
        );
        let fd = java_result(&mut env, result)?
            .i()
            .map_err(io::Error::other)?;
        if fd < 0 {
            return Err(io::Error::other("Не удалось открыть исходный файл"));
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn java_result<'a>(
    env: &mut JNIEnv<'a>,
    r: jni::errors::Result<jni::objects::JValueOwned<'a>>,
) -> io::Result<jni::objects::JValueOwned<'a>> {
    match r {
        Ok(v) => Ok(v),
        Err(err) => {
            if env.exception_check().unwrap_or(false) {
                let ex = env.exception_occurred().map_err(io::Error::other)?;
                let _ = env.exception_clear();
                let message = env
                    .call_method(ex, "toString", "()Ljava/lang/String;", &[])
                    .ok()
                    .and_then(|v| v.l().ok())
                    .and_then(|o| {
                        env.get_string(&JString::from(o))
                            .ok()
                            .map(|s| s.to_string_lossy().into_owned())
                    });
                return Err(io::Error::other(message.unwrap_or_else(|| err.to_string())));
            }
            Err(io::Error::other(err))
        }
    }
}
impl Storage for AndroidStorage {
    fn open(&self, t: &str, e: &Entry) -> io::Result<Option<File>> {
        let fd = self.call_fd(t, &e.path, e.directory)?;
        if e.directory {
            return Ok(None);
        }
        if fd < 0 {
            return Err(io::Error::other("Папка сохранения недоступна"));
        }
        Ok(Some(unsafe { File::from_raw_fd(fd) }))
    }
    fn finish(&self, t: &str, e: &Entry) -> io::Result<()> {
        if e.directory {
            return Ok(());
        }
        let mut env = self.vm.attach_current_thread().map_err(io::Error::other)?;
        let t = env.new_string(t).map_err(io::Error::other)?;
        let p = env.new_string(&e.path).map_err(io::Error::other)?;
        let result = env.call_method(
            self.bridge.as_obj(),
            "finishReceive",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&t), JValue::Object(&p)],
        );
        java_result(&mut env, result)?;
        Ok(())
    }
    fn abort(&self, t: &str) {
        if let Ok(mut env) = self.vm.attach_current_thread() {
            if let Ok(t) = env.new_string(t) {
                let result = env.call_method(
                    self.bridge.as_obj(),
                    "abortReceive",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&t)],
                );
                let _ = java_result(&mut env, result);
            }
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_ru_hl100_drop_NativeBridge_command(
    mut env: JNIEnv,
    bridge: JObject,
    input: JString,
) -> jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> io::Result<Value> {
        let text: String = env.get_string(&input).map_err(io::Error::other)?.into();
        let v: Value = serde_json::from_str(&text)?;
        match v["op"].as_str().unwrap_or("") {
            "start" => {
                let mut guard = registry().lock().unwrap();
                if let Some(e) = guard.as_ref() {
                    if e.busy() {
                        return Err(io::Error::other("Дождитесь завершения передач"));
                    }
                    e.stop();
                    thread::sleep(Duration::from_millis(300));
                }
                let storage = Arc::new(AndroidStorage {
                    vm: env.get_java_vm().map_err(io::Error::other)?,
                    bridge: env.new_global_ref(&bridge).map_err(io::Error::other)?,
                });
                let c: Config = serde_json::from_value(v["config"].clone())?;
                *guard = Some(Engine::start(c, storage)?);
                Ok(json!({"ok":true}))
            }
            "send" => {
                let guard = registry().lock().unwrap();
                let e = guard
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Устройство не настроено"))?;
                let storage = Arc::new(AndroidStorage {
                    vm: env.get_java_vm().map_err(io::Error::other)?,
                    bridge: env.new_global_ref(&bridge).map_err(io::Error::other)?,
                });
                let mut sources = Vec::new();
                for item in v["items"]
                    .as_array()
                    .ok_or_else(|| io::Error::other("Нет файлов"))?
                {
                    let entry: Entry = serde_json::from_value(item.clone())?;
                    valid_path(&entry.path)?;
                    let uri = item["uri"].as_str().unwrap_or("").to_string();
                    let s = storage.clone();
                    let open: Option<Box<dyn FnOnce() -> io::Result<File> + Send>> =
                        if entry.directory {
                            None
                        } else {
                            Some(Box::new(move || s.open_source(&uri)))
                        };
                    sources.push(Source { entry, open });
                }
                let addr: SocketAddr = format!(
                    "{}:{}",
                    v["address"].as_str().unwrap_or(""),
                    v["port"].as_u64().unwrap_or(41000)
                )
                .parse()
                .map_err(io::Error::other)?;
                let id = e.send(
                    addr,
                    v["name"].as_str().unwrap_or("Устройство").into(),
                    sources,
                )?;
                Ok(json!({"ok":true,"transfer":id}))
            }
            _ => Ok(dispatch(&text)),
        }
    }));
    let value = match result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => json!({"ok":false,"error":e.to_string()}),
        Err(_) => json!({"ok":false,"error":"Ошибка ядра"}),
    };
    env.new_string(value.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
