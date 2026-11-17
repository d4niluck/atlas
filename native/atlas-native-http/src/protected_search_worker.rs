use atlas_sqlcipher_proof::Vault;
use std::{sync::{mpsc,Arc,Mutex,atomic::{AtomicBool,Ordering}},thread,time::Duration};

/// Owns separate connections so indexing never holds the editor's Vault mutex.
pub struct SearchWorker{stop:mpsc::Sender<()>,task:Option<thread::JoinHandle<()>>,error:Arc<Mutex<Option<String>>>,active:Arc<AtomicBool>}
impl SearchWorker{
 pub fn start(vault:Arc<Mutex<Vault>>,verify:impl Fn()->Result<(),String>+Send+'static)->Option<Self>{
  if !vault.lock().ok()?.has_secondary_search(){return None}
  let (stop,rx)=mpsc::channel();let active=Arc::new(AtomicBool::new(true));let running=active.clone();let error=Arc::new(Mutex::new(None));let thread_error=error.clone();let task=thread::spawn(move||loop{
   if verify().is_err(){if let Ok(mut e)=thread_error.lock(){*e=Some("Носитель недоступен; обновление поиска остановлено".into())}break}
   let result=match vault.lock(){Ok(vault)=>match vault.has_pending_search_updates(){Ok(true)=>vault.replay_search_updates_guarded(16,||{if !running.load(Ordering::Acquire){return Err("Операция остановлена".into())}verify()}),Ok(false)=>Ok(0),Err(e)=>Err(e)},Err(_)=>Err("Хранилище недоступно".into())};
   let delay=match result{Ok(n)=>{if let Ok(mut e)=thread_error.lock(){*e=None}if n>0{Duration::from_millis(10)}else{Duration::from_millis(250)}},Err(_)=>{if let Ok(mut e)=thread_error.lock(){*e=Some("Не удалось обновить поиск; повторяем попытку".into())}Duration::from_secs(2)}};
   if rx.recv_timeout(delay).is_ok(){break}
  });Some(Self{stop,task:Some(task),error,active})
 }
 pub fn last_error(&self)->Option<String>{self.error.lock().ok().and_then(|e|e.clone())}
}
impl Drop for SearchWorker{fn drop(&mut self){self.active.store(false,Ordering::Release);let _=self.stop.send(());if let Some(task)=self.task.take(){let _=task.join();}}}

#[cfg(all(test,feature="test-hook"))] mod tests{
 use super::*;
 #[test] fn worker_replays_and_stops_on_drop(){
  let root=std::env::temp_dir().join(format!("atlas-search-worker-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let path=root.join("workspace.atlas");let mut vault=Vault::create(&path,"secret").unwrap();vault.put_note("n","title","before").unwrap();vault.enable_search_journal().unwrap();let mut stage=atlas_sqlcipher_proof::search_stage::build_search_stage(&vault,&root,"secret").unwrap();while !vault.pending_search_updates(1).unwrap().is_empty(){stage.replay_pending(&vault,"secret",256).unwrap();}vault.experimental_search_cutover(&stage,&root.join(atlas_sqlcipher_proof::search_index_name(stage.generation()).unwrap()),&root.join("backup"),"secret",||Ok(())).unwrap();let background=Vault::open(&path,"secret").unwrap();let worker=SearchWorker::start(Arc::new(Mutex::new(background)),||Ok(())).unwrap();vault.put_note("n","title","after").unwrap();let until=std::time::Instant::now()+Duration::from_secs(5);loop{if vault.search("after").is_ok_and(|v|v==vec!["n"]){break}assert!(std::time::Instant::now()<until,"background indexing did not finish");thread::sleep(Duration::from_millis(20));}drop(worker);vault.put_note("n","title","later").unwrap();assert!(vault.search("later").is_err());drop(stage);drop(vault);std::fs::remove_dir_all(root).unwrap();
 }
}
