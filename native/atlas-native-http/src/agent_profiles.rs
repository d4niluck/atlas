use rusqlite::{params,Connection};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::path::Path;

// Serializes revocation with the final permission check and response publication.
pub fn publication_gate()->&'static std::sync::Mutex<()> { static GATE:std::sync::OnceLock<std::sync::Mutex<()>>=std::sync::OnceLock::new();GATE.get_or_init(||std::sync::Mutex::new(())) }

pub fn dispatch(root:&Path,method:&str,id:Option<&str>,body:Option<Value>)->Result<(u16,Value),String>{
 let path=root.join(".atlas-agent-profiles.sqlite3");
 if path.is_symlink(){return Err("Invalid agent store".into())}
 let db=Connection::open_with_flags(&path,rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE|rusqlite::OpenFlags::SQLITE_OPEN_CREATE|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).map_err(|e|e.to_string())?;
 { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600)).map_err(|e|e.to_string())?; }
 db.execute_batch("CREATE TABLE IF NOT EXISTS profiles(id TEXT PRIMARY KEY,name TEXT NOT NULL,digest TEXT NOT NULL,permissions TEXT NOT NULL)").map_err(|e|e.to_string())?;
 match (method,id) {
  ("GET",None)=>{let mut q=db.prepare("SELECT id,name,permissions FROM profiles ORDER BY name,id").map_err(|e|e.to_string())?;let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?;let mut agents=Vec::new();for row in rows{let (id,name,p)=row.map_err(|e|e.to_string())?;agents.push(json!({"id":id,"name":name,"permissions":serde_json::from_str::<Value>(&p).map_err(|e|e.to_string())?}));}Ok((200,json!({"agents":agents})))},
  ("POST",None)=>{let b=body.ok_or("Invalid body")?;let name=b["name"].as_str().filter(|s|!s.trim().is_empty()&&s.len()<=200).ok_or("Invalid name")?.trim();let id=uuid::Uuid::new_v4().simple().to_string();let token=format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple());let digest=format!("{:x}",Sha256::digest(token.as_bytes()));db.execute("INSERT INTO profiles VALUES(?,?,?,?)",params![id,name,digest,"{}"]).map_err(|e|e.to_string())?;Ok((201,json!({"id":id,"name":name,"token":token,"permissions":{}})))},
  ("PUT",Some(id))=>{let b=body.ok_or("Invalid body")?;let name=b["name"].as_str().filter(|s|!s.trim().is_empty()&&s.len()<=200).ok_or("Invalid name")?.trim();let permissions=b["permissions"].as_object().ok_or("Invalid permissions")?;for (wid,rights) in permissions {if wid!="default" && (wid.len()!=32||!wid.bytes().all(|b|b.is_ascii_hexdigit())) {return Err("Invalid workspace".into())}let r=rights.as_object().ok_or("Invalid permissions")?;if r.iter().any(|(k,v)|!matches!(k.as_str(),"read"|"add"|"edit"|"lexical"|"semantic"|"smart")||!v.is_boolean()){return Err("Invalid permissions".into())}}
 let n=db.execute("UPDATE profiles SET name=?,permissions=? WHERE id=?",params![name,serde_json::to_string(permissions).map_err(|e|e.to_string())?,id]).map_err(|e|e.to_string())?;Ok(if n==0{(404,json!({"error":"Not found"}))}else{(200,json!({"ok":true}))})},
  ("DELETE",Some(id))=>{let n=db.execute("DELETE FROM profiles WHERE id=?",[id]).map_err(|e|e.to_string())?;Ok(if n==0{(404,json!({"error":"Not found"}))}else{(200,json!({"ok":true}))})},
  _=>Ok((404,json!({"error":"Not found"})))
 }
}

#[cfg(any(feature="test-hook",feature="agent-search",feature="agent-upload"))]
pub fn identity(root:&Path,token:&str)->Option<String>{
 let db=Connection::open_with_flags(root.join(".atlas-agent-profiles.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW).ok()?;
 let digest=format!("{:x}",Sha256::digest(token.as_bytes()));
 let mut q=db.prepare("SELECT id,digest FROM profiles").ok()?;
 let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).ok()?;
 for (id,expected) in rows.flatten(){let d=expected.len()^digest.len();if expected.bytes().zip(digest.bytes()).fold(d,|v,(a,b)|v|usize::from(a^b))==0{return Some(id)}}None
}

/// Fail closed for invalid credentials, missing grants and storage errors.
pub fn authorized(root:&Path, token:&str, workspace:&str, right:&str)->bool {
 if !matches!(right,"read"|"add"|"edit"|"lexical"|"semantic"|"smart") { return false; }
 let path=root.join(".atlas-agent-profiles.sqlite3");
 let Ok(db)=Connection::open_with_flags(path,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW) else{return false};
 let Ok(mut q)=db.prepare("SELECT digest,permissions FROM profiles") else{return false};
 let Ok(rows)=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))) else{return false};
 let digest=format!("{:x}",Sha256::digest(token.as_bytes()));
 for (expected,perms) in rows.flatten() {
  let d=expected.len()^digest.len();let diff=expected.bytes().zip(digest.bytes()).fold(d,|v,(a,b)|v|usize::from(a^b));
  if diff==0 {return serde_json::from_str::<Value>(&perms).ok().and_then(|p|{let grants=p.get(workspace)?;Some(grants.get(right)?.as_bool()? && (right=="read" || grants.get("read")?.as_bool()?))}).unwrap_or(false)}
 }
 false
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn profile_persistence_and_revocation() {
  let dir=std::env::temp_dir().join(format!("atlas-agent-test-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();let dir=dir.canonicalize().unwrap();
  let (_,created)=dispatch(&dir,"POST",None,Some(json!({"name":"Agent"}))).unwrap();
  let id=created["id"].as_str().unwrap();let token=created["token"].as_str().unwrap();
  assert!(!authorized(&dir,token,"default","read"));
  let (_,list)=dispatch(&dir,"GET",None,None).unwrap();assert_eq!(list["agents"][0]["name"],"Agent");assert!(!list.to_string().contains(token));assert_eq!(list["agents"][0]["permissions"],json!({}));
  dispatch(&dir,"PUT",Some(id),Some(json!({"name":"Agent","permissions":{"default":{"read":false,"add":true,"edit":true}}}))).unwrap();
  assert!(!authorized(&dir,token,"default","add"));assert!(!authorized(&dir,token,"default","edit"));
  dispatch(&dir,"PUT",Some(id),Some(json!({"name":"Agent","permissions":{"default":{"read":true}}}))).unwrap();
  assert_eq!(dispatch(&dir,"GET",None,None).unwrap().1["agents"][0]["permissions"]["default"]["read"],true);assert!(authorized(&dir,token,"default","read"));assert!(!authorized(&dir,token,"default","edit"));
  dispatch(&dir,"DELETE",Some(id),None).unwrap();assert!(!authorized(&dir,token,"default","read"));assert_eq!(dispatch(&dir,"GET",None,None).unwrap().1["agents"],json!([]));
  std::fs::remove_dir_all(dir).unwrap();
 }
}
