use std::{fs, path::{Path,PathBuf}, process::Command, time::{SystemTime,UNIX_EPOCH}};
fn sql(p:&Path, s:&str)->String { let o=Command::new("/usr/bin/sqlite3").arg(p.join("atlas.sqlite3")).arg(s).output().unwrap(); assert!(o.status.success(),"{}",String::from_utf8_lossy(&o.stderr)); String::from_utf8(o.stdout).unwrap().trim().into() }
static SEQUENCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn fixture()->PathBuf {
 let p=std::env::temp_dir().join(format!("atlas-update-test-{}-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),SEQUENCE.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
 fs::create_dir_all(p.join("notes")).unwrap(); fs::write(p.join("notes/n.md"),"Старый текст").unwrap();
 sql(&p,"CREATE TABLE objects(id TEXT PRIMARY KEY,kind TEXT NOT NULL,title TEXT NOT NULL,path TEXT NOT NULL,text_status TEXT,description TEXT NOT NULL DEFAULT ''); CREATE TABLE board(id INTEGER PRIMARY KEY,data TEXT NOT NULL); CREATE TABLE trash(id TEXT PRIMARY KEY,deleted_at REAL NOT NULL,board TEXT NOT NULL); CREATE TABLE note_journal(object_id TEXT PRIMARY KEY,title TEXT NOT NULL,text TEXT NOT NULL); CREATE VIRTUAL TABLE search_index USING fts5(object_id UNINDEXED,title,body,tokenize='unicode61'); CREATE VIRTUAL TABLE search_chunks USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content,tokenize='unicode61'); INSERT INTO objects VALUES('n','note','Старый заголовок','notes/n.md',NULL,''); INSERT INTO search_index VALUES('n','Старый заголовок','Старый текст'); INSERT INTO search_chunks VALUES('n',0,'body','Старый текст');"); p
}
fn run(p:&Path,args:&[&str]) { let o=Command::new(env!("CARGO_BIN_EXE_atlas-native-storage-contract")).arg(args[0]).arg(p).args(&args[1..]).output().unwrap(); assert!(o.status.success(),"{}",String::from_utf8_lossy(&o.stderr)); }
#[test] fn update_saves_file_metadata_and_lexical_index() {
 let p=fixture(); run(&p,&["update-note","n","Новый заголовок","Уникальная орхидея"]);
 assert_eq!(fs::read_to_string(p.join("notes/n.md")).unwrap(),"Уникальная орхидея");
 assert_eq!(sql(&p,"SELECT title FROM objects WHERE id='n'"),"Новый заголовок");
 assert_eq!(sql(&p,"SELECT count(*) FROM note_journal"),"0");
 assert_eq!(sql(&p,"SELECT object_id FROM search_index WHERE search_index MATCH 'орхидея'"),"n");
 assert_eq!(sql(&p,"SELECT count(*) FROM search_index WHERE search_index MATCH 'старый'"),"0");
 assert_eq!(sql(&p,"SELECT object_id FROM search_chunks WHERE search_chunks MATCH 'орхидея'"),"n");
 fs::remove_dir_all(p).unwrap();
}
#[test] fn recovery_rebuilds_index_for_pending_update() {
 let p=fixture(); sql(&p,"INSERT INTO note_journal VALUES('n','Другой заголовок','Свежая магнолия')");
 run(&p,&["get-note","n"]);
 assert_eq!(fs::read_to_string(p.join("notes/n.md")).unwrap(),"Свежая магнолия");
 assert_eq!(sql(&p,"SELECT title FROM objects WHERE id='n'"),"Другой заголовок");
 assert_eq!(sql(&p,"SELECT count(*) FROM note_journal"),"0");
 assert_eq!(sql(&p,"SELECT object_id FROM search_index WHERE search_index MATCH 'магнолия'"),"n");
 assert_eq!(sql(&p,"SELECT object_id FROM search_chunks WHERE search_chunks MATCH 'магнолия'"),"n");
 fs::remove_dir_all(p).unwrap();
}
#[test] fn recovery_recreates_missing_note() {
 let p=fixture();fs::remove_file(p.join("notes/n.md")).unwrap();sql(&p,"INSERT INTO note_journal VALUES('n','Восстановлено','Новый текст')");run(&p,&["get-note","n"]);assert_eq!(fs::read_to_string(p.join("notes/n.md")).unwrap(),"Новый текст");fs::remove_dir_all(p).unwrap();
}
#[test] fn failed_update_restores_file_and_clears_journal() {
 let p=fixture();sql(&p,"CREATE TRIGGER reject_update BEFORE UPDATE ON objects BEGIN SELECT RAISE(ABORT, 'reject'); END;");
 let o=Command::new(env!("CARGO_BIN_EXE_atlas-native-storage-contract")).arg("update-note").arg(&p).args(["n","Новое","Новый текст"]).output().unwrap();assert!(!o.status.success());assert_eq!(fs::read_to_string(p.join("notes/n.md")).unwrap(),"Старый текст");assert_eq!(sql(&p,"SELECT count(*) FROM note_journal"),"0");assert_eq!(sql(&p,"SELECT title FROM objects WHERE id='n'"),"Старый заголовок");fs::remove_dir_all(p).unwrap();
}
