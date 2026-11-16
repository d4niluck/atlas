use std::{collections::VecDeque,os::unix::fs::MetadataExt,path::{Path,PathBuf},sync::{Mutex,OnceLock},time::{Duration,Instant}};
use rusqlite::{Connection,OpenFlags};
use serde_json::Value;
struct Entry { path:PathBuf, inode:(u64,u64), actor:String, kind:String, query:String, ids:Option<Vec<String>>, db:Connection, version:i64, created:Instant, value:Value }
static CACHE:OnceLock<Mutex<VecDeque<Entry>>>=OnceLock::new();
fn cache()->&'static Mutex<VecDeque<Entry>> {CACHE.get_or_init(||{std::thread::spawn(||loop{std::thread::sleep(Duration::from_secs(5));if let Some(cache)=CACHE.get(){if let Ok(mut entries)=cache.lock(){entries.retain(|e|e.created.elapsed()<Duration::from_secs(120));}}});Mutex::new(VecDeque::new())})}
fn version(db:&Connection)->rusqlite::Result<i64>{db.query_row("PRAGMA data_version",[],|r|r.get(0))}
// The caller holds the workspace lock and has checked the actor's rights.
pub fn lexical(path:&Path,actor:&str,query:&str,ids:Option<&[String]>)->Result<Value,()> {
 semantic(path,actor,query,ids,"lexical",||{
  let db=Connection::open_with_flags(path.join("atlas.sqlite3"),OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;
  atlas_native_storage_contract::search::lexical(&db,query,ids,128).map(|rows|serde_json::json!({"results":rows})).map_err(|e|e.to_string())
 }).map_err(|_|())
}
// Semantic callers include the model and index revision in kind.
pub fn semantic(path:&Path,actor:&str,query:&str,ids:Option<&[String]>,kind:&str,compute:impl FnOnce()->Result<Value,String>)->Result<Value,String>{
 let db_path=path.join("atlas.sqlite3");let meta=std::fs::symlink_metadata(&db_path).map_err(|e|e.to_string())?;if !meta.is_file()||meta.file_type().is_symlink(){return Err("Invalid database".into())}let inode=(meta.dev(),meta.ino());
 {let mut entries=cache().lock().map_err(|_|"Cache unavailable")?;entries.retain(|e|e.created.elapsed()<Duration::from_secs(120)&&(e.path!=db_path||(e.inode==inode&&version(&e.db).ok()==Some(e.version))));if let Some(i)=entries.iter().position(|e|e.path==db_path&&e.actor==actor&&e.kind==kind&&e.query==query&&e.ids.as_deref()==ids){let entry=entries.remove(i).ok_or("Cache unavailable")?;let value=entry.value.clone();entries.push_back(entry);return Ok(value)}}
 let db=Connection::open_with_flags(&db_path,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;let before=version(&db).map_err(|e|e.to_string())?;let value=compute()?;
 if serde_json::to_vec(&value).map_or(false,|v|v.len()<=1_000_000){let mut entries=cache().lock().map_err(|_|"Cache unavailable")?;if version(&db).ok()==Some(before)&&std::fs::symlink_metadata(&db_path).ok().is_some_and(|m|m.is_file()&&!m.file_type().is_symlink()&&(m.dev(),m.ino())==inode){entries.retain(|e|!(e.path==db_path&&e.actor==actor&&e.kind==kind&&e.query==query&&e.ids.as_deref()==ids));while entries.len()>=10{entries.pop_front();}entries.push_back(Entry{path:db_path,inode,actor:actor.into(),kind:kind.into(),query:query.into(),ids:ids.map(|v|v.to_vec()),db,version:before,created:Instant::now(),value:value.clone()})}}Ok(value)
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn cache_invalidates_after_commit(){
  let dir=std::env::temp_dir().join(format!("atlas-search-cache-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();let dir=dir.canonicalize().unwrap();
  let db=Connection::open(dir.join("atlas.sqlite3")).unwrap();db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE objects(id TEXT,title TEXT); CREATE TABLE trash(id TEXT); CREATE VIRTUAL TABLE search_chunks USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content); INSERT INTO objects VALUES('1','one'); INSERT INTO search_chunks VALUES('1',0,'body','needle');").unwrap();
  assert_eq!(lexical(&dir,"agent","needle",None).unwrap()["results"].as_array().unwrap().len(),1);
  assert_eq!(lexical(&dir,"agent","needle",None).unwrap()["results"].as_array().unwrap().len(),1);
  db.execute("DELETE FROM search_chunks",[]).unwrap();
  assert!(lexical(&dir,"agent","needle",None).unwrap()["results"].as_array().unwrap().is_empty());
  db.execute_batch("INSERT INTO search_chunks VALUES('1',0,'body','needle again'); INSERT INTO search_chunks(search_chunks) VALUES('rebuild');").unwrap();
  assert_eq!(lexical(&dir,"agent","needle",None).unwrap()["results"].as_array().unwrap().len(),1);
  drop(db);CACHE.get().unwrap().lock().unwrap().retain(|entry|!entry.path.starts_with(&dir));std::fs::remove_dir_all(dir).unwrap();
 }
 #[test] fn semantic_cache_checks_revision_and_actor(){
  let dir=std::env::temp_dir().join(format!("atlas-sem-cache-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();let dir=dir.canonicalize().unwrap();let db=Connection::open(dir.join("atlas.sqlite3")).unwrap();db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE state(x INTEGER); INSERT INTO state VALUES(1);").unwrap();
  let calls=std::cell::Cell::new(0);let run=|actor:&str,revision:&str|semantic(&dir,actor,"query",None,revision,||{calls.set(calls.get()+1);Ok(serde_json::json!([calls.get()]))}).unwrap();
  assert_eq!(run("a","model:1"),serde_json::json!([1]));assert_eq!(run("a","model:1"),serde_json::json!([1]));assert_eq!(run("b","model:1"),serde_json::json!([2]));assert_eq!(run("a","model:2"),serde_json::json!([3]));
  db.execute("UPDATE state SET x=2",[]).unwrap();assert_eq!(run("a","model:1"),serde_json::json!([4]));
  drop(db);CACHE.get().unwrap().lock().unwrap().retain(|entry|!entry.path.starts_with(&dir));std::fs::remove_dir_all(dir).unwrap();
 }
 #[test] fn slow_lexical_miss_does_not_block_other_cache_hit(){
  let dir=std::env::temp_dir().join(format!("atlas-cache-concurrent-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();let dir=dir.canonicalize().unwrap();let db=Connection::open(dir.join("atlas.sqlite3")).unwrap();db.execute_batch("CREATE TABLE state(x INTEGER)").unwrap();
  assert_eq!(semantic(&dir,"a","cached",None,"lexical",||Ok(serde_json::json!(1))).unwrap(),serde_json::json!(1));
  let (started_tx,started_rx)=std::sync::mpsc::channel();let (resume_tx,resume_rx)=std::sync::mpsc::channel();let slow_dir=dir.clone();let slow=std::thread::spawn(move||semantic(&slow_dir,"a","slow",None,"lexical",||{started_tx.send(()).unwrap();resume_rx.recv_timeout(Duration::from_secs(5)).map_err(|e|e.to_string())?;Ok(serde_json::json!(2))}));
  started_rx.recv_timeout(Duration::from_secs(2)).unwrap();let (done_tx,done_rx)=std::sync::mpsc::channel();let hit_dir=dir.clone();let hit=std::thread::spawn(move||{let result=semantic(&hit_dir,"a","cached",None,"lexical",||Err("unexpected cache miss".into()));done_tx.send(result).unwrap();});
  let hit_result=done_rx.recv_timeout(Duration::from_secs(2));resume_tx.send(()).unwrap();hit.join().unwrap();slow.join().unwrap().unwrap();assert_eq!(hit_result.unwrap().unwrap(),serde_json::json!(1));
  drop(db);CACHE.get().unwrap().lock().unwrap().retain(|entry|!entry.path.starts_with(&dir));std::fs::remove_dir_all(dir).unwrap();
 }
 #[test] fn simultaneous_misses_leave_one_entry(){
  let dir=std::env::temp_dir().join(format!("atlas-cache-duplicate-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();let dir=dir.canonicalize().unwrap();let db=Connection::open(dir.join("atlas.sqlite3")).unwrap();db.execute_batch("CREATE TABLE state(x INTEGER)").unwrap();
  let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));let threads=(0..2).map(|_|{let dir=dir.clone();let barrier=barrier.clone();std::thread::spawn(move||semantic(&dir,"a","same",None,"lexical",||{barrier.wait();Ok(serde_json::json!(1))}).unwrap())}).collect::<Vec<_>>();for t in threads{assert_eq!(t.join().unwrap(),serde_json::json!(1))}
  let mut entries=CACHE.get().unwrap().lock().unwrap();assert_eq!(entries.iter().filter(|e|e.path.starts_with(&dir)).count(),1);entries.retain(|e|!e.path.starts_with(&dir));drop(entries);drop(db);std::fs::remove_dir_all(dir).unwrap();
 }
}
