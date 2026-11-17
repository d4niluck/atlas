use std::{fs::{self,File,OpenOptions},io::{Read,Write},os::unix::{fs::{FileTypeExt,MetadataExt,OpenOptionsExt,PermissionsExt},io::AsRawFd,net::{UnixListener,UnixStream}},path::PathBuf,sync::{Arc,atomic::{AtomicUsize,Ordering}},time::{Duration,Instant}};

pub struct AgentSocket { pub listener: UnixListener, _lock: File, path:PathBuf, inode:u64 }
impl AgentSocket { pub fn start()->Result<Self,Box<dyn std::error::Error+Send+Sync>> {
 let uid=unsafe{libc::geteuid()};
 #[cfg(feature="test-hook")]
 let dir=if let Some(v)=std::env::var_os("ATLAS_TEST_AGENT_SOCKET_DIR"){let p=PathBuf::from(v);if !p.is_absolute(){return Err("test socket path must be absolute".into())}p}else{PathBuf::from(format!("/tmp/atlas-agent-{uid}"))};
 #[cfg(not(feature="test-hook"))]
 let dir=PathBuf::from(format!("/tmp/atlas-agent-{uid}"));
 Self::start_at(dir,uid)
 }
 fn start_at(dir:PathBuf,uid:libc::uid_t)->Result<Self,Box<dyn std::error::Error+Send+Sync>>{
 match fs::create_dir(&dir){Ok(())=>fs::set_permissions(&dir,fs::Permissions::from_mode(0o700))?,Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{},Err(e)=>return Err(e.into())};
 let meta=fs::symlink_metadata(&dir)?;if !meta.is_dir()||meta.uid()!=uid||meta.mode()&0o077!=0{return Err("unsafe agent socket directory".into())}
 let lock=OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(dir.join("service.lock"))?;let lm=lock.metadata()?;if !lm.is_file()||lm.uid()!=uid||lm.mode()&0o077!=0||lm.nlink()!=1{return Err("unsafe agent socket lock".into())}
 if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("agent socket already in use".into())}
 let path=dir.join("agent.sock");if let Ok(m)=fs::symlink_metadata(&path){if !m.file_type().is_socket()||m.uid()!=uid{return Err("unsafe agent socket".into())}match UnixStream::connect(&path){Ok(_)=>return Err("agent socket already active".into()),Err(e) if e.kind()==std::io::ErrorKind::ConnectionRefused=>{},Err(e)=>return Err(e.into())}fs::remove_file(&path)?;}
 let listener=UnixListener::bind(&path)?;fs::set_permissions(&path,fs::Permissions::from_mode(0o600))?;let inode=fs::symlink_metadata(&path)?.ino();Ok(Self{listener,_lock:lock,path,inode})
}}
impl Drop for AgentSocket {fn drop(&mut self){if fs::symlink_metadata(&self.path).ok().is_some_and(|m|m.file_type().is_socket()&&m.ino()==self.inode){let _=fs::remove_file(&self.path);}}}

// Read complete, bounded requests before consuming a backend worker. A total deadline
// also stops clients that send just one byte before each per-read timeout.
pub struct AgentRequest { method:tiny_http::Method, path:String, headers:Vec<tiny_http::Header>, body:Vec<u8>, client:UnixStream, pub(crate) activity:Option<(PathBuf,String,String,String,Option<String>)> }
impl AgentRequest {
 pub fn method(&self)->&tiny_http::Method{&self.method}
 pub fn url(&self)->&str{&self.path}
 pub fn headers(&self)->&[tiny_http::Header]{&self.headers}
 pub(crate) fn catalog_activity(&self)->Option<&'static str>{let value:serde_json::Value=serde_json::from_slice(&self.body).ok()?;match value.get("query")?{serde_json::Value::Null=>Some("list"),serde_json::Value::String(_)=>Some("search_titles"),_=>None}}
 pub(crate) fn activity_step(&self)->Option<String>{serde_json::from_slice::<serde_json::Value>(&self.body).ok()?.get("action")?.as_str().map(str::to_owned)}
}
pub fn body(req:&mut AgentRequest)->Result<serde_json::Value,()>{let content=req.headers.iter().find(|h|h.field.equiv("Content-Type")).map(|h|h.value.as_str());if !content.is_some_and(|s|s.split(';').next()==Some("application/json")){return Err(())}let v:serde_json::Value=serde_json::from_slice(&req.body).map_err(|_|())?;if !v.is_object(){return Err(())}Ok(v)}
pub(crate) fn search_page(value:&mut serde_json::Value,offset:usize,limit:usize) {
 if let Some(items)=value.get_mut("results").and_then(|v|v.as_array_mut()) {
  let mut rows=Vec::new();for item in items.iter(){match item.get("chunks").and_then(|v|v.as_array()){Some(chunks) if !chunks.is_empty()=>for chunk in chunks {let mut row=item.clone();row["chunks"]=serde_json::json!([chunk]);rows.push(row)},_=>rows.push(item.clone())}}
  let total=rows.len();let page=rows.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
  *items=page;value["total"]=serde_json::json!(total);value["offset"]=serde_json::json!(offset);value["limit"]=serde_json::json!(limit);
 }
}
pub(crate) struct PendingDetail {pub workspace:String,pub uuid:String,pub action:String,pub session:Option<String>,pub owner:String,pub epoch:i64,pub request:serde_json::Value,pub response:serde_json::Value}
thread_local! { static PENDING_DETAIL:std::cell::RefCell<Option<PendingDetail>>=const {std::cell::RefCell::new(None)}; }
pub(crate) fn take_pending_detail()->Option<PendingDetail>{PENDING_DETAIL.with(|v|v.borrow_mut().take())}
pub fn answer(mut req:AgentRequest,status:u16,mut value:serde_json::Value){
 if status==200 && req.method==tiny_http::Method::Post && req.path.split('?').next().is_some_and(|p|{let parts:Vec<_>=p.trim_matches('/').split('/').collect();parts.len()==4&&parts[0]=="api"&&parts[1]=="agent"&&parts[3]=="search"}) {
  if let Ok(body)=serde_json::from_slice::<serde_json::Value>(&req.body) {let offset=body["offset"].as_u64().unwrap_or(0) as usize;let limit=body["limit"].as_u64().unwrap_or(10) as usize;search_page(&mut value,offset,limit);}
 }
 let payload=serde_json::to_vec(&value).unwrap_or_else(|_|b"{}".to_vec());let (status,payload)=if payload.len()>8_000_000{(413,b"{\"error\":\"Response too large\"}".to_vec())}else{(status,payload)};
 if let Some((root,agent,workspace,action,session))=req.activity.take(){match crate::agent_activity::record_event(&root,&agent,&workspace,&action,status){Ok(Some((_,uuid,owner,epoch))) if status<400=>{let mut body=serde_json::from_slice::<serde_json::Value>(&req.body).unwrap_or_else(|_|serde_json::json!({}));if !body.is_object(){body=serde_json::json!({})}let path=req.path.split('?').next().unwrap_or("");let parts:Vec<_>=path.trim_matches('/').split('/').collect();if matches!(action.as_str(),"read"|"edit"|"delete")&&parts.len()>=5&&matches!(parts[3],"notes"|"objects"){let id=parts[4];if !id.is_empty()&&id.len()<=64&&id.bytes().all(|b|b.is_ascii_hexdigit()||b==b'-'){body["object_id"]=serde_json::json!(id);}}if action=="read" {if let Some((_,query))=req.path.split_once('?'){for part in query.split('&'){if let Some((key,value))=part.split_once('='){if matches!(key,"offset"|"limit"){if let Ok(n)=value.parse::<u64>(){if n<=1_000_000_000{body[key]=serde_json::json!(n)}}}}}}}let (request,response)=atlas_native_storage_contract::agent_activity_details::summarize(&action,&body,&value);PENDING_DETAIL.with(|v|*v.borrow_mut()=Some(PendingDetail{workspace,uuid,action,session,owner,epoch,request,response}));},Ok(_)=>{},Err(_)=>crate::agent_activity::record_failure()}}
 let header=format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",payload.len());let mut response=header.into_bytes();response.extend(payload);let deadline=Instant::now()+Duration::from_secs(5);let mut offset=0;
 while offset<response.len(){let remain=deadline.saturating_duration_since(Instant::now());if remain.is_zero()||req.client.set_write_timeout(Some(remain)).is_err(){break}match req.client.write(&response[offset..]){Ok(0)|Err(_)=>break,Ok(n)=>offset+=n}}
}
fn parse(mut client:UnixStream)->Result<AgentRequest,Box<dyn std::error::Error>>{
 let deadline=Instant::now()+Duration::from_secs(3);let mut data=Vec::new();let mut parsed=None;
 loop {
  let remain=deadline.saturating_duration_since(Instant::now());if remain.is_zero(){return Err("request timeout".into())}client.set_read_timeout(Some(remain))?;
  let mut buf=[0u8;4096];let n=client.read(&mut buf)?;if n==0{return Err("incomplete request".into())}data.extend_from_slice(&buf[..n]);
  if parsed.is_none(){if let Some(i)=data.windows(4).position(|w|w==b"\r\n\r\n") {if i>16384{return Err("headers too large".into())}let h=std::str::from_utf8(&data[..i])?;let mut lines=h.lines();let line=lines.next().ok_or("invalid request")?;let parts:Vec<_>=line.split_whitespace().collect();if parts.len()!=3||!matches!(parts[0],"GET"|"POST")||!parts[1].starts_with("/api/agent/")||parts[2]!="HTTP/1.1"{return Err("invalid request".into())}
   let method=parts[0].parse::<tiny_http::Method>().map_err(|_|"invalid method")?;let path=parts[1].to_owned();let mut headers=Vec::new();let mut sizes=Vec::new();let mut tokens=0;
   for l in lines {let (name,value)=l.split_once(':').ok_or("invalid header")?;if name.eq_ignore_ascii_case("transfer-encoding"){return Err("invalid header".into())}if name.eq_ignore_ascii_case("content-length"){sizes.push(value.trim().parse::<usize>()?)}if name.eq_ignore_ascii_case("x-atlas-agent-token"){tokens+=1}headers.push(tiny_http::Header::from_bytes(name,value.trim()).map_err(|_|"invalid header")?)}
   if sizes.len()>1||tokens!=1{return Err("invalid headers".into())}let len=sizes.first().copied().unwrap_or(0);if len>100_000{return Err("body too large".into())}parsed=Some((i,len,method,path,headers));
  }else if data.len()>16384{return Err("headers too large".into())}}
  if let Some((i,len,_,_,_))=&parsed{let have=data.len()-i-4;if have>*len{return Err("trailing bytes".into())}if have==*len{break}}
 }
 let (i,_,method,path,headers)=parsed.unwrap();Ok(AgentRequest{method,path,headers,body:data[i+4..].to_vec(),client,activity:None})
}
struct Slot(Arc<AtomicUsize>);
impl Drop for Slot {fn drop(&mut self){self.0.fetch_sub(1,Ordering::SeqCst);}}
pub fn serve(socket:AgentSocket,handle:impl Fn(AgentRequest)+Send+Sync+'static){let active=Arc::new(AtomicUsize::new(0));let handle=Arc::new(handle);for connection in socket.listener.incoming(){let Ok(client)=connection else{continue};if active.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|n|if n<16{Some(n+1)}else{None}).is_err(){continue}let active=active.clone();let handle=handle.clone();std::thread::spawn(move||{let _slot=Slot(active);if let Ok(req)=parse(client){handle(req)}});}}

#[cfg(test)] mod isolated_tests {
 #[test] fn search_paginates_chunks_preserving_source() {let mut result=serde_json::json!({"results":[{"id":"a","title":"A","chunks":[{"text":"one"},{"text":"two"}]},{"id":"b","title":"B","chunks":[{"text":"three"}]}]});super::search_page(&mut result,1,1);assert_eq!(result["total"],3);assert_eq!(result["results"][0]["id"],"a");assert_eq!(result["results"][0]["chunks"][0]["text"],"two");}
 #[test] fn separate_directory_never_claims_existing_listener(){use super::*;let path=PathBuf::from(format!("/private/tmp/atlas-agent-unit-{}",uuid::Uuid::new_v4().simple()));let uid=unsafe{libc::geteuid()};let socket=AgentSocket::start_at(path.clone(),uid).unwrap();assert!(path.join("agent.sock").exists());assert!(AgentSocket::start_at(path.clone(),uid).is_err());drop(socket);assert!(!path.join("agent.sock").exists());std::fs::remove_dir_all(path).unwrap();}
}

#[cfg(test)]
pub(crate) fn parse_test(client:UnixStream)->Result<AgentRequest,Box<dyn std::error::Error>> { parse(client) }
