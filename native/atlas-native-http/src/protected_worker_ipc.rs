use atlas_sqlcipher_proof::Vault;
use serde_json::{json, Value};
use std::{path::{Path,PathBuf},sync::{Arc,Mutex,mpsc},io::{self,Read},collections::HashMap};
struct Download {id:String,offset:u64,size:u64}
struct PendingIndex {revision:String,model:String,index:atlas_faiss_probe::Index,records:Vec<Value>,cache:crate::worker_models::VectorCache}
fn vector(v:&Value)->Result<[f32;512],String>{let rows=v.as_array().filter(|x|x.len()==512).ok_or("invalid vector")?;let mut result=[0f32;512];for(i,x)in rows.iter().enumerate(){let value=x.as_f64().filter(|v|v.is_finite()&&v.abs()<=f32::MAX as f64).ok_or("invalid vector")?;result[i]=value as f32}Ok(result)}
fn write_marker(dir:&Path,guard:&crate::protected_external_guard::ProtectedExternalGuard,name:&str)->Result<(),String>{use std::io::Write;use std::os::unix::fs::OpenOptionsExt;let marker=dir.join(".workspace.json");if marker.symlink_metadata().is_ok_and(|m|m.file_type().is_symlink()){return Err("invalid workspace marker".into())}let tmp=dir.join(format!(".workspace-{}.tmp",uuid::Uuid::new_v4().simple()));let result=(||->Result<(),String>{let mut file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&tmp).map_err(|_|"workspace unavailable")?;file.write_all(json!({"name":name,"protected":true}).to_string().as_bytes()).map_err(|_|"workspace unavailable")?;file.sync_all().map_err(|_|"workspace unavailable")?;guard.verify()?;std::fs::rename(&tmp,&marker).map_err(|_|"workspace unavailable")?;std::fs::File::open(dir).and_then(|f|f.sync_all()).map_err(|_|"workspace unavailable")?;Ok(())})();if result.is_err(){let _=std::fs::remove_file(tmp);}result}
struct Upload { sender:mpsc::SyncSender<Option<Vec<u8>>>, task:std::thread::JoinHandle<Result<(),String>>, expected:u64, written:u64, id:String, stream:String }
struct ChannelReader { receiver:mpsc::Receiver<Option<Vec<u8>>>, current:std::io::Cursor<Vec<u8>> }
impl Read for ChannelReader {fn read(&mut self, out:&mut [u8])->io::Result<usize>{loop {let n=self.current.read(out)?;if n>0||out.is_empty(){return Ok(n)}match self.receiver.recv(){Ok(Some(bytes))=>self.current=std::io::Cursor::new(bytes),Ok(None)=>return Ok(0),Err(_)=>return Err(io::Error::new(io::ErrorKind::UnexpectedEof,"upload cancelled"))}}}}

// The caller must hold the verified directory and session lock for this object's lifetime.
pub struct ProtectedWorker { vault: Option<Arc<Mutex<Vault>>>, dir:PathBuf, upload:Option<Upload>, guard:Arc<crate::protected_external_guard::ProtectedExternalGuard>, downloads:HashMap<String,Download>, pending:Option<PendingIndex>, index_images:bool, exporting:bool }
impl ProtectedWorker {
 pub fn new(dir:&Path,guard:Arc<crate::protected_external_guard::ProtectedExternalGuard>)->Self { Self{vault:None,dir:dir.to_owned(),upload:None,guard,downloads:HashMap::new(),pending:None,index_images:false,exporting:false} }
 pub fn dispatch(&mut self, command:&Value)->Result<Value,String> {
  let action=command["action"].as_str().ok_or("invalid action")?;
  if let Err(error)=self.guard.verify(){if let Some(upload)=self.upload.take(){drop(upload.sender);let _=upload.task.join();}self.vault.take();return Err(error)}
  if action=="export-prepare" {
   if self.exporting{return Err("Копирование уже выполняется".into())}
   if self.upload.is_some(){return Err("Дождитесь окончания загрузки".into())}
   if self.vault.is_none(){return Err("Пространство заблокировано".into())}
   let password=command["password"].as_str().ok_or("Укажите пароль")?;
   let dir=std::fs::File::open(&self.dir).map_err(|_|"Носитель недоступен")?;
   Vault::verify_closed_dir(&dir,password)?;self.guard.verify()?;
   let vault=self.vault.take().unwrap();
   if Arc::strong_count(&vault)!=1{self.vault=Some(vault);return Err("Пространство занято".into())}
   drop(vault);self.exporting=true;return Ok(json!({"ok":true}))
  }
  if action=="export-resume" {
   if !self.exporting{return Err("Копирование не выполняется".into())}
   let password=command["password"].as_str().ok_or("Укажите пароль")?;
   let vault=Vault::open(&self.dir.join("workspace.atlas"),password)?;self.guard.verify()?;
   self.vault=Some(Arc::new(Mutex::new(vault)));self.exporting=false;return Ok(json!({"ok":true}))
  }
  if action=="export-check" {if !self.exporting{return Err("Копирование не выполняется".into())}self.guard.verify()?;return Ok(json!({"ok":true}))}
  if self.exporting{return Err("Дождитесь окончания копирования".into())}
  if action=="unlock" { if self.vault.is_some(){return Err("Пространство уже открыто".into())} let password=command["password"].as_str().ok_or("Укажите пароль")?;let vault=Vault::open(&self.dir.join("workspace.atlas"),password)?;let marker=self.dir.join(".workspace.json");if marker.symlink_metadata().is_ok_and(|m|m.file_type().is_symlink()){return Err("invalid workspace marker".into())}let metadata={use std::os::unix::fs::OpenOptionsExt;let mut bytes=Vec::new();std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(&marker).ok().and_then(|f|f.take(16385).read_to_end(&mut bytes).ok()).filter(|_|bytes.len()<=16384).and_then(|_|serde_json::from_slice::<Value>(&bytes).ok())};let name=if metadata.as_ref().is_some_and(|v|v["pending_copy_name"]==true){let name=metadata.as_ref().and_then(|v|v["name"].as_str()).map(str::trim).filter(|v|!v.is_empty()&&v.chars().count()<=100).ok_or("invalid workspace name")?;vault.set_workspace_name_guarded(name,||self.guard.verify())?;name.to_owned()}else{vault.workspace_name()?};write_marker(&self.dir,&self.guard,&name)?;self.vault=Some(Arc::new(Mutex::new(vault)));return Ok(json!({"name":name})); }
  if action=="upload-begin" {if self.upload.is_some(){return Err("Загрузка уже идёт".into())}let vault=self.vault.as_ref().ok_or("Пространство заблокировано")?.clone();let title=command["name"].as_str().filter(|v|!v.is_empty()&&!v.contains(['/', '\\', '\0'])).ok_or("invalid name")?.to_owned();let expected=command["size"].as_u64().filter(|v|*v<=10*1024*1024*1024).ok_or("invalid size")?;let id=command["id"].as_str().map(str::to_owned).unwrap_or_else(||uuid::Uuid::new_v4().to_string());let file_id=id.clone();let guard=self.guard.clone();let (sender,receiver)=mpsc::sync_channel(2);let task=std::thread::spawn(move||{let mut vault=vault.lock().map_err(|_|"workspace unavailable")?;vault.put_file_atomic(&file_id,&title,ChannelReader{receiver,current:std::io::Cursor::new(Vec::new())},||guard.verify())?;Ok(())});let stream=uuid::Uuid::new_v4().simple().to_string();self.upload=Some(Upload{sender,task,expected,written:0,id,stream:stream.clone()});return Ok(json!({"stream":stream}))}
  if action=="upload-chunk" {let upload=self.upload.as_mut().ok_or("no upload")?;if command["stream"].as_str()!=Some(upload.stream.as_str()){return Err("invalid stream".into())}let values=command["bytes"].as_array().filter(|v|!v.is_empty()&&v.len()<=65536).ok_or("invalid chunk")?;let bytes=values.iter().map(|x|x.as_u64().filter(|v|*v<=255).map(|v|v as u8).ok_or("invalid byte")).collect::<Result<Vec<_>,_>>()?;if upload.written+bytes.len() as u64>upload.expected{return Err("upload too large".into())}upload.sender.send(Some(bytes)).map_err(|_|"upload failed")?;upload.written+=values.len() as u64;return Ok(json!({"received":upload.written}))}
  if action=="upload-finish" {if self.upload.as_ref().is_none_or(|u|command["stream"].as_str()!=Some(u.stream.as_str())){return Err("invalid stream".into())}let upload=self.upload.take().ok_or("no upload")?;if upload.written!=upload.expected {drop(upload.sender);let _=upload.task.join();return Err("incomplete upload".into())}upload.sender.send(None).map_err(|_|"upload failed")?;drop(upload.sender);upload.task.join().map_err(|_|"upload failed")??;return Ok(json!({"id":upload.id}))}
  if action=="upload-cancel"||action=="close" {if action=="upload-cancel"&&self.upload.as_ref().is_none_or(|u|command["stream"].as_str()!=Some(u.stream.as_str())){return Err("invalid stream".into())}if let Some(upload)=self.upload.take(){drop(upload.sender);let _=upload.task.join();}if action=="close"{self.vault.take();}return Ok(json!({"ok":true}))}
  if self.upload.is_some(){return Err("Дождитесь окончания загрузки".into())}
  let mut vault=self.vault.as_ref().ok_or("Пространство заблокировано")?.lock().map_err(|_|"workspace unavailable")?;
  match action {
   "workspace-info"=>Ok(json!({"name":vault.workspace_name()?,"identity":vault.workspace_identity_guarded(||self.guard.verify())?,"protected":true})),
   "workspace-rename"=>{let name=command["name"].as_str().map(str::trim).filter(|v|!v.is_empty()&&v.chars().count()<=100).ok_or("invalid name")?;vault.set_workspace_name_guarded(name,||self.guard.verify())?;write_marker(&self.dir,&self.guard,name).map_err(|_|"Название сохранено, но не обновлено в списке; откройте пространство заново")?;Ok(json!({"name":name}))},
   "get-board"=>Ok(serde_json::from_str(&vault.board()?).map_err(|_|"Неверная доска")?),
   "put-board"=>{let board=if let Some(raw)=command["args"][0].as_str(){raw.to_owned()}else{command["board"].to_string()};vault.put_board_guarded(&board,||self.guard.verify())?;Ok(json!({"ok":true}))},
   "create-note"=>{let title=command["args"][0].as_str().ok_or("invalid title")?;let text=command["args"][1].as_str().ok_or("invalid text")?;let id=uuid::Uuid::new_v4().simple().to_string();vault.put_note_guarded(&id,title,text,||self.guard.verify())?;Ok(json!({"id":id}))},
   "update-note"=>{let id=command["args"][0].as_str().ok_or("invalid id")?;let title=command["args"][1].as_str().ok_or("invalid title")?;let text=command["args"][2].as_str().ok_or("invalid text")?;vault.put_note_guarded(id,title,text,||self.guard.verify())?;Ok(json!({"ok":true}))},
   "get-note"=>{let id=command["id"].as_str().or_else(||command["args"][0].as_str()).ok_or("invalid id")?;Ok(json!({"id":id,"title":vault.note_title(id)?,"text":vault.get_note(id)?}))},
   "put-note"=>{let id=command["id"].as_str().ok_or("invalid id")?;let title=command["title"].as_str().ok_or("invalid title")?;let text=command["text"].as_str().ok_or("invalid text")?;vault.put_note_guarded(id,title,text,||self.guard.verify())?;Ok(json!({"ok":true}))},
   "list-trash"=>Ok(json!(vault.list_trash()?)),
   "undo-created"=>{let id=command["id"].as_str().ok_or("invalid id")?;let expected=command["expected_revision"].as_str().ok_or("invalid revision")?;vault.undo_create_guarded(id,expected,||self.guard.verify())?;Ok(json!({"ok":true}))},
   "delete-objects"=>{let ids=command["ids"].as_array().ok_or("invalid ids")?.iter().map(|v|v.as_str().map(str::to_owned).ok_or("invalid id")).collect::<Result<Vec<_>,_>>()?;let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_|"invalid time")?.as_secs() as i64;vault.trash_many_guarded(&ids,now,||self.guard.verify())?;Ok(serde_json::from_str(&vault.board()?).map_err(|_|"invalid board")?)},
   "restore-objects"=>{let ids=command["ids"].as_array().ok_or("invalid ids")?.iter().map(|v|v.as_str().map(str::to_owned).ok_or("invalid id")).collect::<Result<Vec<_>,_>>()?;vault.restore_many_guarded(&ids,||self.guard.verify())?;Ok(serde_json::from_str(&vault.board()?).map_err(|_|"invalid board")?)},
   "object-info"=>{let id=command["id"].as_str().ok_or("invalid id")?;vault.object_info(id)},
   "search-text"=>{let id=command["id"].as_str().ok_or("invalid id")?;Ok(json!({"id":id,"text":vault.search_text(id)?}))},
   "get-description"=>{let id=command["id"].as_str().ok_or("invalid id")?;let info=vault.object_info(id)?;if info["kind"]!="file"{return Err("invalid file".into())}Ok(json!({"id":id,"description":info["description"]}))},
   "set-description"=>{let id=command["id"].as_str().ok_or("invalid id")?;let text=command["description"].as_str().ok_or("invalid description")?;vault.set_description_guarded(id,text,||self.guard.verify())?;Ok(json!({"id":id,"description":text}))},
   "lexical-search"=>{let query=command["query"].as_str().ok_or("invalid query")?;let ids=match &command["ids"] {Value::Null=>None,Value::Array(items)=>Some(items.iter().map(|v|v.as_str().map(str::to_owned).ok_or("invalid filter")).collect::<Result<Vec<_>,_>>()?),_=>return Err("invalid filter".into())};Ok(json!({"results":vault.search_chunks(query,ids.as_deref(),128)?,"revision":vault.semantic_fingerprint(false)?}))},
   "history"=>Ok(json!(vault.history_entries()?)),
   "history-save"=>{let mut entry=command.get("entry").filter(|v|v.is_object()).cloned().unwrap_or_else(||command.clone());if let Some(obj)=entry.as_object_mut(){obj.remove("action");}vault.record_history_guarded(entry,||self.guard.verify())?;Ok(json!({"ok":true}))},
   "read-range"=>{let id=command["id"].as_str().ok_or("invalid id")?;let offset=command["offset"].as_u64().ok_or("invalid offset")?;let length=command["length"].as_u64().filter(|v|*v<=65536).ok_or("invalid length")?;let mut bytes=Vec::with_capacity(length as usize);vault.read_range(id,offset,length,&mut bytes)?;Ok(json!({"bytes":bytes}))},
   "object-revision"|"file-revision"=>{let id=command["id"].as_str().ok_or("invalid id")?;Ok(json!({"revision":vault.revision(id)?}))},
   "download-begin"=>{if self.downloads.len()>=32{return Err("too many downloads".into())}let id=command["id"].as_str().ok_or("invalid id")?;let info=vault.object_info(id)?;if info["kind"]!="file" {return Err("not a file".into())}let size=vault.file_size(id)?;let stream=uuid::Uuid::new_v4().simple().to_string();self.downloads.insert(stream.clone(),Download{id:id.to_owned(),offset:0,size});Ok(json!({"title":info["title"],"size":size,"stream":stream}))},
   "download-next"=>{let stream=command["stream"].as_str().ok_or("invalid stream")?;let item=self.downloads.get_mut(stream).ok_or("no download")?;let length=(item.size-item.offset).min(65536);let mut bytes=Vec::with_capacity(length as usize);vault.read_range(&item.id,item.offset,length,&mut bytes)?;if bytes.is_empty()&&length>0{return Err("incomplete download".into())}item.offset+=bytes.len() as u64;let done=item.offset==item.size;if done{self.downloads.remove(stream);}Ok(json!({"bytes":bytes,"done":done}))},
   "download-cancel"=>{self.downloads.remove(command["stream"].as_str().unwrap_or(""));Ok(json!({"ok":true}))},
   "index-check"=>{let revision=vault.semantic_fingerprint(self.index_images)?;Ok(json!({"matches":command["revision"]==revision,"revision":revision}))},
   "index-snapshot"=>{self.index_images=command["images"].as_bool().unwrap_or(false);let revision=vault.semantic_fingerprint(self.index_images)?;let item=vault.semantic_source_after(command["after"].as_str())?;let (documents,next)=match item {Some((id,title,_,_))=>(vec![json!({"id":id,"title":title})],Some(id)),None=>(vec![],None)};Ok(json!({"revision":revision,"documents":documents,"next":next}))},
   "index-chunks"=>{let revision=vault.semantic_fingerprint(self.index_images)?;if command["revision"].as_str()!=Some(revision.as_str()){return Err("workspace changed".into())}let id=command["id"].as_str().ok_or("invalid id")?;let (_,_,body,description)=vault.semantic_source_after(None)?.filter(|x|x.0==id).or_else(||{let mut after=None;loop {let item=vault.semantic_source_after(after.as_deref()).ok().flatten()?;if item.0==id{return Some(item)}if item.0.as_str()>id{return None}after=Some(item.0)}}).ok_or("object missing")?;let empty=body.trim().is_empty();let mut chunks=Vec::new();for(field,text)in [("body",body),("description",description)]{if !text.trim().is_empty(){for mut chunk in atlas_joined_probe::worker_chunks(&text){chunk["field"]=json!(field);chunks.push(chunk)}}}let offset=command["offset"].as_u64().unwrap_or(0) as usize;if offset>chunks.len(){return Err("invalid offset".into())}let end=(offset+64).min(chunks.len());Ok(json!({"revision":revision,"body_empty":empty,"chunks":&chunks[offset..end],"next":if end<chunks.len(){Some(end)}else{None}}))},
   "index-begin"=>{let revision=vault.semantic_fingerprint(self.index_images)?;if command["revision"].as_str()!=Some(revision.as_str()){return Err("workspace changed".into())}let model=command["model"].as_str().filter(|x|!x.is_empty()&&x.len()<=256).ok_or("invalid model")?.to_owned();let cache=vault.load_semantic().ok().filter(|(_,m)|m["key"]==model).and_then(|(i,m)|m["records"].as_array().map(|r|crate::worker_models::make_cache(&i,r))).unwrap_or_default();self.pending=Some(PendingIndex{revision,model,index:atlas_faiss_probe::Index::new().map_err(str::to_string)?,records:Vec::new(),cache});Ok(json!({"ok":true}))},
   "index-abort"=>{self.pending=None;Ok(json!({"ok":true}))},
   "index-reuse"=>{let stage=self.pending.as_ref().ok_or("no index session")?;let key=command["key"].as_str().filter(|s|s.len()<=128).ok_or("invalid key")?;Ok(json!({"vector":stage.cache.get(key).map(|v|v.as_slice())}))},
   "index-add"=>{let stage=self.pending.as_mut().ok_or("no index session")?;let rows=command["records"].as_array().filter(|v|v.len()<=256&&v.len()+stage.records.len()<=16000).ok_or("index too large")?;for row in rows {let mut record=row.clone();let values=vector(&record["vector"])?;let object=record.as_object_mut().ok_or("invalid record")?;object.remove("vector");if record["id"].as_str().is_none()||record["text"].as_str().is_none(){return Err("invalid record".into())}stage.index.add(&[values]).map_err(str::to_string)?;if stage.cache.len()<16000{if let Some(key)=record["input_hash"].as_str().filter(|s|s.len()<=128){stage.cache.entry(key.to_owned()).or_insert_with(||Box::new(values));}}stage.records.push(record)}Ok(json!({"count":stage.records.len()}))},
   "index-commit"=>{let stage=self.pending.take().ok_or("no index session")?;let revision=vault.semantic_fingerprint(self.index_images)?;if stage.revision!=revision{return Err("workspace changed".into())}let bytes=stage.index.to_memory().map_err(str::to_string)?;use sha2::{Digest,Sha256};let metadata=json!({"key":stage.model,"revision":revision,"images":self.index_images,"index_sha256":format!("{:x}",Sha256::digest(&bytes)),"records":stage.records});vault.save_semantic_guarded(&bytes,&metadata,||self.guard.verify())?;Ok(json!({"revision":revision}))},
   "index-search"=>{let (index,metadata)=vault.load_semantic()?;self.index_images=metadata["images"].as_bool().unwrap_or(false);let revision=vault.semantic_fingerprint(self.index_images)?;if metadata["revision"]!=revision||metadata["key"]!=command["model"]{return Err("index stale".into())}let records=metadata["records"].as_array().ok_or("invalid index")?;if index.count()!=records.len() as i64{return Err("invalid index".into())}let ids=match &command["ids"]{Value::Null=>None,Value::Array(ids) if ids.iter().all(Value::is_string)=>Some(ids),_=>return Err("invalid filter".into())};let allowed=records.iter().enumerate().filter(|(_,r)|ids.is_none_or(|ids|ids.iter().any(|id|*id==r["id"]))).map(|(i,_)|i as i64).collect::<Vec<_>>();let hits=index.search(&vector(&command["vector"])?,&allowed,128).map_err(str::to_string)?;let results=hits.into_iter().map(|(i,score)|{let mut row=records[i as usize].clone();row["score"]=json!(score);row}).collect::<Vec<_>>();Ok(json!({"revision":revision,"results":results}))},
   "file-size"=>{let id=command["id"].as_str().ok_or("invalid id")?;Ok(json!({"size":vault.file_size(id)?}))},
   _=>Err("Операция пока не поддерживается".into())
  }
 }
}

impl Drop for ProtectedWorker {fn drop(&mut self){if let Some(upload)=self.upload.take(){drop(upload.sender);let _=upload.task.join();}}}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn guarded_writes_roll_back_on_lost_media() {
  let dir=std::env::temp_dir().join(format!("atlas-vault-rollback-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();
  let mut vault=Vault::create(&dir.join("workspace.atlas"),"secret").unwrap();vault.set_workspace_name("old").unwrap();vault.put_note("note","title","old").unwrap();vault.put_file("file","f.bin",&b"a"[..],None).unwrap();
  let fail=||Err("Носитель отключён".to_string());
  assert!(vault.put_note_guarded("note","title","new",fail).is_err());assert_eq!(vault.get_note("note").unwrap(),"old");
  assert!(vault.set_description_guarded("file","new",fail).is_err());assert_eq!(vault.object_info("file").unwrap()["description"],"");
  assert!(vault.set_workspace_name_guarded("new",fail).is_err());assert_eq!(vault.workspace_name().unwrap(),"old");
  let before=vault.board().unwrap();let mut board:Value=serde_json::from_str(&before).unwrap();board["objects"][0]["x"]=json!(100);assert!(vault.put_board_guarded(&board.to_string(),fail).is_err());assert_eq!(vault.board().unwrap(),before);
  assert!(vault.trash_many_guarded(&["note".to_string()],0,fail).is_err());assert_eq!(vault.get_note("note").unwrap(),"old");
  assert!(vault.record_history_guarded(json!({"query":"x","mode":"lexical"}),fail).is_err());assert!(vault.history_entries().unwrap().is_empty());
  drop(vault);std::fs::remove_dir_all(dir).unwrap();
 }
 #[test] fn unlock_note_board_close() {
  let dir=std::env::temp_dir().join(format!("atlas-vault-worker-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();
  let mut vault=Vault::create(&dir.join("workspace.atlas"),"secret").unwrap();vault.set_workspace_name("test").unwrap();vault.put_file("file","file.png",&b"abc"[..],None).unwrap();drop(vault);
  std::fs::write(dir.join(".workspace.json"),r#"{"name":"stale","protected":true}"#).unwrap();
  std::fs::write(dir.join(".atlas-protected"),b"atlas-protected-v1").unwrap();let guard=Arc::new(crate::protected_external_guard::ProtectedExternalGuard::acquire(std::fs::File::open(&dir).unwrap(),&dir).unwrap());let mut worker=ProtectedWorker::new(&dir,guard);assert!(worker.dispatch(&json!({"action":"unlock","password":"bad"})).is_err());
  assert_eq!(worker.dispatch(&json!({"action":"unlock","password":"secret"})).unwrap()["name"],"test");
  let marker:Value=serde_json::from_slice(&std::fs::read(dir.join(".workspace.json")).unwrap()).unwrap();assert_eq!(marker["name"],"test");
  let identity=worker.dispatch(&json!({"action":"workspace-info"})).unwrap()["identity"].as_str().unwrap().to_owned();assert_eq!(identity.len(),32);
  assert!(worker.dispatch(&json!({"action":"export-prepare","password":"bad"})).is_err());
  assert!(worker.dispatch(&json!({"action":"get-board"})).is_ok());
  assert!(worker.dispatch(&json!({"action":"export-check"})).is_err());
  worker.dispatch(&json!({"action":"export-prepare","password":"secret"})).unwrap();
  assert_eq!(worker.dispatch(&json!({"action":"export-check"})).unwrap()["ok"],true);
  assert!(worker.dispatch(&json!({"action":"get-board"})).is_err());
  assert!(worker.dispatch(&json!({"action":"export-resume","password":"bad"})).is_err());
  assert!(worker.dispatch(&json!({"action":"get-board"})).is_err());
  worker.dispatch(&json!({"action":"export-resume","password":"secret"})).unwrap();
  assert!(worker.dispatch(&json!({"action":"get-board"})).is_ok());
  worker.dispatch(&json!({"action":"put-note","id":"note","title":"Title","text":"Body"})).unwrap();
  assert_eq!(worker.dispatch(&json!({"action":"get-note","id":"note"})).unwrap()["text"],"Body");
  assert!(worker.dispatch(&json!({"action":"object-revision","id":"note"})).unwrap()["revision"].is_string());
  let generated=worker.dispatch(&json!({"action":"create-note","args":["Other","Text"]})).unwrap()["id"].as_str().unwrap().to_owned();
  worker.dispatch(&json!({"action":"update-note","args":[generated,"Other","Changed"]})).unwrap();assert_eq!(worker.dispatch(&json!({"action":"get-note","args":[generated]})).unwrap()["text"],"Changed");

  assert!(!worker.dispatch(&json!({"action":"lexical-search","query":"Body"})).unwrap()["results"].as_array().unwrap().is_empty());
  worker.dispatch(&json!({"action":"set-description","id":"file","description":"sample"})).unwrap();
  assert_eq!(worker.dispatch(&json!({"action":"object-info","id":"file"})).unwrap()["description"],"sample");
  assert!(worker.dispatch(&json!({"action":"get-board"})).unwrap().is_object());
  let page=worker.dispatch(&json!({"action":"index-snapshot"})).unwrap();let revision=page["revision"].as_str().unwrap().to_owned();
  worker.dispatch(&json!({"action":"index-begin","revision":revision,"model":"test"})).unwrap();
  let mut vector=vec![0f32;512];vector[0]=1.;let key=crate::worker_models::input_key("body","Title","Body");worker.dispatch(&json!({"action":"index-add","records":[{"id":"note","title":"Title","field":"body","text":"Body","input_hash":key,"vector":vector}]})).unwrap();assert_eq!(worker.dispatch(&json!({"action":"index-reuse","key":key})).unwrap()["vector"][0],1.0);assert!(worker.dispatch(&json!({"action":"index-reuse","key":crate::worker_models::input_key("body","Title","Changed")})).unwrap()["vector"].is_null());
  worker.dispatch(&json!({"action":"index-commit"})).unwrap();let result=worker.dispatch(&json!({"action":"index-search","model":"test","vector":vector})).unwrap();assert_eq!(result["results"][0]["id"],"note");
  worker.dispatch(&json!({"action":"index-begin","revision":revision,"model":"test"})).unwrap();let key=crate::worker_models::input_key("body","Title","Body");let reused=worker.dispatch(&json!({"action":"index-reuse","key":key})).unwrap();assert_eq!(reused["vector"][0],1.0);assert!(worker.dispatch(&json!({"action":"index-reuse","key":crate::worker_models::input_key("body","Different","Body")})).unwrap()["vector"].is_null());worker.dispatch(&json!({"action":"index-abort"})).unwrap();assert!(worker.dispatch(&json!({"action":"index-commit"})).is_err());let result=worker.dispatch(&json!({"action":"index-search","model":"test","vector":vector})).unwrap();assert_eq!(result["results"][0]["id"],"note");
  let stream=worker.dispatch(&json!({"action":"upload-begin","name":"sample.bin","size":3})).unwrap()["stream"].as_str().unwrap().to_owned();
  worker.dispatch(&json!({"action":"upload-chunk","stream":stream,"bytes":[1,2,3]})).unwrap();
  let file_id=worker.dispatch(&json!({"action":"upload-finish","stream":stream})).unwrap()["id"].as_str().unwrap().to_owned();
  assert_eq!(worker.dispatch(&json!({"action":"file-size","id":file_id})).unwrap()["size"],3);
  assert_eq!(worker.dispatch(&json!({"action":"read-range","id":file_id,"offset":0,"length":3})).unwrap()["bytes"],json!([1,2,3]));
  let download=worker.dispatch(&json!({"action":"download-begin","id":file_id})).unwrap()["stream"].as_str().unwrap().to_owned();
  assert_eq!(worker.dispatch(&json!({"action":"download-next","stream":download})).unwrap()["bytes"],json!([1,2,3]));
  let cancelled=worker.dispatch(&json!({"action":"upload-begin","id":"cancelled","name":"cancel.bin","size":4})).unwrap()["stream"].as_str().unwrap().to_owned();
  worker.dispatch(&json!({"action":"upload-chunk","stream":cancelled,"bytes":[9,8]})).unwrap();
  worker.dispatch(&json!({"action":"upload-cancel","stream":cancelled})).unwrap();
  assert!(worker.dispatch(&json!({"action":"file-size","id":"cancelled"})).is_err());
  let changing=worker.dispatch(&json!({"action":"upload-begin","id":"replaced","name":"replaced.bin","size":4})).unwrap()["stream"].as_str().unwrap().to_owned();
  worker.dispatch(&json!({"action":"upload-chunk","stream":changing,"bytes":[1,2]})).unwrap();
  let original=dir.with_extension("original");std::fs::rename(&dir,&original).unwrap();std::fs::create_dir(&dir).unwrap();
  assert!(worker.dispatch(&json!({"action":"upload-finish","stream":changing})).is_err());drop(worker);
  let vault=Vault::open(&original.join("workspace.atlas"),"secret").unwrap();assert_eq!(vault.workspace_identity().unwrap(),identity);assert!(vault.file_size("replaced").is_err());drop(vault);
  std::fs::remove_dir_all(dir).unwrap();std::fs::remove_dir_all(original).unwrap();
 }
}
