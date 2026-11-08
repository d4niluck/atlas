#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod vision_model_download;
mod voice_temp;
mod voice;
#[cfg(target_os="macos")] mod menu_search;
#[cfg(not(target_os="macos"))] mod menu_search {
 #[tauri::command] pub fn voice_microphone_prompt(_window:tauri::WebviewWindow,_active:bool){}
 #[tauri::command] pub fn set_quick_search_mode()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn show_main_window()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn hide_quick_search()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn open_quick_result()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn take_quick_result()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn open_search_settings()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn take_search_settings_request()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn request_atlas_exit()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn take_atlas_exit_request()->Result<(),String>{Err("Доступно только на macOS".into())}
 #[tauri::command] pub fn finish_atlas_exit()->Result<(),String>{Err("Доступно только на macOS".into())}
}
#[cfg(target_os="linux")] mod memory_linux;



mod model_download;
mod q8_model_download;
mod save_file;
mod rag_model_download;
mod rag_e4b_model_download;
mod nemotron_model_download;

static DOWNLOAD_GATE:std::sync::Mutex<()>=std::sync::Mutex::new(());
fn ensure_downloads_finished(statuses:&[String])->Result<(),String>{if statuses.iter().any(|s|s.starts_with("downloading")){Err("Дождитесь завершения загрузки модели перед перезапуском Atlas.".into())}else{Ok(())}}
fn check_downloads(app:&tauri::AppHandle)->Result<(),String>{if voice::downloading(){return Err("Дождитесь завершения загрузки модели перед перезапуском Atlas.".into())}let dir=model_data_dir(app)?;ensure_downloads_finished(&[model_download::status(dir.clone()),q8_model_download::status(dir.clone()),vision_model_download::status(dir.clone()),rag_model_download::status(dir.clone()),rag_e4b_model_download::status(dir.clone()),nemotron_model_download::status(dir)])}
#[tauri::command]
fn prepare_restart(app:tauri::AppHandle)->Result<(),String>{let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось проверить загрузки")?;check_downloads(&app)}

fn model_data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    match std::env::var_os("ATLAS_TEST_DATA_DIR") { Some(path) => Ok(path.into()), None => app.path().app_data_dir().map_err(|e|e.to_string()) }
}
#[tauri::command]
fn models_intro_seen(app:tauri::AppHandle)->Result<bool,String>{Ok(model_data_dir(&app)?.join("models-intro-seen").is_file())}
#[tauri::command]
fn dismiss_models_intro(app:tauri::AppHandle)->Result<(),String>{let dir=model_data_dir(&app)?;std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;std::fs::write(dir.join("models-intro-seen"),b"1").map_err(|e|e.to_string())}
#[tauri::command]
fn download_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(model_download::status(model_data_dir(&app)?)) }
#[tauri::command]
fn download_q8_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;q8_model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn q8_model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(q8_model_download::status(model_data_dir(&app)?)) }
#[tauri::command]
fn download_vision_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;vision_model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn vision_model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(vision_model_download::status(model_data_dir(&app)?)) }
#[tauri::command]
fn download_rag_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;rag_model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn rag_model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(rag_model_download::status(model_data_dir(&app)?)) }
#[tauri::command]
fn download_rag_e4b_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;rag_e4b_model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn rag_e4b_model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(rag_e4b_model_download::status(model_data_dir(&app)?)) }
#[tauri::command]
fn download_nemotron_model(app: tauri::AppHandle) -> Result<(), String> { let _guard=DOWNLOAD_GATE.lock().map_err(|_|"Не удалось начать загрузку")?;nemotron_model_download::start(model_data_dir(&app)?) }
#[tauri::command]
fn nemotron_model_download_status(app: tauri::AppHandle) -> Result<String, String> { Ok(nemotron_model_download::status(model_data_dir(&app)?)) }

#[cfg(target_os="linux")]
async fn linux_select_directory()->Result<Option<String>,String>{
 tauri::async_runtime::spawn_blocking(||{
  let output=std::process::Command::new("/usr/bin/zenity").args(["--file-selection","--directory","--title=Выберите папку"]).output().map_err(|_|"Не удалось открыть выбор папки. Проверьте установку zenity.")?;
  match output.status.code(){
   Some(0)=>{let text=String::from_utf8(output.stdout).map_err(|_|"Не удалось прочитать путь к папке.")?;let path=text.strip_suffix('\n').unwrap_or(&text);if path.is_empty(){return Err("Не удалось прочитать путь к папке.".into())}Ok(Some(path.to_owned()))},
   Some(1)=>Ok(None),
   _=>Err("Не удалось открыть выбор папки.".into()),
  }
 }).await.map_err(|_|"Не удалось открыть выбор папки.".to_owned())?
}

struct TransferBackend { port:u16, token:String }
#[tauri::command]
async fn select_workspace_directory(app:tauri::AppHandle,operation:String)->Result<Option<String>,String> {
    if operation!="import" && operation!="export" && operation!="attach" {return Err("Недопустимая операция.".into())}
    use tauri::Manager;
    let auth=app.try_state::<TransferBackend>().ok_or("Выбор папки недоступен в этом режиме.")?;
    let port=auth.port;let token=auth.token.clone();
    #[cfg(target_os="macos")]
    let path = {
        let (sender, receiver) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || {
            let Some(mtm) = objc2::MainThreadMarker::new() else { let _=sender.send(None); return; };
            let panel = objc2_app_kit::NSOpenPanel::openPanel(mtm);
            panel.setCanChooseFiles(false);
            panel.setCanChooseDirectories(true);
            panel.setAllowsMultipleSelection(false);
            panel.setCanCreateDirectories(true);
            let selected = if panel.runModal() == 1 { panel.URL().and_then(|url| url.path()).map(|path| path.to_string()) } else { None };
            let _=sender.send(selected);
        }).map_err(|_| "Не удалось открыть выбор папки.")?;
        tauri::async_runtime::spawn_blocking(move || receiver.recv().ok().flatten()).await.map_err(|_| "Не удалось открыть выбор папки.")?
    };
    #[cfg(target_os="linux")]
    let path=linux_select_directory().await?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(any(target_os="macos",target_os="linux"))]
        {
            use std::io::{Read,Write};
            let Some(path)=path else {return Ok(None)};
            let path=path.as_str();
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
        #[cfg(not(any(target_os="macos",target_os="linux")))]
        {let _=(port,token,operation);Err("Выбор папки на этой системе пока не поддерживается.".into())}
    }).await.map_err(|_|"Не удалось открыть выбор папки.".to_owned())?
}

#[tauri::command]
async fn save_workspace_file(app:tauri::AppHandle,workspace:String,id:String,title:String)->Result<bool,String>{
 use tauri::Manager;
 let auth=app.try_state::<TransferBackend>().ok_or("Сохранение недоступно.")?;let port=auth.port;let token=auth.token.clone();
 #[cfg(target_os="macos")]
 let path={let(sender,receiver)=std::sync::mpsc::channel();app.run_on_main_thread(move||{let Some(mtm)=objc2::MainThreadMarker::new() else{let _=sender.send(None);return};let panel=objc2_app_kit::NSOpenPanel::openPanel(mtm);panel.setCanChooseFiles(false);panel.setCanChooseDirectories(true);panel.setAllowsMultipleSelection(false);panel.setCanCreateDirectories(true);let selected=if panel.runModal()==1{panel.URL().and_then(|url|url.path()).map(|path|path.to_string())}else{None};let _=sender.send(selected);}).map_err(|_|"Не удалось открыть окно сохранения")?;tauri::async_runtime::spawn_blocking(move||receiver.recv().ok().flatten()).await.map_err(|_|"Не удалось открыть окно сохранения")?};
 #[cfg(target_os="linux")]
 let path=linux_select_directory().await?;
 #[cfg(any(target_os="macos",target_os="linux"))]
 {let Some(path)=path else{return Ok(false)};let name=std::path::Path::new(&title);if name.file_name().and_then(|n|n.to_str())!=Some(title.as_str())||title=="."||title==".."{return Err("Некорректное имя файла".into())}let path=std::path::Path::new(&path).join(name);tauri::async_runtime::spawn_blocking(move||save_file::download(port,&token,&workspace,&id,&path)).await.map_err(|_|"Не удалось сохранить файл")??;Ok(true)}
 #[cfg(not(any(target_os="macos",target_os="linux")))]
 {let _=(app,port,token,workspace,id,title);Err("Сохранение на этой системе пока не поддерживается".into())}
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
    voice_temp::cleanup(&std::env::temp_dir());
    tauri::Builder::default()
        .setup(|app| { tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External("http://127.0.0.1:5173".parse()?)).disable_drag_drop_handler().title("Atlas").inner_size(1200.0, 800.0).build()?; #[cfg(target_os="macos")] menu_search::install(app,"http://127.0.0.1:5173".into())?; Ok(()) })
        .invoke_handler(tauri::generate_handler![menu_search::voice_microphone_prompt,voice::voice_get_quality, voice::voice_set_quality, voice::voice_model_status, voice::voice_repair_model, voice::voice_download_model, voice::voice_transcribe, voice::voice_cancel, models_intro_seen, dismiss_models_intro, select_workspace_directory, save_workspace_file, open_external_url, download_model, model_download_status, download_q8_model, q8_model_download_status, download_vision_model, vision_model_download_status, download_rag_model, rag_model_download_status, download_rag_e4b_model, rag_e4b_model_download_status, download_nemotron_model, nemotron_model_download_status, prepare_restart, restart_atlas, menu_search::set_quick_search_mode, menu_search::show_main_window, menu_search::hide_quick_search, menu_search::open_quick_result, menu_search::take_quick_result, menu_search::request_atlas_exit, menu_search::take_atlas_exit_request, menu_search::finish_atlas_exit, menu_search::open_search_settings, menu_search::take_search_settings_request])
        .build(tauri::generate_context!()).expect("failed to start Atlas")
        .run(|app,event|{
            #[cfg(target_os="macos")] match &event {tauri::RunEvent::ExitRequested{code:None,api,..}=>{api.prevent_exit();let _=menu_search::request_atlas_exit(app.clone());},tauri::RunEvent::Reopen{has_visible_windows:false,..}=>{let _=menu_search::show_main_window(app.clone());},_=>{}}
            if matches!(event,tauri::RunEvent::Exit){voice::shutdown()}
            #[cfg(not(target_os="macos"))] let _=(app,event);
        });
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
  model_download::cleanup(); q8_model_download::cleanup(); vision_model_download::cleanup(); rag_model_download::cleanup(); rag_e4b_model_download::cleanup(); nemotron_model_download::cleanup();
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
    voice_temp::cleanup(&std::env::temp_dir());
    use std::{fs,io::{BufRead,BufReader,Read,Write},process::{Child,Command,Stdio},sync::Mutex};
    use tauri::{Manager,WebviewUrl,WebviewWindowBuilder};
    use std::os::unix::process::CommandExt;
    use std::sync::atomic::{AtomicI32,Ordering};
    static BACKEND:AtomicI32=AtomicI32::new(0); static PROXY:AtomicI32=AtomicI32::new(0);
    extern "C" fn terminate(_:i32){ model_download::terminate_child(); q8_model_download::terminate_child(); vision_model_download::terminate_child(); rag_model_download::terminate_child(); rag_e4b_model_download::terminate_child(); nemotron_model_download::terminate_child(); for id in [&BACKEND,&PROXY] { let pid=id.load(Ordering::Relaxed); if pid>0 { unsafe { libc::kill(-pid,libc::SIGTERM); } } } unsafe { libc::_exit(0); } }
    fn secret()->std::io::Result<String> {let mut b=[0u8;32];fs::File::open("/dev/urandom")?.read_exact(&mut b)?; Ok(b.iter().map(|b|format!("{b:02x}")).collect())}
    fn launch(exe:&std::path::Path,args:&[String],input:&str,slot:&AtomicI32)->Result<(Child,u16),Box<dyn std::error::Error>> {
        let mut command=Command::new(exe); command.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()); unsafe {command.pre_exec(|| {if libc::setsid()<0 {return Err(std::io::Error::last_os_error());} Ok(())});} let mut c=command.spawn()?; slot.store(c.id() as i32,Ordering::Relaxed);
        let result=(|| -> Result<u16,Box<dyn std::error::Error>> { c.stdin.take().ok_or("missing stdin")?.write_all(input.as_bytes())?; let mut line=String::new(); BufReader::new(c.stdout.take().ok_or("missing stdout")?).read_line(&mut line)?; Ok(line.trim().parse()?) })();
        match result {Ok(port)=>Ok((c,port)),Err(e)=>{unsafe {libc::kill(-(c.id() as i32),libc::SIGTERM);} let _=c.wait();slot.store(0,Ordering::Relaxed);Err(e)}}
    }
    tauri::Builder::default().setup(|app| {
        #[cfg(target_os="macos")]
        {
            use objc2::{AllocAnyThread, MainThreadMarker};
            use objc2_app_kit::{NSApplication, NSImage};
            use objc2_foundation::NSData;
            let bytes=NSData::with_bytes(include_bytes!("../icons/icon.png"));
            if let (Some(mtm),Some(image))=(MainThreadMarker::new(),NSImage::initWithData(NSImage::alloc(),&bytes)) {
                unsafe {NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image));}
            }
        }
        unsafe {libc::signal(libc::SIGTERM, terminate as *const () as libc::sighandler_t); libc::signal(libc::SIGINT, terminate as *const () as libc::sighandler_t);}
        let resources=app.path().resource_dir()?.join("resources");
        let data=model_data_dir(app.handle()).map_err(std::io::Error::other)?;
        let mut workspace=data.join("workspaces");
        let mut legacy_default=None;
        {
            let legacy=if std::env::var_os("ATLAS_TEST_DATA_DIR").is_some() {data.join("legacy/atlas")} else {fs::read_to_string(data.join("legacy-workspace-root")).ok().map(|value|std::path::PathBuf::from(value.trim())).filter(|path|path.is_absolute()).unwrap_or_else(||data.join("legacy/atlas"))};
            let default=legacy.join("demo-workspace"); let registry=legacy.join(".demo-workspace-workspaces");
            if registry.is_dir() && (default.join("atlas.sqlite3").is_file() || registry.join(".default-deleted").is_file()) {workspace=registry;legacy_default=Some(default);}
        }
        fs::create_dir_all(&workspace)?;
        #[cfg(target_os="macos")]
        {
            use tauri::menu::{CheckMenuItem,Menu,MenuItem};
            use std::sync::{Arc,atomic::AtomicBool};
            let setting=data.join("show-memory");
            let enabled=Arc::new(AtomicBool::new(setting.is_file()));
            let menu=Menu::default(app.handle())?;
            let items=menu.items()?;
            let app_menu=items.first().and_then(|i|i.as_submenu()).ok_or("Нет меню Atlas")?;
            let toggle=CheckMenuItem::with_id(app,"atlas-memory","Показывать потребление памяти",true,enabled.load(Ordering::Relaxed),None::<&str>)?;
            let label=MenuItem::with_id(app,"atlas-memory-value","Память: выключено",false,None::<&str>)?;
            let detail=MenuItem::with_id(app,"atlas-memory-detail","Без WebKit; общая память может учитываться повторно",false,None::<&str>)?;
            app_menu.insert(&toggle,1)?;app_menu.insert(&label,2)?;app_menu.insert(&detail,3)?;
            app.set_menu(menu)?;
            let active=enabled.clone();let toggle_event=toggle.clone();
            app.on_menu_event(move |_app,event| {if event.id().as_ref()=="atlas-memory" {let next=!active.load(Ordering::Relaxed);let result=if next {std::fs::write(&setting,b"1")} else {match std::fs::remove_file(&setting){Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),r=>r}};if result.is_ok(){active.store(next,Ordering::Relaxed);}let _=toggle_event.set_checked(active.load(Ordering::Relaxed));}});
            std::thread::spawn(move || loop {
                if enabled.load(Ordering::Relaxed) {
                    let text=std::process::Command::new("/bin/ps").args(["-axo","pid=,ppid=,rss="]).stdout(std::process::Stdio::piped()).spawn().ok().and_then(|child|{let pid=child.id();child.wait_with_output().ok().filter(|o|o.status.success()).and_then(|o|String::from_utf8(o.stdout).ok()).map(|text|(pid,text))});
                    let value=text.and_then(|(ps_pid,text)| {
                        let rows:Vec<(u32,u32,u64)>=text.lines().filter_map(|line|{let mut x=line.split_whitespace();Some((x.next()?.parse().ok()?,x.next()?.parse().ok()?,x.next()?.parse().ok()?))}).collect();
                        let root=std::process::id();if !rows.iter().any(|r|r.0==root){return None}
                        let mut ids=std::collections::HashSet::from([root]);loop {let old=ids.len();for (pid,parent,_) in &rows {if ids.contains(parent){ids.insert(*pid);}}if old==ids.len(){break}}
                        Some(rows.iter().filter(|r|r.0!=ps_pid&&ids.contains(&r.0)).map(|r|r.2).sum::<u64>())
                    });
                    if enabled.load(Ordering::Relaxed) {let _=label.set_text(value.map(|kb|format!("Память Atlas и моделей: ≈ {:.0} МиБ",kb as f64/1024.)).unwrap_or_else(||"Память: недоступна".into()));}
                } else {let _=label.set_text("Память: выключено");}
                std::thread::sleep(std::time::Duration::from_secs(3));
            });
        }
        #[cfg(target_os="linux")]
        {
            use tauri::menu::{CheckMenuItem,Menu,MenuItem,Submenu};
            use std::sync::{Arc,atomic::AtomicBool};
            let setting=data.join("show-memory");
            let enabled=Arc::new(AtomicBool::new(setting.is_file()));
            let toggle=CheckMenuItem::with_id(app,"atlas-memory","Показывать потребление памяти",true,enabled.load(Ordering::Relaxed),None::<&str>)?;
            let label=MenuItem::with_id(app,"atlas-memory-value","Память: выключено",false,None::<&str>)?;
            let detail=MenuItem::with_id(app,"atlas-memory-detail","Сумма процессов; общая память может учитываться повторно",false,None::<&str>)?;
            let submenu=Submenu::with_items(app,"Atlas",true,&[&toggle,&label,&detail])?;
            app.set_menu(Menu::with_items(app,&[&submenu])?)?;
            let active=enabled.clone();let toggle_event=toggle.clone();let data_for_memory=data.clone();
            app.on_menu_event(move |_app,event| {if event.id().as_ref()=="atlas-memory" {let next=!active.load(Ordering::Relaxed);let result=if next {std::fs::create_dir_all(&data_for_memory).and_then(|_|std::fs::write(&setting,b"1"))} else {match std::fs::remove_file(&setting){Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),r=>r}};if result.is_ok(){active.store(next,Ordering::Relaxed);}let _=toggle_event.set_checked(active.load(Ordering::Relaxed));}});
            std::thread::spawn(move || loop {
                if enabled.load(Ordering::Relaxed) {
                    let value=std::process::Command::new("/bin/ps").args(["-axo","pid=,ppid=,rss="]).stdout(std::process::Stdio::piped()).spawn().ok().and_then(|child|{let pid=child.id();child.wait_with_output().ok().filter(|o|o.status.success()).and_then(|o|String::from_utf8(o.stdout).ok()).and_then(|text|memory_linux::process_tree_rss_kib(&text,std::process::id(),pid))});
                    if enabled.load(Ordering::Relaxed) {let _=label.set_text(value.map(|kb|format!("Память Atlas и моделей: ≈ {:.0} МиБ",kb as f64/1024.)).unwrap_or_else(||"Память: недоступна".into()));}
                } else {let _=label.set_text("Память: выключено");}
                std::thread::sleep(std::time::Duration::from_secs(3));
            });
        }
        let model=data.join("models/embeddinggemma-2.gguf");
        let token=secret()?;let bootstrap=secret()?;
        let mut backend_args=vec!["--root".into(),workspace.to_string_lossy().into_owned(),"--port".into(),"0".into(),"--token-stdin".into()];
        if let Some(default)=legacy_default { backend_args.extend(["--default-workspace".into(),default.to_string_lossy().into_owned()]); }
        let rag_model=data.join("models/gemma-4-e2b-it-q4.gguf");
        if model.is_file() { backend_args.extend(["--model-path".into(),model.to_string_lossy().into_owned()]); }
        if rag_model.is_file() { backend_args.extend(["--rag-model-path".into(),rag_model.to_string_lossy().into_owned()]); }
        backend_args.extend(["--rag-model-dir".into(),data.join("models").to_string_lossy().into_owned()]);
        backend_args.extend(["--llama-server".into(),resources.join("llama/llama-server").to_string_lossy().into_owned()]);
        let (backend,backend_port)=launch(&resources.join("atlas-backend"),&backend_args,&format!("{token}\n"),&BACKEND)?;
        app.manage(TransferBackend{port:backend_port,token:token.clone()});
        BACKEND.store(backend.id() as i32,Ordering::Relaxed);
        let (proxy,port)=match launch(&resources.join("atlas-proxy"),&[resources.join("ui").to_string_lossy().into_owned(),backend_port.to_string(),"0".into()],&format!("{bootstrap}\n{token}\n"),&PROXY){Ok(v)=>v,Err(e)=>{let mut b=backend;unsafe {libc::kill(-(b.id() as i32),libc::SIGTERM);} let _=b.wait(); return Err(e.into());}};
        PROXY.store(proxy.id() as i32,Ordering::Relaxed); app.manage(Children(Mutex::new(vec![backend,proxy])));
        let origin=format!("http://127.0.0.1:{port}");let url=format!("{origin}/bootstrap/{bootstrap}").parse()?;
        WebviewWindowBuilder::new(app,"main",WebviewUrl::External(url)).disable_drag_drop_handler().title("Atlas").inner_size(1200.0,800.0).on_navigation(move |url| url.scheme()=="http" && url.host_str()==Some("127.0.0.1") && url.port_or_known_default()==Some(port)).build()?;
        #[cfg(target_os="macos")] menu_search::install(app,origin)?;
        Ok(())
    }).invoke_handler(tauri::generate_handler![menu_search::voice_microphone_prompt,voice::voice_get_quality, voice::voice_set_quality, voice::voice_model_status, voice::voice_repair_model, voice::voice_download_model, voice::voice_transcribe, voice::voice_cancel, models_intro_seen, dismiss_models_intro, select_workspace_directory, save_workspace_file, open_external_url, download_model, model_download_status, download_q8_model, q8_model_download_status, download_vision_model, vision_model_download_status, download_rag_model, rag_model_download_status, download_rag_e4b_model, rag_e4b_model_download_status, download_nemotron_model, nemotron_model_download_status, prepare_restart, restart_atlas, menu_search::set_quick_search_mode, menu_search::show_main_window, menu_search::hide_quick_search, menu_search::open_quick_result, menu_search::take_quick_result, menu_search::request_atlas_exit, menu_search::take_atlas_exit_request, menu_search::finish_atlas_exit, menu_search::open_search_settings, menu_search::take_search_settings_request]).build(tauri::generate_context!()).expect("failed to start Atlas").run(|app,event|{#[cfg(target_os="macos")] match &event {tauri::RunEvent::ExitRequested{code:None,api,..}=>{api.prevent_exit();let _=menu_search::request_atlas_exit(app.clone());},tauri::RunEvent::Reopen{has_visible_windows:false,..}=>{let _=menu_search::show_main_window(app.clone());},_=>{}} if matches!(event,tauri::RunEvent::Exit){voice::shutdown();use tauri::Manager;if let Some(children)=app.try_state::<Children>(){if let Err(e)=children.stop(){eprintln!("Atlas: {e}");}}}});
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
