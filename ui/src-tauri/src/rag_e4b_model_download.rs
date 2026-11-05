#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{fs::{self, OpenOptions}, path::PathBuf, process::{Command, Stdio}, sync::{Mutex, OnceLock}};

const URL: &str = "https://huggingface.co/ggml-org/gemma-4-E4B-it-GGUF/resolve/a69f018/gemma-4-E4B-it-Q4_0.gguf";
const SHA: &str = "a555b900214b477d8880e7832e0b8925e139b0159640036b09fe472b6f2097f2";
const SIZE: u64 = 4_590_807_392;
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(unix)]
static CHILD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
#[cfg(unix)]
pub fn terminate_child() { let pid = CHILD.load(std::sync::atomic::Ordering::Relaxed); if pid > 0 { unsafe { libc::kill(pid, libc::SIGTERM); } } }
#[cfg(unix)]
pub fn cleanup() { use std::sync::atomic::Ordering; CANCELLED.store(true, Ordering::SeqCst); let pid = CHILD.swap(0, Ordering::SeqCst); if pid > 0 { unsafe { libc::kill(pid, libc::SIGTERM); } } if let Some(p) = temp().lock().unwrap().take() { let _ = fs::remove_file(p); } }
#[cfg(windows)]
static CHILD: crate::windows_model_download::ProcessSlot = crate::windows_model_download::ProcessSlot::new();
#[cfg(windows)]
pub fn terminate_child() { CHILD.terminate(); }
#[cfg(windows)]
pub fn cleanup() {
    CANCELLED.store(true, std::sync::atomic::Ordering::SeqCst);
    CHILD.terminate();
    // The worker removes the file after the child has exited and released it.
}
static STATUS: OnceLock<Mutex<String>> = OnceLock::new();
static TEMP: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
fn create_temp(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}
fn temp() -> &'static Mutex<Option<PathBuf>> { TEMP.get_or_init(|| Mutex::new(None)) }
fn state() -> &'static Mutex<String> { STATUS.get_or_init(|| Mutex::new("idle".into())) }
pub fn status(path: PathBuf) -> String {
    let s = state().lock().unwrap().clone();
    if s == "idle" && path.join("models/gemma-4-e4b-it-q4.gguf").is_file() { return "installed".into(); }
    if s == "downloading" { let n = temp().lock().unwrap().as_ref().and_then(|p| fs::metadata(p).ok()).ok_or(()).map(|x| x.len()).unwrap_or(0); format!("downloading:{}", n.saturating_mul(100).checked_div(SIZE).unwrap_or(0).min(100)) } else { s }
}
pub fn start(path: PathBuf) -> Result<(), String> {
    let mut s = state().lock().map_err(|_| "Ошибка загрузки")?;
    if *s == "downloading" { return Err("Загрузка уже выполняется".into()); }
    let dir = path.join("models"); fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Curl has a one-hour limit; much older partial files cannot be active downloads.
    if let Ok(entries) = fs::read_dir(&dir) { for e in entries.flatten() { let name=e.file_name(); if name.to_string_lossy().starts_with(".gemma4-e4b-download-") && e.file_type().is_ok_and(|t| t.is_file()) && e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs()>24*3600) { let _=fs::remove_file(e.path()); } } }
    let dest = dir.join("gemma-4-e4b-it-q4.gguf"); if dest.exists() { return Err("Модель уже существует".into()); }
    let mut candidate = None;
    for i in 0..1000 { let p = dir.join(format!(".gemma4-e4b-download-{}-{i}", std::process::id())); match create_temp(&p) { Ok(f) => { candidate = Some((p,f)); break }, Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue, Err(e) => return Err(e.to_string()) } }
    let (tmp,file) = candidate.ok_or("Не удалось создать временный файл")?;
    *temp().lock().unwrap() = Some(tmp.clone());
    CANCELLED.store(false, std::sync::atomic::Ordering::SeqCst);
    *s = "downloading".into(); drop(s);
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            #[cfg(unix)]
            let curl = PathBuf::from("/usr/bin/curl");
            #[cfg(windows)]
            let curl = crate::windows_model_download::curl_path()?;
            let mut command = Command::new(curl);
            command.args(["--fail", "--location", "--silent", "--show-error", "--max-time", "3600", URL]).stdout(Stdio::from(file)).stderr(Stdio::null());
            #[cfg(windows)]
            crate::windows_model_download::hide_console(&mut command);
            let child = command.spawn().map_err(|e| e.to_string())?;
            #[cfg(unix)]
            let result = {
                let mut child = child;
                CHILD.store(child.id() as i32, std::sync::atomic::Ordering::SeqCst);
                if CANCELLED.load(std::sync::atomic::Ordering::SeqCst) { let _ = child.kill(); }
                let result = child.wait();
                CHILD.store(0, std::sync::atomic::Ordering::SeqCst);
                result
            };
            #[cfg(windows)]
            let result = CHILD.wait(child, &CANCELLED);
            if !result.map_err(|e| e.to_string())?.success() { return Err("Не удалось загрузить модель. Проверьте подключение к интернету.".into()); }
            if CANCELLED.load(std::sync::atomic::Ordering::SeqCst) { return Err("Загрузка отменена".into()); }
            #[cfg(unix)]
            {
                let out = Command::new("/usr/bin/shasum").args(["-a", "256"]).arg(&tmp).output().map_err(|e| e.to_string())?;
                let digest = String::from_utf8_lossy(&out.stdout); if !out.status.success() || digest.split_whitespace().next() != Some(SHA) { return Err("Загруженный файл не прошёл проверку".into()); }
            }
            #[cfg(windows)]
            if !crate::windows_model_download::verify_sha256(&tmp, SHA, &CANCELLED)? { return Err("Загруженный файл не прошёл проверку".into()); }
            if CANCELLED.load(std::sync::atomic::Ordering::SeqCst) { return Err("Загрузка отменена".into()); }
            // Hard-link publication fails rather than overwriting an existing model.
            fs::hard_link(&tmp, &dest).map_err(|e| e.to_string())?; Ok(())
        })();
        let _ = fs::remove_file(&tmp);
        *temp().lock().unwrap() = None;
        *state().lock().unwrap() = match result { Ok(()) => "complete".into(), Err(e) => format!("error:{e}") };
    }); Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn existing_model_is_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("atlas-model-existing-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        let actual = dir.join("models/gemma-4-e4b-it-q4.gguf");
        std::fs::write(&actual, b"old").unwrap();
        assert!(super::start(dir.clone()).is_err());
        assert_eq!(std::fs::read(actual).unwrap(), b"old");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
