use sha2::{Digest, Sha256};
use std::{fs::File, io::{self, Read}, path::{Path, PathBuf}, process::{Child, ExitStatus}, sync::{atomic::{AtomicBool, Ordering}, Mutex}, time::Duration};

// Keep the operating-system process handle, rather than a reusable PID.
pub struct ProcessSlot(Mutex<Option<Child>>);
impl ProcessSlot {
    pub const fn new() -> Self { Self(Mutex::new(None)) }
    pub fn terminate(&self) {
        if let Some(child) = self.0.lock().unwrap().as_mut() { let _ = child.kill(); }
    }
    pub fn wait(&self, child: Child, cancelled: &AtomicBool) -> io::Result<ExitStatus> {
        {
            let mut slot = self.0.lock().unwrap();
            *slot = Some(child);
            if cancelled.load(Ordering::SeqCst) { let _ = slot.as_mut().unwrap().kill(); }
        }
        loop {
            let mut slot = self.0.lock().unwrap();
            match slot.as_mut().unwrap().try_wait() {
                Ok(Some(status)) => { slot.take(); return Ok(status); }
                Err(e) => { if let Some(mut child) = slot.take() { let _ = child.kill(); let _ = child.wait(); } return Err(e); }
                Ok(None) => {}
            }
            drop(slot);
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

pub fn curl_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("SystemRoot").ok_or("Не найден системный каталог Windows")?;
    let root = PathBuf::from(root);
    if !root.is_absolute() { return Err("Некорректный системный каталог Windows".into()); }
    Ok(root.join("System32").join("curl.exe"))
}

pub fn verify_sha256(path: &Path, expected: &str, cancelled: &AtomicBool) -> Result<bool, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        if cancelled.load(Ordering::SeqCst) { return Err("Загрузка отменена".into()); }
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 { break; }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()) == expected)
}

// Downloading a model must not flash a console window in the desktop app.
#[cfg(windows)]
pub fn hide_console(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

// Allows exercising the Windows download logic on a non-Windows test host.
#[cfg(not(windows))]
pub fn hide_console(_: &mut std::process::Command) {}
