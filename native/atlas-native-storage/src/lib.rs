pub mod agent_activity_details;
pub mod chat_state;
pub mod pdf;
pub mod search;
mod snapshot;
pub use snapshot::{snapshot_workspace,snapshot_workspace_named,publish_snapshot};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{env, fs, io::Write, path::{Path, Component}, collections::{HashMap,HashSet}};
type R<T> = Result<T,Box<dyn std::error::Error>>;
fn safe_path(root:&Path, path:&str)->R<std::path::PathBuf>{
    let p=Path::new(path); if p.components().any(|c|!matches!(c,Component::Normal(_))){return Err("unsafe path".into())}
    let root=root.canonicalize()?; let candidate=root.join(p);
    let parent=candidate.parent().ok_or("missing parent")?.canonicalize()?;
    let target=match candidate.canonicalize(){Ok(target)=>target,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>parent.join(candidate.file_name().ok_or("missing filename")?),Err(e)=>return Err(e.into())};
    if !target.starts_with(&root){return Err("path escapes workspace".into())} Ok(target)
}
fn write_atomic(target:&Path,text:&str)->R<()> {
    for n in 0..10000u32 {
        let temp=target.with_extension(format!("atlas-recover-{}-{n}",std::process::id()));
        let mut f=match fs::OpenOptions::new().write(true).create_new(true).open(&temp){Ok(f)=>f,Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(e.into())};
        let result=(|| -> R<()> {f.write_all(text.as_bytes())?;f.sync_all()?;drop(f);fs::rename(&temp,target)?;Ok(())})();
        if result.is_err(){let _=fs::remove_file(&temp);}return result;
    } Err("no temporary file available".into())
}
fn chunks(s:&str)->impl Iterator<Item=(usize,&str)> {
 let mut rest=s;let mut offset=0usize;
 std::iter::from_fn(move || {
  if rest.is_empty(){return None} let mut it=rest.char_indices().peekable();let mut count=0;let mut end=rest.len();let mut last_break=(0,0);let mut last_word=false;
  let word=|c:char|c.is_alphanumeric()||c=='_';
  while count<400 {match it.next(){Some((i,c))=>{count+=1;last_word=word(c);end=i+c.len_utf8();if !last_word{last_break=(end,count)}},None=>break}}
  if count==400&&last_word&&it.peek().is_some_and(|(_,c)|word(*c)) {
   if last_break.0>0{(end,count)=last_break}else{while let Some(&(i,c))=it.peek(){if !word(c){break}it.next();end=i+c.len_utf8();count+=1;}}
  }
  let result=(offset,&rest[..end]);rest=&rest[end..];offset+=count;Some(result)
 })
}
fn recover(conn:&mut Connection,root:&Path)->R<()> {
    let pending={let mut s=conn.prepare("SELECT object_id,title,text FROM note_journal")?;let rows=s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?; rows.collect::<Result<Vec<_>,_>>()?};
    for (id,title,text) in pending {
        let (path,description):(String,String)=conn.query_row("SELECT path,description FROM objects WHERE id=? AND kind='note'",[&id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let target=safe_path(root,&path)?; write_atomic(&target,&text)?;
        let tx=conn.transaction()?;
        tx.execute("UPDATE objects SET title=? WHERE id=?",params![title,id])?;
        tx.execute("DELETE FROM search_index WHERE object_id=?",[&id])?;
        tx.execute("INSERT INTO search_index(object_id,title,body) VALUES(?,?,?)",params![id,title,text])?;
        tx.execute("DELETE FROM search_chunks WHERE object_id=?",[&id])?;
        tx.execute("INSERT INTO search_chunks(object_id,start,field,content) VALUES(?,-1,'title',?)",params![id,title])?;
        for (field,value) in [("body",text.as_str()),("description",description.as_str())]{for (start,part) in chunks(value){tx.execute("INSERT INTO search_chunks(object_id,start,field,content) VALUES(?,?,?,?)",params![id,start as i64,field,part])?;}}
        tx.execute("DELETE FROM note_journal WHERE object_id=?",[&id])?;if table_exists(&tx,"note_drafts")?{tx.execute("DELETE FROM note_drafts WHERE object_id=? AND title=? AND text=?",params![id,title,text])?;}tx.commit()?;
    }Ok(())
}
fn columns(conn:&Connection,table:&str)->R<Vec<String>> {let mut stmt=conn.prepare(&format!("PRAGMA table_info({table})"))?;let rows=stmt.query_map([],|r|r.get(1))?;Ok(rows.collect::<Result<_,_>>()?)}
fn table_exists(conn:&Connection, table:&str)->R<bool>{Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",[table],|r|r.get(0))?)}
fn pair_key(a:&str,b:&str)->R<String>{let mut p=[a,b];p.sort();Ok(format!("[{}, {}]",serde_json::to_string(p[0])?,serde_json::to_string(p[1])?))}
fn migrate(conn:&mut Connection)->R<()> {
 let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 tx.execute_batch("CREATE TABLE IF NOT EXISTS objects (id TEXT PRIMARY KEY,kind TEXT NOT NULL,title TEXT NOT NULL,path TEXT NOT NULL);
 CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(object_id UNINDEXED,title,body,tokenize='unicode61');
 CREATE TABLE IF NOT EXISTS board(id INTEGER PRIMARY KEY CHECK(id=1),data TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS trash(id TEXT PRIMARY KEY,deleted_at REAL NOT NULL,board TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS pending_edges(id TEXT PRIMARY KEY,data TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS note_journal(object_id TEXT PRIMARY KEY,title TEXT NOT NULL,text TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS search_history(id INTEGER PRIMARY KEY AUTOINCREMENT,query TEXT NOT NULL,mode TEXT NOT NULL,results TEXT NOT NULL,created_at REAL NOT NULL);")?;
 let object_cols=columns(&tx,"objects")?; if !object_cols.iter().any(|c|c=="text_status"){tx.execute_batch("ALTER TABLE objects ADD COLUMN text_status TEXT")?;}
 if !object_cols.iter().any(|c|c=="description"){tx.execute_batch("ALTER TABLE objects ADD COLUMN description TEXT NOT NULL DEFAULT ''")?;}
 if table_exists(&tx,"search_history")? {let cols=columns(&tx,"search_history")?;if !cols.iter().any(|c|c=="answer"){tx.execute_batch("ALTER TABLE search_history ADD COLUMN answer TEXT")?;}if !cols.iter().any(|c|c=="sources"){tx.execute_batch("ALTER TABLE search_history ADD COLUMN sources TEXT")?;}if !cols.iter().any(|c|c=="object_ids"){tx.execute_batch("ALTER TABLE search_history ADD COLUMN object_ids TEXT")?;}if !cols.iter().any(|c|c=="agentic"){tx.execute_batch("ALTER TABLE search_history ADD COLUMN agentic INTEGER")?;}if !cols.iter().any(|c|c=="error"){tx.execute_batch("ALTER TABLE search_history ADD COLUMN error TEXT")?;}if !cols.iter().any(|c|c=="scope_known"){tx.execute_batch("ALTER TABLE search_history ADD COLUMN scope_known INTEGER NOT NULL DEFAULT 0")?;}}
 if !table_exists(&tx,"legacy_edge_pairs")? {
  tx.execute_batch("CREATE TABLE legacy_edge_pairs(pair TEXT PRIMARY KEY,ids TEXT NOT NULL)")?;
  let mut edges:Vec<Value>=vec![];
  let data:Option<String>=tx.query_row("SELECT data FROM board WHERE id=1",[],|r|r.get(0)).optional()?;
  if let Some(v)=data {edges.extend(serde_json::from_str::<Value>(&v)?["edges"].as_array().cloned().unwrap_or_default());}
  if table_exists(&tx,"pending_edges")? {let mut stmt=tx.prepare("SELECT data FROM pending_edges")?;let rows=stmt.query_map([],|r|r.get::<_,String>(0))?;for row in rows{edges.push(serde_json::from_str(&row?)?);}}
  if table_exists(&tx,"trash")? {let mut stmt=tx.prepare("SELECT board FROM trash")?;let rows=stmt.query_map([],|r|r.get::<_,String>(0))?;for row in rows{edges.extend(serde_json::from_str::<Value>(&row?)?["edges"].as_array().cloned().unwrap_or_default());}}
  let mut pairs:HashMap<String,Vec<String>>=HashMap::new();let mut seen=HashSet::new();for e in edges {if let (Some(a),Some(b))=(e["from"].as_str(),e["to"].as_str()){let pair=pair_key(a,b)?;let id=serde_json::to_string(&e["id"])?;if seen.insert((pair.clone(),id.clone())) {pairs.entry(pair).or_default().push(id);}}}for (pair,ids) in pairs{if ids.len()>1{tx.execute("INSERT INTO legacy_edge_pairs(pair,ids) VALUES(?,?)",params![pair,serde_json::to_string(&ids)?])?;}}
 }
 let old=table_exists(&tx,"search_chunks")?;let requires=!old||!columns(&tx,"search_chunks")?.iter().any(|c|c=="field");
 if requires {
  tx.execute_batch("CREATE VIRTUAL TABLE search_chunks_next USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content,tokenize='unicode61')")?;
  {let mut stmt=tx.prepare("SELECT search_index.object_id,search_index.title,search_index.body,objects.description FROM search_index JOIN objects ON objects.id=search_index.object_id")?;
   let rows=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?.unwrap_or_default(),r.get::<_,String>(3)?)))?;
   for row in rows {let(id,title,body,desc)=row?;tx.execute("INSERT INTO search_chunks_next VALUES(?,-1,'title',?)",params![id,title])?;for(field,text)in[("body",body),("description",desc)]{for(start,text)in chunks(&text){tx.execute("INSERT INTO search_chunks_next VALUES(?,?,?,?)",params![id,start as i64,field,text])?;}}}
  }
  if old{tx.execute_batch("DROP TABLE search_chunks")?;}tx.execute_batch("ALTER TABLE search_chunks_next RENAME TO search_chunks")?;
 }tx.commit()?;Ok(())
}
pub fn create_workspace(root:&Path)->R<()> {
 if root.exists() {if !root.is_dir() || root.is_symlink() || fs::read_dir(root)?.next().is_some(){return Err("workspace must be a new or empty directory".into())}} else {fs::create_dir(root)?;}
 fs::create_dir(root.join("notes"))?;fs::create_dir(root.join("files"))?;
 let conn=Connection::open(root.join("atlas.sqlite3"))?;
 conn.execute_batch("CREATE TABLE objects (id TEXT PRIMARY KEY,kind TEXT NOT NULL,title TEXT NOT NULL,path TEXT NOT NULL,text_status TEXT,description TEXT NOT NULL DEFAULT '');
 CREATE VIRTUAL TABLE search_index USING fts5(object_id UNINDEXED,title,body,tokenize='unicode61');
 CREATE TABLE board(id INTEGER PRIMARY KEY CHECK(id=1),data TEXT NOT NULL);
 CREATE TABLE trash(id TEXT PRIMARY KEY,deleted_at REAL NOT NULL,board TEXT NOT NULL);
 CREATE TABLE pending_edges(id TEXT PRIMARY KEY,data TEXT NOT NULL);
 CREATE TABLE note_journal(object_id TEXT PRIMARY KEY,title TEXT NOT NULL,text TEXT NOT NULL);
 CREATE TABLE search_history(id INTEGER PRIMARY KEY AUTOINCREMENT,query TEXT NOT NULL,mode TEXT NOT NULL,results TEXT NOT NULL,created_at REAL NOT NULL,object_ids TEXT,scope_known INTEGER NOT NULL DEFAULT 0,answer TEXT,sources TEXT,error TEXT,agentic INTEGER);
 CREATE TABLE legacy_edge_pairs(pair TEXT PRIMARY KEY,ids TEXT NOT NULL);
 CREATE VIRTUAL TABLE search_chunks USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content,tokenize='unicode61');")?;
 fs::write(root.join(".atlas-owned"),"atlas-workspace-v1")?;Ok(())
}
fn file_name(name:&str)->R<()> {if name.is_empty() || name=="." || name==".." || name.contains('/') || name.contains('\\') || name.contains('\0') {return Err("invalid filename".into())}Ok(())}
fn text_info(name:&str,path:&Path)->R<(String,&'static str)>{
 let ext=Path::new(name).extension().and_then(|s|s.to_str()).unwrap_or("").to_lowercase();
 let result:R<String>=match ext.as_str(){
  "txt"|"md"=>fs::read_to_string(path).map_err(|e|e.into()),
  "pdf"=>pdf::extract_pdf_path(path),
  "docx"=>(||{let doc=docx_lite::parse_document_from_path(path)?;let mut text:Vec<String>=doc.paragraphs.iter().map(|p|p.to_text()).collect();for table in &doc.tables{for row in &table.rows{for cell in &row.cells{text.push(cell.paragraphs.iter().map(|p|p.to_text()).collect::<Vec<_>>().join("\n"));}}}Ok(text.join("\n"))})(),
  _=>return Ok((String::new(),"unsupported"))
 };
 let text=if ext=="pdf"{result?}else{result.unwrap_or_default()};let status=if text.trim().is_empty(){"unavailable"}else{"available"};Ok((text,status))
}

fn index_file(tx:&rusqlite::Transaction<'_>,id:&str,title:&str,text:&str,description:&str)->R<()> {
 tx.execute("DELETE FROM search_index WHERE object_id=?",[id])?;tx.execute("INSERT INTO search_index VALUES(?,?,?)",params![id,title,text])?;
 tx.execute("DELETE FROM search_chunks WHERE object_id=?",[id])?;tx.execute("INSERT INTO search_chunks VALUES(?,-1,'title',?)",params![id,title])?;
 for(field,value) in [("body",text),("description",description)]{for(start,part) in chunks(value){tx.execute("INSERT INTO search_chunks VALUES(?,?,?,?)",params![id,start as i64,field,part])?;}}Ok(())
}
fn file_db(root:&Path)->R<Connection>{if root.is_symlink(){return Err("invalid workspace".into())}let absolute=if root.is_absolute(){Some(root.canonicalize()?)}else{None};let root=absolute.as_deref().unwrap_or(root);let db=root.join("atlas.sqlite3");if !db.is_file(){return Err("missing db".into())}Ok(Connection::open_with_flags(db,rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW)?)}
pub fn validate_database_files(root:&Path)->R<()> {for name in ["atlas.sqlite3","atlas.sqlite3-wal","atlas.sqlite3-shm","atlas.sqlite3-journal"]{match fs::symlink_metadata(root.join(name)){Ok(meta) if meta.file_type().is_symlink()||!meta.is_file()=>return Err("unsafe database file".into()),Ok(_)=>{},Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(e.into())}}Ok(())}
/// Exclusive session ownership shared with protection conversion. Never waits on another process.
pub fn workspace_session_lock(root:&Path)->R<fs::File>{use std::os::unix::fs::OpenOptionsExt;if root.is_symlink()||!root.is_dir(){return Err("invalid workspace".into())}let file=fs::OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(root.join(".atlas-session.lock"))?;if !file.metadata()?.is_file(){return Err("invalid session lock".into())}file.try_lock().map_err(|_|"workspace session is already active")?;Ok(file)}
/// Callers serialize every operation on this workspace, including journal recovery.
/// This is advisory: older Atlas backends do not honor it.
pub fn workspace_lock(root:&Path)->R<fs::File>{
 use std::os::unix::fs::OpenOptionsExt;
 if root.is_symlink() || !root.is_dir(){return Err("invalid workspace".into())}
 let file=fs::OpenOptions::new().read(true).write(true).create(true).mode(0o600)
     .custom_flags(libc::O_NOFOLLOW).open(root.join(".atlas-operation.lock"))?;
 if !file.metadata()?.is_file(){return Err("invalid workspace lock".into())}
 file.lock()?;Ok(file)
}
pub fn add_file(root:&Path, name:&str,bytes:&[u8])->R<String>{add_file_stream(root,name,&mut std::io::Cursor::new(bytes))}
pub fn validate_file_rename(previous:&str,name:&str)->R<()> {
 file_name(name)?;if name.trim()!=name||name.trim().is_empty()||name.chars().count()>255{return Err("invalid filename".into())}
 if Path::new(previous).extension()!=Path::new(name).extension(){return Err("file extension cannot be changed".into())}Ok(())
}
/// Change the displayed name without rereading or rewriting the original file.
pub fn rename_file(root:&Path,id:&str,name:&str)->R<()> {
 let mut conn=file_db(root)?;let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 let previous:String=tx.query_row("SELECT title FROM objects WHERE id=? AND kind='file' AND id NOT IN (SELECT id FROM trash)",[id],|r|r.get(0))?;
 validate_file_rename(&previous,name)?;
 if previous!=name {
  tx.execute("UPDATE objects SET title=? WHERE id=?",params![name,id])?;
  if tx.execute("UPDATE search_index SET title=? WHERE object_id=?",params![name,id])?!=1{return Err("file search index missing".into())}
  if tx.execute("UPDATE search_chunks SET content=? WHERE object_id=? AND field='title'",params![name,id])?!=1{return Err("file title index missing".into())}
 }
 tx.commit()?;Ok(())
}
pub fn add_file_stream(root:&Path, name:&str,reader:&mut impl std::io::Read)->R<String>{
 file_name(name)?;let files=root.join("files");if files.is_symlink() || !files.canonicalize()?.starts_with(root.canonicalize()?){return Err("unsafe files directory".into())}
 let id=uuid::Uuid::new_v4().to_string();let dir=files.join(&id);fs::create_dir(&dir)?;let target=dir.join(name);
 let result=(||->R<()>{let mut f=fs::OpenOptions::new().create_new(true).write(true).open(&target)?;std::io::copy(reader,&mut f)?;f.sync_all()?;drop(f);let (text,status)=text_info(name,&target)?;let mut conn=file_db(root)?;let tx=conn.transaction()?;let relative=format!("files/{id}/{name}");tx.execute("INSERT INTO objects(id,kind,title,path,text_status) VALUES(?,'file',?,?,?)",params![id,name,relative,status])?;index_file(&tx,&id,name,&text,"")?;tx.commit()?;Ok(())})();
 if let Err(e)=result{let _=fs::remove_file(&target);let _=fs::remove_dir(&dir);return Err(e)}Ok(id)
}
pub fn set_file_description(root:&Path,id:&str,description:&str)->R<()>{let mut conn=file_db(root)?;let tx=conn.transaction()?;let (title,body):(String,String)=tx.query_row("SELECT objects.title,COALESCE(search_index.body,'') FROM objects JOIN search_index ON search_index.object_id=objects.id WHERE objects.id=? AND objects.kind='file' AND objects.id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;tx.execute("UPDATE objects SET description=? WHERE id=?",params![description,id])?;index_file(&tx,id,&title,&body,description)?;tx.commit()?;Ok(())}
pub fn replace_file(root:&Path,id:&str,name:&str,bytes:&[u8])->R<()>{replace_file_stream(root,id,name,&mut std::io::Cursor::new(bytes))}
pub fn replace_file_stream(root:&Path,id:&str,name:&str,reader:&mut impl std::io::Read)->R<()>{
 file_name(name)?;let mut conn=file_db(root)?;let old_path:String=conn.query_row("SELECT path FROM objects WHERE id=? AND kind='file' AND id NOT IN (SELECT id FROM trash)",[id],|r|r.get(0))?;
 let original=safe_path(root,&old_path)?;let files_root=safe_path(root,&format!("files/{id}"))?;if !original.starts_with(&files_root){return Err("invalid file path".into())}
 let revision=uuid::Uuid::new_v4().to_string();let dir=files_root.join(&revision);fs::create_dir(&dir)?;let target=dir.join(name);
 let result=(||->R<()>{let mut file=fs::OpenOptions::new().create_new(true).write(true).open(&target)?;std::io::copy(reader,&mut file)?;file.sync_all()?;drop(file);let(text,status)=text_info(name,&target)?;let relative=format!("files/{id}/{revision}/{name}");
 let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let (current,description):(String,String)=tx.query_row("SELECT path,description FROM objects WHERE id=? AND kind='file' AND id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;if current!=old_path{return Err("file changed during replacement".into())}
 tx.execute("UPDATE objects SET title=?,path=?,text_status=? WHERE id=?",params![name,relative,status,id])?;index_file(&tx,id,name,&text,&description)?;tx.commit()?;Ok(())})();
 if let Err(e)=result{let _=fs::remove_file(&target);let _=fs::remove_dir(&dir);return Err(e)}
 fs::remove_file(original)?;Ok(())
}

fn board_from_db(conn:&Connection)->R<Value>{
   let saved:Option<String>=conn.query_row("SELECT data FROM board WHERE id=1",[],|r|r.get(0)).optional()?;
   let mut b:Value=serde_json::from_str(saved.as_deref().unwrap_or("{\"objects\":[],\"edges\":[],\"stacks\":[]}"))?;
   let positions:HashMap<String,Value>=b["objects"].as_array().ok_or("invalid board")?.iter().filter_map(|o|o["id"].as_str().map(|id|(id.to_owned(),o.clone()))).collect();
   let mut stmt=conn.prepare("SELECT id,title,kind,text_status FROM objects WHERE id NOT IN (SELECT id FROM trash) ORDER BY rowid")?;
   let objects=stmt.query_map([],|r|{let id:String=r.get(0)?;let p=positions.get(&id);Ok(json!({"id":id,"title":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"text_status":r.get::<_,Option<String>>(3)?,"x":p.and_then(|v|v.get("x")).cloned().unwrap_or(json!(0)),"y":p.and_then(|v|v.get("y")).cloned().unwrap_or(json!(0)),"imageExpanded":p.and_then(|v|v.get("imageExpanded")).and_then(Value::as_bool).unwrap_or(false),"imageWidth":p.and_then(|v|v.get("imageWidth")).and_then(Value::as_f64).filter(|w|w.is_finite()&&*w>=230.0&&*w<=920.0),"searchExcluded":p.and_then(|v|v.get("searchExcluded")).and_then(Value::as_bool).unwrap_or(false)}))})?.collect::<Result<Vec<_>,_>>()?;
   let ids:HashSet<&str>=objects.iter().filter_map(|o|o["id"].as_str()).collect();
   let edges:Vec<Value>=b["edges"].as_array().ok_or("invalid edges")?.iter().filter(|e|e["from"].as_str().is_some_and(|s|ids.contains(s))&&e["to"].as_str().is_some_and(|s|ids.contains(s))).cloned().collect();
   let stacks:Vec<Value>=b["stacks"].as_array().ok_or("invalid stacks")?.iter().map(|s|{let mut s=s.clone();let members=s["ids"].as_array().map(|v|v.iter().filter(|x|x.as_str().is_some_and(|id|ids.contains(id))).cloned().collect::<Vec<_>>()).unwrap_or_default();s["ids"]=json!(members);s}).collect();
   b["objects"]=json!(objects);b["edges"]=json!(edges);b["stacks"]=json!(stacks);Ok(b)
}
/// Resolve the default search scope before limiting search results. Explicit scopes bypass exclusions.
pub fn search_scope(root:&Path,ids:Option<Vec<String>>)->R<Option<Vec<String>>>{
 if ids.is_some(){return Ok(ids)}
 let conn=Connection::open(root.join("atlas.sqlite3"))?;
 let board=board_from_db(&conn)?;
 let objects=board["objects"].as_array().ok_or("invalid board")?;if !objects.iter().any(|o|o["searchExcluded"]==true){return Ok(None)}
 Ok(Some(objects.iter().filter(|o|o["searchExcluded"]!=true).filter_map(|o|o["id"].as_str().map(str::to_owned)).collect()))
}
pub fn search_visibility(root:&Path,id:&str,excluded:Option<bool>)->R<bool>{
 let mut conn=Connection::open(root.join("atlas.sqlite3"))?;
 let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 let mut board=board_from_db(&tx)?;
 let object=board["objects"].as_array_mut().ok_or("invalid board")?.iter_mut().find(|o|o["id"]==id).ok_or("object missing")?;
 let current=object["searchExcluded"].as_bool().unwrap_or(false);
 if let Some(value)=excluded {object["searchExcluded"]=json!(value);tx.execute("INSERT OR REPLACE INTO board(id,data) VALUES(1,?)",[serde_json::to_string(&board)?])?;}
 tx.commit()?;Ok(excluded.unwrap_or(current))
}
fn revision(conn:&Connection,root:&Path,id:&str,only_file:bool)->R<String>{
 use sha2::{Digest,Sha256};use std::io::Read;use std::os::unix::fs::MetadataExt;
 let (kind,title,path,description):(String,String,String,String)=conn.query_row("SELECT kind,title,path,description FROM objects WHERE id=? AND id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;if only_file && kind!="file"{return Err("not a file".into())}
 let fields=[id,kind.as_str(),title.as_str(),description.as_str()];let prefix=format!("[{}]",fields.iter().map(|v|serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join(", "));let target=safe_path(root,&path)?;let mut file=fs::File::open(&target)?;let before=file.metadata()?;let mut hash=Sha256::new();hash.update(prefix.as_bytes());let mut buffer=[0u8;65536];loop{let n=file.read(&mut buffer)?;if n==0{break}hash.update(&buffer[..n]);}let after=file.metadata()?;let current=fs::metadata(&target)?;let identity=|m:&fs::Metadata|(m.dev(),m.ino(),m.len(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec());if identity(&before)!=identity(&after)||identity(&after)!=identity(&current){return Err("file changed during verification".into())}Ok(format!("{:x}",hash.finalize()))
}
pub fn object_revision(root:&Path,id:&str)->R<String>{let conn=file_db(root)?;revision(&conn,root,id,false)}
pub fn file_revision(root:&Path,id:&str)->R<String>{let mut conn=file_db(root)?;let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let result=revision(&tx,root,id,true)?;tx.commit()?;Ok(result)}
pub fn undo_created(root:&Path,id:&str,expected:&str)->R<()> {
 let mut conn=file_db(root)?;
 let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 let board=board_from_db(&tx)?;
 let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM note_journal WHERE object_id=?)",[id],|r|r.get(0))?;
 if pending || revision(&tx,root,id,false)? != expected || board["edges"].as_array().ok_or("invalid edges")?.iter().any(|e|e["from"]==id||e["to"]==id) || board["stacks"].as_array().ok_or("invalid stacks")?.iter().any(|s|s["ids"].as_array().is_some_and(|v|v.iter().any(|x|x==id))) {return Err("object changed".into())}
 let objects:Vec<Value>=board["objects"].as_array().ok_or("invalid board")?.iter().filter(|o|o["id"]==id).cloned().collect();
 if objects.len()!=1{return Err("object missing".into())}
 let saved=json!({"objects":objects,"edges":[],"stacks":[]});
 let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs_f64();
 tx.execute("INSERT INTO trash VALUES(?,?,?)",params![id,now,serde_json::to_string(&saved)?])?;
 tx.commit()?;Ok(())
}
pub fn copy_file_from(dest:&Path,source:&Path,id:&str,expected:&str)->R<String>{
 if source.is_symlink() || source.join("atlas.sqlite3").is_symlink() || source.join("files").is_symlink(){return Err("invalid source".into())}
 let conn=Connection::open_with_flags(source.join("atlas.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
 let has_description={let mut stmt=conn.prepare("PRAGMA table_info(objects)")?;let cols=stmt.query_map([],|r|r.get::<_,String>(1))?.collect::<Result<Vec<_>,_>>()?;cols.iter().any(|c|c=="description")};
 let query=if has_description{"SELECT title,path,COALESCE(description,'') FROM objects WHERE id=? AND kind='file' AND id NOT IN (SELECT id FROM trash)"}else{"SELECT title,path,'' FROM objects WHERE id=? AND kind='file' AND id NOT IN (SELECT id FROM trash)"};
 let (title,path,description):(String,String,String)=conn.query_row(query,[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
 let input=safe_path(source,&path)?;let base=safe_path(source,"files")?;
 if !input.starts_with(&base)||!input.is_file(){return Err("invalid source file".into())}
 let mut reader=fs::File::open(&input)?;
 use std::os::unix::fs::MetadataExt;
 let identity=|m:&fs::Metadata|(m.dev(),m.ino(),m.len(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec());
 let before=reader.metadata()?;
 let files=safe_path(dest,"files")?;let new_id=uuid::Uuid::new_v4().to_string();let dir=files.join(&new_id);fs::create_dir(&dir)?;
 let name=input.file_name().and_then(|n|n.to_str()).ok_or("invalid name")?;let target=dir.join(name);
 let result=(||->R<()>{
 let mut output=fs::OpenOptions::new().write(true).create_new(true).open(&target)?;std::io::copy(&mut reader,&mut output)?;output.sync_all()?;drop(output);
 let current:(String,String,String)=conn.query_row(query,[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
 if current!=(title.clone(),path.clone(),description.clone())||safe_path(source,&path)?!=input||identity(&before)!=identity(&reader.metadata()?)||identity(&before)!=identity(&fs::metadata(&input)?){return Err("source changed".into())}
 // Verify the bytes actually copied against the source revision, not just the final source.
 use sha2::{Digest,Sha256};use std::io::Read;
 let prefix=format!("[{}]",[id,"file",&title,&description].iter().map(|v|serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join(", "));
 let mut hash=Sha256::new();hash.update(prefix.as_bytes());let mut copied=fs::File::open(&target)?;let mut buf=[0u8;65536];loop{let n=copied.read(&mut buf)?;if n==0{break}hash.update(&buf[..n]);}if format!("{:x}",hash.finalize())!=expected{return Err("source changed".into())}
 let (text,status)=text_info(name,&target)?;let relative=format!("files/{new_id}/{name}");
 let mut db=file_db(dest)?;let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 tx.execute("INSERT INTO objects(id,kind,title,path,text_status,description) VALUES(?,'file',?,?,?,?)",params![new_id,title,relative,status,description])?;
 index_file(&tx,&new_id,&title,&text,&description)?;tx.commit()?;Ok(())
 })();
 if let Err(e)=result{let _=fs::remove_file(&target);let _=fs::remove_dir(&dir);return Err(e)}Ok(new_id)
}
fn validate_ids(ids:&[String])->R<()> {let mut seen=HashSet::new();if ids.is_empty()||ids.iter().any(|id|id.is_empty()||!seen.insert(id)){return Err("invalid ids".into())}Ok(())}
pub fn delete_objects(root:&Path,ids:&[String])->R<Value>{validate_ids(ids)?;let mut conn=file_db(root)?;let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let board=board_from_db(&tx)?;
 for id in ids{if !board["objects"].as_array().unwrap().iter().any(|x|x["id"]==*id){return Err("object missing".into())}let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM note_journal WHERE object_id=?)",[id],|r|r.get(0))?;if pending{return Err("object being saved".into())}}
 for id in ids{let objects:Vec<Value>=board["objects"].as_array().unwrap().iter().filter(|x|x["id"]==*id).cloned().collect();let edges:Vec<Value>=board["edges"].as_array().unwrap().iter().filter(|x|x["from"]==*id||x["to"]==*id).cloned().collect();let stacks:Vec<Value>=board["stacks"].as_array().unwrap().iter().filter(|x|x["ids"].as_array().is_some_and(|a|a.iter().any(|v|v==id))).cloned().collect();let saved=json!({"objects":objects,"edges":edges,"stacks":stacks});let time=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs_f64();tx.execute("INSERT INTO trash VALUES(?,?,?)",params![id,time,serde_json::to_string(&saved)?])?;for edge in edges{let edge_id=edge["id"].as_str().ok_or("bad edge id")?;tx.execute("INSERT OR IGNORE INTO pending_edges VALUES(?,?)",params![edge_id,serde_json::to_string(&edge)?])?;}}
 tx.commit()?;board_from_db(&conn)
}
pub fn restore_objects(root:&Path,ids:&[String])->R<Value>{validate_ids(ids)?;let mut conn=file_db(root)?;let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 let mut saved=vec![];for id in ids{let raw:String=tx.query_row("SELECT board FROM trash WHERE id=?",[id],|r|r.get(0))?;saved.push((id,serde_json::from_str::<Value>(&raw)?));}for id in ids{tx.execute("DELETE FROM trash WHERE id=?",[id])?;}
 let raw:Option<String>=tx.query_row("SELECT data FROM board WHERE id=1",[],|r|r.get(0)).optional()?;let mut board:Value=serde_json::from_str(raw.as_deref().unwrap_or("{\"objects\":[],\"edges\":[],\"stacks\":[]}"))?;
 for (id,s) in &saved{let objects=board["objects"].as_array_mut().ok_or("invalid board")?;objects.retain(|o|o["id"]!=**id);objects.extend(s["objects"].as_array().ok_or("invalid trash")?.iter().cloned());}
 let active:HashSet<String>={let mut stmt=tx.prepare("SELECT id FROM objects WHERE id NOT IN (SELECT id FROM trash)")?;let rows=stmt.query_map([],|r|r.get(0))?;rows.collect::<Result<_,_>>()?};
 let pending={let mut stmt=tx.prepare("SELECT id,data FROM pending_edges")?;let rows=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?;rows.collect::<Result<Vec<_>,_>>()?};
 for (id,data) in pending{let edge:Value=serde_json::from_str(&data)?;if edge["from"].as_str().is_some_and(|x|active.contains(x))&&edge["to"].as_str().is_some_and(|x|active.contains(x)){let edges=board["edges"].as_array_mut().ok_or("invalid edges")?;if !edges.iter().any(|e|e["id"]==edge["id"]){edges.push(edge)}tx.execute("DELETE FROM pending_edges WHERE id=?",[id])?;}}
 for(id,s) in saved{for stack in s["stacks"].as_array().ok_or("invalid stacks")?{if let Some(existing)=board["stacks"].as_array_mut().ok_or("invalid stacks")?.iter_mut().find(|v|v["id"]==stack["id"]){let members=existing["ids"].as_array_mut().ok_or("invalid members")?;if !members.iter().any(|v|v==id){members.push(json!(id))}}}}
 validate_edges(&tx,board["edges"].as_array().ok_or("invalid edges")?)?;
 tx.execute("INSERT OR REPLACE INTO board VALUES(1,?)",[serde_json::to_string(&board)?])?;if table_exists(&tx,"note_drafts")?{for id in ids{tx.execute("INSERT OR REPLACE INTO note_journal(object_id,title,text) SELECT object_id,title,text FROM note_drafts WHERE object_id=?",[id])?;}}tx.commit()?;recover(&mut conn,root)?;board_from_db(&conn)
}
pub fn list_trash(root:&Path)->R<Value>{let conn=file_db(root)?;let mut stmt=conn.prepare("SELECT objects.id,title,kind FROM objects JOIN trash ON objects.id=trash.id ORDER BY deleted_at DESC")?;let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?})))?;Ok(json!(rows.collect::<Result<Vec<_>,_>>()?))}
pub fn purge_trash(root:&Path)->R<()> {let cutoff=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs_f64()-86400.0;purge_trash_before(root,cutoff)}
pub fn purge_trash_before(root:&Path,cutoff:f64)->R<()> {let expired=purge_trash_rows_before(root,cutoff)?;for (id,kind,path) in expired{if kind=="file"{let folder=safe_path(root,&format!("files/{id}"))?;if folder.exists(){fs::remove_dir_all(folder)?;}}else{let target=safe_path(root,&path)?;if target.exists(){fs::remove_file(target)?;}}}Ok(())}
pub fn purge_trash_rows_before(root:&Path,cutoff:f64)->R<Vec<(String,String,String)>> {purge_trash_rows_before_inner(root,cutoff,None)}
// Preserve the index revision proof in the same transaction as expired-row removal.
pub fn purge_trash_rows_before_with_proof(root:&Path,cutoff:f64,id:&str,before:&str,counter:i64)->R<Vec<(String,String,String)>> {purge_trash_rows_before_inner(root,cutoff,Some((id,before,counter)))}
fn purge_trash_rows_before_inner(root:&Path,cutoff:f64,proof:Option<(&str,&str,i64)>)->R<Vec<(String,String,String)>> {let mut conn=file_db(root)?;let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 if let Some((_,_,counter))=proof {let current:i64=tx.query_row("SELECT revision FROM atlas_content_revision WHERE id=1",[],|r|r.get(0))?;if current!=counter{return Err("workspace changed during cleanup".into())}}
 let expired={let mut stmt=tx.prepare("SELECT objects.id,objects.kind,objects.path FROM objects JOIN trash ON objects.id=trash.id WHERE trash.deleted_at <= ?")?;let rows=stmt.query_map([cutoff],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?;rows.collect::<Result<Vec<_>,_>>()?};
 #[cfg(any(feature="test-hook",feature="owner-leases"))] let has_rescue=table_exists(&tx,"note_rescue_drafts")?;
 let has_drafts=table_exists(&tx,"note_drafts")?;for(id,_,_) in &expired{#[cfg(any(feature="test-hook",feature="owner-leases"))] if has_rescue{tx.execute("DELETE FROM note_rescue_drafts WHERE object_id=?",[id])?;}if has_drafts{tx.execute("DELETE FROM note_drafts WHERE object_id=?",[id])?;}for table in ["search_index","search_chunks"]{tx.execute(&format!("DELETE FROM {table} WHERE object_id=?"),[id])?;}tx.execute("DELETE FROM objects WHERE id=?",[id])?;tx.execute("DELETE FROM trash WHERE id=?",[id])?;}
 if let Some((id,before,_))=proof {tx.execute_batch("CREATE TABLE IF NOT EXISTS atlas_purge_proof(id TEXT PRIMARY KEY,before_revision TEXT NOT NULL,after_counter INTEGER NOT NULL)")?;let after:i64=tx.query_row("SELECT revision FROM atlas_content_revision WHERE id=1",[],|r|r.get(0))?;tx.execute("DELETE FROM atlas_purge_proof WHERE id<>?",[id])?;tx.execute("INSERT OR IGNORE INTO atlas_purge_proof(id,before_revision,after_counter) VALUES(?1,?2,?3)",rusqlite::params![id,before,after])?;}
 tx.commit()?;
 #[cfg(feature="test-hook")] if let Some(path)=std::env::var_os("ATLAS_TEST_NATIVE_PURGE_BARRIER"){let dir=Path::new(&path);fs::write(dir.join("ready"),b"ready")?;let started=std::time::Instant::now();while !dir.join("go").exists()&&started.elapsed()<std::time::Duration::from_secs(30){std::thread::sleep(std::time::Duration::from_millis(10));}}
 Ok(expired)}
fn validate_edges(tx:&Connection,edges:&[Value])->R<()> {
let mut pairs:HashMap<String,Vec<String>>=HashMap::new();for edge in edges{let f=edge["from"].as_str().ok_or("invalid edge")?;let t=edge["to"].as_str().ok_or("invalid edge")?;for field in ["arrowStart","arrowEnd"]{if edge.get(field).is_some_and(|x|!x.is_boolean()){return Err("invalid arrow".into())}} let pair=pair_key(f,t)?;pairs.entry(pair).or_default().push(serde_json::to_string(&edge["id"])?);}
 for (pair,ids) in pairs{if ids.len()<2{continue}let permitted:Option<String>=tx.query_row("SELECT ids FROM legacy_edge_pairs WHERE pair=?",[pair],|r|r.get(0)).optional()?;let mut allowed:Vec<String>=serde_json::from_str(permitted.as_deref().unwrap_or("[]"))?;for id in ids{if let Some(i)=allowed.iter().position(|v|v==&id){allowed.remove(i);}else{return Err("duplicate edge".into())}}}
 Ok(())
}
pub fn execute(a:&[String])->R<Value>{
 if a.len()<3{return Err("missing args".into())}let root=Path::new(&a[2]);if a[1]=="create-workspace" { create_workspace(root)?; return Ok(json!({"ok":true})); } if root.is_symlink(){return Err("invalid workspace".into())}let absolute=if root.is_absolute(){Some(root.canonicalize()?)}else{None};let root=absolute.as_deref().unwrap_or(root);let db=root.join("atlas.sqlite3");if !db.is_file(){return Err("missing db".into())}
 let mut conn=Connection::open_with_flags(db,rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW)?;migrate(&mut conn)?;recover(&mut conn,root)?;
 match a[1].as_str(){
 "delete-objects"=>{let ids:Vec<String>=serde_json::from_str(a.get(3).ok_or("missing ids")?)?;return delete_objects(root,&ids)},
 "restore-objects"=>{let ids:Vec<String>=serde_json::from_str(a.get(3).ok_or("missing ids")?)?;return restore_objects(root,&ids)},
 "list-trash"=>return list_trash(root),
 "purge-trash"=>{purge_trash(root)?;return Ok(json!({"ok":true}))},
 "create-note"=>{let title=a.get(3).ok_or("missing title")?;let text=a.get(4).ok_or("missing text")?;let id=uuid::Uuid::new_v4().to_string();let path=format!("notes/{id}.md");let target=safe_path(root,&path)?;write_atomic(&target,text)?;
 let result=(||->R<()>{let tx=conn.transaction()?;tx.execute("INSERT INTO objects(id,kind,title,path) VALUES(?,'note',?,?)",params![id,title,path])?;tx.execute("INSERT INTO search_index(object_id,title,body) VALUES(?,?,?)",params![id,title,text])?;tx.execute("INSERT INTO search_chunks(object_id,start,field,content) VALUES(?,-1,'title',?)",params![id,title])?;for (start,part) in chunks(text){tx.execute("INSERT INTO search_chunks(object_id,start,field,content) VALUES(?,?,'body',?)",params![id,start as i64,part])?;}tx.commit()?;Ok(())})();if let Err(e)=result{fs::remove_file(target)?;return Err(e)}return Ok(json!({"id":id}));},
 "put-board"=>{let mut board:Value=serde_json::from_str(a.get(3).ok_or("missing board")?)?;for key in ["objects","edges","stacks"] {if !board[key].is_array(){return Err("invalid board".into())}}
 let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;validate_edges(&tx,board["edges"].as_array().unwrap())?;
 let previous=board_from_db(&tx)?;for object in board["objects"].as_array_mut().unwrap(){if let Some(old)=previous["objects"].as_array().and_then(|a|a.iter().find(|v|v["id"]==object["id"])){object["searchExcluded"]=old["searchExcluded"].clone();}else{object["searchExcluded"]=json!(false);}}
 tx.execute("INSERT OR REPLACE INTO board(id,data) VALUES(1,?)",[serde_json::to_string(&board)?])?;tx.commit()?;return Ok(json!({"ok":true}));},
 #[cfg(any(feature="test-hook",feature="owner-leases"))]
 "board-rescue"=>{return board_rescue(root,a.get(3).ok_or("action")?,a.get(4).ok_or("hash")?,a.get(5).ok_or("board")?);},
 #[cfg(any(feature="test-hook",feature="owner-leases"))]
 "register-rescue-session"=>{register_rescue_session(root,a.get(3).ok_or("id")?,a.get(4).ok_or("session")?)?;return Ok(json!({"ok":true}));},
 #[cfg(any(feature="test-hook",feature="owner-leases"))]
 "save-rescue-draft"=>{save_rescue_draft(root,a.get(3).ok_or("id")?,a.get(4).ok_or("session")?,a.get(5).ok_or("title")?,a.get(6).ok_or("text")?)?;return Ok(json!({"ok":true}));},
 #[cfg(any(feature="test-hook",feature="owner-leases"))]
 "delete-rescue-draft"=>{let removed=delete_rescue_draft(root,a.get(3).ok_or("id")?,a.get(4).ok_or("hash")?,a.get(5).ok_or("title")?,a.get(6).ok_or("text")?)?;return Ok(json!({"deleted":removed}));},
 #[cfg(any(feature="test-hook",feature="owner-leases"))]
 "clear-empty-rescue-session"=>{let removed=clear_empty_rescue_session(root,a.get(3).ok_or("id")?,a.get(4).ok_or("hash")?)?;return Ok(json!({"cleared":removed}));},
 #[cfg(any(feature="test-hook",feature="owner-leases"))]
 "list-rescue-drafts"=>{let drafts=list_rescue_drafts(root,a.get(3).ok_or("id")?)?;return Ok(json!({"drafts":drafts.into_iter().map(|(id,title,text)|json!({"id":id,"title":title,"text":text})).collect::<Vec<_>>() }));},
 #[cfg(any(feature="test-hook",feature="owner-leases"))]
 "get-rescue-draft"=>{let draft=get_rescue_draft(root,a.get(3).ok_or("id")?,a.get(4).ok_or("session")?)?;return Ok(json!({"draft":draft.map(|(title,text)|json!({"title":title,"text":text}))}));},
 "recover-note-drafts"=>{return Ok(json!({"count":recover_note_drafts(root)?}));},
 "save-note-draft"=>{let id=a.get(3).ok_or("missing id")?;let title=a.get(4).ok_or("missing title")?;let text=a.get(5).ok_or("missing text")?;save_note_draft(root,id,title,text)?;return Ok(json!({"ok":true}));},
 "get-note-draft"=>{let id=a.get(3).ok_or("missing id")?;return Ok(json!({"draft":get_note_draft(root,id)?.map(|(title,text)|json!({"title":title,"text":text}))}));},
 "clear-note-draft"=>{let id=a.get(3).ok_or("missing id")?;let title=a.get(4).ok_or("missing title")?;let text=a.get(5).ok_or("missing text")?;return Ok(json!({"cleared":clear_note_draft(root,id,title,text)?}));},
 "get-note"=>{let id=a.get(3).ok_or("missing id")?;let (title,path):(String,String)=conn.query_row("SELECT title,path FROM objects WHERE id=? AND kind='note' AND id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;let text=fs::read_to_string(safe_path(root,&path)?)?;return Ok(json!({"id":id,"title":title,"text":text}));},
 "agent-read-page"=>{let id=a.get(3).ok_or("missing id")?;let offset:usize=a.get(4).ok_or("missing offset")?.parse()?;let limit:usize=a.get(5).ok_or("missing limit")?.parse()?;if limit==0||limit>10000||offset>100000000{return Err("invalid range".into())}let (title,kind,path,status):(String,String,String,Option<String>)=conn.query_row("SELECT title,kind,path,text_status FROM objects WHERE id=? AND id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;let (text,total)=if kind=="note"{let body=fs::read_to_string(safe_path(root,&path)?)?;let total=body.chars().count();(body.chars().skip(offset).take(limit).collect::<String>(),total)}else if kind=="file"&&status.as_deref()==Some("available"){conn.query_row("SELECT substr(body,?2,?3), length(body) FROM search_index WHERE object_id=?1",params![id,offset as i64+1,limit as i64],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)? as usize)))?}else{return Err("Текст объекта недоступен для чтения".into())};let next=offset.saturating_add(text.chars().count());return Ok(json!({"id":id,"title":title,"text":text,"offset":offset,"next_offset":if next<total{Some(next)}else{None},"total":total}));},
 "rename-file"=>{let id=a.get(3).ok_or("missing id")?;let title=a.get(4).ok_or("missing title")?;drop(conn);rename_file(root,id,title)?;return Ok(json!({"id":id,"title":title}));},
 "update-note"=>{let id=a.get(3).ok_or("missing id")?;let title=a.get(4).ok_or("missing title")?;let text=a.get(5).ok_or("missing text")?; let path:String=conn.query_row("SELECT path FROM objects WHERE id=? AND kind='note' AND id NOT IN (SELECT id FROM trash)",[id],|r|r.get(0))?;let target=safe_path(root,&path)?;let previous=fs::read_to_string(&target)?;conn.execute("INSERT INTO note_journal(object_id,title,text) VALUES(?,?,?)",params![id,title,text])?;if let Err(error)=recover(&mut conn,root){write_atomic(&target,&previous)?;conn.execute("DELETE FROM note_journal WHERE object_id=?",[id])?;return Err(error)};return Ok(json!({"ok":true}));},
 "get-board"=>{
return board_from_db(&conn);
 },_=>return Err("unknown action".into())}
}
pub fn run()->R<()>{let args:Vec<String>=env::args().collect();println!("{}",execute(&args)?);Ok(())}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn streaming_chunks_match_previous_algorithm() {
  fn previous(s:&str)->Vec<(usize,String)>{let a:Vec<char>=s.chars().collect();let mut out=vec![];let mut start=0;while start<a.len(){let mut end=(start+400).min(a.len());let word=|c:char|c.is_alphanumeric()||c=='_';if end<a.len()&&word(a[end-1])&&word(a[end]){let mut i=end;while i>start&&word(a[i-1]){i-=1}if i>start{end=i}else{while end<a.len()&&word(a[end]){end+=1}}}out.push((start,a[start..end].iter().collect()));start=end;}out}
  let cases=vec![String::new(),"я".repeat(399),"я".repeat(400),"я".repeat(401),"слово_тест💻 ".repeat(100),format!("{} {} {}", "x".repeat(399),"я".repeat(1200),"конец"),format!("{} aя", "x".repeat(398))];
  for input in cases {let expected=previous(&input);let actual:Vec<_>=chunks(&input).map(|(offset,text)|(offset,text.to_owned())).collect();assert_eq!(actual,expected,"input length {}",input.len());}
 }

 #[test] fn pair_key_preserves_commas_in_ids() { assert_eq!(pair_key("x,y", "z").unwrap(), "[\"x,y\", \"z\"]"); }
 #[test] fn stale_temporary_file_does_not_block_write() {
  let dir=std::env::temp_dir().join(format!("atlas-stale-test-{}",std::process::id()));fs::create_dir_all(&dir).unwrap();
  let target=dir.join("n.md");let stale=target.with_extension(format!("atlas-recover-{}-0",std::process::id()));fs::write(&stale,"previous crash").unwrap();
  write_atomic(&target,"new text").unwrap();assert_eq!(fs::read_to_string(&target).unwrap(),"new text");assert_eq!(fs::read_to_string(&stale).unwrap(),"previous crash");fs::remove_dir_all(dir).unwrap();
 }
}

/// A draft is durable in the workspace but does not update either search index.
pub fn save_note_draft(root:&Path,id:&str,title:&str,text:&str)->R<()> {
 let mut db=file_db(root)?;let tx=db.transaction()?;
 let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM objects WHERE id=? AND kind='note' AND id NOT IN (SELECT id FROM trash))",[id],|r|r.get(0))?;
 if !exists{return Err("note not found".into())}
 tx.execute_batch("CREATE TABLE IF NOT EXISTS note_drafts(object_id TEXT PRIMARY KEY,title TEXT NOT NULL,text TEXT NOT NULL)")?;
 tx.execute("INSERT OR REPLACE INTO note_drafts VALUES(?,?,?)",params![id,title,text])?;tx.commit()?;Ok(())
}
pub fn get_note_draft(root:&Path,id:&str)->R<Option<(String,String)>> {
 let db=file_db(root)?;if !table_exists(&db,"note_drafts")?{return Ok(None)}
 Ok(db.query_row("SELECT title,text FROM note_drafts WHERE object_id=?",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?)
}
pub fn clear_note_draft(root:&Path,id:&str,title:&str,text:&str)->R<bool> {
 let db=file_db(root)?;if !table_exists(&db,"note_drafts")?{return Ok(false)}
 Ok(db.execute("DELETE FROM note_drafts WHERE object_id=? AND title=? AND text=?",params![id,title,text])?>0)
}
pub fn list_note_drafts(root:&Path)->R<Vec<(String,String,String)>>{let db=file_db(root)?;if !table_exists(&db,"note_drafts")?{return Ok(vec![])}let mut s=db.prepare("SELECT note_drafts.object_id,note_drafts.title,note_drafts.text FROM note_drafts JOIN objects ON objects.id=note_drafts.object_id WHERE objects.kind='note' AND objects.id NOT IN (SELECT id FROM trash) ORDER BY objects.id")?;let rows=s.query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;Ok(rows.collect::<Result<Vec<_>,_>>()?)}
pub fn recover_note_drafts(root:&Path)->R<usize>{let drafts=list_note_drafts(root)?;let mut count=0;for (id,title,text) in drafts{execute(&["atlas".into(),"update-note".into(),root.to_string_lossy().into_owned(),id.clone(),title.clone(),text.clone()])?;clear_note_draft(root,&id,&title,&text)?;count+=1;}Ok(count)}

pub fn has_note_drafts(root:&Path)->R<bool>{let db=file_db(root)?;if !table_exists(&db,"note_drafts")?{return Ok(false)}Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM note_drafts JOIN objects ON objects.id=note_drafts.object_id WHERE objects.id NOT IN (SELECT id FROM trash))",[],|r|r.get(0))?)}
#[cfg(test)] mod rename_file_tests {
 use super::*;
 #[test] fn file_title_changes_without_rewriting_content_or_body_chunks() {
  let root=std::env::temp_dir().join(format!("atlas-rename-file-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();
  let id=add_file(&root,"original.txt",b"alpha beta").unwrap();
  let db=file_db(&root).unwrap();let path:String=db.query_row("SELECT path FROM objects WHERE id=?",[&id],|r|r.get(0)).unwrap();
  let before:Vec<(i64,String)>=db.prepare("SELECT start,content FROM search_chunks WHERE object_id=? AND field='body'").unwrap().query_map([&id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();drop(db);
  rename_file(&root,&id,"renamed.txt").unwrap();
  let db=file_db(&root).unwrap();let (title,new_path,body):(String,String,String)=db.query_row("SELECT objects.title,objects.path,search_index.body FROM objects JOIN search_index ON objects.id=search_index.object_id WHERE objects.id=?",[&id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
  assert_eq!((title.as_str(),new_path.as_str(),body.as_str()),("renamed.txt",path.as_str(),"alpha beta"));assert_eq!(fs::read(root.join(&path)).unwrap(),b"alpha beta");
  let after:Vec<(i64,String)>=db.prepare("SELECT start,content FROM search_chunks WHERE object_id=? AND field='body'").unwrap().query_map([&id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();assert_eq!(before,after);
  let title_hit:i64=db.query_row("SELECT count(*) FROM search_chunks WHERE search_chunks MATCH 'renamed' AND object_id=?",[&id],|r|r.get(0)).unwrap();assert_eq!(title_hit,1);
  assert!(rename_file(&root,&id,"renamed.pdf").is_err());assert!(rename_file(&root,&id,"../bad.txt").is_err());
  drop(db);fs::remove_dir_all(root).unwrap();
 }
}
#[cfg(test)] mod search_visibility_tests {
 use super::*;
 #[test] fn excluded_is_preserved_by_stale_board_and_restored_after_trash(){
  let root=std::env::temp_dir().join(format!("atlas-visibility-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();
  let args=|action:&str,more:Vec<String>| {let mut a=vec!["atlas".into(),action.into(),root.to_string_lossy().into_owned()];a.extend(more);execute(&a).unwrap()};
  let id=args("create-note",vec!["Title".into(),"alpha".into()])["id"].as_str().unwrap().to_owned();
  let old=args("get-board",vec![]);assert!(!search_visibility(&root,&id,None).unwrap());
  assert!(search_visibility(&root,&id,Some(true)).unwrap());assert!(search_scope(&root,None).unwrap().unwrap().is_empty());assert_eq!(search_scope(&root,Some(vec![id.clone()])).unwrap(),Some(vec![id.clone()]));
  args("put-board",vec![old.to_string()]);assert!(search_visibility(&root,&id,None).unwrap());
  delete_objects(&root,&[id.clone()]).unwrap();let board=args("get-board",vec![]);args("put-board",vec![board.to_string()]);restore_objects(&root,&[id.clone()]).unwrap();assert!(search_visibility(&root,&id,None).unwrap());
  assert!(!search_visibility(&root,&id,Some(false)).unwrap());assert_eq!(search_scope(&root,None).unwrap(),None);fs::remove_dir_all(root).unwrap();
 }
}

/// Rescue sessions contain only a hash of the random session secret.
#[cfg(any(feature="test-hook",feature="owner-leases"))]
pub fn register_rescue_session(root:&Path,id:&str,session_hash:&str)->R<()> {
 let mut db=file_db(root)?;let tx=db.transaction()?;let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM objects WHERE id=? AND kind='note' AND id NOT IN (SELECT id FROM trash))",[id],|r|r.get(0))?;if !exists{return Err("note not found".into())}
 tx.execute_batch("CREATE TABLE IF NOT EXISTS note_rescue_drafts(object_id TEXT NOT NULL,session_hash TEXT NOT NULL,title TEXT,text TEXT,PRIMARY KEY(object_id,session_hash))")?;tx.execute("INSERT INTO note_rescue_drafts(object_id,session_hash) VALUES(?,?)",params![id,session_hash])?;tx.commit()?;Ok(())
}
#[cfg(any(feature="test-hook",feature="owner-leases"))]
pub fn save_rescue_draft(root:&Path,id:&str,session_hash:&str,title:&str,text:&str)->R<()> {let db=file_db(root)?;if !table_exists(&db,"note_rescue_drafts")?{return Err("rescue session unavailable".into())}let changed=db.execute("UPDATE note_rescue_drafts SET title=?,text=? WHERE object_id=? AND session_hash=?",params![title,text,id,session_hash])?;if changed!=1{return Err("rescue session unavailable".into())}Ok(())}
#[cfg(any(feature="test-hook",feature="owner-leases"))]
pub fn get_rescue_draft(root:&Path,id:&str,session_hash:&str)->R<Option<(String,String)>>{let db=file_db(root)?;if !table_exists(&db,"note_rescue_drafts")?{return Ok(None)}Ok(db.query_row("SELECT title,text FROM note_rescue_drafts WHERE object_id=? AND session_hash=? AND text IS NOT NULL",params![id,session_hash],|r|Ok((r.get(0)?,r.get(1)?))).optional()?)}
#[cfg(any(feature="test-hook",feature="owner-leases"))]
pub fn list_rescue_drafts(root:&Path,id:&str)->R<Vec<(String,String,String)>>{let db=file_db(root)?;if !table_exists(&db,"note_rescue_drafts")?{return Ok(vec![])}let mut stmt=db.prepare("SELECT session_hash,title,text FROM note_rescue_drafts WHERE object_id=? AND text IS NOT NULL ORDER BY session_hash")?;let rows=stmt.query_map([id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;Ok(rows.collect::<Result<Vec<_>,_>>()?)}
#[cfg(any(feature="test-hook",feature="owner-leases"))]
pub fn delete_rescue_draft(root:&Path,id:&str,hash:&str,title:&str,text:&str)->R<bool>{let db=file_db(root)?;if !table_exists(&db,"note_rescue_drafts")?{return Ok(false)}Ok(db.execute("DELETE FROM note_rescue_drafts WHERE object_id=? AND session_hash=? AND title=? AND text=?",params![id,hash,title,text])?>0)}
#[cfg(any(feature="test-hook",feature="owner-leases"))]
pub fn clear_empty_rescue_session(root:&Path,id:&str,hash:&str)->R<bool>{let db=file_db(root)?;if !table_exists(&db,"note_rescue_drafts")?{return Ok(false)}Ok(db.execute("DELETE FROM note_rescue_drafts WHERE object_id=? AND session_hash=? AND text IS NULL",params![id,hash])?>0)}

// Agent creation is kept experimental until all storage variants share this contract.
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn agent_create_note(root:&Path,actor:&str,request_id:&str,title:&str,text:&str)->R<Value>{agent_create_note_guarded(root,actor,request_id,title,text,||Ok(()))}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn agent_create_note_guarded(root:&Path,actor:&str,request_id:&str,title:&str,text:&str,before_publish:impl FnOnce()->R<()>)->R<Value>{let _lock=workspace_lock(root)?;agent_create_note_inner(root,actor,request_id,title,text,before_publish)}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn agent_create_note_locked(root:&Path,actor:&str,request_id:&str,title:&str,text:&str)->R<Value>{agent_create_note_inner(root,actor,request_id,title,text,||Ok(()))}
#[cfg(any(feature="test-hook",feature="agent-write"))]
fn agent_create_note_inner(root:&Path,actor:&str,request_id:&str,title:&str,text:&str,before_publish:impl FnOnce()->R<()>)->R<Value>{
 use sha2::{Digest,Sha256};
 if actor.is_empty()||request_id.is_empty()||request_id.len()>128||title.trim().is_empty()||title.len()>1000||text.len()>1_000_000{return Err("invalid request".into())}
 let mut conn=file_db(root)?;migrate(&mut conn)?;recover(&mut conn,root)?;
 let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_create_receipts(actor TEXT NOT NULL, request_id TEXT NOT NULL, fingerprint TEXT NOT NULL, object_id TEXT NOT NULL, PRIMARY KEY(actor,request_id))")?;tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_create_cancelled(actor TEXT NOT NULL,request_id TEXT NOT NULL,PRIMARY KEY(actor,request_id))")?;let cancelled:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM agent_create_cancelled WHERE actor=? AND request_id=?)",params![actor,request_id],|r|r.get(0))?;if cancelled{return Err("request was cancelled".into())}
 let fingerprint=format!("{:x}",Sha256::digest(serde_json::to_vec(&[title,text])?));
 let prior:Option<(String,String)>=tx.query_row("SELECT fingerprint,object_id FROM agent_create_receipts WHERE actor=? AND request_id=?",params![actor,request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
 if let Some((old,id))=prior{if old!=fingerprint{return Err("request id reused with different content".into())}return Ok(json!({"id":id,"replayed":true}))}
 let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&[actor,request_id])?));let id=&key[..32];let relative=format!("notes/{id}.md");
 tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_create_intents_v2(actor TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,object_id TEXT NOT NULL,staged TEXT NOT NULL,device INTEGER NOT NULL,inode INTEGER NOT NULL,PRIMARY KEY(actor,request_id))")?;
 let existing:Option<(String,String,String,i64,i64)>=tx.query_row("SELECT fingerprint,object_id,staged,device,inode FROM agent_create_intents_v2 WHERE actor=? AND request_id=?",params![actor,request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
 let (staged,dev,ino)=if let Some((old,old_id,staged,dev,ino))=existing{if old!=fingerprint||old_id!=id{return Err("request id reused with different content".into())}(staged,dev,ino)}else{let (staged,dev,ino)=agent_stage_file(root,text)?;tx.execute("INSERT INTO agent_create_intents_v2 VALUES(?,?,?,?,?,?,?)",params![actor,request_id,fingerprint,id,staged,dev,ino])?;(staged,dev,ino)};let occupied:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM objects WHERE id=?1 OR path=?2)",params![id,relative],|r|r.get(0))?;if occupied{return Err("creation path conflict".into())}tx.commit()?;agent_create_test_barrier("intent")?;
 agent_publish_file(root,id,text,&staged,dev,ino)?;agent_create_test_barrier("published")?;before_publish()?;let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 let raw:Option<String>=tx.query_row("SELECT data FROM board WHERE id=1",[],|r|r.get(0)).optional()?;let mut board:Value=serde_json::from_str(raw.as_deref().unwrap_or("{\"objects\":[],\"edges\":[],\"stacks\":[]}"))?;
 let (x,y)=agent_grid_position(&board)?;let objects=board["objects"].as_array_mut().ok_or("invalid board")?;objects.push(json!({"id":id,"title":title,"kind":"note","x":x,"y":y,"searchExcluded":false}));
 tx.execute("INSERT INTO objects(id,kind,title,path) VALUES(?,'note',?,?)",params![id,title,relative])?;tx.execute("INSERT INTO search_index(object_id,title,body) VALUES(?,?,?)",params![id,title,text])?;tx.execute("INSERT INTO search_chunks(object_id,start,field,content) VALUES(?,-1,'title',?)",params![id,title])?;for(start,part) in chunks(text){tx.execute("INSERT INTO search_chunks(object_id,start,field,content) VALUES(?,?,'body',?)",params![id,start as i64,part])?;}tx.execute("INSERT OR REPLACE INTO board(id,data) VALUES(1,?)",[serde_json::to_string(&board)?])?;tx.execute("INSERT INTO agent_create_receipts VALUES(?,?,?,?)",params![actor,request_id,fingerprint,id])?;tx.execute("DELETE FROM agent_create_intents_v2 WHERE actor=? AND request_id=?",params![actor,request_id])?;tx.commit()?;Ok(json!({"id":id,"x":x,"y":y,"replayed":false}))
}

#[cfg(all(test,feature="test-hook"))]
mod agent_creation_contract_tests {
 use super::*;
 #[test] fn guarded_creation_failure_preserves_recoverable_intent(){let root=std::env::temp_dir().join(format!("atlas-agent-guard-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();assert!(agent_create_note_guarded(&root,"a","r","Title","body",||Err("marker failed".into())).is_err());let conn=file_db(&root).unwrap();assert_eq!(board_from_db(&conn).unwrap()["objects"].as_array().unwrap().len(),0);drop(conn);let result=agent_create_note(&root,"a","r","Title","body").unwrap();assert_eq!(result["replayed"],false);fs::remove_dir_all(root).unwrap();}
 #[test] fn placement_avoids_expanded_stack_bounds(){let board=json!({"objects":[{"id":"a","x":0,"y":0},{"id":"b","x":540,"y":240}],"edges":[],"stacks":[{"id":"group","ids":["a","b"],"collapsed":false}]});let(x,y)=agent_grid_position(&board).unwrap();assert!(x>=790.||y>=342.,"position ({x},{y}) overlaps stack");}
 #[test] fn owner_cancel_removes_only_proven_file_and_unblocks_snapshot(){let root=std::env::temp_dir().join(format!("atlas-agent-cancel-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let ws=root.join("ws");create_workspace(&ws).unwrap();let (staged,dev,ino)=agent_stage_file(&ws,"body").unwrap();let conn=file_db(&ws).unwrap();conn.execute_batch("CREATE TABLE agent_create_intents_v2(actor TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,object_id TEXT NOT NULL,staged TEXT NOT NULL,device INTEGER NOT NULL,inode INTEGER NOT NULL,PRIMARY KEY(actor,request_id))").unwrap();conn.execute("INSERT INTO agent_create_intents_v2 VALUES(?,?,?,?,?,?,?)",params!["a","r","fp","0123456789abcdef0123456789abcdef",staged,dev,ino]).unwrap();drop(conn);assert_eq!(list_pending_agent_creations(&ws).unwrap().len(),1);assert!(snapshot_workspace(&ws,&root.join("blocked")).is_err());assert_eq!(cancel_pending_agent_creation(&ws,"a","r").unwrap()["cancelled"],true);assert!(!ws.join("notes").join(staged).exists());assert!(list_pending_agent_creations(&ws).unwrap().is_empty());assert!(agent_create_note(&ws,"a","r","Title","body").is_err());assert_eq!(list_agent_recoveries(&ws).unwrap().len(),1);assert_eq!(read_agent_recovery(&ws,"a","r").unwrap()["text"],"body");snapshot_workspace(&ws,&root.join("copy")).unwrap();assert!(reject_unclaimed_agent_recovery(&ws).is_err());let saved=root.join("copy/notes/.atlas-agent-recovered-0123456789abcdef0123456789abcdef.md");assert_eq!(fs::read_to_string(&saved).unwrap(),"body");assert_eq!(read_agent_recovery(&root.join("copy"),"a","r").unwrap()["text"],"body");fs::write(saved,"changed").unwrap();assert!(read_agent_recovery(&root.join("copy"),"a","r").is_err());let restored=restore_agent_recovery(&ws,"a","r").unwrap();assert_eq!(restored["restored"],true);assert_eq!(restore_agent_recovery(&ws,"a","r").unwrap()["id"],restored["id"]);assert!(list_agent_recoveries(&ws).unwrap().is_empty());assert!(reject_unclaimed_agent_recovery(&ws).is_ok());let note=ws.join(format!("notes/{}.md",restored["id"].as_str().unwrap()));assert_eq!(fs::read_to_string(note).unwrap(),"body");fs::remove_dir_all(root).unwrap();}
 #[test] fn owner_cancel_refuses_unproven_replacement(){let root=std::env::temp_dir().join(format!("atlas-agent-cancel-foreign-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();let(staged,dev,ino)=agent_stage_file(&root,"body").unwrap();let moved=root.join("original");fs::rename(root.join("notes").join(&staged),&moved).unwrap();fs::write(root.join("notes").join(&staged),"different").unwrap();let conn=file_db(&root).unwrap();conn.execute_batch("CREATE TABLE agent_create_intents_v2(actor TEXT,request_id TEXT,object_id TEXT,staged TEXT,device INTEGER,inode INTEGER)").unwrap();conn.execute("INSERT INTO agent_create_intents_v2 VALUES(?,?,?,?,?,?)",params!["a","r","0123456789abcdef0123456789abcdef",staged,dev,ino]).unwrap();drop(conn);assert!(cancel_pending_agent_creation(&root,"a","r").is_err());assert_eq!(fs::read_to_string(root.join("notes").join(staged)).unwrap(),"different");assert_eq!(fs::read_to_string(moved).unwrap(),"body");fs::remove_dir_all(root).unwrap();}
 #[test] fn owner_cancel_race_preserves_replacement_and_original(){use std::time::{Duration,Instant};let root=std::env::temp_dir().join(format!("atlas-cancel-race-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let ws=root.join("ws");create_workspace(&ws).unwrap();let(staged,dev,ino)=agent_stage_file(&ws,"original").unwrap();let conn=file_db(&ws).unwrap();conn.execute_batch("CREATE TABLE agent_create_intents_v2(actor TEXT,request_id TEXT,object_id TEXT,staged TEXT,device INTEGER,inode INTEGER)").unwrap();conn.execute("INSERT INTO agent_create_intents_v2 VALUES(?,?,?,?,?,?)",params!["a","r","0123456789abcdef0123456789abcdef",staged,dev,ino]).unwrap();drop(conn);std::env::set_var("ATLAS_TEST_AGENT_CANCEL_BARRIER",&root);let copy=ws.clone();let thread=std::thread::spawn(move||cancel_pending_agent_creation(&copy,"a","r").is_err());let until=Instant::now()+Duration::from_secs(3);while !root.join("ready").exists()&&Instant::now()<until{std::thread::sleep(Duration::from_millis(5))}assert!(root.join("ready").exists());fs::rename(ws.join("notes").join(&staged),root.join("original")).unwrap();fs::write(ws.join("notes").join(&staged),"replacement").unwrap();fs::write(root.join("resume"),b"go").unwrap();assert!(thread.join().unwrap());std::env::remove_var("ATLAS_TEST_AGENT_CANCEL_BARRIER");assert_eq!(fs::read_to_string(root.join("original")).unwrap(),"original");assert_eq!(fs::read_to_string(ws.join("notes").join(staged)).unwrap(),"replacement");assert_eq!(list_pending_agent_creations(&ws).unwrap().len(),1);fs::remove_dir_all(root).unwrap();}
 #[test] fn cancel_child(){if let Some(ws)=std::env::var_os("ATLAS_TEST_AGENT_CANCEL_CHILD"){cancel_pending_agent_creation(Path::new(&ws),"a","r").unwrap();}}
 #[test] fn cancel_crash_after_rename_preserves_and_reports_recovery(){use std::process::{Command,Stdio};use std::time::{Duration,Instant};let root=std::env::temp_dir().join(format!("atlas-agent-cancel-crash-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let ws=root.join("ws");create_workspace(&ws).unwrap();let(staged,dev,ino)=agent_stage_file(&ws,"recover-me").unwrap();let id="0123456789abcdef0123456789abcdef";let conn=file_db(&ws).unwrap();conn.execute_batch("CREATE TABLE agent_create_intents_v2(actor TEXT,request_id TEXT,object_id TEXT,staged TEXT,device INTEGER,inode INTEGER)").unwrap();conn.execute("INSERT INTO agent_create_intents_v2 VALUES(?,?,?,?,?,?)",params!["a","r",id,staged,dev,ino]).unwrap();drop(conn);let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","agent_creation_contract_tests::cancel_child"]).env("ATLAS_TEST_AGENT_CANCEL_CHILD",&ws).env("ATLAS_TEST_AGENT_CANCEL_AFTER_MOVE",&root).stdout(Stdio::null()).spawn().unwrap();let until=Instant::now()+Duration::from_secs(5);while !root.join("cancel-moved").exists()&&Instant::now()<until{std::thread::sleep(Duration::from_millis(5))}assert!(root.join("cancel-moved").exists());child.kill().unwrap();child.wait().unwrap();let pending=list_pending_agent_creations(&ws).unwrap();assert_eq!(pending.len(),1);let expected=format!("notes/.atlas-agent-recovered-{id}.md");assert_eq!(pending[0]["recovery_path"],expected);assert_eq!(fs::read_to_string(ws.join(&expected)).unwrap(),"recover-me");let cancelled=cancel_pending_agent_creation(&ws,"a","r").unwrap();assert_eq!(cancelled["saved_as"],expected);assert_eq!(fs::read_to_string(ws.join(expected)).unwrap(),"recover-me");fs::remove_dir_all(root).unwrap();}
 #[test] fn cancel_restore_conflict_preserves_all_files(){use std::time::{Duration,Instant};let root=std::env::temp_dir().join(format!("atlas-cancel-restore-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let ws=root.join("ws");create_workspace(&ws).unwrap();let(staged,dev,ino)=agent_stage_file(&ws,"original").unwrap();let id="0123456789abcdef0123456789abcdef";let conn=file_db(&ws).unwrap();conn.execute_batch("CREATE TABLE agent_create_intents_v2(actor TEXT,request_id TEXT,object_id TEXT,staged TEXT,device INTEGER,inode INTEGER)").unwrap();conn.execute("INSERT INTO agent_create_intents_v2 VALUES(?,?,?,?,?,?)",params!["a","r",id,staged,dev,ino]).unwrap();drop(conn);std::env::set_var("ATLAS_TEST_AGENT_CANCEL_BARRIER",&root);std::env::set_var("ATLAS_TEST_AGENT_CANCEL_AFTER_MOVE",&root);let w=ws.clone();let t=std::thread::spawn(move||cancel_pending_agent_creation(&w,"a","r").is_err());let wait=|name:&str|{let until=Instant::now()+Duration::from_secs(3);while !root.join(name).exists()&&Instant::now()<until{std::thread::sleep(Duration::from_millis(5))}assert!(root.join(name).exists())};wait("ready");fs::rename(ws.join("notes").join(&staged),root.join("original")).unwrap();fs::write(ws.join("notes").join(&staged),"replacement").unwrap();fs::write(root.join("resume"),b"go").unwrap();wait("cancel-moved");fs::write(ws.join("notes").join(&staged),"new owner").unwrap();fs::write(root.join("cancel-resume"),b"go").unwrap();assert!(t.join().unwrap());std::env::remove_var("ATLAS_TEST_AGENT_CANCEL_BARRIER");std::env::remove_var("ATLAS_TEST_AGENT_CANCEL_AFTER_MOVE");assert_eq!(fs::read_to_string(root.join("original")).unwrap(),"original");assert_eq!(fs::read_to_string(ws.join("notes").join(staged)).unwrap(),"new owner");let pending=list_pending_agent_creations(&ws).unwrap();assert_eq!(fs::read_to_string(ws.join(pending[0]["recovery_path"].as_str().unwrap())).unwrap(),"replacement");fs::remove_dir_all(root).unwrap();}
 #[test] fn restore_child(){if let Some(ws)=std::env::var_os("ATLAS_TEST_AGENT_RESTORE_CHILD"){restore_agent_recovery(Path::new(&ws),"a","r").unwrap();}}
 #[test] fn restore_crash_at_each_boundary_is_idempotent(){use std::process::{Command,Stdio};use std::time::{Duration,Instant};for phase in ["intent","archived","note-created"]{let root=std::env::temp_dir().join(format!("atlas-restore-crash-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let ws=root.join("ws");create_workspace(&ws).unwrap();assert!(agent_create_note_guarded(&ws,"a","r","Title","valuable body",||Err("injected".into())).is_err());assert_eq!(cancel_pending_agent_creation(&ws,"a","r").unwrap()["cancelled"],true);let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","agent_creation_contract_tests::restore_child"]).env("ATLAS_TEST_AGENT_RESTORE_CHILD",&ws).env("ATLAS_TEST_AGENT_RESTORE_BARRIER",&root).env("ATLAS_TEST_AGENT_RESTORE_PHASE",phase).stdout(Stdio::null()).spawn().unwrap();let until=Instant::now()+Duration::from_secs(10);while !root.join("restore-ready").exists()&&Instant::now()<until{std::thread::sleep(Duration::from_millis(10))}assert!(root.join("restore-ready").exists());child.kill().unwrap();child.wait().unwrap();let restored=restore_agent_recovery(&ws,"a","r").unwrap();assert_eq!(restored["restored"],true);assert_eq!(restore_agent_recovery(&ws,"a","r").unwrap()["id"],restored["id"]);let db=file_db(&ws).unwrap();assert_eq!(board_from_db(&db).unwrap()["objects"].as_array().unwrap().len(),1);assert_eq!(fs::read_to_string(ws.join(format!("notes/{}.md",restored["id"].as_str().unwrap()))).unwrap(),"valuable body");assert_eq!(fs::read_dir(ws.join("notes")).unwrap().count(),1);drop(db);fs::remove_dir_all(root).unwrap();}}
 #[test] fn crash_at_intent_and_published_recovers(){use std::process::{Command,Stdio};use std::time::{Duration,Instant};for phase in ["intent","published"] {let root=std::env::temp_dir().join(format!("atlas-agent-crash-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let ws=root.join("workspace");create_workspace(&ws).unwrap();let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","agent_creation_contract_tests::crash_child","--nocapture"]).env("ATLAS_TEST_AGENT_CREATE_CHILD",&ws).env("ATLAS_TEST_AGENT_CREATE_BARRIER",&root).env("ATLAS_TEST_AGENT_CREATE_PHASE",phase).stdout(Stdio::null()).spawn().unwrap();let until=Instant::now()+Duration::from_secs(10);while !root.join("ready").exists()&&Instant::now()<until{std::thread::sleep(Duration::from_millis(10))}assert!(root.join("ready").exists());child.kill().unwrap();child.wait().unwrap();let result=agent_create_note(&ws,"agent","crash","Title","body").unwrap();assert_eq!(result["replayed"],false);assert_eq!(agent_create_note(&ws,"agent","crash","Title","body").unwrap()["replayed"],true);fs::remove_dir_all(root).unwrap();}}
 #[test] fn crash_child(){if let Some(ws)=std::env::var_os("ATLAS_TEST_AGENT_CREATE_CHILD"){agent_create_note(Path::new(&ws),"agent","crash","Title","body").unwrap();}}
 #[test] fn snapshot_refuses_pending_creation(){let root=std::env::temp_dir().join(format!("atlas-pending-export-{}",uuid::Uuid::new_v4()));let source=root.join("source");fs::create_dir(&root).unwrap();create_workspace(&source).unwrap();let conn=file_db(&source).unwrap();conn.execute_batch("CREATE TABLE agent_create_intents_v2(actor TEXT,request_id TEXT);INSERT INTO agent_create_intents_v2 VALUES('a','r')").unwrap();drop(conn);let dest=root.join("copy");assert!(snapshot_workspace(&source,&dest).is_err());assert!(!dest.exists());fs::remove_dir_all(root).unwrap();}
 #[test] fn existing_same_content_never_adopted_after_retries(){use sha2::{Digest,Sha256};let root=std::env::temp_dir().join(format!("atlas-agent-foreign-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&["agent","foreign"]).unwrap()));let path=root.join(format!("notes/{}.md",&key[..32]));fs::write(&path,"same").unwrap();assert!(agent_create_note(&root,"agent","foreign","Title","same").is_err());assert!(agent_create_note(&root,"agent","foreign","Title","same").is_err());assert_eq!(fs::read_to_string(path).unwrap(),"same");fs::remove_dir_all(root).unwrap();}
 #[test] fn leftover_partial_temp_cannot_replace_final_content(){
  let root=std::env::temp_dir().join(format!("atlas-agent-partial-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();let partial=root.join("notes/.agent-abandoned.tmp");fs::write(&partial,"partial").unwrap();let result=agent_create_note(&root,"agent","retry","Title","complete").unwrap();let id=result["id"].as_str().unwrap();assert_eq!(fs::read_to_string(root.join(format!("notes/{id}.md"))).unwrap(),"complete");assert_eq!(fs::read_to_string(partial).unwrap(),"partial");fs::remove_dir_all(root).unwrap();
 }
 #[test] fn refuses_existing_symlink_and_hardlink_without_mutating_source(){
  use sha2::{Digest,Sha256};use std::os::unix::fs::symlink;let root=std::env::temp_dir().join(format!("atlas-agent-link-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();let source=root.join("source");fs::write(&source,"original").unwrap();
  for (request,hard) in [("symlink",false),("hardlink",true)]{let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&["agent",request]).unwrap()));let target=root.join(format!("notes/{}.md",&key[..32]));if hard{fs::hard_link(&source,&target).unwrap()}else{symlink(&source,&target).unwrap()}assert!(agent_create_note(&root,"agent",request,"Title","original").is_err());assert_eq!(fs::read_to_string(&source).unwrap(),"original");}
  fs::remove_dir_all(root).unwrap();
 }
 #[test] fn retry_recovers_existing_uncommitted_file_without_overwriting(){
  use sha2::{Digest,Sha256};let root=std::env::temp_dir().join(format!("atlas-agent-orphan-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();
  let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&["agent","lost"]).unwrap()));let path=root.join(format!("notes/{}.md",&key[..32]));let (staged,dev,ino)=agent_stage_file(&root,"recover").unwrap();let conn=file_db(&root).unwrap();conn.execute_batch("CREATE TABLE agent_create_intents_v2(actor TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,object_id TEXT NOT NULL,staged TEXT NOT NULL,device INTEGER NOT NULL,inode INTEGER NOT NULL,PRIMARY KEY(actor,request_id))").unwrap();let fp=format!("{:x}",Sha256::digest(serde_json::to_vec(&["Title","recover"]).unwrap()));conn.execute("INSERT INTO agent_create_intents_v2 VALUES(?,?,?,?,?,?,?)",params!["agent","lost",fp,&key[..32],staged,dev,ino]).unwrap();drop(conn);agent_publish_file(&root,&key[..32],"recover",&staged,dev,ino).unwrap();
  let result=agent_create_note(&root,"agent","lost","Title","recover").unwrap();assert_eq!(result["id"],&key[..32]);assert_eq!(fs::read_to_string(&path).unwrap(),"recover");fs::remove_dir_all(root).unwrap();
 }
 #[test] fn replay_and_conflicting_request_and_position(){
  let root=std::env::temp_dir().join(format!("atlas-agent-create-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();
  let a=agent_create_note(&root,"agent","1","Alpha","hello").unwrap();let replay=agent_create_note(&root,"agent","1","Alpha","hello").unwrap();assert_eq!(a["id"],replay["id"]);assert_eq!(replay["replayed"],true);assert!(agent_create_note(&root,"agent","1","Alpha","different").is_err());
  let b=agent_create_note(&root,"agent","2","Beta","world").unwrap();assert_ne!((a["x"].clone(),a["y"].clone()),(b["x"].clone(),b["y"].clone()));let conn=file_db(&root).unwrap();let board=board_from_db(&conn).unwrap();assert_eq!(board["objects"].as_array().unwrap().len(),2);drop(conn);fs::remove_dir_all(root).unwrap();
 }
}

#[cfg(any(feature="test-hook",feature="agent-upload",feature="agent-write"))]
pub fn agent_grid_position(board:&Value)->R<(f64,f64)>{
 let objects=board["objects"].as_array().ok_or("invalid board")?;
 let rect=|o:&Value|->Option<(f64,f64,f64,f64)>{let x=o["x"].as_f64()?;let y=o["y"].as_f64()?;if !x.is_finite()||!y.is_finite(){return None}let w=if o["imageExpanded"]==true{o["imageWidth"].as_f64().unwrap_or(230.).clamp(230.,920.)}else{230.};let h=if o["imageExpanded"]==true{w*0.9565+34.}else{82.};Some((x,y,x+w,y+h))};
 let mut occupied:Vec<(f64,f64,f64,f64)>=objects.iter().map(|o|rect(o).ok_or("invalid object position")).collect::<Result<_,_>>()?;
 for stack in board["stacks"].as_array().ok_or("invalid board")? {let ids=stack["ids"].as_array().ok_or("invalid stack")?;let members:Vec<_>=objects.iter().filter(|o|ids.iter().any(|id|id==&o["id"])).filter_map(&rect).collect();if members.is_empty(){continue}let min_x=members.iter().map(|r|r.0).fold(f64::INFINITY,f64::min)-20.;let min_y=members.iter().map(|r|r.1).fold(f64::INFINITY,f64::min)-45.;let (max_x,max_y)=if stack["collapsed"]==true{(min_x+240.,min_y+72.)}else{(members.iter().map(|r|r.2).fold(f64::NEG_INFINITY,f64::max)+20.,members.iter().map(|r|r.3).fold(f64::NEG_INFINITY,f64::max)+20.)};occupied.push((min_x,min_y,max_x,max_y));}
 for ring in 0..1000i64{for y in 0..=ring{for x in 0..=ring{if x!=ring&&y!=ring{continue}let px=(x*270) as f64;let py=(y*120) as f64;if occupied.iter().all(|(x1,y1,x2,y2)|px+230.<=*x1||*x2<=px||py+82.<=*y1||*y2<=py){return Ok((px,py))}}}}Err("board is full".into())
}

#[cfg(any(feature="test-hook",feature="agent-write"))]
fn agent_stage_file(root:&Path,text:&str)->R<(String,i64,i64)>{
 use std::os::fd::{AsRawFd,FromRawFd};use std::os::unix::fs::{OpenOptionsExt,MetadataExt};use std::ffi::CString;
 let dir=fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(root.join("notes"))?;let name=format!(".agent-{}.tmp",uuid::Uuid::new_v4().simple());let c=CString::new(name.as_str())?;let fd=unsafe{libc::openat(dir.as_raw_fd(),c.as_ptr(),libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL|libc::O_NOFOLLOW|libc::O_CLOEXEC,0o600)};if fd<0{return Err(std::io::Error::last_os_error().into())}let mut file=unsafe{fs::File::from_raw_fd(fd)};let done=(||->R<(i64,i64)>{file.write_all(text.as_bytes())?;file.sync_all()?;let m=file.metadata()?;dir.sync_all()?;Ok((m.dev() as i64,m.ino() as i64))})();match done{Ok((dev,ino))=>Ok((name,dev,ino)),Err(e)=>{unsafe{libc::unlinkat(dir.as_raw_fd(),c.as_ptr(),0)};Err(e)}}
}
#[cfg(any(feature="test-hook",feature="agent-write"))]
fn agent_publish_file(root:&Path,id:&str,text:&str,staged:&str,dev:i64,ino:i64)->R<()>{
 use std::os::fd::{AsRawFd,FromRawFd};use std::os::unix::fs::{OpenOptionsExt,MetadataExt};use std::io::Read;use std::ffi::CString;
 if staged.len()!=43||!staged.starts_with(".agent-")||!staged.ends_with(".tmp")||!staged[7..39].bytes().all(|c|c.is_ascii_hexdigit()){return Err("invalid staging name".into())}let dir=fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(root.join("notes"))?;let src=CString::new(staged)?;let dst=CString::new(format!("{id}.md"))?;
 let verify=|name:&CString|->R<()>{let fd=unsafe{libc::openat(dir.as_raw_fd(),name.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC)};if fd<0{return Err(std::io::Error::last_os_error().into())}let mut file=unsafe{fs::File::from_raw_fd(fd)};let m=file.metadata()?;if !m.is_file()||m.nlink()!=1||m.uid()!=unsafe{libc::geteuid()}||m.dev() as i64!=dev||m.ino() as i64!=ino||m.len()!=text.len() as u64{return Err("creation file conflict".into())}let mut bytes=Vec::with_capacity(text.len());file.read_to_end(&mut bytes)?;if bytes!=text.as_bytes(){return Err("creation file conflict".into())}Ok(())};
 if verify(&dst).is_ok(){return Ok(())}verify(&src)?;
 #[cfg(target_os="macos")] let rc=unsafe{libc::renameatx_np(dir.as_raw_fd(),src.as_ptr(),dir.as_raw_fd(),dst.as_ptr(),libc::RENAME_EXCL)};
 #[cfg(target_os="linux")] let rc=unsafe{libc::renameat2(dir.as_raw_fd(),src.as_ptr(),dir.as_raw_fd(),dst.as_ptr(),libc::RENAME_NOREPLACE)};
 if rc<0{return Err(std::io::Error::last_os_error().into())}verify(&dst)?;dir.sync_all()?;Ok(())
}

#[cfg(any(feature="test-hook",feature="agent-upload",feature="agent-write"))]
pub fn reject_pending_agent_create(conn:&Connection)->R<()>{
 let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='agent_create_intents_v2')",[],|r|r.get(0))?;
 if exists&&conn.query_row("SELECT EXISTS(SELECT 1 FROM agent_create_intents_v2)",[],|r|r.get::<_,bool>(0))?{return Err("Незавершённое создание заметки агентом: повторите запрос перед переносом".into())}
 let files:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='agent_file_intents')",[],|r|r.get(0))?;
 if files&&conn.query_row("SELECT EXISTS(SELECT 1 FROM agent_file_intents)",[],|r|r.get::<_,bool>(0))?{return Err("Незавершённая загрузка файла агентом: повторите запрос перед переносом".into())}Ok(())
}

#[cfg(feature="test-hook")]
fn agent_create_test_barrier(phase:&str)->R<()>{if let (Some(dir),Some(target))=(std::env::var_os("ATLAS_TEST_AGENT_CREATE_BARRIER"),std::env::var_os("ATLAS_TEST_AGENT_CREATE_PHASE")){if target==phase{let path=Path::new(&dir);fs::write(path.join("ready"),phase)?;let start=std::time::Instant::now();while !path.join("resume").exists()&&start.elapsed()<std::time::Duration::from_secs(20){std::thread::sleep(std::time::Duration::from_millis(10))}}}Ok(())}

#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn list_pending_agent_creations(root:&Path)->R<Vec<Value>>{let _lock=workspace_lock(root)?;list_pending_agent_creations_locked(root)}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn list_pending_agent_creations_locked(root:&Path)->R<Vec<Value>>{
 let conn=file_db(root)?;if !table_exists(&conn,"agent_create_intents_v2")?{return Ok(Vec::new())}let mut st=conn.prepare("SELECT actor,request_id,object_id FROM agent_create_intents_v2 ORDER BY actor,request_id")?;let rows=st.query_map([],|r|Ok(json!({"actor":r.get::<_,String>(0)?,"request_id":r.get::<_,String>(1)?,"object_id":r.get::<_,String>(2)?,"recovery_path":format!("notes/.atlas-agent-recovered-{}.md",r.get::<_,String>(2)?)})))?;Ok(rows.collect::<Result<Vec<_>,_>>()?)
}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn cancel_pending_agent_creation(root:&Path,actor:&str,request_id:&str)->R<Value>{let _lock=workspace_lock(root)?;cancel_pending_agent_creation_locked(root,actor,request_id)}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn cancel_pending_agent_creation_locked(root:&Path,actor:&str,request_id:&str)->R<Value>{
 use std::ffi::CString;use std::os::fd::{AsRawFd,FromRawFd};use std::os::unix::fs::{MetadataExt,OpenOptionsExt};
 let mut conn=file_db(root)?;if !table_exists(&conn,"agent_create_intents_v2")?{return Ok(json!({"cancelled":false,"saved_as":null}))}let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 let intent:Option<(String,String,i64,i64)>=tx.query_row("SELECT object_id,staged,device,inode FROM agent_create_intents_v2 WHERE actor=? AND request_id=?",params![actor,request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;let Some((id,staged,dev,ino))=intent else{return Ok(json!({"cancelled":false,"saved_as":null}))};
 if id.len()!=32||!id.bytes().all(|c|c.is_ascii_hexdigit())||staged.len()!=43||!staged.starts_with(".agent-")||!staged.ends_with(".tmp")||!staged[7..39].bytes().all(|c|c.is_ascii_hexdigit()){return Err("invalid intent".into())}
 let path=format!("notes/{id}.md");let occupied:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM objects WHERE id=?1 OR path=?2)",params![id,path],|r|r.get(0))?;if occupied{return Err("creation path is in use".into())}
 let dir=fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(root.join("notes"))?;let mut found=Vec::new();let recovered_name=format!(".atlas-agent-recovered-{id}.md");for name in [staged,format!("{id}.md"),recovered_name.clone()] {let c=CString::new(name)?;let fd=unsafe{libc::openat(dir.as_raw_fd(),c.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC)};if fd<0{let e=std::io::Error::last_os_error();if e.kind()==std::io::ErrorKind::NotFound{continue}return Err(e.into())}let f=unsafe{fs::File::from_raw_fd(fd)};let m=f.metadata()?;if !m.is_file()||m.nlink()!=1||m.uid()!=unsafe{libc::geteuid()}||m.dev() as i64!=dev||m.ino() as i64!=ino{return Err("creation file conflict".into())}found.push(c)}if found.len()>1{return Err("ambiguous creation files".into())}
 // Preserve the bytes in a private recovery file; unlink-by-path cannot atomically bind to an inode.
 if let Some(source)=found.first(){
  #[cfg(feature="test-hook")] if let Some(p)=std::env::var_os("ATLAS_TEST_AGENT_CANCEL_BARRIER"){let path=Path::new(&p);fs::write(path.join("ready"),b"ready")?;let started=std::time::Instant::now();while !path.join("resume").exists()&&started.elapsed()<std::time::Duration::from_secs(10){std::thread::sleep(std::time::Duration::from_millis(10));}}
  let quarantine=CString::new(recovered_name.as_str())?;if source.as_bytes()==quarantine.as_bytes(){dir.sync_all()?;record_agent_recovery(&tx,&dir,&recovered_name,actor,request_id,dev,ino)?;tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_create_cancelled(actor TEXT NOT NULL,request_id TEXT NOT NULL,PRIMARY KEY(actor,request_id))")?;tx.execute("INSERT OR IGNORE INTO agent_create_cancelled VALUES(?,?)",params![actor,request_id])?;tx.execute("DELETE FROM agent_create_intents_v2 WHERE actor=? AND request_id=?",params![actor,request_id])?;tx.commit()?;return Ok(json!({"cancelled":true,"saved_as":format!("notes/{recovered_name}")}))}
  #[cfg(target_os="macos")] let moved=unsafe{libc::renameatx_np(dir.as_raw_fd(),source.as_ptr(),dir.as_raw_fd(),quarantine.as_ptr(),libc::RENAME_EXCL)};
  #[cfg(target_os="linux")] let moved=unsafe{libc::renameat2(dir.as_raw_fd(),source.as_ptr(),dir.as_raw_fd(),quarantine.as_ptr(),libc::RENAME_NOREPLACE)};
  if moved!=0{return Err(std::io::Error::last_os_error().into())}#[cfg(feature="test-hook")] if let Some(p)=std::env::var_os("ATLAS_TEST_AGENT_CANCEL_AFTER_MOVE"){let path=Path::new(&p);fs::write(path.join("cancel-moved"),b"ready")?;let started=std::time::Instant::now();while !path.join("cancel-resume").exists()&&started.elapsed()<std::time::Duration::from_secs(10){std::thread::sleep(std::time::Duration::from_millis(10));}}let fd=unsafe{libc::openat(dir.as_raw_fd(),quarantine.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC)};let verified=if fd<0{false}else{let file=unsafe{fs::File::from_raw_fd(fd)};file.metadata().is_ok_and(|m|m.is_file()&&m.nlink()==1&&m.uid()==unsafe{libc::geteuid()}&&m.dev() as i64==dev&&m.ino() as i64==ino)};
  if !verified{
   #[cfg(target_os="macos")] let _=unsafe{libc::renameatx_np(dir.as_raw_fd(),quarantine.as_ptr(),dir.as_raw_fd(),source.as_ptr(),libc::RENAME_EXCL)};
   #[cfg(target_os="linux")] let _=unsafe{libc::renameat2(dir.as_raw_fd(),quarantine.as_ptr(),dir.as_raw_fd(),source.as_ptr(),libc::RENAME_NOREPLACE)};
   dir.sync_all()?;return Err(format!("Файл изменился; данные сохранены, проверьте notes/{recovered_name}").into())
  }
 }
 dir.sync_all()?;if !found.is_empty(){record_agent_recovery(&tx,&dir,&recovered_name,actor,request_id,dev,ino)?;}tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_create_cancelled(actor TEXT NOT NULL,request_id TEXT NOT NULL,PRIMARY KEY(actor,request_id))")?;tx.execute("INSERT OR IGNORE INTO agent_create_cancelled VALUES(?,?)",params![actor,request_id])?;tx.execute("DELETE FROM agent_create_intents_v2 WHERE actor=? AND request_id=?",params![actor,request_id])?;tx.commit()?;Ok(json!({"cancelled":true,"saved_as":if found.is_empty(){None}else{Some(format!("notes/{recovered_name}"))}}))
}
#[cfg(any(feature="test-hook",feature="agent-upload",feature="agent-write"))]
pub mod agent_file_upload;

#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn reject_unclaimed_agent_recovery(root:&Path)->R<()>{let dir=root.join("notes");let meta=fs::symlink_metadata(&dir)?;if !meta.is_dir()||meta.file_type().is_symlink(){return Err("invalid notes directory".into())}for item in fs::read_dir(dir)?{let item=item?;if item.file_name().to_string_lossy().starts_with(".atlas-agent-recovered-"){return Err("Сначала перенесите восстановленные файлы агента из папки notes".into())}}Ok(())}

#[cfg(any(feature="test-hook",feature="agent-write"))]
fn record_agent_recovery(conn:&Connection,dir:&fs::File,name:&str,actor:&str,request_id:&str,dev:i64,ino:i64)->R<()>{use std::os::fd::{AsRawFd,FromRawFd};use std::os::unix::fs::MetadataExt;use std::io::Read;use sha2::{Digest,Sha256};let c=std::ffi::CString::new(name)?;let fd=unsafe{libc::openat(dir.as_raw_fd(),c.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC)};if fd<0{return Err(std::io::Error::last_os_error().into())}let mut f=unsafe{fs::File::from_raw_fd(fd)};let m=f.metadata()?;if !m.is_file()||m.nlink()!=1||m.uid()!=unsafe{libc::geteuid()}||m.dev() as i64!=dev||m.ino() as i64!=ino||m.len()>1_000_000{return Err("recovery file changed".into())}let mut bytes=Vec::with_capacity(m.len() as usize);std::io::Read::by_ref(&mut f).take(1_000_001).read_to_end(&mut bytes)?;if bytes.len()>1_000_000{return Err("recovery file too large".into())}let digest=format!("{:x}",Sha256::digest(&bytes));conn.execute_batch("CREATE TABLE IF NOT EXISTS agent_create_recoveries(actor TEXT NOT NULL,request_id TEXT NOT NULL,path TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(actor,request_id))")?;conn.execute("INSERT OR REPLACE INTO agent_create_recoveries VALUES(?,?,?,?)",params![actor,request_id,format!("notes/{name}"),digest])?;Ok(())}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn list_agent_recoveries(root:&Path)->R<Vec<Value>>{let _lock=workspace_lock(root)?;list_agent_recoveries_locked(root)}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn list_agent_recoveries_locked(root:&Path)->R<Vec<Value>>{let db=file_db(root)?;if !table_exists(&db,"agent_create_recoveries")?{return Ok(Vec::new())}let mut q=db.prepare("SELECT actor,request_id,path FROM agent_create_recoveries ORDER BY actor,request_id")?;let rows=q.query_map([],|r|Ok(json!({"actor":r.get::<_,String>(0)?,"request_id":r.get::<_,String>(1)?,"saved_as":r.get::<_,String>(2)?})))?;Ok(rows.collect::<Result<Vec<_>,_>>()?)}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn read_agent_recovery(root:&Path,actor:&str,request_id:&str)->R<Value>{let _lock=workspace_lock(root)?;read_agent_recovery_locked(root,actor,request_id)}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn read_agent_recovery_locked(root:&Path,actor:&str,request_id:&str)->R<Value>{use std::os::fd::{AsRawFd,FromRawFd};use std::os::unix::fs::{MetadataExt,OpenOptionsExt};use std::io::Read;use sha2::{Digest,Sha256};let db=file_db(root)?;if !table_exists(&db,"agent_create_recoveries")?{return Err("recovery unavailable".into())}let(path,digest):(String,String)=db.query_row("SELECT path,digest FROM agent_create_recoveries WHERE actor=? AND request_id=?",params![actor,request_id],|r|Ok((r.get(0)?,r.get(1)?)))?;let name=path.strip_prefix("notes/.atlas-agent-recovered-").and_then(|n|n.strip_suffix(".md")).filter(|n|n.len()==32&&n.bytes().all(|c|c.is_ascii_hexdigit())).ok_or("invalid recovery path")?;let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&[actor,request_id])?));let note_key=format!("{:x}",Sha256::digest(serde_json::to_vec(&["owner-recovery",key.as_str()])?));let dir=fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(root.join("notes"))?;let candidates=[format!(".atlas-agent-recovered-{name}.md"),format!(".agent-{name}.tmp"),format!("{}.md",&note_key[..32])];let mut file=None;for name in candidates{let c=std::ffi::CString::new(name)?;let fd=unsafe{libc::openat(dir.as_raw_fd(),c.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC)};if fd>=0{file=Some(unsafe{fs::File::from_raw_fd(fd)});break}let e=std::io::Error::last_os_error();if e.kind()!=std::io::ErrorKind::NotFound{return Err(e.into())}}let mut f=file.ok_or("recovery file missing")?;let m=f.metadata()?;if !m.is_file()||m.nlink()!=1||m.len()>1_000_000{return Err("invalid recovery file".into())}let mut bytes=Vec::with_capacity(m.len() as usize);std::io::Read::by_ref(&mut f).take(1_000_001).read_to_end(&mut bytes)?;if bytes.len()>1_000_000{return Err("recovery file too large".into())}if format!("{:x}",Sha256::digest(&bytes))!=digest{return Err("recovery file changed".into())}Ok(json!({"text":String::from_utf8(bytes)?,"saved_as":path}))}

#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn restore_agent_recovery(root:&Path,actor:&str,request_id:&str)->R<Value>{restore_agent_recovery_guarded(root,actor,request_id,||Ok(()))}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn restore_agent_recovery_guarded(root:&Path,actor:&str,request_id:&str,before_publish:impl FnOnce()->R<()>)->R<Value>{let _lock=workspace_lock(root)?;restore_agent_recovery_inner(root,actor,request_id,before_publish)}
#[cfg(any(feature="test-hook",feature="agent-write"))]
pub fn restore_agent_recovery_locked(root:&Path,actor:&str,request_id:&str)->R<Value>{restore_agent_recovery_inner(root,actor,request_id,||Ok(()))}
#[cfg(any(feature="test-hook",feature="agent-write"))]
fn restore_agent_recovery_inner(root:&Path,actor:&str,request_id:&str,before_publish:impl FnOnce()->R<()>)->R<Value>{
 use sha2::{Digest,Sha256};use rusqlite::OptionalExtension;use std::os::{fd::{AsRawFd,FromRawFd},unix::fs::{OpenOptionsExt,MetadataExt}};use std::ffi::CString;
 let key=format!("{:x}",Sha256::digest(serde_json::to_vec(&[actor,request_id])?));let note_key=format!("{:x}",Sha256::digest(serde_json::to_vec(&["owner-recovery",key.as_str()])?));let note_id=&note_key[..32];let db=file_db(root)?;db.execute_batch("CREATE TABLE IF NOT EXISTS agent_recovery_restored(actor TEXT NOT NULL,request_id TEXT NOT NULL,object_id TEXT NOT NULL,PRIMARY KEY(actor,request_id))")?;let prior:Option<String>=db.query_row("SELECT object_id FROM agent_recovery_restored WHERE actor=? AND request_id=?",params![actor,request_id],|r|r.get(0)).optional()?;if let Some(id)=prior{return Ok(json!({"restored":true,"id":id,"replayed":true}))}
 let recovery=read_agent_recovery_locked(root,actor,request_id)?;let text=recovery["text"].as_str().ok_or("invalid recovery")?;let name=recovery["saved_as"].as_str().and_then(|p|p.strip_prefix("notes/.atlas-agent-recovered-")).and_then(|p|p.strip_suffix(".md")).filter(|id|id.len()==32&&id.bytes().all(|c|c.is_ascii_hexdigit())).ok_or("invalid recovery")?;let dir=fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(root.join("notes"))?;let from=CString::new(format!(".atlas-agent-recovered-{name}.md"))?;let stage_name=format!(".agent-{name}.tmp");let stage=CString::new(stage_name.as_str())?;
 let receipt_exists=table_exists(&db,"agent_create_receipts")?&&db.query_row("SELECT EXISTS(SELECT 1 FROM agent_create_receipts WHERE actor='owner-recovery' AND request_id=?)",[&key],|r|r.get::<_,bool>(0))?;
 if !receipt_exists {db.execute_batch("CREATE TABLE IF NOT EXISTS agent_create_intents_v2(actor TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,object_id TEXT NOT NULL,staged TEXT NOT NULL,device INTEGER NOT NULL,inode INTEGER NOT NULL,PRIMARY KEY(actor,request_id))")?;let existing:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM agent_create_intents_v2 WHERE actor='owner-recovery' AND request_id=?)",[&key],|r|r.get(0))?;if !existing{let fd=unsafe{libc::openat(dir.as_raw_fd(),from.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC)};if fd<0{return Err(std::io::Error::last_os_error().into())}let f=unsafe{fs::File::from_raw_fd(fd)};let meta=f.metadata()?;if !meta.is_file()||meta.nlink()!=1||meta.uid()!=unsafe{libc::geteuid()}{return Err("invalid recovery file".into())}let fp=format!("{:x}",Sha256::digest(serde_json::to_vec(&["Восстановленная заметка",text])?));db.execute("INSERT INTO agent_create_intents_v2 VALUES('owner-recovery',?,?,?,?,?,?)",params![key,fp,note_id,stage_name,meta.dev() as i64,meta.ino() as i64])?;}
  agent_restore_test_barrier("intent")?;
  #[cfg(target_os="macos")] let rc=unsafe{libc::renameatx_np(dir.as_raw_fd(),from.as_ptr(),dir.as_raw_fd(),stage.as_ptr(),libc::RENAME_EXCL)};
  #[cfg(target_os="linux")] let rc=unsafe{libc::renameat2(dir.as_raw_fd(),from.as_ptr(),dir.as_raw_fd(),stage.as_ptr(),libc::RENAME_NOREPLACE)};
  if rc!=0&&std::io::Error::last_os_error().kind()!=std::io::ErrorKind::NotFound{return Err(std::io::Error::last_os_error().into())}dir.sync_all()?;agent_restore_test_barrier("archived")?;
 }
 drop(db);let restored=agent_create_note_inner(root,"owner-recovery",&key,"Восстановленная заметка",text,before_publish)?;if restored["id"]!=note_id{return Err("invalid restored note".into())}agent_restore_test_barrier("note-created")?;
 let confirmed=read_agent_recovery_locked(root,actor,request_id)?;if confirmed["text"]!=text{return Err("recovery changed".into())}let mut db=file_db(root)?;let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;tx.execute("INSERT INTO agent_recovery_restored VALUES(?,?,?)",params![actor,request_id,note_id])?;tx.execute("DELETE FROM agent_create_recoveries WHERE actor=? AND request_id=?",params![actor,request_id])?;tx.commit()?;Ok(json!({"restored":true,"id":note_id,"replayed":false}))
}

#[cfg(feature="test-hook")]
fn agent_restore_test_barrier(phase:&str)->R<()>{if let (Some(dir),Some(target))=(std::env::var_os("ATLAS_TEST_AGENT_RESTORE_BARRIER"),std::env::var_os("ATLAS_TEST_AGENT_RESTORE_PHASE")){if target==phase{let path=Path::new(&dir);fs::write(path.join("restore-ready"),phase)?;let started=std::time::Instant::now();while !path.join("restore-resume").exists()&&started.elapsed()<std::time::Duration::from_secs(20){std::thread::sleep(std::time::Duration::from_millis(10))}}}Ok(())}
/// Durable board recovery is isolated from the published board.

#[cfg(all(feature="agent-write",not(feature="test-hook")))]
fn agent_create_test_barrier(_: &str)->R<()>{Ok(())}
#[cfg(all(feature="agent-write",not(feature="test-hook")))]
fn agent_restore_test_barrier(_: &str)->R<()>{Ok(())}
#[cfg(any(feature="test-hook",feature="owner-leases"))]
pub fn board_rescue(root:&Path,action:&str,hash:&str,board:&str)->R<Value>{let mut db=file_db(root)?;let tx=db.transaction()?;tx.execute_batch("CREATE TABLE IF NOT EXISTS board_rescue_drafts(session_hash TEXT PRIMARY KEY,board TEXT)")?;let result=match action{"open"=>{tx.execute("INSERT INTO board_rescue_drafts(session_hash) VALUES(?)",[hash])?;json!({"ok":true})},"save"=>{let _:Value=serde_json::from_str(board)?;let n=tx.execute("UPDATE board_rescue_drafts SET board=? WHERE session_hash=?",params![board,hash])?;if n!=1{return Err("Board session unavailable".into())}json!({"ok":true})},"list"=>{let mut stmt=tx.prepare("SELECT session_hash,board FROM board_rescue_drafts WHERE board IS NOT NULL ORDER BY session_hash")?;let rows=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<Result<Vec<_>,_>>()?;json!({"drafts":rows.into_iter().map(|(id,raw)|serde_json::from_str::<Value>(&raw).map(|board|json!({"id":id,"board":board}))).collect::<Result<Vec<_>,_>>()?})},"delete"=>{let n=tx.execute("DELETE FROM board_rescue_drafts WHERE session_hash=? AND board=?",params![hash,board])?;json!({"deleted":n==1})},"close"=>{let n=tx.execute("DELETE FROM board_rescue_drafts WHERE session_hash=? AND board IS NULL",[hash])?;json!({"cleared":n==1})},_=>return Err("Invalid action".into())};tx.commit()?;Ok(result)}

#[cfg(test)] mod agent_read_page_tests {
 use super::*;
 #[test] fn pages_unicode_notes_and_indexed_files() {
  let root=std::env::temp_dir().join(format!("atlas-page-{}",uuid::Uuid::new_v4()));create_workspace(&root).unwrap();
  let run=|args:Vec<String>|execute(&args).unwrap();let base=|op:&str|vec!["atlas".into(),op.into(),root.to_string_lossy().into_owned()];
  let mut a=base("create-note");a.extend(["Тема".into(),"абвгде".into()]);let note=run(a)["id"].as_str().unwrap().to_owned();
  let page=|id:&str,offset:&str|{let mut a=base("agent-read-page");a.extend([id.into(),offset.into(),"2".into()]);run(a)};
  let first=page(&note,"0");assert_eq!(first["text"],"аб");assert_eq!(first["next_offset"],2);assert_eq!(first["total"],6);assert_eq!(page(&note,"4")["next_offset"],Value::Null);
  let db=Connection::open(root.join("atlas.sqlite3")).unwrap();db.execute("INSERT INTO objects(id,kind,title,path,text_status) VALUES('f','file','f.pdf','files/f/f.pdf','available')",[]).unwrap();db.execute("INSERT INTO search_index(object_id,title,body) VALUES('f','f.pdf','текст')",[]).unwrap();drop(db);assert_eq!(page("f","2")["text"],"кс");std::fs::remove_dir_all(root).unwrap();
 }
}
