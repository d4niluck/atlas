use rusqlite::{params, Connection};
use std::path::Path;

pub fn gate()->&'static std::sync::Mutex<()>{static GATE:std::sync::OnceLock<std::sync::Mutex<()>>=std::sync::OnceLock::new();GATE.get_or_init(||std::sync::Mutex::new(()))}
fn identity_db(db:&Connection)->Result<(String,i64),String>{db.execute_batch("CREATE TABLE IF NOT EXISTS agent_activity_identity(id INTEGER PRIMARY KEY CHECK(id=1),owner TEXT NOT NULL,epoch INTEGER NOT NULL)").map_err(|e|e.to_string())?;db.execute("INSERT OR IGNORE INTO agent_activity_identity(id,owner,epoch) VALUES(1,?,0)",[uuid::Uuid::new_v4().to_string()]).map_err(|e|e.to_string())?;db.query_row("SELECT owner,epoch FROM agent_activity_identity WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())}
pub fn identity(root:&Path)->Result<(String,i64),String>{let db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;configure(&db)?;identity_db(&db)}
static FAILED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn failed() -> u64 { FAILED.load(std::sync::atomic::Ordering::Relaxed) }
pub fn record_failure() { FAILED.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
fn configure(db: &Connection) -> Result<(), String> { db.busy_timeout(std::time::Duration::from_secs(3)).map_err(|e|e.to_string())?; db.execute_batch("CREATE TABLE IF NOT EXISTS agent_activity_settings(workspace_id TEXT PRIMARY KEY, enabled INTEGER NOT NULL)").map_err(|e|e.to_string()) }
pub fn settings(root:&Path) -> Result<serde_json::Value,String> { let db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?; configure(&db)?; let mut q=db.prepare("SELECT workspace_id,enabled FROM agent_activity_settings").map_err(|e|e.to_string())?; let mut map=serde_json::Map::new(); for row in q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,bool>(1)?))).map_err(|e|e.to_string())? { let (id,enabled)=row.map_err(|e|e.to_string())?;map.insert(id,enabled.into()); } Ok(map.into()) }
pub fn set_enabled(root:&Path, workspace:&str, enabled:bool)->Result<(),String>{ if workspace!="default" && (workspace.len()!=32 || !workspace.bytes().all(|c|c.is_ascii_hexdigit())) {return Err("invalid workspace".into())} let db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;configure(&db)?;db.execute("INSERT INTO agent_activity_settings VALUES (?,?) ON CONFLICT(workspace_id) DO UPDATE SET enabled=excluded.enabled",params![workspace,enabled]).map_err(|e|e.to_string())?;Ok(())}

/// Store only identifiers and outcome; never persist request or response content.
pub fn record(root: &Path, agent: &str, workspace: &str, action: &str, status: u16) -> Result<(), String> { record_with_id(root,agent,workspace,action,status).map(|_|()) }

pub fn record_with_id(root: &Path, agent: &str, workspace: &str, action: &str, status: u16) -> Result<Option<i64>, String> { record_event(root,agent,workspace,action,status).map(|v|v.map(|(id,_,_,_)|id)) }

pub fn record_event(root: &Path, agent: &str, workspace: &str, action: &str, status: u16) -> Result<Option<(i64,String,String,i64)>, String> {
    if !matches!(action, "read" | "search" | "list" | "search_titles" | "create" | "edit" | "upload" | "rag" | "delete") { return Err("invalid action".into()); }
    let path = root.join(".atlas-agent-profiles.sqlite3");
    let mut db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e| e.to_string())?;
    configure(&db)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_activity(id INTEGER PRIMARY KEY, created_at INTEGER NOT NULL, agent_id TEXT NOT NULL, workspace_id TEXT NOT NULL, action TEXT NOT NULL, status INTEGER NOT NULL)").map_err(|e| e.to_string())?;
    let has_uuid:bool=tx.prepare("PRAGMA table_info(agent_activity)").map_err(|e|e.to_string())?.query_map([],|r|r.get::<_,String>(1)).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?.iter().any(|v|v=="event_uuid");
    if !has_uuid {tx.execute("ALTER TABLE agent_activity ADD COLUMN event_uuid TEXT",[]).map_err(|e|e.to_string())?;}
    let enabled:bool=tx.query_row("SELECT COALESCE((SELECT enabled FROM agent_activity_settings WHERE workspace_id=?),1)",[workspace],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !enabled { return Ok(None); }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_activity_sequence (id INTEGER PRIMARY KEY CHECK(id=1), last_id INTEGER NOT NULL)").map_err(|e|e.to_string())?;
    tx.execute("INSERT INTO agent_activity_sequence(id,last_id) SELECT 1,COALESCE(MAX(id),0) FROM agent_activity WHERE 1 ON CONFLICT(id) DO UPDATE SET last_id=MAX(last_id,excluded.last_id)",[]).map_err(|e|e.to_string())?;
    let id:i64=tx.query_row("UPDATE agent_activity_sequence SET last_id=last_id+1 WHERE id=1 RETURNING last_id",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let (owner,epoch)=identity_db(&tx)?;
    let event_uuid=uuid::Uuid::new_v4().to_string();
    tx.execute("INSERT INTO agent_activity(id,created_at,agent_id,workspace_id,action,status,event_uuid) VALUES(?,unixepoch(),?,?,?,?,?)",params![id, agent, workspace, action, status, event_uuid]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e|e.to_string())?;
    Ok(Some((id,event_uuid,owner,epoch)))
}

pub fn list(root: &Path) -> Result<Vec<serde_json::Value>, String> {
    let db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;
    let mut q=match db.prepare("SELECT created_at,agent_id,workspace_id,action,status FROM agent_activity ORDER BY id DESC LIMIT 1000") { Ok(q)=>q, Err(e) if e.to_string().contains("no such table")=>return Ok(vec![]), Err(e)=>return Err(e.to_string()) };
    let rows=q.query_map([],|r| Ok(serde_json::json!({"time":r.get::<_,i64>(0)?,"agent_id":r.get::<_,String>(1)?,"workspace_id":r.get::<_,String>(2)?,"action":r.get::<_,String>(3)?,"status":r.get::<_,i64>(4)?}))).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}
pub fn page(root:&Path,before:Option<i64>,workspaces:&[String])->Result<(Vec<serde_json::Value>,Option<i64>),String>{
 let db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;
 let has_uuid=db.prepare("PRAGMA table_info(agent_activity)").map_err(|e|e.to_string())?.query_map([],|r|r.get::<_,String>(1)).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?.iter().any(|v|v=="event_uuid");
 let mut sql=if has_uuid {"SELECT id,created_at,agent_id,workspace_id,action,status,event_uuid FROM agent_activity WHERE id < ?"}else{"SELECT id,created_at,agent_id,workspace_id,action,status,NULL FROM agent_activity WHERE id < ?"}.to_owned();
 let mut values:Vec<rusqlite::types::Value>=vec![before.unwrap_or(i64::MAX).into()];if !workspaces.is_empty(){sql.push_str(" AND workspace_id IN (");sql.push_str(&vec!["?";workspaces.len()].join(","));sql.push(')');values.extend(workspaces.iter().cloned().map(Into::into));}sql.push_str(" ORDER BY id DESC LIMIT 201");
 let mut q=match db.prepare(&sql){Ok(q)=>q,Err(e) if e.to_string().contains("no such table")=>return Ok((vec![],None)),Err(e)=>return Err(e.to_string())};
 let mut rows=q.query(rusqlite::params_from_iter(values)).map_err(|e|e.to_string())?;let mut out=Vec::new();
 while let Some(r)=rows.next().map_err(|e|e.to_string())?{let wid:String=r.get(3).map_err(|e|e.to_string())?;out.push(serde_json::json!({"id":r.get::<_,i64>(0).map_err(|e|e.to_string())?,"time":r.get::<_,i64>(1).map_err(|e|e.to_string())?,"agent_id":r.get::<_,String>(2).map_err(|e|e.to_string())?,"workspace_id":wid,"action":r.get::<_,String>(4).map_err(|e|e.to_string())?,"status":r.get::<_,i64>(5).map_err(|e|e.to_string())?,"event_uuid":r.get::<_,Option<String>>(6).map_err(|e|e.to_string())?}));}

 let next=if out.len()>200{out.pop();out.last().and_then(|v|v["id"].as_i64())}else{None};Ok((out,next))
}
pub fn clear(root:&Path)->Result<(),String>{let _gate=gate().lock().map_err(|_|"activity unavailable")?;let mut db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;configure(&db)?;let tx=db.transaction().map_err(|e|e.to_string())?;identity_db(&tx)?;tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_activity_prune(workspace_id TEXT PRIMARY KEY,epoch INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS agent_activity(id INTEGER PRIMARY KEY, created_at INTEGER NOT NULL, agent_id TEXT NOT NULL, workspace_id TEXT NOT NULL, action TEXT NOT NULL, status INTEGER NOT NULL)").map_err(|e|e.to_string())?;tx.execute("UPDATE agent_activity_identity SET epoch=epoch+1 WHERE id=1",[]).map_err(|e|e.to_string())?;tx.execute("UPDATE agent_activity_prune SET epoch=(SELECT epoch FROM agent_activity_identity WHERE id=1)",[]).map_err(|e|e.to_string())?;tx.execute("INSERT INTO agent_activity_prune(workspace_id,epoch) SELECT DISTINCT workspace_id,(SELECT epoch FROM agent_activity_identity WHERE id=1) FROM agent_activity WHERE 1 ON CONFLICT(workspace_id) DO UPDATE SET epoch=excluded.epoch",[]).map_err(|e|e.to_string())?;match tx.execute("DELETE FROM agent_activity",[]){Ok(_)=>{},Err(e) if e.to_string().contains("no such table")=>{},Err(e)=>return Err(e.to_string())}tx.commit().map_err(|e|e.to_string())}

pub fn pending_prune(root:&Path)->Result<Vec<(String,i64)>,String>{let db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;let mut q=match db.prepare("SELECT workspace_id,epoch FROM agent_activity_prune"){Ok(v)=>v,Err(e) if e.to_string().contains("no such table")=>return Ok(vec![]),Err(e)=>return Err(e.to_string())};let rows=q.query_map([],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;Ok(rows)}
pub fn prune_done(root:&Path,wid:&str,epoch:i64)->Result<(),String>{let db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;db.execute("DELETE FROM agent_activity_prune WHERE workspace_id=? AND epoch<=?",params![wid,epoch]).map_err(|e|e.to_string())?;Ok(())}

#[cfg(test)] mod tests { use super::*; #[test] fn bounded_and_no_content(){let d=std::env::temp_dir().join(format!("atlas-agent-activity-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&d).unwrap();let d=d.canonicalize().unwrap();Connection::open(d.join(".atlas-agent-profiles.sqlite3")).unwrap();for _ in 0..1002{record(&d,"agent-id","workspace-id","read",200).unwrap()}let v=list(&d).unwrap();assert_eq!(v.len(),1000);assert!(!v[0].to_string().contains("secret"));assert!(record(&d,"a","w","sensitive content",200).is_err());clear(&d).unwrap();assert!(list(&d).unwrap().is_empty());set_enabled(&d,"default",false).unwrap();record(&d,"a","default","read",200).unwrap();assert!(list(&d).unwrap().is_empty());set_enabled(&d,"default",true).unwrap();record(&d,"a","default","read",413).unwrap();assert_eq!(list(&d).unwrap()[0]["status"],413);std::fs::remove_dir_all(d).unwrap();}}
#[cfg(test)] mod page_tests {use super::*; #[test] fn paging_filters_and_continues(){let d=std::env::temp_dir().join(format!("atlas-activity-page-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&d).unwrap();let d=d.canonicalize().unwrap();Connection::open(d.join(".atlas-agent-profiles.sqlite3")).unwrap();for n in 0..2450{record(&d,"agent",if n%2==0{"a"}else{"b"},"search",200).unwrap();}let (first,cursor)=page(&d,None,&["a".into()]).unwrap();assert_eq!(first.len(),200);let (last,next)=page(&d,cursor,&["a".into()]).unwrap();assert_eq!(last.len(),200);assert!(next.is_some());assert!(first.iter().all(|v|v["workspace_id"]=="a"));std::fs::remove_dir_all(d).unwrap();}}
#[cfg(test)] mod event_id_tests {
 use super::*;
 #[test] fn record_id_is_usable_and_respects_disabled_history(){
  let dir=std::env::temp_dir().join(format!("atlas-activity-id-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();let dir=dir.canonicalize().unwrap();Connection::open(dir.join(".atlas-agent-profiles.sqlite3")).unwrap();
  let (id,event,_,_)=record_event(&dir,"agent","default","read",200).unwrap().unwrap();
  let other=dir.join("other");std::fs::create_dir(&other).unwrap();Connection::open(other.join(".atlas-agent-profiles.sqlite3")).unwrap();let (other_id,other_event,_,_)=record_event(&other,"agent","default","read",200).unwrap().unwrap();assert_eq!(id,other_id);assert_ne!(event,other_event);
  let (rows,_)=page(&dir,None,&[]).unwrap();assert_eq!(rows[0]["id"],id);
  clear(&dir).unwrap();let next=record_with_id(&dir,"agent","default","read",200).unwrap().unwrap();assert!(next>id);
  set_enabled(&dir,"default",false).unwrap();assert!(record_with_id(&dir,"agent","default","read",200).unwrap().is_none());
  std::fs::remove_dir_all(dir).unwrap();
 }
}
