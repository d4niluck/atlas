use std::{collections::HashMap, path::{Path,PathBuf}, sync::{Arc,Mutex,mpsc,atomic::{AtomicU64,Ordering}}, thread};
use serde_json::{json, Value};
use atlas_llama_probe::Encoder;
use atlas_joined_probe::{Source,prepare_sources_cancellable,publish_sources};
use rusqlite::Connection;

fn cleanup_generations(dir:&Path,current:&str)->std::io::Result<()> {
 for entry in std::fs::read_dir(dir)? {let entry=entry?;let name=entry.file_name();let Some(name)=name.to_str() else {continue};
  if name!=current && name.starts_with("gen-") && name.chars().all(|c|c.is_ascii_alphanumeric()||c=='-') && entry.file_type()?.is_dir(){std::fs::remove_dir_all(entry.path())?;}
 }
 Ok(())
}

struct ModelSlot { enabled:bool, encoder:Option<Encoder>, exe:String, path:String, device:String, last_used:std::time::Instant }
struct ModelGuard<'a>(std::sync::MutexGuard<'a,ModelSlot>);
impl std::ops::Deref for ModelGuard<'_> {type Target=Encoder;fn deref(&self)->&Encoder {self.0.encoder.as_ref().expect("loaded encoder")}}
impl Drop for ModelGuard<'_> {fn drop(&mut self){self.0.last_used=std::time::Instant::now();}}
fn acquire(slot:&Mutex<ModelSlot>)->Result<ModelGuard<'_>,String> {let mut guard=loop {match slot.try_lock(){Ok(g)=>break g,Err(std::sync::TryLockError::WouldBlock)=>{atlas_llama_probe::remaining_search_time()?;std::thread::sleep(std::time::Duration::from_millis(25))},Err(_)=>return Err("model unavailable".into())}};if !guard.enabled{return Err("Семантический поиск выключен".into())}if guard.encoder.is_none(){guard.encoder=Some(Encoder::start_device(&guard.exe,&guard.path,&guard.device).map_err(|e|if guard.device=="metal"{format!("Не удалось запустить модель на GPU Apple. Выберите CPU в настройках. {e}")}else{e})?);}Ok(ModelGuard(guard))}

#[derive(Default)]
struct State { revision:u64, indexed:u64, error:Option<String>, processing:Vec<String> }
#[derive(Clone)]
pub struct SemanticService { identity:Arc<std::sync::RwLock<String>>, sender:mpsc::Sender<PathBuf>, states:Arc<Mutex<HashMap<PathBuf,State>>>, model:Arc<Mutex<ModelSlot>>, enabled:Arc<std::sync::atomic::AtomicBool>, counter:Arc<AtomicU64> }

impl SemanticService {
 pub fn reconfigure(&self,device:&str,enabled:bool)->Result<(),String>{let mut slot=self.model.lock().map_err(|_|"model unavailable")?;let was_enabled=slot.enabled;let changed=slot.device!=device;if changed {let identity=Encoder::model_identity(&slot.path,device)?;*self.identity.write().map_err(|_|"model unavailable")?=identity;}if changed||!enabled {slot.encoder.take();}slot.device=device.to_owned();slot.enabled=enabled;self.enabled.store(enabled,Ordering::Release);drop(slot);if changed {let mut states=self.states.lock().map_err(|_|"state unavailable")?;for state in states.values_mut(){state.revision=self.counter.fetch_add(1,Ordering::Relaxed)+1;state.error=None;}}if enabled&&(!was_enabled||changed) {let pending:Vec<_>=self.states.lock().map_err(|_|"state unavailable")?.iter().filter(|(_,s)|s.revision>s.indexed).map(|(p,_)|p.clone()).collect();for dir in pending {self.sender.send(dir).map_err(|e|e.to_string())?;}}Ok(())}
 pub fn active_device(&self)->Result<Option<String>,()>{self.model.try_lock().map(|slot|slot.encoder.as_ref().map(|_|slot.device.clone())).map_err(|_|())}
 pub fn unload_if_idle(&self,age:std::time::Duration)->bool {if let Ok(mut slot)=self.model.try_lock(){if slot.last_used.elapsed()>=age {slot.encoder.take();return true}}false}
 pub fn worker_model_identity(&self)->Result<String,String>{Ok(self.identity.read().map_err(|_|"model unavailable")?.clone())}
 pub fn worker_encode(&self,text:&str,query:bool)->Result<Vec<f32>,String>{acquire(&self.model)?.encode(text,query)}

 pub fn start(exe:&str,model_path:&str)->Result<Self,String>{Self::start_inner(exe,model_path,None,"cpu")}
 #[cfg(feature="test-hook")]
 pub fn start_with_hook(exe:&str,model_path:&str,ready:mpsc::Sender<()>,resume:mpsc::Receiver<()>)->Result<Self,String>{Self::start_inner(exe,model_path,Some((ready,resume)),"cpu")}
 pub fn start_device(exe:&str,model_path:&str,device:&str)->Result<Self,String>{Self::start_inner(exe,model_path,None,device)}
 pub fn start_lazy(exe:&str,model_path:&str,device:&str)->Result<Self,String>{Self::start_slot(exe,model_path,None,device,true)}
 fn start_inner(exe:&str,model_path:&str,mut hook:Option<(mpsc::Sender<()>,mpsc::Receiver<()>)>,device:&str)->Result<Self,String>{
  Self::start_slot(exe,model_path,hook.take(),device,false)
 }
 fn start_slot(exe:&str,model_path:&str,mut hook:Option<(mpsc::Sender<()>,mpsc::Receiver<()>)>,device:&str,lazy:bool)->Result<Self,String>{
  let identity=Encoder::model_identity(model_path,device)?;let encoder=if lazy {None}else{Some(Encoder::start_device(exe,model_path,device)?)};let model=Arc::new(Mutex::new(ModelSlot{enabled:true,encoder,exe:exe.to_owned(),path:model_path.to_owned(),device:device.to_owned(),last_used:std::time::Instant::now()}));
  let states:Arc<Mutex<HashMap<PathBuf,State>>>=Arc::new(Mutex::new(HashMap::new()));
  let (sender,receiver)=mpsc::channel::<PathBuf>();
  let worker_states=Arc::clone(&states);let worker_model=Arc::clone(&model);
  thread::spawn(move || {while let Ok(dir)=receiver.recv(){
   let rev={let states=worker_states.lock().unwrap();match states.get(&dir){Some(s) if s.revision>s.indexed=>s.revision,_=>continue}};
   let result=(||->Result<String,String>{
    let guard=atlas_native_storage_contract::workspace_lock(&dir).map_err(|e|e.to_string())?;
    let db=Connection::open(dir.join("atlas.sqlite3")).map_err(|e|e.to_string())?;
    let mut stmt=db.prepare("SELECT objects.id, objects.title, COALESCE(search_index.body,''), objects.description FROM objects JOIN search_index ON objects.id=search_index.object_id WHERE objects.id NOT IN (SELECT id FROM trash) ORDER BY objects.id").map_err(|e|e.to_string())?;
    let rows=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    drop(stmt);drop(db);drop(guard);
    let sources:Vec<_>=rows.iter().map(|(id,title,body,description)|Source{id,title,body,description}).collect();
    let model=acquire(&worker_model)?;prepare_sources_cancellable(&model,&sources,&dir.join(".atlas-semantic-native"),||worker_states.lock().unwrap().get(&dir).is_some_and(|s|s.revision==rev))
   })();
   if let Some((ready,resume))=hook.take(){let _=ready.send(());let _=resume.recv();}
   // Keep generation files alive while another query reads them.
   let _model_guard=worker_model.lock().unwrap();
   let workspace_guard=atlas_native_storage_contract::workspace_lock(&dir);
   let mut states=worker_states.lock().unwrap();if let Some(s)=states.get_mut(&dir){match result {Ok(generation) if s.revision==rev && workspace_guard.is_ok()=>{match publish_sources(&dir.join(".atlas-semantic-native"),&generation){Ok(())=>{if let Err(e)=cleanup_generations(&dir.join(".atlas-semantic-native"),&generation){eprintln!("Atlas: cannot remove old search index: {e}");}s.indexed=rev;s.error=None;s.processing.clear()},Err(e)=>s.error=Some(e)}},Ok(generation)=>{if s.revision==rev && workspace_guard.is_err(){s.error=Some("Workspace unavailable".to_owned());}let _=std::fs::remove_dir_all(dir.join(".atlas-semantic-native").join(generation));},Err(e) if s.revision==rev=>s.error=Some(e),Err(_)=>{}}} else if let Ok(generation)=result {let _=std::fs::remove_dir_all(dir.join(".atlas-semantic-native").join(generation));}
  }});
  Ok(Self {identity:Arc::new(std::sync::RwLock::new(identity)),sender,states,model,enabled:Arc::new(std::sync::atomic::AtomicBool::new(true)),counter:Arc::new(AtomicU64::new(0))})
 }

 pub fn protected_identity(&self)->String{self.identity.read().unwrap().clone()}
 pub fn protected_build(&self,mut next:impl FnMut()->Result<Option<(String,String,String,String)>,String>,previous:Option<(&atlas_faiss_probe::Index,&Value)>,should_continue:impl Fn()->bool)->Result<(Vec<u8>,Value),String>{let model=acquire(&self.model)?;let mut builder=atlas_joined_probe::MemoryBuilder::new(&model,previous)?;while let Some((id,title,body,description))=next()?{if !should_continue(){return Err("indexing cancelled".into())}builder.push(&model,&Source{id:&id,title:&title,body:&body,description:&description},&should_continue)?;}if !should_continue(){return Err("indexing cancelled".into())}builder.finish(&model)}
 pub fn protected_search(&self,index:&atlas_faiss_probe::Index,meta:&Value,q:&str,ids:Option<&[String]>)->Result<Vec<Value>,String>{let model=acquire(&self.model)?;let rows=atlas_joined_probe::search_memory(&model,index,meta,q,ids,128)?;Ok(rows.into_iter().map(|v|{let score=v["score"].clone();let chunks=if v["field"]=="title"{vec![]}else{vec![json!({"text":v["text"],"start":v["start"],"end":v["end"],"field":v["field"],"score":score})]};json!({"id":v["id"],"title":v["title"],"chunks":chunks,"score":score})}).collect())}
 pub fn ensure(&self,dir:&Path)->Result<(),String>{
  let pending={let mut states=self.states.lock().unwrap();if states.contains_key(dir){false}else{states.insert(dir.to_owned(),State{revision:self.counter.fetch_add(1,Ordering::Relaxed)+1,..State::default()});true}};
  if pending && self.enabled.load(Ordering::Acquire) {self.sender.send(dir.to_owned()).map_err(|e|e.to_string())?}Ok(())
 }
 #[cfg(feature="test-hook")]
 pub fn with_model_lock_for_test(&self,ready:mpsc::Sender<()>,resume:mpsc::Receiver<()>){let _guard=self.model.lock().unwrap();let _=ready.send(());let _=resume.recv();}
 pub fn close(&self,dir:&Path)->Result<(),String>{self.states.lock().unwrap().remove(dir);Ok(())}
 pub fn enqueue(&self,dir:&Path,id:Option<&str>)->Result<(),String>{
  {let mut states=self.states.lock().unwrap();let s=states.entry(dir.to_owned()).or_default();s.revision=self.counter.fetch_add(1,Ordering::Relaxed)+1;s.error=None;if let Some(id)=id {if !s.processing.iter().any(|x|x==id){s.processing.push(id.to_owned())}}}
  if self.enabled.load(Ordering::Acquire) {self.sender.send(dir.to_owned()).map_err(|e|e.to_string())?;}Ok(())
 }
 pub fn revision(&self,dir:&Path)->Option<u64>{let states=self.states.lock().ok()?;let s=states.get(dir)?;if s.indexed>0 && s.indexed==s.revision && s.error.is_none(){Some(s.revision)}else{None}}
 pub fn status(&self,dir:&Path)->Value{let states=self.states.lock().unwrap();match states.get(dir){None=>json!({"available":true,"message":"","processing_ids":[],"failed_ids":[]}),Some(s)=>json!({"available":s.indexed==s.revision&&s.indexed>0&&s.error.is_none(),"message":s.error.clone().unwrap_or_else(||if s.indexed!=s.revision {"Подготовка семантического поиска".to_string()} else {String::new()}),"processing_ids":s.processing,"failed_ids":if s.error.is_some(){s.processing.clone()}else{vec![]}})}}
 pub fn search(&self,dir:&Path,query:&str,ids:Option<&[String]>)->Result<Vec<Value>,String>{
  let rev={let states=self.states.lock().unwrap();let s=states.get(dir).ok_or("index unavailable")?;if s.indexed==0||s.indexed!=s.revision||s.error.is_some(){return Err("index unavailable".into())}s.revision};
  let model=acquire(&self.model)?;let filter=ids.map(|v|v.iter().map(String::as_str).collect::<Vec<_>>());let rows=atlas_joined_probe::search(&model,&dir.join(".atlas-semantic-native"),query,filter.as_deref(),128)?;
  let states=self.states.lock().unwrap();let state=states.get(dir).ok_or("index unavailable")?;if state.revision!=rev||state.indexed!=rev{return Err("index changed".into())}
  Ok(rows.into_iter().map(|v|{let score=v["score"].clone();let chunks=if v["field"]=="title"{vec![]}else{vec![json!({"text":v["text"],"start":v["start"],"end":v["end"],"field":v["field"],"score":score})]};json!({"id":v["id"],"title":v["title"],"chunks":chunks,"score":score})}).collect())
 }
}

#[cfg(test)]
mod lifecycle_tests {
 use super::*;
 #[test]
 fn locked_model_is_not_unloaded() {
  let service=SemanticService{identity:Arc::new(std::sync::RwLock::new(String::new())),sender:mpsc::channel().0,states:Arc::new(Mutex::new(HashMap::new())),model:Arc::new(Mutex::new(ModelSlot{enabled:true,encoder:None,exe:String::new(),path:String::new(),device:String::new(),last_used:std::time::Instant::now()})),enabled:Arc::new(std::sync::atomic::AtomicBool::new(true)),counter:Arc::new(AtomicU64::new(0))};
  let held=service.model.lock().unwrap();
  assert!(!service.unload_if_idle(std::time::Duration::ZERO));
  drop(held);
  assert!(service.unload_if_idle(std::time::Duration::ZERO));
 }
 #[test]
 #[ignore = "requires local embedding model and llama-server"]
 fn real_model_unloads_and_reloads() {
  let exe=std::env::var("ATLAS_TEST_LLAMA_SERVER").unwrap();let path=std::env::var("ATLAS_TEST_EMBEDDING_MODEL").unwrap();
  let service=SemanticService::start_device(&exe,&path,"cpu").unwrap();
  assert_eq!(service.worker_encode("test",true).unwrap().len(),512);
  assert!(service.unload_if_idle(std::time::Duration::ZERO));
  assert!(service.model.lock().unwrap().encoder.is_none());
  assert_eq!(service.worker_encode("test",true).unwrap().len(),512);
  assert!(service.model.lock().unwrap().encoder.is_some());
 }
}
