#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]



mod model_download;
mod rag_model_download;
mod rag_e4b_model_download;

static DOWNLOAD_GATE:std::sync::Mutex<()>=std::sync::Mutex::new(());
fn ensure_downloads_finished(statuses:&[String])->Result<(),String>{if statuses.iter().any(|s|s.starts_with("downloading")){Err("Дождитесь завершения загрузки модели перед перезапуском Atlas.".into())}else{Ok(())}}
fn check_downloads(app:&tauri::AppHandle)->Result<(),String>{let dir=model_data_dir(app)?;ensure_downloads_finished(&[model_download::status(dir.clone()),rag_model_download::status(dir.clone()),rag_e4b_model_download::status(dir)])}
#[tauri::command]
fn prepare_restart(app:tauri::AppHandle)->Result<(),String>{let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось проверить загрузки")?;check_downloads(&app)}

fn model_data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    match std::env::var_os("ATLAS_TEST_DATA_DIR") { Some(path) => Ok(path.into()), None => app.path().app_data_dir().map_err(|e|e.to_string()) }
}
#[tauri::command]
fn download_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(model_download::status(model_data_dir(&app)?)) }
#[tauri::command]
fn download_rag_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;rag_model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn rag_model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(rag_model_download::status(model_data_dir(&app)?)) }
#[tauri::command]
fn download_rag_e4b_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;rag_e4b_model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn rag_e4b_model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(rag_e4b_model_download::status(model_data_dir(&app)?)) }

struct TransferBackend { port:u16, token:String }
#[tauri::command]
async fn select_workspace_directory(app:tauri::AppHandle,operation:String)->Result<Option<String>,String> {
    if operation!="import" && operation!="export" {return Err("Недопустимая операция.".into())}
    use tauri::Manager;
    let auth=app.try_state::<TransferBackend>().ok_or("Выбор папки недоступен в этом режиме.")?;
    let port=auth.port;let token=auth.token.clone();
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os="macos")]
        {
            use std::io::{Read,Write};
            let output=std::process::Command::new("/usr/bin/osascript").args(["-e","POSIX path of (choose folder with prompt \"Выберите папку\")"]).output().map_err(|_|"Не удалось открыть выбор папки.")?;
            if !output.status.success(){if String::from_utf8_lossy(&output.stderr).contains("(-128)"){return Ok(None)}return Err("Не удалось выбрать папку.".into())}
            let raw=String::from_utf8(output.stdout).map_err(|_|"Не удалось прочитать путь к папке.")?;
            let path=raw.strip_suffix('\n').unwrap_or(&raw);
            if !std::path::Path::new(path).is_absolute(){return Err("Не удалось прочитать путь к папке.".into())}
            let body=serde_json::json!({"operation":operation,"path":path}).to_string();
            let mut socket=std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST,port)).map_err(|_|"Не удалось связаться с Atlas.")?;
            socket.set_read_timeout(Some(std::time::Duration::from_secs(10))).map_err(|_|"Не удалось связаться с Atlas.")?;
            write!(socket,"POST /api/workspaces/authorize HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Atlas-Token: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",token,body.len(),body).map_err(|_|"Не удалось связаться с Atlas.")?;
            let mut response=String::new();socket.read_to_string(&mut response).map_err(|_|"Не удалось связаться с Atlas.")?;
            let (head,data)=response.split_once("\r\n\r\n").ok_or("Не удалось подтвердить выбор папки.")?;
            if !head.lines().next().is_some_and(|line|line.starts_with("HTTP/1.1 200 ")||line.starts_with("HTTP/1.0 200 ")){return Err("Не удалось подтвердить выбор папки.".into())}
            let value:serde_json::Value=serde_json::from_str(data).map_err(|_|"Не удалось подтвердить выбор папки.")?;
            let capability=value.get("capability").and_then(|v|v.as_str()).filter(|v|!v.is_empty()).ok_or("Не удалось подтвердить выбор папки.")?;
            Ok(Some(capability.to_owned()))
        }
        #[cfg(not(target_os="macos"))]
        {let _=(port,token,operation);Err("Выбор папки пока доступен только на macOS.".into())}
    }).await.map_err(|_|"Не удалось открыть выбор папки.".to_owned())?
}

fn allowed_external_url(url: &str) -> bool {
    tauri::Url::parse(url).is_ok_and(|parsed| {
        matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some()
    })
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    if !allowed_external_url(&url) {
        return Err("Недопустимый адрес".into());
    }
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    #[cfg(target_os = "windows")]
    return Err("Открытие ссылок на Windows пока не поддерживается".into());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    std::process::Command::new(program).arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(debug_assertions)]
fn main() {
    tauri::Builder::default()
        .setup(|app| { tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External("http://127.0.0.1:5173".parse()?)).disable_drag_drop_handler().title("Atlas").inner_size(1200.0, 800.0).build()?; Ok(()) })
        .invoke_handler(tauri::generate_handler![select_workspace_directory, open_external_url, download_model, model_download_status, download_rag_model, rag_model_download_status, download_rag_e4b_model, rag_e4b_model_download_status, prepare_restart, restart_atlas])
        .run(tauri::generate_context!())
        .expect("failed to start Atlas");
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_http_and_https_urls_allowed() {
        assert!(super::allowed_external_url("https://example.com/path"));
        assert!(super::allowed_external_url("http://example.com/"));
        assert!(!super::allowed_external_url("file:///etc/passwd"));
        assert!(!super::allowed_external_url("javascript:alert(1)"));
        assert!(!super::allowed_external_url("https://"));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn external_link_invokes_system_opener_with_one_url_argument() {
        use std::{fs, os::unix::fs::PermissionsExt, time::Duration};
        let dir = std::env::temp_dir().join(format!("atlas-opener-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let opener = dir.join("open");
        let capture = dir.join("capture");
        fs::write(&opener, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", capture.display())).unwrap();
        fs::set_permissions(&opener, fs::Permissions::from_mode(0o700)).unwrap();
        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &dir);
        super::open_external_url("https://example.com/a?x=1".into()).unwrap();
        for _ in 0..100 {
            if capture.exists() { break; }
            std::thread::sleep(Duration::from_millis(10));
        }
        let actual = fs::read_to_string(&capture).unwrap();
        match old_path { Some(value) => std::env::set_var("PATH", value), None => std::env::remove_var("PATH") };
        fs::remove_dir_all(dir).unwrap();
        assert_eq!(actual, "https://example.com/a?x=1\n");
        assert!(super::open_external_url("file:///etc/passwd".into()).is_err());
    }

}

#[cfg(not(debug_assertions))]
struct Children(std::sync::Mutex<Vec<std::process::Child>>);
#[cfg(not(debug_assertions))]
impl Children {
 fn stop(&self) -> Result<(),String> {
  model_download::cleanup(); rag_model_download::cleanup(); rag_e4b_model_download::cleanup();
  let mut children=self.0.lock().map_err(|_|"Не удалось остановить службы Atlas".to_string())?;
  for child in children.iter_mut(){let pid=child.id() as i32;let rc=unsafe{libc::kill(-pid,libc::SIGTERM)};if rc!=0 {let err=std::io::Error::last_os_error();if err.raw_os_error()!=Some(libc::ESRCH){return Err(format!("Не удалось остановить службу: {err}"));}}}
  let started=std::time::Instant::now();let mut killed=false;
  loop {
   let mut running=false;
   for child in children.iter_mut(){let _=child.try_wait().map_err(|e|format!("Не удалось проверить службу: {e}"))?;let pid=child.id() as i32;let rc=unsafe{libc::kill(-pid,0)};if rc==0 {running=true;} else {let err=std::io::Error::last_os_error();if err.raw_os_error()!=Some(libc::ESRCH){return Err(format!("Не удалось проверить процессы службы: {err}"));}}}
   if !running {break;}
   if started.elapsed()>=std::time::Duration::from_secs(3) && !killed {for child in children.iter(){let pid=child.id() as i32;let rc=unsafe{libc::kill(-pid,libc::SIGKILL)};if rc!=0 {let err=std::io::Error::last_os_error();if err.raw_os_error()!=Some(libc::ESRCH){return Err(format!("Не удалось завершить службу: {err}"));}}}killed=true;}
   if started.elapsed()>=std::time::Duration::from_secs(8){return Err("Не удалось завершить процессы Atlas. Закройте приложение и откройте его вручную.".into());}
   std::thread::sleep(std::time::Duration::from_millis(50));
  }
  for child in children.iter_mut(){let _=child.try_wait().map_err(|e|format!("Не удалось дождаться завершения службы: {e}"))?;}
  children.clear();Ok(())
 }
}
#[cfg(not(debug_assertions))]
impl Drop for Children {fn drop(&mut self){let _=self.stop();}}
#[cfg(not(debug_assertions))]
fn restart_after_stop(stop:impl FnOnce()->Result<(),String>,spawn:impl FnOnce()->Result<(),String>)->Result<(),String>{stop()?;spawn()}
#[cfg(not(debug_assertions))]
#[tauri::command]
fn restart_atlas(app:tauri::AppHandle)->Result<(),String>{
 use tauri::Manager;
 let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось проверить загрузки")?;
 check_downloads(&app)?;
 let exe=std::env::current_exe().map_err(|e|format!("Не удалось найти приложение: {e}"))?;
 restart_after_stop(||app.state::<Children>().stop(),||std::process::Command::new(exe).args(std::env::args_os().skip(1)).spawn().map(|_|()).map_err(|e|format!("Не удалось запустить Atlas заново. Закройте и откройте его вручную: {e}")))?;
 app.exit(0);Ok(())
}
#[cfg(debug_assertions)]
#[tauri::command]
fn restart_atlas()->Result<(),String>{Err("Перезапуск доступен в собранном приложении Atlas".into())}

#[cfg(not(debug_assertions))]
fn main() {
    use std::{fs,io::{BufRead,BufReader,Read,Write},process::{Child,Command,Stdio},sync::Mutex};
    use tauri::{Manager,WebviewUrl,WebviewWindowBuilder};
    use std::os::unix::process::CommandExt;
    use std::sync::atomic::{AtomicI32,Ordering};
    static BACKEND:AtomicI32=AtomicI32::new(0); static PROXY:AtomicI32=AtomicI32::new(0);
    extern "C" fn terminate(_:i32){ model_download::terminate_child(); rag_model_download::terminate_child(); rag_e4b_model_download::terminate_child(); for id in [&BACKEND,&PROXY] { let pid=id.load(Ordering::Relaxed); if pid>0 { unsafe { libc::kill(-pid,libc::SIGTERM); } } } unsafe { libc::_exit(0); } }
    fn secret()->std::io::Result<String> {let mut b=[0u8;32];fs::File::open("/dev/urandom")?.read_exact(&mut b)?; Ok(b.iter().map(|b|format!("{b:02x}")).collect())}
    fn launch(exe:&std::path::Path,args:&[String],input:&str,slot:&AtomicI32)->Result<(Child,u16),Box<dyn std::error::Error>> {
        let mut command=Command::new(exe); command.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()); unsafe {command.pre_exec(|| {if libc::setsid()<0 {return Err(std::io::Error::last_os_error());} Ok(())});} let mut c=command.spawn()?; slot.store(c.id() as i32,Ordering::Relaxed);
        let result=(|| -> Result<u16,Box<dyn std::error::Error>> { c.stdin.take().ok_or("missing stdin")?.write_all(input.as_bytes())?; let mut line=String::new(); BufReader::new(c.stdout.take().ok_or("missing stdout")?).read_line(&mut line)?; Ok(line.trim().parse()?) })();
        match result {Ok(port)=>Ok((c,port)),Err(e)=>{unsafe {libc::kill(-(c.id() as i32),libc::SIGTERM);} let _=c.wait();slot.store(0,Ordering::Relaxed);Err(e)}}
    }
    tauri::Builder::default().setup(|app| {
        unsafe {libc::signal(libc::SIGTERM, terminate as *const () as libc::sighandler_t); libc::signal(libc::SIGINT, terminate as *const () as libc::sighandler_t);}
        let resources=app.path().resource_dir()?.join("resources");
        let data=model_data_dir(app.handle()).map_err(std::io::Error::other)?;
        let mut workspace=data.join("workspaces");
        let mut legacy_default=None;
        {
            let legacy=if std::env::var_os("ATLAS_TEST_DATA_DIR").is_some() {data.join("legacy/atlas")} else {app.path().home_dir()?.join("Documents/Agent/TODO/atlas")};
            let default=legacy.join("demo-workspace"); let registry=legacy.join(".demo-workspace-workspaces");
            if registry.is_dir() && (default.join("atlas.sqlite3").is_file() || registry.join(".default-deleted").is_file()) {workspace=registry;legacy_default=Some(default);}
        }
        fs::create_dir_all(&workspace)?;
        let model=data.join("models/embeddinggemma-2.gguf");
        let token=secret()?;let bootstrap=secret()?;
        let mut backend_args=vec!["--root".into(),workspace.to_string_lossy().into_owned(),"--port".into(),"0".into(),"--token-stdin".into()];
        if let Some(default)=legacy_default { backend_args.extend(["--default-workspace".into(),default.to_string_lossy().into_owned()]); }
        let rag_model=data.join("models/gemma-4-e2b-it-q4.gguf");
        let rag_e4b_model=data.join("models/gemma-4-e4b-it-q4.gguf");
        if model.is_file() { backend_args.extend(["--model-path".into(),model.to_string_lossy().into_owned()]); }
        if rag_model.is_file() { backend_args.extend(["--rag-model-path".into(),rag_model.to_string_lossy().into_owned()]); }
        backend_args.extend(["--rag-model-dir".into(),data.join("models").to_string_lossy().into_owned()]);
        if model.is_file() || rag_model.is_file() || rag_e4b_model.is_file() { backend_args.extend(["--llama-server".into(),resources.join("llama/llama-server").to_string_lossy().into_owned()]); }
        let (backend,backend_port)=launch(&resources.join("atlas-backend"),&backend_args,&format!("{token}\n"),&BACKEND)?;
        app.manage(TransferBackend{port:backend_port,token:token.clone()});
        BACKEND.store(backend.id() as i32,Ordering::Relaxed);
        let (proxy,port)=match launch(&resources.join("atlas-proxy"),&[resources.join("ui").to_string_lossy().into_owned(),backend_port.to_string(),"0".into()],&format!("{bootstrap}\n{token}\n"),&PROXY){Ok(v)=>v,Err(e)=>{let mut b=backend;unsafe {libc::kill(-(b.id() as i32),libc::SIGTERM);} let _=b.wait(); return Err(e.into());}};
        PROXY.store(proxy.id() as i32,Ordering::Relaxed); app.manage(Children(Mutex::new(vec![backend,proxy])));
        let origin=format!("http://127.0.0.1:{port}");let url=format!("{origin}/bootstrap/{bootstrap}").parse()?;
        WebviewWindowBuilder::new(app,"main",WebviewUrl::External(url)).disable_drag_drop_handler().title("Atlas").inner_size(1200.0,800.0).on_navigation(move |url| url.scheme()=="http" && url.host_str()==Some("127.0.0.1") && url.port_or_known_default()==Some(port)).build()?;
        Ok(())
    }).invoke_handler(tauri::generate_handler![select_workspace_directory, open_external_url, download_model, model_download_status, download_rag_model, rag_model_download_status, download_rag_e4b_model, rag_e4b_model_download_status, prepare_restart, restart_atlas]).build(tauri::generate_context!()).expect("failed to start Atlas").run(|app,event|{if matches!(event,tauri::RunEvent::Exit){use tauri::Manager;if let Some(children)=app.try_state::<Children>(){if let Err(e)=children.stop(){eprintln!("Atlas: {e}");}}}});
}
#[cfg(all(test,not(debug_assertions),unix))]
mod restart_group_contract_tests {
 use super::*;use std::os::unix::process::CommandExt;
 #[test]fn stop_reaps_stubborn_process_group_before_return(){
  let mut command=std::process::Command::new("/bin/sh");command.args(["-c","trap '' TERM; (trap '' TERM; while :; do sleep 1; done) & wait"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());unsafe{command.pre_exec(||{if libc::setsid()<0{return Err(std::io::Error::last_os_error())}Ok(())});}let child=command.spawn().unwrap();let pid=child.id() as i32;std::thread::sleep(std::time::Duration::from_millis(150));let state=Children(std::sync::Mutex::new(vec![child]));let start=std::time::Instant::now();state.stop().expect("stubborn process group should terminate");assert!(start.elapsed()<std::time::Duration::from_secs(9));assert_eq!(unsafe{libc::kill(-pid,0)},-1);assert_eq!(std::io::Error::last_os_error().raw_os_error(),Some(libc::ESRCH));assert!(state.0.lock().unwrap().is_empty());
 }
}
#[cfg(test)] mod download_restart_contract_tests {
 #[test] fn blocks_downloads_including_finalization(){for value in ["downloading","downloading:0","downloading:100"] {let e=super::ensure_downloads_finished(&["complete".into(),value.into(),"idle".into()]);assert!(e.is_err(),"{value}");}}
 #[test] fn completed_failed_and_idle_do_not_block_restart(){for value in ["idle","complete","error","error:network"] {assert!(super::ensure_downloads_finished(&[value.into()]).is_ok(),"{value}");}}
}
#[cfg(all(test,not(debug_assertions)))] mod restart_failure_contract_tests {
 #[test]fn failed_stop_never_spawns(){let called=std::cell::Cell::new(false);let result=super::restart_after_stop(||Err("stop failed".to_owned()),||{called.set(true);Ok(())});assert!(result.is_err());assert!(!called.get());}
 #[test]fn failed_spawn_after_stop_reports_failure(){let stopped=std::cell::Cell::new(false);let result=super::restart_after_stop(||{stopped.set(true);Ok(())},||Err("spawn failed".to_owned()));assert!(stopped.get());assert!(result.unwrap_err().contains("spawn failed"));}
}
