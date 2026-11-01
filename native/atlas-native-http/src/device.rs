use std::{path::PathBuf,sync::{Mutex,atomic::{AtomicBool,Ordering}}};
use serde_json::{json,Value};
#[derive(Clone)] struct Pair { embedding:String, llm:String }
struct State {selected:Pair, active_embedding:Option<String>, active_llm:Option<String>, embedding_message:String, llm_message:String}
pub struct DeviceSettings {dir:PathBuf,exe:String,state:Mutex<State>,ready:AtomicBool,restart_only:AtomicBool}
impl DeviceSettings {
 pub fn new(dir:PathBuf,exe:String)->Self {let valid=|s:&str|s=="cpu"||s=="metal";let saved=std::fs::read_to_string(dir.join("settings.json")).or_else(|_|std::fs::read_to_string(dir.join("device-choices.json"))).ok().and_then(|v|serde_json::from_str::<Value>(&v).ok()).and_then(|v|{let a=v["embedding"].as_str()?;let b=v["llm"].as_str()?;if valid(a)&&valid(b){Some(Pair{embedding:a.into(),llm:b.into()})}else{None}});let legacy=std::fs::read_to_string(dir.join("device-choice")).ok().filter(|s|valid(s));let selected=saved.unwrap_or_else(||legacy.map(|d|Pair{embedding:d.clone(),llm:d}).unwrap_or(Pair{embedding:"cpu".into(),llm:"metal".into()}));Self{dir,exe,state:Mutex::new(State{selected,active_embedding:None,active_llm:None,embedding_message:String::new(),llm_message:String::new()}),ready:AtomicBool::new(false),restart_only:AtomicBool::new(false)}}
 pub fn restart_only(&self)->bool{self.restart_only.load(Ordering::Acquire)}
 pub fn is_ready(&self)->bool{self.ready.load(Ordering::Acquire)}
 pub fn mark_ready(&self){self.ready.store(true,Ordering::Release)}
 pub fn selected_pair(&self)->(String,String){let s=self.state.lock().unwrap();(s.selected.embedding.clone(),s.selected.llm.clone())}
 pub fn active_llm(&self)->Option<String>{self.state.lock().unwrap().active_llm.clone()}
 pub fn llm_requested(&self)->String{let s=self.state.lock().unwrap();s.active_llm.clone().unwrap_or_else(||s.selected.llm.clone())}
 pub fn set_embedding(&self,active:Option<&str>,message:&str){let mut s=self.state.lock().unwrap();s.active_embedding=active.map(str::to_owned);s.embedding_message=message.into();}
 pub fn set_llm(&self,active:Option<&str>,message:&str){let mut s=self.state.lock().unwrap();s.active_llm=active.map(str::to_owned);s.llm_message=message.into();}
 pub fn status(&self)->Value{let s=self.state.lock().unwrap();let entry=|selected:&str,active:&Option<String>,msg:&str|json!({"device":selected,"active_device":if self.is_ready(){active.clone()}else{None},"restart_required":self.is_ready() && active.as_deref().is_some_and(|a|a!=selected),"message":if self.is_ready(){msg}else{"Подготовка моделей"}});json!({"embedding":entry(&s.selected.embedding,&s.active_embedding,&s.embedding_message),"llm":entry(&s.selected.llm,&s.active_llm,&s.llm_message),"startup_complete":self.is_ready(),"restart_only":self.restart_only()})}
 pub fn validate(&self,embedding:&str,llm:&str)->Result<(),String>{if !["cpu","metal"].contains(&embedding)||!["cpu","metal"].contains(&llm){return Err("Неизвестное устройство".into())}if embedding=="metal"||llm=="metal"{let out=std::process::Command::new(&self.exe).arg("--list-devices").output().map_err(|_|"Не удалось проверить GPU Apple".to_string())?;if !out.status.success()||!String::from_utf8_lossy(&out.stdout).lines().any(|l|l.trim_start().starts_with("MTL")){return Err("GPU Apple недоступен на этом компьютере".into())}}
 Ok(())}
 pub fn apply_selected(&self,embedding:&str,llm:&str){let mut s=self.state.lock().unwrap();s.selected=Pair{embedding:embedding.into(),llm:llm.into()};s.active_embedding=None;s.active_llm=None;}
 pub fn select(&self,embedding:&str,llm:&str)->Result<(),String>{self.validate(embedding,llm)?;
 std::fs::create_dir_all(&self.dir).map_err(|e|e.to_string())?;let tmp=self.dir.join(format!(".device-choices-{}",uuid::Uuid::new_v4()));let bytes=serde_json::to_vec(&json!({"embedding":embedding,"llm":llm})).map_err(|e|e.to_string())?;let result=(||{std::fs::write(&tmp,bytes)?;std::fs::rename(&tmp,self.dir.join("device-choices.json"))})();if result.is_err(){let _=std::fs::remove_file(tmp);}result.map_err(|e|e.to_string())?;self.state.lock().unwrap().selected=Pair{embedding:embedding.into(),llm:llm.into()};Ok(())}
}
