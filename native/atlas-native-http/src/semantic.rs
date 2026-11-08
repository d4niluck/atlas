use std::{collections::HashMap, path::{Path,PathBuf}, sync::{Arc,Mutex,mpsc,atomic::{AtomicU64,Ordering}}, thread};
use serde_json::{json, Value};
use atlas_llama_probe::Encoder;
use atlas_joined_probe::{Source,publish_sources};
use rusqlite::Connection;
use sha2::{Digest,Sha256};
fn source_fingerprint(db:&Connection)->Result<String,String>{
 let mut hash=Sha256::new();
 // Hash all source fields under the workspace lock to detect a commit before enqueue.
 for query in ["SELECT objects.id, objects.title, COALESCE(search_index.body,''), objects.description FROM objects JOIN search_index ON objects.id=search_index.object_id WHERE NOT (objects.kind='note' AND objects.title='Новая заметка' AND trim(COALESCE(search_index.body,''), char(9)||char(10)||char(13)||' ')='') ORDER BY objects.id", "SELECT id,title,path,'' FROM objects WHERE kind='file' ORDER BY id", "SELECT id,CAST(deleted_at AS TEXT),'','' FROM trash ORDER BY id"] {
  let mut statement=db.prepare(query).map_err(|e|e.to_string())?;let mut rows=statement.query([]).map_err(|e|e.to_string())?;while let Some(row)=rows.next().map_err(|e|e.to_string())?{for i in 0..4{let value:String=row.get(i).map_err(|e|e.to_string())?;hash.update((value.len() as u64).to_le_bytes());hash.update(value.as_bytes());}}hash.update([0xff]);
 }Ok(format!("{:x}",hash.finalize()))
}

// A workspace may be opened by another Atlas process. Serialize preparation
// through cleanup without holding the workspace lock during model inference.
fn acquire_prepare_lock(dir:&Path,still_current:impl Fn()->bool)->Result<std::fs::File,String>{
 use std::os::{fd::AsRawFd,unix::fs::OpenOptionsExt};
 let guard=atlas_native_storage_contract::workspace_lock(dir).map_err(|e|e.to_string())?;let index=dir.join(".atlas-semantic-native");
 if !index.exists(){std::fs::create_dir(&index).map_err(|e|e.to_string())?;}let metadata=std::fs::symlink_metadata(&index).map_err(|e|e.to_string())?;if !metadata.is_dir()||metadata.file_type().is_symlink(){return Err("invalid index directory".into())}
 let lock=std::fs::OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(index.join(".prepare.lock")).map_err(|e|e.to_string())?;if !lock.metadata().map_err(|e|e.to_string())?.is_file(){return Err("invalid index lock".into())}drop(guard);
 loop{if !still_current(){return Err("indexing cancelled".into())}if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0{return Ok(lock)}let error=std::io::Error::last_os_error();if !matches!(error.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::Interrupted){return Err(error.to_string())}std::thread::sleep(std::time::Duration::from_millis(25));}
}
fn image_mime(title:&str)->Option<&'static str>{match Path::new(title).extension()?.to_str()?.to_ascii_lowercase().as_str(){"png"=>Some("image/png"),"jpg"|"jpeg"=>Some("image/jpeg"),"webp"=>Some("image/webp"),"gif"=>Some("image/gif"),_=>None}}

fn cleanup_generations(dir:&Path,current:&str)->std::io::Result<()> {
 let base=atlas_joined_probe::overlay_gen::base_generation(dir,current).map_err(std::io::Error::other)?;
 for entry in std::fs::read_dir(dir)? {let entry=entry?;let name=entry.file_name();let Some(name)=name.to_str() else {continue};
  if name!=current && base.as_deref()!=Some(name) && name.starts_with("gen-") && name.chars().all(|c|c.is_ascii_alphanumeric()||c=='-') && entry.file_type()?.is_dir(){std::fs::remove_dir_all(entry.path())?;}
 }
 Ok(())
}

struct ModelSlot { images:bool, enabled:bool, encoder:Option<Encoder>, exe:String, path:String, device:String, last_used:std::time::Instant }
struct ModelGuard<'a>(std::sync::MutexGuard<'a,ModelSlot>);
impl std::ops::Deref for ModelGuard<'_> {type Target=Encoder;fn deref(&self)->&Encoder {self.0.encoder.as_ref().expect("loaded encoder")}}
impl Drop for ModelGuard<'_> {fn drop(&mut self){self.0.last_used=std::time::Instant::now();}}
fn acquire(slot:&Mutex<ModelSlot>)->Result<ModelGuard<'_>,String> {acquire_cancellable(slot,||true)}
fn acquire_cancellable(slot:&Mutex<ModelSlot>,should_continue:impl Fn()->bool)->Result<ModelGuard<'_>,String> {let mut guard=loop {if !should_continue(){return Err("indexing cancelled".into())}match slot.try_lock(){Ok(g)=>break g,Err(std::sync::TryLockError::WouldBlock)=>{atlas_llama_probe::remaining_search_time()?;std::thread::sleep(std::time::Duration::from_millis(25))},Err(_)=>return Err("model unavailable".into())}};if !guard.enabled{return Err("Семантический поиск выключен".into())}if guard.encoder.is_none(){guard.encoder=Some((if guard.images {Encoder::start_device_with_images(&guard.exe,&guard.path,&guard.device,&std::path::Path::new(&guard.path).with_file_name("embeddinggemma-2-mmproj-q8.gguf").to_string_lossy())}else{Encoder::start_device(&guard.exe,&guard.path,&guard.device)}).map_err(|e|if guard.device=="metal"{format!("Не удалось запустить модель на GPU Apple. Выберите CPU в настройках. {e}")}else{e})?);}Ok(ModelGuard(guard))}

#[derive(Default)]
struct State { force:bool, revision:u64, indexed:u64, error:Option<String>, processing:Vec<String>, cancelled:bool, progress:Option<Value>, running:bool }
#[derive(Clone)]
pub struct SemanticService { identity:Arc<std::sync::RwLock<(String,String)>>, sender:mpsc::Sender<PathBuf>, states:Arc<Mutex<HashMap<PathBuf,State>>>, model:Arc<Mutex<ModelSlot>>, enabled:Arc<std::sync::atomic::AtomicBool>, counter:Arc<AtomicU64>, model_epoch:Arc<AtomicU64> }

impl SemanticService {
 pub fn reconfigure(&self,device:&str,enabled:bool)->Result<(),String>{self.reconfigure_model(None,device,enabled)}
 pub fn reconfigure_model(&self,path:Option<&str>,device:&str,enabled:bool)->Result<(),String>{let proposed=path.map(|p|Encoder::model_identities(p,device)).transpose()?;let compatible=proposed.as_ref().is_some_and(|p|self.worker_model_identity().map(|i|atlas_llama_probe::index_keys_compatible(i.split(':').next().unwrap_or(&i),&p.0)).unwrap_or(false));let will_change=proposed.as_ref().is_some_and(|p|self.worker_model_identity().map(|i|i.split(':').next()!=Some(p.0.as_str())).unwrap_or(true));if will_change {self.model_epoch.fetch_add(1,Ordering::AcqRel);let mut states=self.states.lock().map_err(|_|"state unavailable")?;for state in states.values_mut(){if !compatible||state.running{state.revision=self.counter.fetch_add(1,Ordering::Relaxed)+1;state.error=None;}}}let mut slot=self.model.lock().map_err(|_|"model unavailable")?;let path=path.unwrap_or(&slot.path).to_owned();let was_enabled=slot.enabled;let changed=slot.device!=device||slot.path!=path;if changed {let (mut identity,mut alternate)=match proposed{Some(pair)=>pair,None=>Encoder::model_identities(&path,device)?};if slot.images {identity.push_str(":vision-q8-v1");alternate.push_str(":vision-q8-v1");}*self.identity.write().map_err(|_|"model unavailable")?=(identity,alternate);}if changed||!enabled {slot.encoder.take();}slot.path=path;slot.device=device.to_owned();slot.enabled=enabled;self.enabled.store(enabled,Ordering::Release);drop(slot);if changed {let mut states=self.states.lock().map_err(|_|"state unavailable")?;for state in states.values_mut(){if !compatible||state.running{state.revision=self.counter.fetch_add(1,Ordering::Relaxed)+1;state.error=None;}}}if enabled&&(!was_enabled||changed) {let pending:Vec<_>=self.states.lock().map_err(|_|"state unavailable")?.iter().filter(|(_,s)|s.revision>s.indexed).map(|(p,_)|p.clone()).collect();for dir in pending {self.sender.send(dir).map_err(|e|e.to_string())?;}}Ok(())}
 pub fn set_images(&self,images:bool)->Result<(),String>{let mut slot=self.model.lock().map_err(|_|"model unavailable")?;if slot.images==images{return Ok(())}let (mut identity,mut alternate)=Encoder::model_identities(&slot.path,&slot.device)?;if images {identity.push_str(":vision-q8-v1");alternate.push_str(":vision-q8-v1");}*self.identity.write().map_err(|_|"model unavailable")?=(identity,alternate);slot.encoder.take();slot.images=images;let enabled=slot.enabled;drop(slot);let mut states=self.states.lock().map_err(|_|"state unavailable")?;for (path,state) in states.iter_mut(){state.revision=self.counter.fetch_add(1,Ordering::Relaxed)+1;state.error=None;if enabled {self.sender.send(path.clone()).map_err(|e|e.to_string())?;}}Ok(())}
 pub fn active_device(&self)->Result<Option<String>,()>{self.model.try_lock().map(|slot|slot.encoder.as_ref().map(|_|slot.device.clone())).map_err(|_|())}
 pub fn unload_if_idle(&self,age:std::time::Duration)->bool {if let Ok(mut slot)=self.model.try_lock(){if slot.last_used.elapsed()>=age {slot.encoder.take();return true}}false}
 pub fn worker_model_identity(&self)->Result<String,String>{Ok(self.identity.read().map_err(|_|"model unavailable")?.0.clone())}
 pub fn worker_model_identities(&self)->Result<(String,String),String>{Ok(self.identity.read().map_err(|_|"model unavailable")?.clone())}
 pub fn matches_identity(&self,key:&str)->bool {self.identity.read().is_ok_and(|ids|atlas_llama_probe::index_keys_compatible(&ids.0,key)||atlas_llama_probe::index_keys_compatible(&ids.1,key))}
 pub fn worker_encode(&self,text:&str,query:bool)->Result<Vec<f32>,String>{acquire(&self.model)?.encode(text,query)}
 pub fn with_model_identity<T>(&self,expected:&str,f:impl FnOnce()->Result<T,String>)->Result<T,String>{let _guard=self.model.lock().map_err(|_|"model unavailable")?;if self.worker_model_identity()?!=expected{return Err("Модель поиска изменилась".into())}f()}
 pub fn worker_encode_expected(&self,expected:&str,text:&str,query:bool)->Result<Vec<f32>,String>{let model=acquire(&self.model)?;if self.worker_model_identity()?!=expected{return Err("Модель поиска изменилась".into())}model.encode(text,query)}
 pub fn worker_encode_image_expected(&self,expected:&str,bytes:&[u8],mime:&str)->Result<Vec<f32>,String>{let model=acquire(&self.model)?;if self.worker_model_identity()?!=expected{return Err("Модель поиска изменилась".into())}model.encode_image(bytes,mime)}
 pub fn index_enabled(&self)->bool{self.enabled.load(Ordering::Acquire)}
 pub fn worker_images_enabled(&self)->bool{self.model.lock().map(|m|m.images).unwrap_or(false)}
 pub fn worker_encode_image(&self,bytes:&[u8],mime:&str)->Result<Vec<f32>,String>{acquire(&self.model)?.encode_image(bytes,mime)}

 pub fn start(exe:&str,model_path:&str)->Result<Self,String>{Self::start_inner(exe,model_path,None,"cpu")}
 #[cfg(feature="test-hook")]
 pub fn start_with_hook(exe:&str,model_path:&str,ready:mpsc::Sender<()>,resume:mpsc::Receiver<()>)->Result<Self,String>{Self::start_inner(exe,model_path,Some((ready,resume)),"cpu")}
 pub fn start_device(exe:&str,model_path:&str,device:&str)->Result<Self,String>{Self::start_inner(exe,model_path,None,device)}
 pub fn start_lazy(exe:&str,model_path:&str,device:&str)->Result<Self,String>{Self::start_slot(exe,model_path,None,device,true)}
 fn start_inner(exe:&str,model_path:&str,mut hook:Option<(mpsc::Sender<()>,mpsc::Receiver<()>)>,device:&str)->Result<Self,String>{
  Self::start_slot(exe,model_path,hook.take(),device,false)
 }
 fn start_slot(exe:&str,model_path:&str,mut hook:Option<(mpsc::Sender<()>,mpsc::Receiver<()>)>,device:&str,lazy:bool)->Result<Self,String>{
  let (initial_identity,initial_alternate)=Encoder::model_identities(model_path,device)?;let identity=Arc::new(std::sync::RwLock::new((initial_identity,initial_alternate)));let worker_identity=Arc::clone(&identity);let encoder=if lazy {None}else{Some(Encoder::start_device(exe,model_path,device)?)};let model=Arc::new(Mutex::new(ModelSlot{images:false,enabled:true,encoder,exe:exe.to_owned(),path:model_path.to_owned(),device:device.to_owned(),last_used:std::time::Instant::now()}));
  let states:Arc<Mutex<HashMap<PathBuf,State>>>=Arc::new(Mutex::new(HashMap::new()));
  let (sender,receiver)=mpsc::channel::<PathBuf>();
  let counter=Arc::new(AtomicU64::new(0));let worker_counter=Arc::clone(&counter);let worker_sender=sender.clone();
  let worker_states=Arc::clone(&states);let worker_model=Arc::clone(&model);
  thread::spawn(move || {while let Ok(dir)=receiver.recv(){
   let (rev,force)={let mut states=worker_states.lock().unwrap();match states.get_mut(&dir){Some(s) if s.revision>s.indexed&&!s.cancelled=>{s.running=true;(s.revision,s.force)},_=>continue}};
   let _prepare_guard=match acquire_prepare_lock(&dir,||worker_states.lock().unwrap().get(&dir).is_some_and(|s|s.revision==rev&&!s.cancelled)){Ok(guard)=>guard,Err(e)=>{if let Some(s)=worker_states.lock().unwrap().get_mut(&dir){s.running=false;if s.revision==rev&&!s.cancelled{s.error=Some(e);}}continue}};
   let result=(||->Result<(String,String,String,bool,Option<String>),String>{
    let guard=atlas_native_storage_contract::workspace_lock(&dir).map_err(|e|e.to_string())?;
    let db=Connection::open(dir.join("atlas.sqlite3")).map_err(|e|e.to_string())?;
    let mut stmt=db.prepare("SELECT objects.id, objects.title, COALESCE(search_index.body,''), objects.description FROM objects JOIN search_index ON objects.id=search_index.object_id WHERE NOT (objects.kind='note' AND objects.title='Новая заметка' AND trim(COALESCE(search_index.body,''), char(9)||char(10)||char(13)||' ')='') ORDER BY objects.id").map_err(|e|e.to_string())?;
    let rows=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    drop(stmt);
    let mut image_stmt=db.prepare("SELECT id,title,path FROM objects WHERE kind='file' ORDER BY id").map_err(|e|e.to_string())?;
    let image_rows=image_stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    drop(image_stmt);let mut trash_stmt=db.prepare("SELECT id FROM trash ORDER BY id").map_err(|e|e.to_string())?;let trash=trash_stmt.query_map([],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;drop(trash_stmt);let fingerprint=source_fingerprint(&db)?;drop(db);drop(guard);
    let sources:Vec<_>=rows.iter().map(|(id,title,body,description)|Source{id,title,body,description}).collect();
    let model=acquire_cancellable(&worker_model,||worker_states.lock().unwrap().get(&dir).is_some_and(|s|s.revision==rev))?;
    let root=dir.canonicalize().map_err(|e|e.to_string())?;
    let images=||image_rows.clone().into_iter().filter_map(|(id,title,path)|{let mime=image_mime(&title)?;Some((id,title,path,mime))}).filter(|_|model.supports_images()).map(|(id,title,path,mime)|{let target=root.join(path).canonicalize().map_err(|e|e.to_string())?;if !target.starts_with(&root){return Err("Недопустимый путь файла".into())}let metadata=std::fs::metadata(&target).map_err(|e|e.to_string())?;if !metadata.is_file()||metadata.len()>32*1024*1024{return Err("Изображение превышает предел 32 МиБ".into())}let bytes=std::fs::read(target).map_err(|e|e.to_string())?;Ok((id,title,bytes,mime.to_owned()))});
    let keep=||worker_states.lock().unwrap().get(&dir).is_some_and(|s|s.revision==rev);let progress=|title:&str,completed:usize,total:usize|{if let Some(s)=worker_states.lock().unwrap().get_mut(&dir){if s.revision==rev{s.progress=Some(json!({"phase":"Индексация текста","current_title":title,"completed":completed,"total":total}));}}};let index_dir=dir.join(".atlas-semantic-native");let mut worker_generation=std::fs::read_to_string(dir.join(".atlas-worker-index/current")).ok();let worker_exists=worker_generation.is_some();let reconciled=std::fs::read_to_string(index_dir.join("worker-reconciled")).ok()==worker_generation;let prepared=if force{atlas_joined_probe::overlay_gen::prepare_fresh_with_trash(&model,&sources,images(),&trash,&index_dir,keep,progress)?}else if worker_exists&&!reconciled{match atlas_joined_probe::overlay_gen::prepare_from_worker(&model,&sources,images(),&trash,&index_dir,&dir.join(".atlas-worker-index"),keep,progress){Ok(Some(prepared))=>prepared,Ok(None)=>{worker_generation=None;atlas_joined_probe::overlay_gen::prepare_with_trash(&model,&sources,images(),&trash,&index_dir,keep,progress)?},Err(e) if !keep()=>return Err(e),Err(_)=>{worker_generation=None;atlas_joined_probe::overlay_gen::prepare_with_trash(&model,&sources,images(),&trash,&index_dir,keep,progress)?}}}else{atlas_joined_probe::overlay_gen::prepare_with_trash(&model,&sources,images(),&trash,&index_dir,keep,progress)?};Ok((prepared.generation,fingerprint,model.identity().to_owned(),prepared.owned,worker_generation))
   })();
   if let Some((ready,resume))=hook.take(){let _=ready.send(());let _=resume.recv();}
   // Keep generation files alive while another query reads them.
   let _model_guard=worker_model.lock().unwrap();
   let workspace_guard=atlas_native_storage_contract::workspace_lock(&dir);
   let mut states=worker_states.lock().unwrap();if let Some(s)=states.get_mut(&dir){s.running=false;match result {Ok((generation,fingerprint,identity,owned,worker_generation)) if s.revision==rev && workspace_guard.is_ok()=>{let fresh=Connection::open(dir.join("atlas.sqlite3")).map_err(|e|e.to_string()).and_then(|db|source_fingerprint(&db));if fresh.as_deref()!=Ok(fingerprint.as_str()){let index_dir=dir.join(".atlas-semantic-native");if owned {let _=atlas_joined_probe::overlay_gen::discard_prepared(&index_dir,&generation);}match fresh{Ok(_)=>{s.revision=worker_counter.fetch_add(1,Ordering::Relaxed)+1;s.error=None;let _=worker_sender.send(dir.clone());},Err(e)=>s.error=Some(e)}continue}let index_dir=dir.join(".atlas-semantic-native");let alternate=match worker_identity.read(){Ok(value) if value.0==identity=>value.1.clone(),_=>{if owned {let _=atlas_joined_probe::overlay_gen::discard_prepared(&index_dir,&generation);}s.revision=worker_counter.fetch_add(1,Ordering::Relaxed)+1;s.error=None;let _=worker_sender.send(dir.clone());continue}};if let Err(e)=atlas_joined_probe::overlay_gen::validate_prepared_compatible(&index_dir,&generation,&identity,&alternate){if owned {let _=atlas_joined_probe::overlay_gen::discard_prepared(&index_dir,&generation);}if e=="base generation missing"{s.revision=worker_counter.fetch_add(1,Ordering::Relaxed)+1;s.error=None;let _=worker_sender.send(dir.clone());}else{s.error=Some(e)}continue}match publish_sources(&dir.join(".atlas-semantic-native"),&generation){Ok(())=>{if let Some(expected)=worker_generation {if std::fs::read_to_string(dir.join(".atlas-worker-index/current")).ok().as_deref()==Some(expected.as_str()){let path=index_dir.join("worker-reconciled");let temporary=index_dir.join(format!(".worker-reconciled-{}",std::process::id()));let written=std::fs::write(&temporary,expected).and_then(|_|std::fs::rename(&temporary,&path));if let Err(e)=written{eprintln!("Atlas: cannot record reconciled index: {e}");}}}if let Err(e)=cleanup_generations(&dir.join(".atlas-semantic-native"),&generation){eprintln!("Atlas: cannot remove old search index: {e}");}s.indexed=rev;s.force=false;s.error=None;s.processing.clear()},Err(e)=>s.error=Some(e)}},Ok((generation,_,_,owned,_))=>{if s.revision==rev && workspace_guard.is_err(){s.error=Some("Workspace unavailable".to_owned());}let index_dir=dir.join(".atlas-semantic-native");if owned {let _=atlas_joined_probe::overlay_gen::discard_prepared(&index_dir,&generation);}},Err(e) if s.revision==rev=>s.error=Some(e),Err(_)=>{}}} else if let Ok((generation,_,_,owned,_))=result {let index_dir=dir.join(".atlas-semantic-native");if owned {let _=atlas_joined_probe::overlay_gen::discard_prepared(&index_dir,&generation);}}
  }});
  Ok(Self {identity,sender,states,model,enabled:Arc::new(std::sync::atomic::AtomicBool::new(true)),counter,model_epoch:Arc::new(AtomicU64::new(0))})
 }

 pub fn protected_identity(&self)->String{self.identity.read().unwrap().0.clone()}
 pub fn protected_build(&self,mut next:impl FnMut()->Result<Option<(String,String,String,String)>,String>,mut image:impl FnMut(&str)->Result<Option<Vec<u8>>,String>,previous:Option<(&atlas_faiss_probe::Index,&Value)>,force:bool,should_continue:impl Fn()->bool,progress:impl Fn(&str,usize,usize))->Result<(Vec<u8>,Value),String>{let epoch=self.model_epoch.load(Ordering::Acquire);let still_current=||self.model_epoch.load(Ordering::Acquire)==epoch && should_continue();let model=acquire_cancellable(&self.model,&still_current)?;let mut builder=atlas_joined_probe::MemoryBuilder::new(&model,previous)?;while let Some((id,title,body,description))=next()?{if !still_current(){return Err("indexing cancelled".into())}builder.push_progress(&model,&Source{id:&id,title:&title,body:&body,description:&description},&still_current,&progress)?;if model.supports_images(){if let Some(mime)=image_mime(&title){match image(&id){Ok(Some(bytes))=>{if let Err(e)=builder.push_image(&model,&id,&title,&bytes,mime){if force{return Err(e)}eprintln!("Atlas: image indexing skipped: {e}");}},Ok(None)=>{},Err(e)=>{if force{return Err(e)}eprintln!("Atlas: image indexing skipped: {e}")}}}}}if !still_current(){return Err("indexing cancelled".into())}builder.finish(&model)}
 pub fn protected_build_delta(&self,base:atlas_faiss_probe::Index,base_meta:&Value,previous:Option<(&[u8],&Value)>,next:impl FnMut()->Result<Option<(String,String,String,String)>,String>,image:impl FnMut(&str)->Result<Option<Vec<u8>>,String>,trash:&[String],should_continue:impl Fn()->bool,progress:impl Fn(&str,usize,usize))->Result<(Vec<u8>,Value,Option<(Vec<u8>,Value)>),String>{let epoch=self.model_epoch.load(Ordering::Acquire);let still_current=||self.model_epoch.load(Ordering::Acquire)==epoch&&should_continue();let model=acquire_cancellable(&self.model,&still_current)?;atlas_joined_probe::protected_overlay::prepare(&model,base,base_meta,previous,next,image,trash,still_current,progress)}
 pub fn protected_search_delta(&self,index:&atlas_joined_probe::protected_overlay::SearchIndex,q:&str,ids:Option<&[String]>)->Result<Vec<Value>,String>{let model=acquire(&self.model)?;let rows=atlas_joined_probe::protected_overlay::search(&model,index,q,ids,128)?;Ok(rows.into_iter().map(|v|{let score=v["score"].clone();let chunks=if v["field"]=="title"{vec![]}else{vec![json!({"text":v["text"],"start":v["start"],"end":v["end"],"field":v["field"],"score":score})]};json!({"id":v["id"],"title":v["title"],"chunks":chunks,"score":score})}).collect())}
 pub fn protected_search(&self,index:&atlas_faiss_probe::Index,meta:&Value,q:&str,ids:Option<&[String]>)->Result<Vec<Value>,String>{let model=acquire(&self.model)?;let rows=atlas_joined_probe::search_memory(&model,index,meta,q,ids,128)?;Ok(rows.into_iter().map(|v|{let score=v["score"].clone();let chunks=if v["field"]=="title"{vec![]}else{vec![json!({"text":v["text"],"start":v["start"],"end":v["end"],"field":v["field"],"score":score})]};json!({"id":v["id"],"title":v["title"],"chunks":chunks,"score":score})}).collect())}
 pub fn ensure(&self,dir:&Path)->Result<(),String>{
  let pending={let mut states=self.states.lock().unwrap();if states.contains_key(dir){false}else{states.insert(dir.to_owned(),State{revision:self.counter.fetch_add(1,Ordering::Relaxed)+1,..State::default()});true}};
  if pending && self.enabled.load(Ordering::Acquire) {self.sender.send(dir.to_owned()).map_err(|e|e.to_string())?}Ok(())
 }
 #[cfg(feature="test-hook")]
 pub fn with_model_lock_for_test(&self,ready:mpsc::Sender<()>,resume:mpsc::Receiver<()>){let _guard=self.model.lock().unwrap();let _=ready.send(());let _=resume.recv();}
 pub fn close(&self,dir:&Path)->Result<(),String>{let mut states=self.states.lock().unwrap();if !states.get(dir).is_some_and(|s|s.cancelled){states.remove(dir);}Ok(())}
 pub fn enqueue(&self,dir:&Path,id:Option<&str>)->Result<(),String>{
  {let mut states=self.states.lock().unwrap();let s=states.entry(dir.to_owned()).or_default();s.revision=self.counter.fetch_add(1,Ordering::Relaxed)+1;s.error=None;s.progress=None;if let Some(id)=id {if !s.processing.iter().any(|x|x==id){s.processing.push(id.to_owned())}}}
  if self.enabled.load(Ordering::Acquire) {self.sender.send(dir.to_owned()).map_err(|e|e.to_string())?;}Ok(())
 }
 pub fn resume(&self,dir:&Path)->Result<(),String>{if let Some(s)=self.states.lock().unwrap().get_mut(dir){if s.running&&s.cancelled{return Err("Индексация останавливается".into())}s.cancelled=false;}self.enqueue(dir,None)}
 pub fn rebuild_fresh(&self,dir:&Path)->Result<(),String>{if !self.index_enabled(){return Err("Включите поиск по смыслу в настройках".into())} {let mut states=self.states.lock().unwrap();let s=states.entry(dir.to_owned()).or_default();if s.running{return Err("Дождитесь завершения текущей индексации".into())}s.force=true;s.cancelled=false;}self.enqueue(dir,None)}
 pub fn cancel(&self,dir:&Path){if let Some(s)=self.states.lock().unwrap().get_mut(dir){s.force=false;s.revision=self.counter.fetch_add(1,Ordering::Relaxed)+1;s.cancelled=true;s.progress=None;s.processing.clear();s.error=None;}}
 pub fn revision(&self,dir:&Path)->Option<u64>{let states=self.states.lock().ok()?;let s=states.get(dir)?;if s.indexed>0 && s.indexed==s.revision && s.error.is_none(){Some(s.revision)}else{None}}
 pub fn wait_ready(&self,dir:&Path)->Result<u64,String>{self.ensure(dir)?;let mut announced=false;loop{atlas_llama_probe::remaining_search_time()?;if crate::rag_stream::cancelled(){return Err("Поиск остановлен".into())}if let Some(revision)=self.revision(dir){return Ok(revision)}let status=self.status(dir);if status["cancelled"]==true||status["stopping"]==true{return Err("Индексация остановлена".into())}if !self.index_enabled(){return Err("Семантический поиск выключен".into())}if status["indexing"]!=true{let message=status["message"].as_str().filter(|s|!s.is_empty()).unwrap_or("Индекс поиска недоступен");return Err(message.into())}if !announced{crate::rag_stream::progress("indexing","Подготавливаем индекс поиска…");announced=true}std::thread::sleep(std::time::Duration::from_millis(150));}}
 pub fn status(&self,dir:&Path)->Value{let states=self.states.lock().unwrap();match states.get(dir){None=>json!({"available":true,"message":"","processing_ids":[],"failed_ids":[]}),Some(s)=>json!({"available":s.indexed==s.revision&&s.indexed>0&&s.error.is_none(),"message":s.error.clone().unwrap_or_else(||if s.cancelled {"Индексация остановлена".to_owned()}else if s.indexed!=s.revision {"Подготовка семантического поиска".to_string()} else {String::new()}),"indexing":(self.index_enabled()&&s.indexed!=s.revision&&s.error.is_none()&&!s.cancelled)||s.running,"cancelled":s.cancelled&&!s.running,"stopping":s.cancelled&&s.running,"progress":if s.cancelled&&s.running{Some(json!({"phase":"Останавливаем…","current_title":null,"completed":null,"total":null}))}else{s.progress.clone()},"processing_ids":s.processing,"failed_ids":if s.error.is_some(){s.processing.clone()}else{vec![]}})}}
 pub fn search(&self,dir:&Path,query:&str,ids:Option<&[String]>)->Result<Vec<Value>,String>{
  let rev={let states=self.states.lock().unwrap();let s=states.get(dir).ok_or("index unavailable")?;if s.indexed==0||s.indexed!=s.revision||s.error.is_some(){return Err("index unavailable".into())}s.revision};
  let model=acquire(&self.model)?;let vector:[f32;512]=model.encode(query,true)?.try_into().map_err(|_|"invalid dimensions")?;let filter=ids.map(|v|v.iter().map(String::as_str).collect::<Vec<_>>());let rows={let _guard=atlas_native_storage_contract::workspace_lock(dir).map_err(|e|e.to_string())?;match atlas_joined_probe::overlay_gen::search_vector(model.identity(),&dir.join(".atlas-semantic-native"),&vector,filter.as_deref(),128){Err(e) if e=="model key mismatch"||e=="model mismatch"=>atlas_joined_probe::overlay_gen::search_vector(model.alternate_identity(),&dir.join(".atlas-semantic-native"),&vector,filter.as_deref(),128)?,result=>result?}};
  let states=self.states.lock().unwrap();let state=states.get(dir).ok_or("index unavailable")?;if state.revision!=rev||state.indexed!=rev{return Err("index changed".into())}
  Ok(rows.into_iter().map(|v|{let score=v["score"].clone();let chunks=if v["field"]=="title"{vec![]}else{vec![json!({"text":v["text"],"start":v["start"],"end":v["end"],"field":v["field"],"score":score})]};json!({"id":v["id"],"title":v["title"],"chunks":chunks,"score":score})}).collect())
 }
}

#[cfg(test)]
mod lifecycle_tests {
 use super::*;
 fn idle_service()->SemanticService{SemanticService{identity:Arc::new(std::sync::RwLock::new((String::new(),String::new()))),sender:mpsc::channel().0,states:Arc::new(Mutex::new(HashMap::new())),model:Arc::new(Mutex::new(ModelSlot{images:false,enabled:true,encoder:None,exe:String::new(),path:String::new(),device:String::new(),last_used:std::time::Instant::now()})),enabled:Arc::new(std::sync::atomic::AtomicBool::new(true)),counter:Arc::new(AtomicU64::new(0)),model_epoch:Arc::new(AtomicU64::new(0))}}
 #[test] fn chat_waits_for_pending_index_and_returns_ready_revision(){let service=idle_service();let path=PathBuf::from("/tmp/chat-index-wait-test");service.states.lock().unwrap().insert(path.clone(),State{revision:2,indexed:1,..State::default()});let copy=service.clone();let target=path.clone();let t=std::thread::spawn(move||copy.wait_ready(&target));std::thread::sleep(std::time::Duration::from_millis(30));assert!(!t.is_finished());service.states.lock().unwrap().get_mut(&path).unwrap().indexed=2;assert_eq!(t.join().unwrap().unwrap(),2);}
 #[test] fn chat_does_not_wait_for_failed_or_cancelled_index(){let service=idle_service();let path=PathBuf::from("/tmp/chat-index-failure-test");service.states.lock().unwrap().insert(path.clone(),State{revision:2,indexed:1,error:Some("failure".into()),..State::default()});assert_eq!(service.wait_ready(&path).unwrap_err(),"failure");let mut states=service.states.lock().unwrap();let state=states.get_mut(&path).unwrap();state.error=None;state.cancelled=true;drop(states);assert_eq!(service.wait_ready(&path).unwrap_err(),"Индексация остановлена");}
 #[test]
 fn locked_model_is_not_unloaded() {
  let service=SemanticService{identity:Arc::new(std::sync::RwLock::new((String::new(),String::new()))),sender:mpsc::channel().0,states:Arc::new(Mutex::new(HashMap::new())),model:Arc::new(Mutex::new(ModelSlot{images:false,enabled:true,encoder:None,exe:String::new(),path:String::new(),device:String::new(),last_used:std::time::Instant::now()})),enabled:Arc::new(std::sync::atomic::AtomicBool::new(true)),counter:Arc::new(AtomicU64::new(0)),model_epoch:Arc::new(AtomicU64::new(0))};
  let dir=PathBuf::from("/tmp/atlas-status-test");
  service.states.lock().unwrap().insert(dir.clone(),State{revision:2,indexed:1,..State::default()});
  assert_eq!(service.status(&dir)["indexing"],true);
  service.enabled.store(false,Ordering::Release);assert_eq!(service.status(&dir)["indexing"],false);
  service.enabled.store(true,Ordering::Release);service.states.lock().unwrap().get_mut(&dir).unwrap().error=Some("error".into());assert_eq!(service.status(&dir)["indexing"],false);
  service.states.lock().unwrap().remove(&dir);assert_ne!(service.status(&dir)["indexing"],true);
  let held=service.model.lock().unwrap();
  assert!(!service.unload_if_idle(std::time::Duration::ZERO));
  drop(held);
  assert!(service.unload_if_idle(std::time::Duration::ZERO));
 }
 #[test]
 fn changing_variant_updates_identity_and_missing_model_preserves_it() {
  let dir=std::env::temp_dir().join(format!("atlas-variant-unit-{}",uuid::Uuid::new_v4()));
  std::fs::create_dir(&dir).unwrap();
  let bf=dir.join("bf.gguf");let q8=dir.join("q8.gguf");
  std::fs::write(&bf,b"bf16").unwrap();std::fs::write(&q8,b"q8").unwrap();
  let service=SemanticService::start_lazy("unused",bf.to_str().unwrap(),"cpu").unwrap();
  let before=service.worker_model_identity().unwrap();
  assert!(service.reconfigure_model(Some(dir.join("missing").to_str().unwrap()),"cpu",true).is_err());
  assert_eq!(service.worker_model_identity().unwrap(),before);
  service.reconfigure_model(Some(q8.to_str().unwrap()),"cpu",true).unwrap();
  assert_ne!(service.worker_model_identity().unwrap(),before);
  assert_eq!(service.worker_model_identity().unwrap(),Encoder::model_identity(q8.to_str().unwrap(),"cpu").unwrap());
  std::fs::remove_dir_all(dir).unwrap();
 }
 #[test]
 #[ignore = "requires verified Q8 and BF16 model files"]
 fn compatible_variant_preserves_ready_index_and_invalidates_running_work() {
  let q8=std::env::var("ATLAS_TEST_Q8_MODEL").unwrap();let bf=std::env::var("ATLAS_TEST_BF16_MODEL").unwrap();
  let identities=Encoder::model_identities(&q8,"cpu").unwrap();let (sender,receiver)=mpsc::channel();
  let service=SemanticService{identity:Arc::new(std::sync::RwLock::new(identities)),sender,states:Arc::new(Mutex::new(HashMap::new())),model:Arc::new(Mutex::new(ModelSlot{images:false,enabled:true,encoder:None,exe:String::new(),path:q8.clone(),device:"cpu".into(),last_used:std::time::Instant::now()})),enabled:Arc::new(std::sync::atomic::AtomicBool::new(true)),counter:Arc::new(AtomicU64::new(100)),model_epoch:Arc::new(AtomicU64::new(0))};
  let dir=PathBuf::from("/not-a-real-workspace");service.states.lock().unwrap().insert(dir.clone(),State{revision:7,indexed:7,..State::default()});
  service.reconfigure_model(Some(&bf),"metal",true).unwrap();
  assert_eq!(service.revision(&dir),Some(7),"a ready compatible index must not be invalidated");assert!(receiver.try_recv().is_err(),"a ready compatible index must not be queued");
  service.states.lock().unwrap().get_mut(&dir).unwrap().running=true;
  service.reconfigure_model(Some(&q8),"cpu",true).unwrap();
  assert_ne!(service.states.lock().unwrap().get(&dir).unwrap().revision,7,"active work must be invalidated");assert_eq!(receiver.try_recv().unwrap(),dir);
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
