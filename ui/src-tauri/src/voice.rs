use std::{fs::{self,OpenOptions},io::Write,os::unix::fs::OpenOptionsExt,path::{Path,PathBuf},process::{Child,Command,Stdio},sync::Mutex,time::Duration};
use tauri::Manager;
static DOWNLOAD: Mutex<Option<String>> = Mutex::new(None);
static ACTIVE: Mutex<Option<(String,Child)>> = Mutex::new(None);
static CANCELLED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn model(q:&str)->Result<(&'static str,&'static str,u64),String>{match q{
 "base"=>Ok(("ggml-base-q5_1.bin","422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",59707625)),
 "small"=>Ok(("ggml-small-q5_1.bin","ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",190085487)),
 "medium"=>Ok(("ggml-medium-q5_0.bin","19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",539212467)),
 _=>Err("Неизвестная модель распознавания".into())}}
fn dir(app:&tauri::AppHandle)->Result<PathBuf,String>{Ok(super::model_data_dir(app)?.join("models"))}
#[tauri::command]
pub fn voice_model_status(app:tauri::AppHandle,quality:String)->Result<String,String>{let(name,_,size)=model(&quality)?;if DOWNLOAD.lock().map_err(|_|"Ошибка загрузки")?.as_deref()==Some(&quality){return Ok("downloading".into())}Ok(if fs::metadata(dir(&app)?.join(name)).is_ok_and(|m|m.is_file()&&m.len()==size){"installed"}else{"missing"}.into())}
#[tauri::command]
pub async fn voice_download_model(app:tauri::AppHandle,quality:String)->Result<(),String>{
 let(name,sha,size)=model(&quality)?;let path=dir(&app)?;
 {let mut active=DOWNLOAD.lock().map_err(|_|"Ошибка загрузки")?;if active.is_some(){return Err("Другая модель уже загружается".into())}*active=Some(quality)}
 let result=tauri::async_runtime::spawn_blocking(move||download(&path,name,sha,size)).await.map_err(|e|e.to_string());
 *DOWNLOAD.lock().map_err(|_|"Ошибка загрузки")?=None;result?
}
fn download(path:&Path,name:&str,sha:&str,size:u64)->Result<(),String>{
 fs::create_dir_all(path).map_err(|e|e.to_string())?;let dest=path.join(name);if dest.exists(){
  if !fs::metadata(&dest).is_ok_and(|m|m.is_file()&&m.len()==size){return Err("Файл модели повреждён".into())}
  let digest=Command::new("/usr/bin/shasum").args(["-a","256"]).arg(&dest).output().map_err(|e|e.to_string())?;
  return if digest.status.success()&&String::from_utf8_lossy(&digest.stdout).split_whitespace().next()==Some(sha){Ok(())}else{Err("Файл модели повреждён".into())}
 }
 let tmp=path.join(format!(".atlas-voice-{}-{}",std::process::id(),name));
 let file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp).map_err(|e|e.to_string())?;
 let run=||->Result<(),String>{
  let url=format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861/{name}");
  let status=Command::new("/usr/bin/curl").args(["--fail","--location","--silent","--show-error","--max-time","3600",&url]).stdout(Stdio::from(file)).status().map_err(|e|e.to_string())?;
  if !status.success()||fs::metadata(&tmp).map_err(|e|e.to_string())?.len()!=size{return Err("Не удалось загрузить модель".into())}
  let digest=Command::new("/usr/bin/shasum").args(["-a","256"]).arg(&tmp).output().map_err(|e|e.to_string())?;
  if !digest.status.success()||String::from_utf8_lossy(&digest.stdout).split_whitespace().next()!=Some(sha){return Err("Модель не прошла проверку".into())}
  fs::hard_link(&tmp,&dest).map_err(|e|e.to_string())?;Ok(())};
 let result=run();let _=fs::remove_file(tmp);result
}
#[cfg(test)]
#[path="voice_tests.rs"]
mod tests;
#[tauri::command]
pub fn voice_cancel(id:String)->Result<(),String>{
 let mut active=ACTIVE.lock().map_err(|_|"Ошибка остановки")?;
 if let Some((key,child))=active.as_mut(){if *key==id{child.kill().map_err(|e|e.to_string())?}}
 let mut cancelled=CANCELLED.lock().map_err(|_|"Ошибка остановки")?;
 if cancelled.len()>=128{cancelled.remove(0);}
 cancelled.push(id);
 Ok(())
}
#[tauri::command]
pub async fn voice_transcribe(app:tauri::AppHandle,quality:String,id:String,wav:Vec<u8>)->Result<String,String>{
 let(name,sha,size)=model(&quality)?;
 if id.len()>64||id.is_empty()||wav.len()>1_920_044||wav.len()<46||&wav[..4]!=b"RIFF"{return Err("Некорректная запись".into())}
 let model_path=dir(&app)?.join(name);if !fs::metadata(&model_path).is_ok_and(|m|m.is_file()&&m.len()==size){return Err("Сначала загрузите модель распознавания".into())}
 let executable=app.path().resource_dir().map_err(|e|e.to_string())?.join("resources/whisper/whisper-cli");
 tauri::async_runtime::spawn_blocking(move||{let digest=Command::new("/usr/bin/shasum").args(["-a","256"]).arg(&model_path).output().map_err(|e|e.to_string())?;if !digest.status.success()||String::from_utf8_lossy(&digest.stdout).split_whitespace().next()!=Some(sha){return Err("Файл модели повреждён".into())}transcribe(&executable,&model_path,&id,&wav)}).await.map_err(|e|e.to_string())?
}
fn transcribe(executable:&Path,model:&Path,id:&str,wav:&[u8])->Result<String,String>{
 if !id.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'){return Err("Некорректный идентификатор".into())}
 let path=std::env::temp_dir().join(format!("atlas-voice-{}-{}.wav",std::process::id(),id));
 let mut file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).map_err(|e|e.to_string())?;
 let result=(||{
  file.write_all(wav).map_err(|e|e.to_string())?;drop(file);
  let mut active=ACTIVE.lock().map_err(|_|"Ошибка распознавания")?;
  {let mut cancelled=CANCELLED.lock().map_err(|_|"Ошибка остановки")?;if let Some(i)=cancelled.iter().position(|x|x==id){cancelled.remove(i);return Err("Распознавание остановлено".into())}}
  if active.is_some(){return Err("Распознавание уже выполняется".into())}
  let child=Command::new(executable).arg("-m").arg(model).arg("-f").arg(&path).args(["-l","auto","-nt","-t","4"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e|e.to_string())?;
  *active=Some((id.to_string(),child));drop(active);
  loop{std::thread::sleep(Duration::from_millis(50));let mut active=ACTIVE.lock().map_err(|_|"Ошибка распознавания")?;
   let done=active.as_mut().ok_or("Распознавание остановлено")?.1.try_wait().map_err(|e|e.to_string())?.is_some();
   if done{let(_,child)=active.take().ok_or("Распознавание остановлено")?;drop(active);let output=child.wait_with_output().map_err(|e|e.to_string())?;if !output.status.success(){return Err("Распознавание остановлено или не удалось".into())}return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())}
  }
 })();let _=fs::remove_file(path);result
}
