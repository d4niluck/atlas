use std::{fs,path::{Path,PathBuf},process::Command,time::{SystemTime,UNIX_EPOCH},os::unix::fs::symlink};
fn path(name:&str)->PathBuf{std::env::temp_dir().join(format!("atlas-failure-{name}-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))}
fn run(p:&Path,args:&[&str])->std::process::Output{Command::new(env!("CARGO_BIN_EXE_atlas-native-storage-contract")).arg(args[0]).arg(p).args(&args[1..]).output().unwrap()}
fn sql(p:&Path,s:&str){let o=Command::new("/usr/bin/sqlite3").arg(p.join("atlas.sqlite3")).arg(s).output().unwrap();assert!(o.status.success(),"{}",String::from_utf8_lossy(&o.stderr))}
#[test] fn create_note_rejects_notes_symlink_escape(){
 let p=path("symlink");let outside=path("outside");fs::create_dir_all(&outside).unwrap();assert!(run(&p,&["create-workspace"]).status.success());fs::remove_dir(p.join("notes")).unwrap();symlink(&outside,p.join("notes")).unwrap();
 let out=run(&p,&["create-note","a","user data"]);let outside_files=fs::read_dir(&outside).unwrap().count();fs::remove_dir_all(&p).unwrap();fs::remove_dir_all(&outside).unwrap();assert!(!out.status.success(),"write via escaping symlink must fail");assert_eq!(outside_files,0,"must not write outside workspace");
}
#[test] fn old_schema_is_migrated_when_opening(){
 let p=path("legacy");fs::create_dir_all(p.join("notes")).unwrap();fs::create_dir_all(p.join("files")).unwrap();fs::write(p.join("notes/n.md"),"старый текст").unwrap();
 sql(&p,"CREATE TABLE objects(id TEXT PRIMARY KEY,kind TEXT NOT NULL,title TEXT NOT NULL,path TEXT NOT NULL);CREATE VIRTUAL TABLE search_index USING fts5(object_id UNINDEXED,title,body,tokenize='unicode61');CREATE TABLE board(id INTEGER PRIMARY KEY,data TEXT NOT NULL);CREATE TABLE trash(id TEXT PRIMARY KEY,deleted_at REAL NOT NULL,board TEXT NOT NULL);CREATE TABLE pending_edges(id TEXT PRIMARY KEY,data TEXT NOT NULL);CREATE TABLE note_journal(object_id TEXT PRIMARY KEY,title TEXT NOT NULL,text TEXT NOT NULL);CREATE TABLE search_history(id INTEGER PRIMARY KEY AUTOINCREMENT,query TEXT NOT NULL,mode TEXT NOT NULL,results TEXT NOT NULL,created_at REAL NOT NULL);INSERT INTO objects VALUES('n','note','Заголовок','notes/n.md');INSERT INTO search_index VALUES('n','Заголовок','старый текст');");
 let o=run(&p,&["get-board"]);fs::remove_dir_all(p).unwrap();assert!(o.status.success(),"old workspace must open: {}",String::from_utf8_lossy(&o.stderr));
}
#[test] fn existing_empty_database_initializes_base_schema(){
 let p=path("empty-db");fs::create_dir_all(p.join("notes")).unwrap();fs::create_dir_all(p.join("files")).unwrap();sql(&p,"PRAGMA user_version=0;");
 let o=run(&p,&["get-board"]);fs::remove_dir_all(p).unwrap();assert!(o.status.success(),"existing empty SQLite workspace must initialize tables: {}",String::from_utf8_lossy(&o.stderr));
}
