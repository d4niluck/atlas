use std::{fs,path::Path,sync::{Mutex,OnceLock}};use serde_json::{Value,json};
type R<T>=Result<T,Box<dyn std::error::Error>>;
static LOCK:OnceLock<Mutex<()>>=OnceLock::new();fn lock()->&'static Mutex<()>{LOCK.get_or_init(||Mutex::new(()))}
fn load(root:&Path)->R<Vec<Value>>{let path=root.join(".atlas-external-workspaces.json");if path.is_symlink(){return Err("invalid registry".into())}match fs::read(path){Ok(v)=>Ok(serde_json::from_slice(&v)?),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(vec![]),Err(e)=>Err(e.into())}}
fn save(root:&Path,entries:&[Value])->R<()> {use std::{io::Write,os::unix::fs::OpenOptionsExt};let dest=root.join(".atlas-external-workspaces.json");if dest.is_symlink(){return Err("invalid registry".into())}let tmp=root.join(format!(".atlas-external-{}.tmp",uuid::Uuid::new_v4().simple()));let result=(||->R<()>{let mut f=fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;f.write_all(&serde_json::to_vec(entries)?)?;f.sync_all()?;fs::rename(&tmp,dest)?;fs::File::open(root)?.sync_all()?;Ok(())})();if result.is_err(){let _=fs::remove_file(tmp);}result}
pub fn entries(root:&Path)->R<Vec<Value>> {let _guard=lock().lock().map_err(|_|"registry unavailable")?;load(root)}
pub fn register(root:&Path,id:&str,name:&str,identity:&str)->R<()> {let _guard=lock().lock().map_err(|_|"registry unavailable")?;let mut entries=load(root)?;if let Some(entry)=entries.iter_mut().find(|e|e["id"]==id){if entry["identity"]!=identity{return Err("Выбрано другое пространство".into())}entry["name"]=json!(name)}else{if entries.iter().any(|e|e["identity"]==identity){return Err("Пространство уже добавлено; откройте существующую запись".into())}entries.push(json!({"id":id,"name":name,"identity":identity}))}save(root,&entries)}
pub fn forget(root:&Path,id:&str)->R<bool>{let _guard=lock().lock().map_err(|_|"registry unavailable")?;let mut entries=load(root)?;let old=entries.len();entries.retain(|entry|entry["id"]!=id);if old!=entries.len(){save(root,&entries)?;Ok(true)}else{Ok(false)}}
#[cfg(any(feature="test-hook",feature="protected-external"))]
pub fn register_protected(root:&Path,id:&str,name:&str,identity:&str)->R<()> {
 let _guard=lock().lock().map_err(|_|"registry unavailable")?;let mut entries=load(root)?;
 if let Some(entry)=entries.iter_mut().find(|entry|entry["id"]==id) {
  if entry["identity"]!=identity||entry["protected"]!=true{return Err("Выбрано другое пространство".into())}entry["name"]=json!(name);
 } else {
  if entries.iter().any(|entry|entry["identity"]==identity){return Err("Пространство уже добавлено; откройте существующую запись".into())}
  entries.push(json!({"id":id,"name":name,"identity":identity,"protected":true}));
 }
 save(root,&entries)
}
