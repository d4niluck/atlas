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
fn chunks(s:&str)->Vec<(usize,String)>{
    let a:Vec<char>=s.chars().collect(); let mut out=vec![];let mut start=0;
    while start<a.len(){let mut end=(start+400).min(a.len());let word=|c:char|c.is_alphanumeric()||c=='_';
        if end<a.len()&&word(a[end-1])&&word(a[end]) {let mut i=end;while i>start&&word(a[i-1]){i-=1}if i>start{end=i}else{while end<a.len()&&word(a[end]){end+=1}}}
        out.push((start,a[start..end].iter().collect()));start=end;
    }out
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
        tx.execute("DELETE FROM note_journal WHERE object_id=?",[&id])?;tx.commit()?;
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
fn text_info(name:&str,path:&Path)->(String,&'static str){
 let ext=Path::new(name).extension().and_then(|s|s.to_str()).unwrap_or("").to_lowercase();
 let result:R<String>=match ext.as_str(){
  "txt"|"md"=>fs::read_to_string(path).map_err(|e|e.into()),
  "pdf"=>pdf_extract::extract_text_by_pages(path).map(|pages|pages.join("\n")).map_err(|e|e.into()),
  "docx"=>(||{let doc=docx_lite::parse_document_from_path(path)?;let mut text:Vec<String>=doc.paragraphs.iter().map(|p|p.to_text()).collect();for table in &doc.tables{for row in &table.rows{for cell in &row.cells{text.push(cell.paragraphs.iter().map(|p|p.to_text()).collect::<Vec<_>>().join("\n"));}}}Ok(text.join("\n"))})(),
  _=>return (String::new(),"unsupported")
 };
 let text=result.unwrap_or_default();let status=if text.trim().is_empty(){"unavailable"}else{"available"};(text,status)
}

fn index_file(tx:&rusqlite::Transaction<'_>,id:&str,title:&str,text:&str,description:&str)->R<()> {
 tx.execute("DELETE FROM search_index WHERE object_id=?",[id])?;tx.execute("INSERT INTO search_index VALUES(?,?,?)",params![id,title,text])?;
 tx.execute("DELETE FROM search_chunks WHERE object_id=?",[id])?;tx.execute("INSERT INTO search_chunks VALUES(?,-1,'title',?)",params![id,title])?;
 for(field,value) in [("body",text),("description",description)]{for(start,part) in chunks(value){tx.execute("INSERT INTO search_chunks VALUES(?,?,?,?)",params![id,start as i64,field,part])?;}}Ok(())
}
fn file_db(root:&Path)->R<Connection>{let db=root.join("atlas.sqlite3");if !db.is_file(){return Err("missing db".into())}Ok(Connection::open_with_flags(db,rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?)}
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
pub fn add_file_stream(root:&Path, name:&str,reader:&mut impl std::io::Read)->R<String>{
 file_name(name)?;let files=root.join("files");if files.is_symlink() || !files.canonicalize()?.starts_with(root.canonicalize()?){return Err("unsafe files directory".into())}
 let id=uuid::Uuid::new_v4().to_string();let dir=files.join(&id);fs::create_dir(&dir)?;let target=dir.join(name);
 let result=(||->R<()>{let mut f=fs::OpenOptions::new().create_new(true).write(true).open(&target)?;std::io::copy(reader,&mut f)?;drop(f);let (text,status)=text_info(name,&target);let mut conn=file_db(root)?;let tx=conn.transaction()?;let relative=format!("files/{id}/{name}");tx.execute("INSERT INTO objects(id,kind,title,path,text_status) VALUES(?,'file',?,?,?)",params![id,name,relative,status])?;index_file(&tx,&id,name,&text,"")?;tx.commit()?;Ok(())})();
 if let Err(e)=result{let _=fs::remove_file(&target);let _=fs::remove_dir(&dir);return Err(e)}Ok(id)
}
pub fn set_file_description(root:&Path,id:&str,description:&str)->R<()>{let mut conn=file_db(root)?;let tx=conn.transaction()?;let (title,body):(String,String)=tx.query_row("SELECT objects.title,COALESCE(search_index.body,'') FROM objects JOIN search_index ON search_index.object_id=objects.id WHERE objects.id=? AND objects.kind='file' AND objects.id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;tx.execute("UPDATE objects SET description=? WHERE id=?",params![description,id])?;index_file(&tx,id,&title,&body,description)?;tx.commit()?;Ok(())}
pub fn replace_file(root:&Path,id:&str,name:&str,bytes:&[u8])->R<()>{replace_file_stream(root,id,name,&mut std::io::Cursor::new(bytes))}
pub fn replace_file_stream(root:&Path,id:&str,name:&str,reader:&mut impl std::io::Read)->R<()>{
 file_name(name)?;let mut conn=file_db(root)?;let old_path:String=conn.query_row("SELECT path FROM objects WHERE id=? AND kind='file' AND id NOT IN (SELECT id FROM trash)",[id],|r|r.get(0))?;
 let original=safe_path(root,&old_path)?;let files_root=safe_path(root,&format!("files/{id}"))?;if !original.starts_with(&files_root){return Err("invalid file path".into())}
 let revision=uuid::Uuid::new_v4().to_string();let dir=files_root.join(&revision);fs::create_dir(&dir)?;let target=dir.join(name);
 let result=(||->R<()>{let mut file=fs::OpenOptions::new().create_new(true).write(true).open(&target)?;std::io::copy(reader,&mut file)?;file.sync_all()?;drop(file);let(text,status)=text_info(name,&target);let relative=format!("files/{id}/{revision}/{name}");
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
   let objects=stmt.query_map([],|r|{let id:String=r.get(0)?;let p=positions.get(&id);Ok(json!({"id":id,"title":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"text_status":r.get::<_,Option<String>>(3)?,"x":p.and_then(|v|v.get("x")).cloned().unwrap_or(json!(0)),"y":p.and_then(|v|v.get("y")).cloned().unwrap_or(json!(0)),"imageExpanded":p.and_then(|v|v.get("imageExpanded")).and_then(Value::as_bool).unwrap_or(false)}))})?.collect::<Result<Vec<_>,_>>()?;
   let ids:HashSet<&str>=objects.iter().filter_map(|o|o["id"].as_str()).collect();
   let edges:Vec<Value>=b["edges"].as_array().ok_or("invalid edges")?.iter().filter(|e|e["from"].as_str().is_some_and(|s|ids.contains(s))&&e["to"].as_str().is_some_and(|s|ids.contains(s))).cloned().collect();
   let stacks:Vec<Value>=b["stacks"].as_array().ok_or("invalid stacks")?.iter().map(|s|{let mut s=s.clone();let members=s["ids"].as_array().map(|v|v.iter().filter(|x|x.as_str().is_some_and(|id|ids.contains(id))).cloned().collect::<Vec<_>>()).unwrap_or_default();s["ids"]=json!(members);s}).collect();
   b["objects"]=json!(objects);b["edges"]=json!(edges);b["stacks"]=json!(stacks);Ok(b)
}
fn revision(conn:&Connection,root:&Path,id:&str,only_file:bool)->R<String>{
 use sha2::{Digest,Sha256};use std::io::Read;use std::os::unix::fs::MetadataExt;
 let (kind,title,path,description):(String,String,String,String)=conn.query_row("SELECT kind,title,path,description FROM objects WHERE id=? AND id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;if only_file && kind!="file"{return Err("not a file".into())}
 let fields=[id,kind.as_str(),title.as_str(),description.as_str()];let prefix=format!("[{}]",fields.iter().map(|v|serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join(", "));let target=safe_path(root,&path)?;let mut file=fs::File::open(&target)?;let before=file.metadata()?;let mut hash=Sha256::new();hash.update(prefix.as_bytes());let mut buffer=[0u8;65536];loop{let n=file.read(&mut buffer)?;if n==0{break}hash.update(&buffer[..n]);}let after=file.metadata()?;let current=fs::metadata(&target)?;let identity=|m:&fs::Metadata|(m.dev(),m.ino(),m.len(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec());if identity(&before)!=identity(&after)||identity(&after)!=identity(&current){return Err("file changed during verification".into())}Ok(format!("{:x}",hash.finalize()))
}
pub fn object_revision(root:&Path,id:&str)->R<String>{let conn=file_db(root)?;revision(&conn,root,id,false)}
pub fn file_revision(root:&Path,id:&str)->R<String>{let mut conn=file_db(root)?;let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let result=revision(&tx,root,id,true)?;tx.commit()?;Ok(result)}
pub fn undo_created(root:&Path,id:&str,expected:&str)->R<()> {
 purge_trash(root)?;
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
 let (text,status)=text_info(name,&target);let relative=format!("files/{new_id}/{name}");
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
 tx.execute("INSERT OR REPLACE INTO board VALUES(1,?)",[serde_json::to_string(&board)?])?;tx.commit()?;board_from_db(&conn)
}
pub fn list_trash(root:&Path)->R<Value>{let conn=file_db(root)?;let mut stmt=conn.prepare("SELECT objects.id,title,kind FROM objects JOIN trash ON objects.id=trash.id ORDER BY deleted_at DESC")?;let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?})))?;Ok(json!(rows.collect::<Result<Vec<_>,_>>()?))}
pub fn purge_trash(root:&Path)->R<()> {let mut conn=file_db(root)?;let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let cutoff=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs_f64()-86400.0;
 let expired={let mut stmt=tx.prepare("SELECT objects.id,objects.kind,objects.path FROM objects JOIN trash ON objects.id=trash.id WHERE trash.deleted_at <= ?")?;let rows=stmt.query_map([cutoff],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?;rows.collect::<Result<Vec<_>,_>>()?};
 for(id,_,_) in &expired{for table in ["search_index","search_chunks"]{tx.execute(&format!("DELETE FROM {table} WHERE object_id=?"),[id])?;}tx.execute("DELETE FROM objects WHERE id=?",[id])?;tx.execute("DELETE FROM trash WHERE id=?",[id])?;}tx.commit()?;
 for (id,kind,path) in expired{if kind=="file"{let folder=safe_path(root,&format!("files/{id}"))?;if folder.exists(){fs::remove_dir_all(folder)?;}}else{let target=safe_path(root,&path)?;if target.exists(){fs::remove_file(target)?;}}}Ok(())}
fn validate_edges(tx:&Connection,edges:&[Value])->R<()> {
let mut pairs:HashMap<String,Vec<String>>=HashMap::new();for edge in edges{let f=edge["from"].as_str().ok_or("invalid edge")?;let t=edge["to"].as_str().ok_or("invalid edge")?;for field in ["arrowStart","arrowEnd"]{if edge.get(field).is_some_and(|x|!x.is_boolean()){return Err("invalid arrow".into())}} let pair=pair_key(f,t)?;pairs.entry(pair).or_default().push(serde_json::to_string(&edge["id"])?);}
 for (pair,ids) in pairs{if ids.len()<2{continue}let permitted:Option<String>=tx.query_row("SELECT ids FROM legacy_edge_pairs WHERE pair=?",[pair],|r|r.get(0)).optional()?;let mut allowed:Vec<String>=serde_json::from_str(permitted.as_deref().unwrap_or("[]"))?;for id in ids{if let Some(i)=allowed.iter().position(|v|v==&id){allowed.remove(i);}else{return Err("duplicate edge".into())}}}
 Ok(())
}
pub fn execute(a:&[String])->R<Value>{
 if a.len()<3{return Err("missing args".into())}let root=Path::new(&a[2]);let db=root.join("atlas.sqlite3");if a[1]=="create-workspace" { create_workspace(root)?; return Ok(json!({"ok":true})); } if !db.is_file(){return Err("missing db".into())}
 let mut conn=Connection::open_with_flags(db,rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;migrate(&mut conn)?;recover(&mut conn,root)?;
 match a[1].as_str(){
 "delete-objects"=>{let ids:Vec<String>=serde_json::from_str(a.get(3).ok_or("missing ids")?)?;return delete_objects(root,&ids)},
 "restore-objects"=>{let ids:Vec<String>=serde_json::from_str(a.get(3).ok_or("missing ids")?)?;return restore_objects(root,&ids)},
 "list-trash"=>return list_trash(root),
 "purge-trash"=>{purge_trash(root)?;return Ok(json!({"ok":true}))},
 "create-note"=>{let title=a.get(3).ok_or("missing title")?;let text=a.get(4).ok_or("missing text")?;let id=uuid::Uuid::new_v4().to_string();let path=format!("notes/{id}.md");let target=safe_path(root,&path)?;write_atomic(&target,text)?;
 let result=(||->R<()>{let tx=conn.transaction()?;tx.execute("INSERT INTO objects(id,kind,title,path) VALUES(?,'note',?,?)",params![id,title,path])?;tx.execute("INSERT INTO search_index(object_id,title,body) VALUES(?,?,?)",params![id,title,text])?;tx.execute("INSERT INTO search_chunks(object_id,start,field,content) VALUES(?,-1,'title',?)",params![id,title])?;for (start,part) in chunks(text){tx.execute("INSERT INTO search_chunks(object_id,start,field,content) VALUES(?,?,'body',?)",params![id,start as i64,part])?;}tx.commit()?;Ok(())})();if let Err(e)=result{fs::remove_file(target)?;return Err(e)}return Ok(json!({"id":id}));},
 "put-board"=>{let board:Value=serde_json::from_str(a.get(3).ok_or("missing board")?)?;for key in ["objects","edges","stacks"] {if !board[key].is_array(){return Err("invalid board".into())}}
 let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;validate_edges(&tx,board["edges"].as_array().unwrap())?;
 tx.execute("INSERT OR REPLACE INTO board(id,data) VALUES(1,?)",[serde_json::to_string(&board)?])?;tx.commit()?;return Ok(json!({"ok":true}));},
 "get-note"=>{let id=a.get(3).ok_or("missing id")?;let (title,path):(String,String)=conn.query_row("SELECT title,path FROM objects WHERE id=? AND kind='note' AND id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;let text=fs::read_to_string(safe_path(root,&path)?)?;return Ok(json!({"id":id,"title":title,"text":text}));},
 "update-note"=>{let id=a.get(3).ok_or("missing id")?;let title=a.get(4).ok_or("missing title")?;let text=a.get(5).ok_or("missing text")?; let path:String=conn.query_row("SELECT path FROM objects WHERE id=? AND kind='note' AND id NOT IN (SELECT id FROM trash)",[id],|r|r.get(0))?;let target=safe_path(root,&path)?;let previous=fs::read_to_string(&target)?;conn.execute("INSERT INTO note_journal(object_id,title,text) VALUES(?,?,?)",params![id,title,text])?;if let Err(error)=recover(&mut conn,root){write_atomic(&target,&previous)?;conn.execute("DELETE FROM note_journal WHERE object_id=?",[id])?;return Err(error)};return Ok(json!({"ok":true}));},
 "get-board"=>{
return board_from_db(&conn);
 },_=>return Err("unknown action".into())}
}
pub fn run()->R<()>{let args:Vec<String>=env::args().collect();println!("{}",execute(&args)?);Ok(())}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn pair_key_preserves_commas_in_ids() { assert_eq!(pair_key("x,y", "z").unwrap(), "[\"x,y\", \"z\"]"); }
 #[test] fn stale_temporary_file_does_not_block_write() {
  let dir=std::env::temp_dir().join(format!("atlas-stale-test-{}",std::process::id()));fs::create_dir_all(&dir).unwrap();
  let target=dir.join("n.md");let stale=target.with_extension(format!("atlas-recover-{}-0",std::process::id()));fs::write(&stale,"previous crash").unwrap();
  write_atomic(&target,"new text").unwrap();assert_eq!(fs::read_to_string(&target).unwrap(),"new text");assert_eq!(fs::read_to_string(&stale).unwrap(),"previous crash");fs::remove_dir_all(dir).unwrap();
 }
}
