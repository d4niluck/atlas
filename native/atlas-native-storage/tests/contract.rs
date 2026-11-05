use std::{fs, process::Command, time::{SystemTime, UNIX_EPOCH}};
use std::sync::atomic::{AtomicUsize, Ordering};
static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);
fn run(action: &str, workspace: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_atlas-native-storage-contract"))
        .arg(action).arg(workspace).args(args).output().unwrap();
    assert!(output.status.success(), "action {action} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}
fn fixture() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("atlas-storage-contract-{}-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(), FIXTURE_ID.fetch_add(1, Ordering::Relaxed)));
    fs::create_dir_all(path.join("notes")).unwrap();
    fs::write(path.join("notes/n.md"), "Исходный текст").unwrap();
    let sql = "CREATE TABLE objects(id TEXT PRIMARY KEY, kind TEXT NOT NULL, title TEXT NOT NULL, path TEXT NOT NULL, text_status TEXT, description TEXT NOT NULL DEFAULT ''); CREATE TABLE board(id INTEGER PRIMARY KEY, data TEXT NOT NULL); CREATE TABLE trash(id TEXT PRIMARY KEY, deleted_at REAL NOT NULL, board TEXT NOT NULL); CREATE TABLE note_journal(object_id TEXT PRIMARY KEY,title TEXT NOT NULL,text TEXT NOT NULL); CREATE VIRTUAL TABLE search_index USING fts5(object_id UNINDEXED,title,body, tokenize='unicode61'); CREATE VIRTUAL TABLE search_chunks USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content, tokenize='unicode61'); INSERT INTO objects(id,kind,title,path) VALUES('n','note','Заголовок','notes/n.md'); INSERT INTO board VALUES(1,'{\"objects\":[{\"id\":\"n\",\"x\":12,\"y\":34}],\"edges\":[],\"stacks\":[]}');";
    assert!(Command::new("/usr/bin/sqlite3").arg(path.join("atlas.sqlite3")).arg(sql).status().unwrap().success());
    path
}
#[test] fn reads_existing_note_and_board() {
    let p=fixture();
    let note=run("get-note",&p,&["n"]); assert!(note.contains("Исходный текст") && note.contains("Заголовок"));
    let board=run("get-board",&p,&[]); assert!(board.contains("12") && board.contains("34") && board.contains("n"));
    fs::remove_dir_all(p).unwrap();
}
#[test] fn recovers_pending_note_journal() {
    let p=fixture();
    assert!(Command::new("/usr/bin/sqlite3").arg(p.join("atlas.sqlite3")).arg("INSERT INTO note_journal VALUES('n','Новый заголовок','Восстановленный текст')").status().unwrap().success());
    let note=run("get-note",&p,&["n"]); assert!(note.contains("Новый заголовок") && note.contains("Восстановленный текст"));
    assert_eq!(fs::read_to_string(p.join("notes/n.md")).unwrap(), "Восстановленный текст");
    let q=Command::new("/usr/bin/sqlite3").arg(p.join("atlas.sqlite3")).arg("SELECT COUNT(*) FROM note_journal").output().unwrap(); assert_eq!(String::from_utf8(q.stdout).unwrap().trim(),"0");
    fs::remove_dir_all(p).unwrap();
}
