//! Transactional conversion of legacy FTS content to ordinary encrypted rows.
//! The caller must publish and validate the secondary index while holding the
//! workspace lock before using this conversion.

pub(crate) fn convert(tx:&rusqlite::Transaction<'_>)->Result<(),String>{
 let journal:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='atlas_fts_sequence') AND EXISTS(SELECT 1 FROM sqlite_master WHERE name='atlas_fts_dirty')",[],|r|r.get(0)).map_err(|e|e.to_string())?;if !journal{return Err("Журнал поиска не подготовлен".into())}
 let sql:String=tx.query_row("SELECT sql FROM sqlite_master WHERE name='search_index'",[],|r|r.get(0)).map_err(|e|e.to_string())?;
 if !sql.to_ascii_uppercase().contains("VIRTUAL TABLE") {return Err("Неожиданный формат поискового индекса".into())}
 tx.execute_batch("CREATE TABLE atlas_content_new(id TEXT PRIMARY KEY NOT NULL,title TEXT NOT NULL,body TEXT NOT NULL,description TEXT NOT NULL);
 INSERT INTO atlas_content_new(id,title,body,description) SELECT id,title,body,description FROM search_index;
 DROP TRIGGER IF EXISTS atlas_fts_index_insert;
 DROP TRIGGER IF EXISTS atlas_fts_index_delete;
 DROP TRIGGER IF EXISTS atlas_fts_index_update;
 DROP TABLE search_index;
 DROP TABLE search_chunks;
 ALTER TABLE atlas_content_new RENAME TO search_index;
 CREATE TRIGGER atlas_fts_index_insert AFTER INSERT ON search_index BEGIN UPDATE atlas_fts_sequence SET seq=seq+1 WHERE id=1; INSERT INTO atlas_fts_dirty VALUES(new.id,(SELECT seq FROM atlas_fts_sequence WHERE id=1)) ON CONFLICT(id) DO UPDATE SET generation=excluded.generation; END;
 CREATE TRIGGER atlas_fts_index_delete AFTER DELETE ON search_index BEGIN UPDATE atlas_fts_sequence SET seq=seq+1 WHERE id=1; INSERT INTO atlas_fts_dirty VALUES(old.id,(SELECT seq FROM atlas_fts_sequence WHERE id=1)) ON CONFLICT(id) DO UPDATE SET generation=excluded.generation; END;
 CREATE TRIGGER atlas_fts_index_update AFTER UPDATE ON search_index BEGIN UPDATE atlas_fts_sequence SET seq=seq+1 WHERE id=1; INSERT INTO atlas_fts_dirty VALUES(old.id,(SELECT seq FROM atlas_fts_sequence WHERE id=1)) ON CONFLICT(id) DO UPDATE SET generation=excluded.generation; UPDATE atlas_fts_sequence SET seq=seq+1 WHERE id=1; INSERT INTO atlas_fts_dirty VALUES(new.id,(SELECT seq FROM atlas_fts_sequence WHERE id=1)) ON CONFLICT(id) DO UPDATE SET generation=excluded.generation; END;").map_err(|e|e.to_string())?;
 Ok(())
}

#[cfg(test)] mod tests {
 use super::*;
 use crate::Vault;
 use rusqlite::params;
 #[test] fn export_rebuilds_chunks_after_conversion(){
  let root=std::env::temp_dir().join(format!("atlas-content-export-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let mut v=Vault::create(&root.join("vault"),"secret").unwrap();v.set_workspace_name("Test").unwrap();v.put_note("aaaa","title","unique exported text").unwrap();v.enable_search_journal().unwrap();{let tx=v.db.transaction().unwrap();convert(&tx).unwrap();tx.commit().unwrap();}let out=root.join("export");v.export_plain(&out).unwrap();let db=rusqlite::Connection::open(out.join("atlas.sqlite3")).unwrap();let rows=atlas_native_storage_contract::search::lexical(&db,"unique",None,10).unwrap();assert!(rows.iter().any(|r|r["id"]=="aaaa"));drop(db);drop(v);std::fs::remove_dir_all(root).unwrap();
 }
 #[test] fn duplicate_ids_abort_without_changing_legacy(){
  let root=std::env::temp_dir().join(format!("atlas-content-duplicate-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let mut v=Vault::create(&root.join("vault"),"secret").unwrap();v.put_note("n","title","first").unwrap();v.enable_search_journal().unwrap();v.db.execute("INSERT INTO search_index VALUES(?,?,?,?)",params!["n","title","second",""]).unwrap();
  {let tx=v.db.transaction().unwrap();assert!(convert(&tx).is_err());tx.rollback().unwrap();}assert_eq!(v.db.query_row("SELECT count(*) FROM search_index WHERE id='n'",[],|r|r.get::<_,i64>(0)).unwrap(),2);assert!(v.search("first").unwrap().contains(&"n".to_owned()));drop(v);std::fs::remove_dir_all(root).unwrap();
 }
 #[test] fn conversion_preserves_unicode_and_rolls_back_atomically(){
  let root=std::env::temp_dir().join(format!("atlas-content-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
  let mut v=Vault::create(&root.join("vault"),"secret").unwrap();v.put_note("n","Заголовок","текст 🦊").unwrap();v.enable_search_journal().unwrap();
  {let tx=v.db.transaction().unwrap();convert(&tx).unwrap();assert_eq!(tx.query_row("SELECT body FROM search_index WHERE id='n'",[],|r|r.get::<_,String>(0)).unwrap(),"текст 🦊");tx.rollback().unwrap();}
  assert!(v.search("текст").unwrap().contains(&"n".to_string()));
  {let tx=v.db.transaction().unwrap();convert(&tx).unwrap();tx.commit().unwrap();}
  assert_eq!(v.search_text("n").unwrap(),"текст 🦊");let (_,_,size,_,row)=v.semantic_source_header_after(None).unwrap().unwrap();assert_eq!(v.semantic_source_body_part(row,0,size).unwrap(),"текст 🦊".as_bytes());
  let before=v.pending_search_updates(10).unwrap()[0].1;v.db.execute("UPDATE search_index SET body='новый' WHERE id='n'",[]).unwrap();assert!(v.pending_search_updates(10).unwrap()[0].1>before);
  v.put_note("n","Новое","после миграции").unwrap();assert_eq!(v.get_note("n").unwrap(),"после миграции");v.put_file("f","file.txt",&b"file"[..],Some("file text")).unwrap();v.rename_file_guarded("f","other.txt",||Ok(())).unwrap();v.set_description("f","description").unwrap();assert_eq!(v.search_text("f").unwrap(),"file text");
  drop(v);let v=Vault::open(&root.join("vault"),"secret").unwrap();assert_eq!(v.search_text("n").unwrap(),"после миграции");drop(v);std::fs::remove_dir_all(root).unwrap();
 }
}

/// Encrypted backup pinned to the exact source state used by a search stage.
pub struct RecordedSearchCopy{file:std::fs::File,path:std::path::PathBuf,stamp:(u64,u64,u64,i64,i64,i64,i64),generation:String}
pub struct SearchBackup{file:std::fs::File,path:std::path::PathBuf,token:(i64,i64),stamp:(u64,u64,u64,i64,i64,i64,i64)}
impl crate::Vault {
 /// Run outside the workspace gate; concurrent changes make this attempt fail.
 pub fn prepare_search_backup(&self,path:&std::path::Path,password:&str)->Result<SearchBackup,String>{self.prepare_search_backup_guarded(path,password,||Ok(()))}
 pub fn prepare_search_backup_guarded(&self,path:&std::path::Path,password:&str,verify:impl Fn()->Result<(),String>)->Result<SearchBackup,String>{
  verify()?;let token=self.semantic_mutation_token()?;self.snapshot_to_guarded(path,password,|_,_|verify())?;
  std::fs::File::open(path.parent().ok_or("Папка недоступна")?).and_then(|dir|dir.sync_all()).map_err(|e|e.to_string())?;
  if self.semantic_mutation_token()?!=token{return Err("Пространство изменилось во время копирования".into())}
  use std::os::unix::fs::OpenOptionsExt;
  let file=std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(path).map_err(|e|e.to_string())?;
  let stamp=crate::source_stamp(&file.metadata().map_err(|e|e.to_string())?);Ok(SearchBackup{file,path:path.to_owned(),token,stamp})
 }
 /// Caller holds the workspace gate through publication and primary commit.
 #[cfg(feature="test-hook")]
 pub fn commit_search_cutover<G>(&mut self,stage:&crate::search_stage::SearchStage,destination:&std::path::Path,backup:&SearchBackup,password:&str,guard:impl FnOnce()->Result<G,String>)->Result<(),String>{self.commit_search_cutover_inner(stage,destination,backup,password,guard,None)}
 pub fn commit_recorded_search_cutover<G>(&mut self,stage:&crate::search_stage::SearchStage,copy:&RecordedSearchCopy,backup:&SearchBackup,password:&str,guard:impl FnOnce()->Result<G,String>)->Result<(),String>{self.commit_search_cutover_inner(stage,&copy.path,backup,password,guard,Some(copy))}
 fn commit_search_cutover_inner<G>(&mut self,stage:&crate::search_stage::SearchStage,destination:&std::path::Path,backup:&SearchBackup,password:&str,guard:impl FnOnce()->Result<G,String>,recorded:Option<&RecordedSearchCopy>)->Result<(),String>{
  let _guard=guard()?;
  let source_path=std::path::Path::new(self.db.path().ok_or("Источник недоступен")?);let source_dir=std::fs::metadata(source_path.parent().ok_or("Папка недоступна")?).map_err(|e|e.to_string())?;let target_dir=std::fs::metadata(destination.parent().ok_or("Папка недоступна")?).map_err(|e|e.to_string())?;use std::os::unix::fs::MetadataExt;if destination.file_name().and_then(|v|v.to_str())!=Some(crate::search_index_name(stage.generation())?.as_str())||source_dir.dev()!=target_dir.dev()||source_dir.ino()!=target_dir.ino(){return Err("Неверный путь поискового индекса".into())}
  let active:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='atlas_fts_active')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
  if active {
   let generation:String=self.db.query_row("SELECT generation FROM atlas_fts_active WHERE singleton=1",[],|r|r.get(0)).map_err(|_|"Поисковый индекс повреждён")?;
   if generation!=stage.generation(){return Err("Другой поисковый индекс уже активирован".into())}
   let source_path=std::path::PathBuf::from(self.db.path().ok_or("Источник недоступен")?);
   let (db,file,path)=Self::open_secondary(&self.db,&source_path,password)?.ok_or("Поисковый индекс недоступен")?;self.secondary=Some(db);self.secondary_pin=Some((file,path));return Ok(())
  }
  let token=stage.source_token();
  if token!=backup.token||self.semantic_mutation_token()?!=token||!self.pending_search_updates(1)?.is_empty(){return Err("Пространство изменилось во время подготовки поиска".into())}
  if crate::source_stamp(&backup.file.metadata().map_err(|e|e.to_string())?)!=backup.stamp||!crate::same_file(&backup.file.metadata().map_err(|e|e.to_string())?,&std::fs::symlink_metadata(&backup.path).map_err(|_|"Резервная копия недоступна")?){return Err("Резервная копия изменилась".into())}
  if let Some(copy)=recorded{let meta=copy.file.metadata().map_err(|e|e.to_string())?;if copy.generation!=stage.generation()||crate::source_stamp(&meta)!=copy.stamp||!crate::same_file(&meta,&std::fs::symlink_metadata(destination).map_err(|_|"Поисковый индекс недоступен")?){return Err("Поисковый индекс изменился".into())}}else{stage.publish(self,destination,||Ok(()))?;}
  let tx=self.db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
  let external:i64=tx.query_row("PRAGMA data_version",[],|r|r.get(0)).map_err(|e|e.to_string())?;
  let local:i64=tx.query_row("SELECT total_changes()",[],|r|r.get(0)).map_err(|e|e.to_string())?;
  let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM atlas_fts_dirty)",[],|r|r.get(0)).map_err(|e|e.to_string())?;
  if (external,local)!=token||pending{return Err("Пространство изменилось во время публикации".into())}
  convert(&tx)?;
  tx.execute_batch("CREATE TABLE atlas_fts_active(singleton INTEGER PRIMARY KEY CHECK(singleton=1),source_identity TEXT NOT NULL,generation TEXT NOT NULL,checkpoint INTEGER NOT NULL);").map_err(|e|e.to_string())?;
  tx.execute("INSERT INTO atlas_fts_active SELECT 1,identity,?1,?2 FROM atlas_workspace_identity WHERE singleton=1",rusqlite::params![stage.generation(),stage.checkpoint()]).map_err(|e|e.to_string())?;
  let marker:i64=tx.query_row("SELECT count(*) FROM atlas_fts_active",[],|r|r.get(0)).map_err(|e|e.to_string())?;if marker!=1{return Err("Неизвестное пространство".into())}
  tx.commit().map_err(|e|e.to_string())?;
  let source_path=std::path::PathBuf::from(self.db.path().ok_or("Источник недоступен")?);let (secondary,pin)=match Self::open_secondary(&self.db,&source_path,password)?{Some((db,file,path))=>{if let Some(copy)=recorded{let a=copy.file.metadata().map_err(|e|e.to_string())?;let b=file.metadata().map_err(|e|e.to_string())?;if crate::source_stamp(&a)!=copy.stamp||!crate::same_file(&a,&b){return Err("Поисковый индекс изменился после активации".into())}}(Some(db),Some((file,path)))},None=>(None,None)};self.secondary=secondary;self.secondary_pin=pin;
  Ok(())
 }
 #[cfg(feature="test-hook")]
 pub fn experimental_search_cutover<G>(&mut self,stage:&crate::search_stage::SearchStage,destination:&std::path::Path,backup:&std::path::Path,password:&str,guard:impl FnOnce()->Result<G,String>)->Result<(),String>{if self.semantic_mutation_token()?!=stage.source_token(){return Err("Пространство изменилось во время подготовки поиска".into())}let backup=self.prepare_search_backup(backup,password)?;self.commit_search_cutover(stage,destination,&backup,password,guard)}
}

#[cfg(all(test,feature="test-hook"))] mod cutover_tests {
 use crate::{Vault,search_stage::build_search_stage};
 #[test] fn encrypted_pair_snapshot_keeps_ready_search_index(){
  let root=std::env::temp_dir().join(format!("atlas-pair-snapshot-{}",uuid::Uuid::new_v4()));let source=root.join("source");let target=root.join("target");std::fs::create_dir_all(&source).unwrap();std::fs::create_dir(&target).unwrap();let mut v=Vault::create(&source.join("workspace.atlas"),"secret").unwrap();v.put_note("n","title","unique content").unwrap();v.enable_search_journal().unwrap();let mut stage=build_search_stage(&v,&source,"secret").unwrap();while !v.pending_search_updates(1).unwrap().is_empty(){stage.replay_pending(&v,"secret",256).unwrap();}let index_name=crate::search_index_name(stage.generation()).unwrap();v.experimental_search_cutover(&stage,&source.join(&index_name),&source.join("backup"),"secret",||Ok(())).unwrap();assert!(v.snapshot_to(&root.join("single"),"secret",|_,_|{}).is_err());assert!(!root.join("single").exists());drop(v);drop(stage);let missing=root.join("missing");std::fs::create_dir(&missing).unwrap();std::fs::copy(source.join("workspace.atlas"),missing.join("workspace.atlas")).unwrap();let before=std::fs::read(missing.join("workspace.atlas")).unwrap();assert!(Vault::open(&missing.join("workspace.atlas"),"secret").err().unwrap().contains("Поисковый индекс отсутствует"));assert_eq!(std::fs::read(missing.join("workspace.atlas")).unwrap(),before);let dir=std::fs::File::open(&source).unwrap();let pinned=Vault::verify_closed_dir(&dir,"secret").unwrap();let missing_out=root.join("missing-out");std::fs::create_dir(&missing_out).unwrap();std::fs::rename(source.join(&index_name),source.join("index-held")).unwrap();assert!(Vault::snapshot_closed_pair_pinned(&dir,&pinned,&missing_out.join("workspace.atlas"),"secret",|_,_|{}).is_err());assert!(missing_out.join("workspace.atlas").exists());std::fs::rename(source.join("index-held"),source.join(&index_name)).unwrap();assert!(Vault::snapshot_closed_pinned(&dir,&pinned,&root.join("single-closed"),"secret",|_,_|{}).is_err());assert!(!root.join("single-closed").exists());let conflict=root.join("conflict");std::fs::create_dir(&conflict).unwrap();std::fs::write(conflict.join(&index_name),b"preexisting").unwrap();assert!(Vault::snapshot_closed_pair_pinned(&dir,&pinned,&conflict.join("workspace.atlas"),"secret",|_,_|{}).is_err());assert!(conflict.join("workspace.atlas").exists());assert_eq!(std::fs::read(conflict.join(&index_name)).unwrap(),b"preexisting");Vault::snapshot_closed_pair_pinned(&dir,&pinned,&target.join("workspace.atlas"),"secret",|_,_|{}).unwrap();let copy=Vault::open(&target.join("workspace.atlas"),"secret").unwrap();assert_eq!(copy.get_note("n").unwrap(),"unique content");assert_eq!(copy.search("unique").unwrap(),vec!["n"]);assert!(copy.search_chunks("unique",None,10).unwrap().iter().any(|r|r["id"]=="n"));let hits=crate::search_stage::search_published(&copy,&target.join(&index_name),"secret","unique",None,10).unwrap();assert!(hits.iter().any(|r|r["id"]=="n"));drop(copy);let mut source_vault=Vault::open(&source.join("workspace.atlas"),"secret").unwrap();source_vault.put_note("n","title","edited").unwrap();drop(source_vault);let pending=root.join("pending");std::fs::create_dir(&pending).unwrap();assert!(Vault::snapshot_closed_pair_pinned(&dir,&pinned,&pending.join("workspace.atlas"),"secret",|_,_|{}).is_err());assert!(!pending.join("workspace.atlas").exists());assert!(!pending.join(&index_name).exists());std::fs::remove_dir_all(root).unwrap();
 }
 #[test] fn cutover_rejects_stale_stage_before_removing_legacy(){
  let root=std::env::temp_dir().join(format!("atlas-cutover-stale-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let mut v=Vault::create(&root.join("vault"),"secret").unwrap();v.put_note("n","title","old").unwrap();v.enable_search_journal().unwrap();let mut stage=build_search_stage(&v,&root,"secret").unwrap();while !v.pending_search_updates(1).unwrap().is_empty(){stage.replay_pending(&v,"secret",256).unwrap();}v.put_note("n","title","new").unwrap();let backup=root.join("backup");let secondary=root.join("index");assert!(v.experimental_search_cutover(&stage,&secondary,&backup,"secret",||Ok(())).is_err());assert!(!backup.exists());assert!(!secondary.exists());assert_eq!(v.search("new").unwrap(),vec!["n"]);drop(v);drop(stage);std::fs::remove_dir_all(root).unwrap();
 }
 #[test] fn cutover_keeps_verified_legacy_backup(){
  let root=std::env::temp_dir().join(format!("atlas-cutover-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let mut v=Vault::create(&root.join("vault"),"secret").unwrap();v.put_note("n","title","sensitive text").unwrap();v.enable_search_journal().unwrap();let mut stage=build_search_stage(&v,&root,"secret").unwrap();while !v.pending_search_updates(1).unwrap().is_empty(){stage.replay_pending(&v,"secret",256).unwrap();}let backup=root.join("backup");let secondary=root.join(crate::search_index_name(stage.generation()).unwrap());let saved=v.prepare_search_backup(&backup,"secret").unwrap();v.commit_search_cutover(&stage,&secondary,&saved,"secret",||Ok(())).unwrap();v.commit_search_cutover(&stage,&secondary,&saved,"secret",||Ok(())).unwrap();assert_eq!(v.search_text("n").unwrap(),"sensitive text");assert_eq!(v.search("sensitive").unwrap(),vec!["n"]);assert!(v.search_chunks("sensitive",None,10).unwrap().iter().any(|r|r["id"]=="n"));let matches=crate::search_stage::search_published(&v,&secondary,"secret","sensitive",None,10).unwrap();assert!(matches.iter().any(|r|r["id"]=="n"));v.put_note("n","title","changed").unwrap();assert!(crate::search_stage::search_published(&v,&secondary,"secret","sensitive",None,10).is_err());assert!(v.search("sensitive").is_err());drop(v);let v=Vault::open(&root.join("vault"),"secret").unwrap();assert!(v.search("sensitive").is_err());assert_eq!(v.replay_search_updates(256).unwrap(),1);assert_eq!(v.search("changed").unwrap(),vec!["n"]);let old=Vault::open(&backup,"secret").unwrap();assert_eq!(old.search("sensitive").unwrap(),vec!["n"]);drop(old);drop(v);let reopened=Vault::open(&root.join("vault"),"secret").unwrap();assert!(reopened.secondary.is_some());assert_eq!(reopened.get_note("n").unwrap(),"changed");drop(reopened);drop(stage);std::fs::remove_dir_all(root).unwrap();
 }
}

/// Reject stale or substituted derived data both before and after reading it.
pub(crate) fn read_secondary<T>(source:&crate::Vault,read:impl FnOnce(&rusqlite::Connection)->Result<T,String>)->Result<T,String>{
 source.verify_search_files()?;let db=source.secondary.as_ref().ok_or("Поисковый индекс недоступен")?;
 let token=source.semantic_mutation_token()?;
 if !source.pending_search_updates(1)?.is_empty(){return Err("Поисковый индекс ещё обновляется".into())}
 let (identity,generation,checkpoint):(String,String,i64)=source.db.query_row("SELECT source_identity,generation,checkpoint FROM atlas_fts_active WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"Поисковый индекс не активирован")?;
 if identity!=source.workspace_identity()?||checkpoint!=source.search_journal_sequence()?{return Err("Поисковый индекс ещё обновляется".into())}
 let (complete,actual_identity,actual_generation,actual_checkpoint):(i64,String,String,i64)=db.query_row("SELECT complete,source_identity,generation,checkpoint FROM atlas_fts_stage_meta WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(|_|"Поисковый индекс повреждён")?;
 if complete!=1||identity!=actual_identity||generation!=actual_generation||checkpoint!=actual_checkpoint{return Err("Поисковый индекс устарел".into())}
 let result=read(db)?;source.verify_search_files()?;
 if source.semantic_mutation_token()?!=token||!source.pending_search_updates(1)?.is_empty(){return Err("Поисковый индекс ещё обновляется".into())}Ok(result)
}


#[cfg(test)] mod inode_guard_tests{
 use crate::Vault;
 #[test] fn replaced_secondary_aborts_before_ack(){
  let root=std::env::temp_dir().join(format!("atlas-search-swap-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let path=root.join("workspace.atlas");let generation="0123456789abcdef0123456789abcdef";let index=root.join(crate::search_index_name(generation).unwrap());let mut source=Vault::create(&path,"secret").unwrap();source.put_note("n","title","before").unwrap();source.enable_search_journal().unwrap();for (id,seq) in source.pending_search_updates(256).unwrap(){source.acknowledge_search_update(&id,seq).unwrap();}let identity=source.workspace_identity().unwrap();let seq=source.search_journal_sequence().unwrap();std::fs::File::create(&index).unwrap();let derived=Vault::connect(&index,"secret").unwrap();derived.execute_batch("CREATE TABLE objects(id TEXT PRIMARY KEY,title TEXT NOT NULL); CREATE TABLE trash(id TEXT PRIMARY KEY); CREATE VIRTUAL TABLE search_index USING fts5(id UNINDEXED,title,body,description); CREATE VIRTUAL TABLE search_chunks USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content,tokenize='unicode61'); CREATE TABLE atlas_fts_stage_meta(id INTEGER PRIMARY KEY,complete INTEGER,source_identity TEXT,generation TEXT,checkpoint INTEGER)").unwrap();derived.execute("INSERT INTO atlas_fts_stage_meta VALUES(1,1,?1,'0123456789abcdef0123456789abcdef',?2)",rusqlite::params![identity,seq]).unwrap();drop(derived);{let tx=source.db.transaction().unwrap();super::convert(&tx).unwrap();tx.execute_batch("CREATE TABLE atlas_fts_active(singleton INTEGER PRIMARY KEY,source_identity TEXT,generation TEXT,checkpoint INTEGER)").unwrap();tx.execute("INSERT INTO atlas_fts_active VALUES(1,?1,'0123456789abcdef0123456789abcdef',?2)",rusqlite::params![identity,seq]).unwrap();tx.commit().unwrap();}drop(source);let mut source=Vault::open(&path,"secret").unwrap();source.put_note("n","title","middle").unwrap();let checks=std::cell::Cell::new(0);let error=source.replay_search_updates_guarded(16,||{checks.set(checks.get()+1);if checks.get()==4{Err("stopped before primary commit".into())}else{Ok(())}}).unwrap_err();assert_eq!(error,"stopped before primary commit");assert!(!source.pending_search_updates(1).unwrap().is_empty());source.replay_search_updates_guarded(16,||Ok(())).unwrap();assert_eq!(source.search("middle").unwrap(),vec!["n"]);source.put_note("n","title","new").unwrap();let old=root.join("old");std::fs::rename(&index,&old).unwrap();std::fs::copy(&old,&index).unwrap();let replacement=std::fs::read(&index).unwrap();let error=source.replay_search_updates_guarded(16,||Ok(())).unwrap_err();assert!(error.contains("индекс изменился"),"{error}");assert!(!source.pending_search_updates(1).unwrap().is_empty());assert_eq!(std::fs::read(&index).unwrap(),replacement);drop(source);std::fs::remove_dir_all(root).unwrap();
 }
}

impl crate::Vault {
 /// Start a durable publication record before taking the source snapshot.
 pub fn begin_search_migration<G>(&self,generation:&str,destination:&std::path::Path,guard:impl FnOnce()->Result<G,String>)->Result<(),String>{
  let _guard=guard()?;let source=std::path::Path::new(self.db.path().ok_or("Источник недоступен")?);let a=std::fs::metadata(source.parent().ok_or("Папка недоступна")?).map_err(|e|e.to_string())?;let b=std::fs::metadata(destination.parent().ok_or("Папка недоступна")?).map_err(|e|e.to_string())?;use std::os::unix::fs::MetadataExt;if destination.file_name().and_then(|v|v.to_str())!=Some(crate::search_index_name(generation)?.as_str())||a.dev()!=b.dev()||a.ino()!=b.ino(){return Err("Неверный путь индекса".into())}
  crate::ciphertext_copy::begin_recorded_copy(&self.db,destination,generation)
 }
 /// Copy ciphertext outside the workspace gate; only marker writes take it.
 pub fn prepare_recorded_search_copy<G>(&self,stage:&mut crate::search_stage::SearchStage,backup:&mut SearchBackup,destination:&std::path::Path,verify:impl Fn()->Result<(),String>,guard:impl Fn()->Result<G,String>,verify_parent:impl Fn(&std::fs::File)->Result<(),String>)->Result<RecordedSearchCopy,String>{
  let old=stage.source_token();if backup.token!=old{return Err("Резервная копия устарела".into())}let expected=std::cell::Cell::new(old);let input=stage.pinned_file()?;
  crate::ciphertext_copy::prepare_recorded_copy_anchored(&input,&self.db,destination,stage.generation(),||{verify()?;if self.semantic_mutation_token()?!=expected.get(){return Err("Пространство изменилось во время подготовки поиска".into())}Ok(())},|write|{let _guard=guard()?;verify()?;let before=self.semantic_mutation_token()?;if before!=expected.get(){return Err("Пространство изменилось во время подготовки поиска".into())}write()?;let after=self.semantic_mutation_token()?;if after.0!=before.0{return Err("Пространство изменилось во время подготовки поиска".into())}expected.set(after);Ok(())},|dir|{verify()?;verify_parent(dir)?;self.verify_search_parent(dir)})?;
  let _guard=guard()?;verify()?;if self.semantic_mutation_token()?!=expected.get(){return Err("Пространство изменилось во время подготовки поиска".into())}stage.rebase_source_token(old,expected.get())?;backup.token=expected.get();drop(_guard);
  let before=std::fs::symlink_metadata(destination).map_err(|e|e.to_string())?;let file=crate::ciphertext_copy::open_recorded_copy_anchored(&self.db,destination,stage.generation(),||{verify()?;if self.semantic_mutation_token()?!=expected.get(){return Err("Пространство изменилось во время проверки индекса".into())}Ok(())},|dir|{verify()?;verify_parent(dir)?;self.verify_search_parent(dir)})?;let metadata=file.metadata().map_err(|e|e.to_string())?;let stamp=crate::source_stamp(&metadata);if !crate::same_file(&metadata,&before)||crate::source_stamp(&before)!=stamp{return Err("Поисковый индекс изменился во время проверки".into())}Ok(RecordedSearchCopy{file,path:destination.to_owned(),stamp,generation:stage.generation().to_owned()})
 }
}

#[cfg(all(test,feature="test-hook"))] mod recorded_copy_tests {
 use crate::{Vault,search_index_name,search_stage::build_search_stage_local_with_generation};
 #[test] fn marker_changes_preserve_snapshot_contract(){
  let root=std::env::temp_dir().join(format!("atlas-recorded-cutover-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let mut v=Vault::create(&root.join("workspace.atlas"),"secret").unwrap();v.put_note("n","title","recorded text").unwrap();v.enable_search_journal().unwrap();let generation=uuid::Uuid::new_v4().simple().to_string();let dest=root.join(search_index_name(&generation).unwrap());v.begin_search_migration(&generation,&dest,||Ok(())).unwrap();let mut stage=build_search_stage_local_with_generation(&v,"secret",Some(&generation),||Ok(())).unwrap();while !v.pending_search_updates(1).unwrap().is_empty(){stage.replay_pending(&v,"secret",256).unwrap();}let mut backup=v.prepare_search_backup(&root.join("backup"),"secret").unwrap();let copy=v.prepare_recorded_search_copy(&mut stage,&mut backup,&dest,||Ok(()),||Ok(()),|_|Ok(())).unwrap();v.commit_recorded_search_cutover(&stage,&copy,&backup,"secret",||Ok(())).unwrap();assert_eq!(v.search("recorded").unwrap(),vec!["n"]);drop(stage);drop(v);std::fs::remove_dir_all(root).unwrap();
 }
 #[test] fn changed_prepared_copy_is_never_activated(){
  use std::io::Write;let root=std::env::temp_dir().join(format!("atlas-recorded-tamper-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let mut v=Vault::create(&root.join("workspace.atlas"),"secret").unwrap();v.put_note("n","title","text").unwrap();v.enable_search_journal().unwrap();let generation=uuid::Uuid::new_v4().simple().to_string();let dest=root.join(search_index_name(&generation).unwrap());v.begin_search_migration(&generation,&dest,||Ok(())).unwrap();let mut stage=build_search_stage_local_with_generation(&v,"secret",Some(&generation),||Ok(())).unwrap();while !v.pending_search_updates(1).unwrap().is_empty(){stage.replay_pending(&v,"secret",256).unwrap();}let mut backup=v.prepare_search_backup(&root.join("backup"),"secret").unwrap();let copy=v.prepare_recorded_search_copy(&mut stage,&mut backup,&dest,||Ok(()),||Ok(()),|_|Ok(())).unwrap();std::fs::OpenOptions::new().append(true).open(&dest).unwrap().write_all(b"tamper").unwrap();assert!(v.commit_recorded_search_cutover(&stage,&copy,&backup,"secret",||Ok(())).is_err());assert_eq!(v.search("text").unwrap(),vec!["n"]);drop(copy);drop(stage);drop(v);std::fs::remove_dir_all(root).unwrap();
 }
 #[test] fn board_change_after_backup_rejects_copy(){
  let root=std::env::temp_dir().join(format!("atlas-recorded-stale-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let mut v=Vault::create(&root.join("workspace.atlas"),"secret").unwrap();v.put_note("n","title","text").unwrap();v.enable_search_journal().unwrap();let generation=uuid::Uuid::new_v4().simple().to_string();let dest=root.join(search_index_name(&generation).unwrap());v.begin_search_migration(&generation,&dest,||Ok(())).unwrap();let mut stage=build_search_stage_local_with_generation(&v,"secret",Some(&generation),||Ok(())).unwrap();while !v.pending_search_updates(1).unwrap().is_empty(){stage.replay_pending(&v,"secret",256).unwrap();}let mut backup=v.prepare_search_backup(&root.join("backup"),"secret").unwrap();let board=v.board().unwrap();v.put_board(&board).unwrap();assert!(v.prepare_recorded_search_copy(&mut stage,&mut backup,&dest,||Ok(()),||Ok(()),|_|Ok(())).is_err());assert!(!dest.exists());assert_eq!(v.search("text").unwrap(),vec!["n"]);drop(stage);drop(v);std::fs::remove_dir_all(root).unwrap();
 }
}
