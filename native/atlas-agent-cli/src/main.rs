use std::{env, fs::OpenOptions, io::{Read, Write}, os::unix::{fs::{FileTypeExt,MetadataExt,OpenOptionsExt}, io::AsRawFd, net::UnixStream}, time::Duration};
use serde_json::{json, Value};
mod search_retry;

fn secure_file(path: &str) -> Result<String, String> {
 let mut file=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path).map_err(|_| "Не удалось открыть файл ключа".to_owned())?;
 let meta=file.metadata().map_err(|e|e.to_string())?;
 if !meta.is_file() || meta.nlink() != 1 || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {return Err("Файл ключа должен быть доступен только владельцу".into())}
 let mut text=String::new();std::io::Read::by_ref(&mut file).take(4097).read_to_string(&mut text).map_err(|e|e.to_string())?;let text=text.trim();
 if text.is_empty() || text.len()>4096 || text.contains(['\r','\n']) {return Err("Некорректный ключ".into())}Ok(text.to_owned())
}
fn connect_agent()->Result<UnixStream,String>{
 let uid=unsafe{libc::geteuid()};
 #[cfg(feature="test-hook")]
 let dir=if let Some(v)=std::env::var_os("ATLAS_TEST_AGENT_SOCKET_DIR"){let p=std::path::PathBuf::from(v);if !p.is_absolute(){return Err("Некорректный тестовый путь".into())}p}else{std::path::PathBuf::from(format!("/tmp/atlas-agent-{uid}"))};
 #[cfg(not(feature="test-hook"))]
 let dir=std::path::PathBuf::from(format!("/tmp/atlas-agent-{uid}"));
 connect_agent_at(&dir)
}
fn connection_error(stage:&str,error:&std::io::Error)->String {
 let reason=match (stage,error.kind()) {
  ("directory"|"socket"|"connect",std::io::ErrorKind::NotFound)=>"Локальное соединение Atlas не найдено. Запустите Atlas в этой учётной записи и повторите запрос.",
  (_,std::io::ErrorKind::PermissionDenied)=>"Доступ к локальному соединению Atlas запрещён. Проверьте ограничения среды агента (песочницы) и разрешите подключение к локальному сокету; запускайте CLI под той же учётной записью, что и Atlas.",
  ("connect",std::io::ErrorKind::ConnectionRefused)=>"Atlas не принимает локальные подключения. Проверьте, что приложение запущено; если ошибка сохраняется, перезапустите Atlas и повторите запрос.",
  _=>"Не удалось подключиться к Atlas. Проверьте, что приложение работает в той же учётной записи, и повторите запрос.",
 };format!("{reason} Этап: {stage}; системная ошибка: {error}")
}
fn connect_agent_at(dir:&std::path::Path)->Result<UnixStream,String>{
 let uid=unsafe{libc::geteuid()};let m=std::fs::symlink_metadata(dir).map_err(|e|connection_error("directory",&e))?;
 if !m.is_dir()||m.uid()!=uid||m.mode()&0o077!=0{return Err("Небезопасный каталог Atlas".into())}
 let path=dir.join("agent.sock");let m=std::fs::symlink_metadata(&path).map_err(|e|connection_error("socket",&e))?;
 if !m.file_type().is_socket()||m.uid()!=uid||m.mode()&0o077!=0{return Err("Небезопасный сокет Atlas".into())}
 let socket=UnixStream::connect(path).map_err(|e|connection_error("connect",&e))?;
 #[cfg(target_os="macos")] {let mut peer=0;let mut group=0;if unsafe{libc::getpeereid(socket.as_raw_fd(),&mut peer,&mut group)}!=0||peer!=uid{return Err("Недоверенный сервер Atlas".into())}}
 #[cfg(target_os="linux")] {let mut peer:libc::ucred=unsafe{std::mem::zeroed()};let mut size=std::mem::size_of::<libc::ucred>() as libc::socklen_t;if unsafe{libc::getsockopt(socket.as_raw_fd(),libc::SOL_SOCKET,libc::SO_PEERCRED,&mut peer as *mut _ as *mut _,&mut size)}!=0||peer.uid!=uid{return Err("Недоверенный сервер Atlas".into())}}
 Ok(socket)
}
fn agent_error(status:u16,value:&Value)->String {
 let detail=value["error"].as_str().unwrap_or("неизвестная ошибка");
 let advice=match (status,detail) {
  (403,"Forbidden")=>" Проверьте ключ агента и разрешения на пространство в настройках Atlas; защищённое пространство может требовать открытия в приложении.",
  (404|503,"Workspace unavailable")=>" Откройте пространство в Atlas и проверьте подключение внешнего носителя.",
  (503,"Search mode disabled")=>" Включите нужный режим в настройках поиска Atlas.",
  (503,"Model unavailable")=>" Проверьте загрузку модели в настройках поиска Atlas и повторите запрос после её подготовки.",
  _=>"",
 };format!("Atlas: {detail} ({status}).{advice}")
}
fn resolve_workspace(input:&str,key:&str)->Result<String,String>{
 if input=="default"||(input.len()==32&&input.bytes().all(|b|b.is_ascii_hexdigit())){return Ok(input.to_owned())}
 if input.is_empty(){return Err("Укажите название или идентификатор пространства; список: atlas-agent --key-file ФАЙЛ workspaces".into())}
 let (status,value)=request("GET","/api/agent/workspaces",None,key,Duration::from_secs(60))?;
 if status>=400{return Err(agent_error(status,&value))}
 let entries=value["workspaces"].as_array().ok_or("Atlas вернул некорректный список пространств")?;
 let mut matches=Vec::new();
 for entry in entries {if entry["name"].as_str()!=Some(input){continue}let id=entry["id"].as_str().ok_or("Atlas вернул некорректный идентификатор пространства")?;if !(id=="default"||(id.len()==32&&id.bytes().all(|b|b.is_ascii_hexdigit()))){return Err("Atlas вернул некорректный идентификатор пространства".into())}if !matches.iter().any(|(existing,_)|*existing==id){matches.push((id,entry));}}
 if matches.is_empty(){return Err(format!("Пространство {input:?} не найдено или недоступно этому ключу. Проверьте название и права агента; список: atlas-agent --key-file ФАЙЛ workspaces."))}
 if matches.len()>1{return Err(format!("Несколько пространств называются {input:?}. Укажите идентификатор: {}",matches.iter().map(|(id,_)|*id).collect::<Vec<_>>().join(", ")))}
 let (id,entry)=matches[0];if entry["available"]==false{return Err(format!("Пространство {input:?} [{id}] недоступно. Откройте его в Atlas, разблокируйте защищённое пространство или подключите внешний носитель."))}Ok(id.to_owned())
}
fn human_output(command:&str,value:&Value)->String {
 match command {
  "workspaces"=>if let Some(items)=value["workspaces"].as_array(){return if items.is_empty(){"Нет доступных пространств".into()}else{items.iter().map(|v|format!("{}  {}{}",v["id"].as_str().unwrap_or("?"),v["name"].as_str().unwrap_or("Без названия"),if v["available"]==false{" (недоступно)"}else{""})).collect::<Vec<_>>().join("\n")}},
  "list"|"search-titles"=>if let Some(items)=value["items"].as_array(){return if items.is_empty(){"Карточки не найдены".into()}else{items.iter().map(|v|format!("{}  [{}]  {}",v["title"].as_str().unwrap_or("Без названия"),v["id"].as_str().unwrap_or("?"),match v["kind"].as_str(){Some("note")=>"Заметка",Some("file")=>"Файл",_=>"Карточка"})).collect::<Vec<_>>().join("\n")}},
  "read"=>if let Some(text)=value["text"].as_str(){return format!("{}\n\n{text}",value["title"].as_str().unwrap_or("Заметка"))},
  "search"=>if let Some(items)=value["results"].as_array(){return if items.is_empty(){"Ничего не найдено".into()}else{let mut number=value["offset"].as_u64().unwrap_or(0);items.iter().flat_map(|v|{let source=format!("{}  [{}]",v["title"].as_str().unwrap_or("Без названия"),v["id"].as_str().unwrap_or("?"));let chunks=v["chunks"].as_array().into_iter().flatten().filter_map(|c|c["text"].as_str()).collect::<Vec<_>>();if chunks.is_empty(){number+=1;vec![format!("# Chunk {number}\n{source}")]}else{chunks.into_iter().map(|text|{number+=1;format!("# Chunk {number}\n{source}\n{text}")}).collect()}}).collect::<Vec<_>>().join("\n\n")}},
  _=>{}
 }
 serde_json::to_string_pretty(value).unwrap_or_else(|_|"{}".into())
}
fn request(method:&str,path:&str,body:Option<String>,key:&str,timeout:Duration)->Result<(u16,Value),String>{request_inner(method,path,body,key,timeout,false)}
fn request_inner(method:&str,path:&str,body:Option<String>,key:&str,timeout:Duration,interruptible:bool)->Result<(u16,Value),String>{
 let socket=connect_agent()?;request_on_stream(method,path,body,key,timeout,interruptible,socket)
}
fn request_on_stream(method:&str,path:&str,body:Option<String>,key:&str,timeout:Duration,interruptible:bool,mut socket:UnixStream)->Result<(u16,Value),String>{
 socket.set_write_timeout(Some(Duration::from_secs(2))).map_err(|e|e.to_string())?;let payload=body.unwrap_or_default();write!(socket,"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).map_err(|e|format!("Не удалось отправить запрос в Atlas: {e}. Проверьте, что Atlas работает; перед повтором изменения проверьте, не сохранилось ли оно уже."))?;
 socket.set_read_timeout(Some(Duration::from_millis(200))).map_err(|e|format!("socket read timeout: {e}"))?;let mut raw=Vec::new();let deadline=std::time::Instant::now()+timeout;loop {
  #[cfg(any(feature="agent-search",feature="agent-upload"))] if interruptible&&INTERRUPTED.load(std::sync::atomic::Ordering::Acquire){return Err("interrupted".into())}
  #[cfg(not(any(feature="agent-search",feature="agent-upload")))] let _=interruptible;
  let remain=deadline.saturating_duration_since(std::time::Instant::now());if remain.is_zero(){return Err("Atlas не ответил за отведённое время. Проверьте, что приложение работает и не занято длительной операцией, затем повторите запрос; перед повтором изменения проверьте, не сохранилось ли оно уже.".into())}
  let mut buf=[0u8;8192];match socket.read(&mut buf){Ok(0)=>break,Ok(n)=>raw.extend_from_slice(&buf[..n]),Err(e) if matches!(e.kind(),std::io::ErrorKind::TimedOut|std::io::ErrorKind::WouldBlock|std::io::ErrorKind::Interrupted)=>continue,Err(e)=>return Err(format!("Соединение с Atlas прервалось при ожидании ответа: {e}. Проверьте работу Atlas; перед повтором изменения проверьте, не сохранилось ли оно уже."))}if raw.len()>8_388_608{return Err("Слишком большой ответ Atlas".into())}
 }if raw.is_empty(){return Err("Atlas закрыл соединение без ответа. Проверьте работу приложения; перед повтором изменения проверьте, не сохранилось ли оно уже.".into())}if raw.len()>8_388_608{return Err("Слишком большой ответ Atlas".into())}let split=raw.windows(4).position(|w|w==b"\r\n\r\n").ok_or("Некорректный ответ Atlas")?;let head=std::str::from_utf8(&raw[..split]).map_err(|_|"Некорректный ответ")?;let status=head.lines().next().and_then(|l|l.split_whitespace().nth(1)).and_then(|s|s.parse::<u16>().ok()).ok_or("Некорректный статус")?;
 let result:Value=serde_json::from_slice(&raw[split+4..]).map_err(|_|"Некорректный JSON ответа")?;
 Ok((status,result))
}
#[cfg(any(feature="agent-search",feature="agent-upload"))]
static INTERRUPTED:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
#[cfg(any(feature="agent-search",feature="agent-upload"))]
extern "C" fn interrupt(_signal:libc::c_int){INTERRUPTED.store(true,std::sync::atomic::Ordering::Release)}
#[cfg(feature="agent-search")]
fn stream_rag(workspace:&str,query:&str,key:&str,json_output:bool,objects:&Option<Vec<String>>,stream:bool)->Result<(),String>{
 use std::sync::{atomic::Ordering,mpsc};
 let mut b=[0u8;16];std::fs::File::open("/dev/urandom").and_then(|mut f|f.read_exact(&mut b)).map_err(|e|e.to_string())?;b[6]=(b[6]&15)|64;b[8]=(b[8]&63)|128;
 let h=b.iter().map(|v|format!("{v:02x}")).collect::<String>();let id=format!("{}-{}-{}-{}-{}",&h[0..8],&h[8..12],&h[12..16],&h[16..20],&h[20..32]);
 INTERRUPTED.store(false,Ordering::Release);let previous=unsafe{libc::signal(libc::SIGINT,interrupt as libc::sighandler_t)};if previous==libc::SIG_ERR{return Err("Не удалось настроить остановку".into())}
 struct Restore(libc::sighandler_t);impl Drop for Restore{fn drop(&mut self){unsafe{libc::signal(libc::SIGINT,self.0);}}}let _restore=Restore(previous);
 if stream {if json_output {println!("{}",json!({"type":"request","request_id":id}));}else{eprintln!("Запрос: {id}");}}
 let prefix=format!("/api/agent/{workspace}/rag");let (tx,rx)=mpsc::channel();let post_path=format!("{prefix}/search");let post_key=key.to_owned();let mut payload=json!({"query":query,"request_id":id});if let Some(ids)=objects{payload["object_ids"]=json!(ids)}let body=payload.to_string();
 std::thread::spawn(move||{let _=tx.send(request("POST",&post_path,Some(body),&post_key,Duration::from_secs(3600)));});
 let mut after=0;let mut cancelled=false;let mut done=false;let mut post_result=None;let mut saw_token=false;let mut finished_at=None;let mut cancel_started=None;let mut cancel_confirmed=false;
 loop {
  if INTERRUPTED.load(Ordering::Acquire)&&!cancelled {cancelled=true;cancel_started=Some(std::time::Instant::now());if !json_output{eprintln!("Останавливаем ответ…")}}
  if cancelled&&!cancel_confirmed {let path=format!("{prefix}/cancel/{id}");cancel_confirmed=matches!(request("POST",&path,Some("{}".into()),key,Duration::from_secs(2)),Ok((200,v)) if v["ok"]==true);}
  if post_result.is_none(){if let Ok(v)=rx.try_recv(){post_result=Some(v);finished_at=Some(std::time::Instant::now())}}
  if !done&&!cancelled {let path=format!("{prefix}/events/{id}?after={after}");match request_inner("GET",&path,None,key,Duration::from_secs(5),true) {
   Ok((200,data))=>{if let Some(events)=data["events"].as_array(){for event in events {let Some(seq)=event["seq"].as_u64() else{continue};if seq<=after{continue}after=seq;if !stream||event["type"]=="results"{continue}if json_output{println!("{}",event)}else{match event["type"].as_str(){Some("token")=>{print!("{}",event["text"].as_str().unwrap_or(""));let _=std::io::stdout().flush();saw_token=true},Some("progress")=>eprintln!("{}",event["text"].as_str().unwrap_or("")),Some("reset")=>eprintln!("\nФормируем ответ заново…"),Some("error")=>eprintln!("{}",event["error"].as_str().unwrap_or("Ошибка")),_=>{}}}}}done=data["done"]==true;},
   Ok((404,_))=>{},Ok((status,v))=>return Err(agent_error(status,&v)),Err(e)=>{if post_result.is_some()&&!INTERRUPTED.load(Ordering::Acquire){return Err(e)}}
  }}
  if cancelled&&(cancel_confirmed||cancel_started.is_some_and(|t|t.elapsed()>Duration::from_secs(3))){break}if done&&post_result.is_some(){break}if finished_at.is_some_and(|at|at.elapsed()>Duration::from_secs(3)){break}if let Some(Err(_))=post_result.as_ref(){break}if let Some(Ok((status,_)))=post_result.as_ref(){if *status>=400{break}}std::thread::sleep(Duration::from_millis(200));
 }
 if cancelled{return Err(if cancel_confirmed{"Ответ остановлен"}else{"Не удалось подтвердить остановку ответа в Atlas"}.into())}if !json_output&&saw_token{println!()}
 let result=post_result.ok_or("Ответ не завершился")??;if result.0>=400{return Err(agent_error(result.0,&result.1))}if let Some(error)=result.1["error"].as_str().filter(|s|!s.is_empty()){return Err(format!("Atlas: {error}"))}let data=rag_cited_result(result.1);if json_output{if stream{println!("{}",json!({"type":"final","data":data}))}else{println!("{}",data)}}else{if !saw_token {println!("{}",data["answer"].as_str().unwrap_or(""));}if let Some(sources)=data["sources"].as_array(){if !sources.is_empty(){println!("\nИсточники:");}for source in sources{let numbers=source["citations"].as_array().into_iter().flatten().filter_map(|n|n.as_u64()).map(|n|format!("[{n}]")).collect::<Vec<_>>().join(" ");println!("{} {} [{}]",numbers,source["source"]["title"].as_str().unwrap_or("Без названия"),source["source"]["id"].as_str().unwrap_or("?"));}}}Ok(())
}
#[cfg(feature="agent-search")]
fn rag_cited_result(mut value:Value)->Value{
 let answer=value["answer"].as_str().unwrap_or("").to_owned();let sources=value["sources"].as_array().cloned().unwrap_or_default();let mut cited=Vec::<Value>::new();
 for (i,source) in sources.into_iter().enumerate(){if answer.contains(&format!("[{}]",i+1)){let reference=json!({"id":source["id"],"title":source["title"]});if let Some(entry)=cited.iter_mut().find(|entry|entry["source"]==reference){entry["citations"].as_array_mut().unwrap().push(json!(i+1));}else{cited.push(json!({"citations":[i+1],"source":reference}));}}}
 if let Some(obj)=value.as_object_mut(){obj.remove("results");obj.insert("sources".into(),json!(cited));}value
}
#[cfg(all(test,feature="agent-search"))]
#[test] fn rag_keeps_only_cited_sources(){let v=rag_cited_result(json!({"answer":"Test [2] and [3]", "sources":[{"id":"a"},{"id":"b","title":"B","text":"first"},{"id":"b","title":"B","text":"second"}],"results":[1]}));assert!(v.get("results").is_none());assert_eq!(v["sources"],json!([{"citations":[2,3],"source":{"id":"b","title":"B"}}]));}
#[cfg(feature="agent-upload")]
fn upload_notice(value:&Value)->Option<&str>{
 if let Some(notice)=value["index_notice"].as_str().filter(|s|!s.is_empty()){return Some(notice)}
 if value["text_status"]=="too_large"{return Some("Текст файла превышает предел индексации 300 МиБ; файл сохранён.")}if value["text_status"]=="unavailable"{return Some("Текст файла недоступен для поиска.")}None
}
#[cfg(feature="agent-upload")]
fn upload_file(workspace:&str,path:&str,request_id:&str,key:&str,json_output:bool)->Result<(),String>{
 use sha2::{Digest,Sha256};use std::sync::atomic::Ordering;
 INTERRUPTED.store(false,Ordering::Release);let previous=unsafe{libc::signal(libc::SIGINT,interrupt as libc::sighandler_t)};if previous==libc::SIG_ERR{return Err("Не удалось настроить остановку".into())}
 struct Restore(libc::sighandler_t);impl Drop for Restore{fn drop(&mut self){unsafe{libc::signal(libc::SIGINT,self.0);}}}let _restore=Restore(previous);
 let mut file=std::fs::File::open(path).map_err(|e|format!("Не удалось открыть файл: {e}"))?;let meta=file.metadata().map_err(|e|e.to_string())?;if !meta.is_file(){return Err("Нужен обычный файл".into())}
 let name=std::path::Path::new(path).file_name().and_then(|x|x.to_str()).ok_or("Некорректное имя файла")?;
 let mut hash=Sha256::new();let mut buf=[0u8;16000];loop {if INTERRUPTED.load(Ordering::Acquire){return Err("Загрузка остановлена".into())}let n=file.read(&mut buf).map_err(|e|e.to_string())?;if n==0{break}hash.update(&buf[..n]);}
 use std::io::Seek;file.rewind().map_err(|e|e.to_string())?;
 let endpoint=format!("/api/agent/{workspace}/uploads");let payload=json!({"action":"begin","name":name,"size":meta.len(),"request_id":request_id,"digest":format!("{:x}",hash.finalize())});
 let deadline=std::time::Instant::now()+Duration::from_secs(60);let mut last_error=None;let start=loop {
  if INTERRUPTED.load(Ordering::Acquire){let cancelled=request("POST",&endpoint,Some(json!({"action":"cancel","request_id":request_id}).to_string()),key,Duration::from_secs(5));return Err(if matches!(cancelled,Ok((200|202,v)) if v["ok"]==true){"Отмена отправлена; завершение очистки выполняется в фоне"}else{"Не удалось подтвердить остановку загрузки"}.into())}
  if std::time::Instant::now()>=deadline{return Err(last_error.unwrap_or_else(||"Превышено время ожидания начала загрузки. Проверьте работу Atlas и повторите запрос.".into()))}
  match request_inner("POST",&endpoint,Some(payload.to_string()),key,Duration::from_secs(5),true){
   Ok((status,value)) if status>=400=>return Err(agent_error(status,&value)),
   Ok((_,value)) if value["pending"]==true=>{last_error=None;std::thread::sleep(Duration::from_millis(200))},
   Ok((_,value))=>break value,
   Err(_) if INTERRUPTED.load(Ordering::Acquire)=>continue,
   Err(e)=>{last_error=Some(e);std::thread::sleep(Duration::from_millis(200))},
  }
 };
 if start["replayed"]==true {if json_output{println!("{start}")}else{if let Some(notice)=upload_notice(&start){eprintln!("{notice}")}println!("{}",serde_json::to_string_pretty(&start).map_err(|e|e.to_string())?)}return Ok(())}
 let upload=start["upload"].as_str().filter(|v|!v.is_empty()).ok_or("Atlas не вернул идентификатор загрузки")?.to_owned();let mut offset=start["offset"].as_u64().unwrap_or(0);if offset>meta.len(){return Err("Некорректная позиция загрузки".into())}file.seek(std::io::SeekFrom::Start(offset)).map_err(|e|e.to_string())?;
 while offset<meta.len(){if INTERRUPTED.load(Ordering::Acquire){let cancelled=request("POST",&endpoint,Some(json!({"action":"cancel","upload":upload}).to_string()),key,Duration::from_secs(5));return Err(if matches!(cancelled,Ok((200|202,v)) if v["ok"]==true){"Отмена запрошена; очистка выполняется в фоне"}else{"Не удалось подтвердить запрос на отмену"}.into())}
  let n=file.read(&mut buf).map_err(|e|e.to_string())?;if n==0{return Err("Файл изменился во время загрузки".into())}let body=json!({"action":"chunk","upload":upload,"offset":offset,"bytes":&buf[..n]}).to_string();let reply=request_inner("POST",&endpoint,Some(body),key,Duration::from_secs(60),true);let (status,result)=match reply {Ok(v)=>v,Err(_e) if INTERRUPTED.load(Ordering::Acquire)=>{let cancelled=request("POST",&endpoint,Some(json!({"action":"cancel","upload":upload}).to_string()),key,Duration::from_secs(5));return Err(if matches!(cancelled,Ok((200|202,v)) if v["ok"]==true){"Отмена запрошена; очистка выполняется в фоне"}else{"Не удалось подтвердить запрос на отмену"}.into())},Err(e)=>return Err(e)};if status>=400{return Err(agent_error(status,&result))}let next=result["offset"].as_u64().ok_or("Atlas не подтвердил позицию")?;if next!=offset+n as u64{return Err("Некорректное подтверждение загрузки".into())}offset=next;if !json_output{eprint!("\rЗагружено: {offset}/{} байт",meta.len());let _=std::io::stderr().flush();}}
 if INTERRUPTED.load(Ordering::Acquire){let cancelled=request("POST",&endpoint,Some(json!({"action":"cancel","upload":upload}).to_string()),key,Duration::from_secs(5));return Err(if matches!(cancelled,Ok((200|202,v)) if v["ok"]==true){"Отмена запрошена; очистка выполняется в фоне"}else{"Не удалось подтвердить запрос на отмену"}.into())}
 if !json_output{eprintln!()}let finish=request_inner("POST",&endpoint,Some(json!({"action":"finish","upload":upload}).to_string()),key,Duration::from_secs(60),true);let (status,result)=match finish{Ok(v)=>v,Err(_) if INTERRUPTED.load(Ordering::Acquire)=>{let cancelled=request("POST",&endpoint,Some(json!({"action":"cancel","upload":upload}).to_string()),key,Duration::from_secs(5));return Err(if matches!(cancelled,Ok((200|202,v)) if v["ok"]==true){"Отмена запрошена; сохранение могло завершиться, проверьте результат"}else{"Состояние сохранения неизвестно; проверьте результат"}.into())},Err(e)=>return Err(e)};if INTERRUPTED.load(Ordering::Acquire){eprintln!("Сохранение завершилось до подтверждения отмены")}if status>=400{return Err(agent_error(status,&result))}if json_output{println!("{result}")}else{if let Some(notice)=upload_notice(&result){eprintln!("{notice}")}println!("{}",serde_json::to_string_pretty(&result).map_err(|e|e.to_string())?)}Ok(())
}
fn search_payload(query:&str,mode:Option<&str>,objects:Option<&[String]>,offset:usize,limit:usize)->String{let mut value=json!({"query":query,"offset":offset,"limit":limit});if let Some(m)=mode{value["mode"]=json!(m)}if let Some(ids)=objects{value["object_ids"]=json!(ids)}value.to_string()}
#[cfg(any(feature="agent-write",feature="agent-upload"))]
fn validate_request_id(id:&str)->Result<(),String>{
 if id.is_empty()||id.len()>128||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'){return Err("Некорректный идентификатор запроса: используйте 1–128 латинских букв, цифр, дефисов или подчёркиваний".into())}Ok(())
}
#[cfg(feature="agent-write")]
fn write_command(args:&[String])->Result<(&'static str,String,Option<String>),String>{
 let valid=|id:&str|id=="default"||(!id.is_empty()&&id.bytes().all(|b|b.is_ascii_hexdigit()||b==b'-'));
 match args.first().map(String::as_str){
  Some("create") if args.len()==5&&valid(&args[1])=>{
   let mut text=String::new();std::fs::File::open(&args[3]).map_err(|e|format!("Не удалось открыть файл: {e}"))?.take(64_001).read_to_string(&mut text).map_err(|e|e.to_string())?;if text.len()>64_000{return Err("Файл заметки слишком большой".into())}
   let id=&args[4];validate_request_id(id)?;
   Ok(("POST",format!("/api/agent/{}/notes",args[1]),Some(json!({"title":args[2],"text":text,"request_id":id}).to_string())))
  },
  Some("edit") if args.len()>=4&&valid(&args[1])&&valid(&args[2])=>{
   let action=args[3].as_str();let payload=match action{
    "acquire" if args.len()==4=>json!({"action":"acquire"}),
    "renew"|"release" if args.len()==5=>json!({"action":action,"lease":args[4]}),
    "commit" if args.len()==7=>{let mut text=String::new();std::fs::File::open(&args[6]).map_err(|e|format!("Не удалось открыть файл: {e}"))?.take(80_001).read_to_string(&mut text).map_err(|e|e.to_string())?;if text.len()>80_000{return Err("Файл заметки слишком большой".into())}json!({"action":"commit","lease":args[4],"title":args[5],"text":text})},
    _=>return Err("Использование: edit WORKSPACE NOTE acquire|renew LEASE|release LEASE|commit LEASE TITLE FILE".into())
   };Ok(("POST",format!("/api/agent/{}/notes/{}/edit",args[1],args[2]),Some(payload.to_string())))
  },_=>Err("Некорректные аргументы создания или редактирования. Проверьте идентификатор заметки и порядок аргументов: atlas-agent --help".into())
 }
}
fn run()->Result<(),String>{
 let mut args:Vec<String>=env::args().skip(1).collect();
 if args.len()==1 && matches!(args[0].as_str(),"--help"|"-h"){
  println!("Atlas — доступ к открытым пространствам\n\nИспользование: atlas-agent --key-file ФАЙЛ [--json] [параметры] команда\n\nКоманды:\n  workspaces                         Доступные пространства\n  read ПРОСТРАНСТВО ID               Прочитать текст заметки или индексируемого файла\n  list ПРОСТРАНСТВО                  Список карточек\n  search-titles ПРОСТРАНСТВО ЗАПРОС  Поиск по названиям карточек\n  search ПРОСТРАНСТВО ЗАПРОС         Поиск по словам\n\nПараметры (перед командой):\n  --offset N --limit N              Диапазон: поиск и список 0/10, чтение 0/4000 символов\n  --json                            Данные в JSON для обработки программой");
  #[cfg(feature="agent-search")] println!("  --objects ID1,ID2                 Поиск только по указанным объектам\n\nРежимы поиска (после запроса):\n  semantic                          Поиск по смыслу\n  hybrid                            Поиск по словам и смыслу\n  rag ПРОСТРАНСТВО ВОПРОС [--stream] Умный поиск; --stream показывает ответ по мере генерации\n  cancel ПРОСТРАНСТВО ID            Остановить умный поиск");
  #[cfg(feature="agent-upload")] println!("\n  upload ПРОСТРАНСТВО ФАЙЛ ID       Добавить файл (ID — уникальный идентификатор запроса)");
  #[cfg(feature="agent-write")] println!("  create ПРОСТРАНСТВО НАЗВАНИЕ ФАЙЛ ID  Создать заметку из текста файла\n  delete ПРОСТРАНСТВО ID             Удалить карточку в буфер\n  edit ПРОСТРАНСТВО ID acquire       Занять заметку для редактирования\n  edit ПРОСТРАНСТВО ID renew|release БЛОКИРОВКА\n  edit ПРОСТРАНСТВО ID commit БЛОКИРОВКА НАЗВАНИЕ ФАЙЛ");
  println!("\nПример: atlas-agent --key-file ~/.atlas-key --objects ID search ПРОСТРАНСТВО 'тема' semantic\nПРОСТРАНСТВО — точное название или идентификатор из workspaces. Если названия совпадают, укажите идентификатор; default и 32-значный ID имеют приоритет. Путь к файлу ключа указывается без содержимого ключа.");return Ok(())
 }
 if args.len()<3{return Err("Укажите --key-file и команду; справка: atlas-agent --help".into())}
 let mut key=None;let mut output_json=false;let mut i=0;let mut offset=0usize;let mut requested_limit:Option<usize>=None;let mut page_set=false;
 #[cfg(feature="agent-search")] let mut objects:Option<Vec<String>>=None;
 while i<args.len() {match args[i].as_str(){"--key-file"=>{i+=1;key=Some(args.get(i).ok_or("Нет файла ключа")?.clone());},"--json"=>output_json=true,"--offset"|"--limit"=>{let which=args[i].as_str();i+=1;let n=args.get(i).ok_or("Не указан диапазон поиска")?.parse::<usize>().map_err(|_|"Некорректный диапазон поиска")?;if which=="--offset"{offset=n}else{if n==0||n>10000{return Err("Некорректный диапазон".into())}requested_limit=Some(n)}page_set=true},#[cfg(feature="agent-search")] "--objects"=>{i+=1;let raw=args.get(i).ok_or("Нет списка объектов")?;if objects.is_some(){return Err("Область задана несколько раз".into())}let ids:Vec<String>=if raw.is_empty(){vec![]}else{raw.split(',').map(str::to_owned).collect()};if ids.len()>1000||ids.iter().any(|id|id.is_empty()||!id.bytes().all(|b|b.is_ascii_hexdigit()||b==b'-')){return Err("Некорректный список объектов".into())}objects=Some(ids)},_=>break}i+=1;}
 let key_path=key.ok_or("Укажите файл ключа")?;let key=secure_file(&key_path)?;
 let command_value=args.get(i).ok_or("Нет команды")?.clone();let command=command_value.as_str();
 if matches!(command,"read"|"search"|"list"|"search-titles"|"rag"|"cancel"|"upload"|"create"|"edit"|"delete"){let workspace=args.get(i+1).ok_or("Укажите название или идентификатор пространства; справка: atlas-agent --help")?.clone();args[i+1]=resolve_workspace(&workspace,&key)?;}
 let valid_id=|s:&String|s=="default"||(!s.is_empty()&&s.bytes().all(|b|b.is_ascii_hexdigit()||b==b'-'));
 if page_set&&!matches!(command,"search"|"read"|"list"|"search-titles"){return Err("Диапазон поддерживается только для поиска, списка и чтения".into())}
 let limit=requested_limit.unwrap_or(if command=="read"{4000}else{10});if (command=="search"&&offset>10000)||(matches!(command,"search"|"list"|"search-titles")&&limit>128){return Err("Некорректный диапазон поиска".into())}
 #[cfg(feature="agent-upload")]
 if command=="upload" {if args.len()!=i+4||!valid_id(&args[i+1]){return Err("Использование: upload ПРОСТРАНСТВО ФАЙЛ ID_ЗАПРОСА; укажите путь к локальному файлу и уникальный идентификатор запроса".into())}validate_request_id(&args[i+3])?;return upload_file(&args[i+1],&args[i+2],&args[i+3],&key,output_json)}
 #[cfg(feature="agent-search")]
 if command=="rag" {let stream=args.len()==i+4&&args[i+3]=="--stream";if (!stream&&args.len()!=i+3)||!valid_id(&args[i+1]){return Err("Использование: rag WORKSPACE QUERY [--stream]".into())}return stream_rag(&args[i+1],&args[i+2],&key,output_json,&objects,stream)}
 #[cfg(feature="agent-search")]
 if objects.is_some()&&!matches!(command,"search"|"rag"){return Err("Область объектов поддерживается только для поиска".into())}
 let query_body=|query:&str,mode:Option<&str>|{#[cfg(feature="agent-search")] let scope=objects.as_deref();#[cfg(not(feature="agent-search"))] let scope=None;search_payload(query,mode,scope,offset,limit)};
 let (method,path,body) = match command {
  #[cfg(feature="agent-write")]
  "delete" if args.len()==i+3 && valid_id(&args[i+1]) && valid_id(&args[i+2]) => ("POST",format!("/api/agent/{}/objects/{}/delete",args[i+1],args[i+2]),Some("{}".into())),
  "workspaces" if args.len()==i+1=>("GET","/api/agent/workspaces".to_owned(),None),
  "list" if args.len()==i+2 && valid_id(&args[i+1]) => ("POST",format!("/api/agent/{}/objects/list",args[i+1]),Some(json!({"offset":offset,"limit":limit,"query":null}).to_string())),
  "search-titles" if args.len()==i+3 && valid_id(&args[i+1]) && !args[i+2].trim().is_empty() => ("POST",format!("/api/agent/{}/objects/list",args[i+1]),Some(json!({"offset":offset,"limit":limit,"query":args[i+2]}).to_string())),

  "read" if args.len()==i+3 && valid_id(&args[i+1]) && valid_id(&args[i+2]) => ("GET",format!("/api/agent/{}/notes/{}?offset={offset}&limit={limit}",args[i+1],args[i+2]),None),
  "search" if args.len()==i+3&&valid_id(&args[i+1]) => ("POST",format!("/api/agent/{}/search",args[i+1]),Some(query_body(&args[i+2],None))),
  #[cfg(feature="agent-search")]
  "search" if args.len()==i+4 && valid_id(&args[i+1]) && matches!(args[i+3].as_str(),"semantic"|"hybrid") => ("POST",format!("/api/agent/{}/search",args[i+1]),Some(query_body(&args[i+2],Some(&args[i+3])))),
  #[cfg(feature="agent-search")]
  "cancel" if args.len()==i+3 && valid_id(&args[i+1]) && valid_id(&args[i+2]) => ("POST",format!("/api/agent/{}/rag/cancel/{}",args[i+1],args[i+2]),Some("{}".into())),
  #[cfg(feature="agent-write")]
  "create"|"edit"=>write_command(&args[i..])?,
  _=>return Err("Некорректная команда или аргументы. Проверьте название команды, идентификатор объекта и порядок аргументов: atlas-agent --help".into())
 };
 let search_semantic=command=="search"&&args.len()==i+4&&matches!(args[i+3].as_str(),"semantic"|"hybrid");
 let (status,result)=if search_semantic{search_retry::search(||request(method,&path,body.clone(),&key,Duration::from_secs(60)))?}else{request(method,&path,body,&key,Duration::from_secs(60))?};if status>=400{return Err(agent_error(status,&result))}if output_json{println!("{}",serde_json::to_string(&result).map_err(|e|e.to_string())?)}else{println!("{}",human_output(command,&result));if matches!(command,"list"|"search-titles"){if let Some(total)=result["total"].as_u64(){println!("\nВсего: {total}")}if let Some(next)=result["next_offset"].as_u64(){let quote=|s:&str|format!("'{}'",s.replace('\'',"'\\''"));let suffix=if command=="search-titles"{format!(" {}",quote(&args[i+2]))}else{String::new()};println!("Следующая страница:\n{} --key-file {} --offset {} --limit {} {} {}{}",quote(&env::args().next().unwrap_or_else(||"atlas-agent".into())),quote(&key_path),next,limit,command,quote(&args[i+1]),suffix);}}if command=="read"{if let Some(next)=result["next_offset"].as_u64(){let quote=|s:&str|format!("'{}'",s.replace('\'',"'\\''"));println!("\nДальше есть текст. Продолжить:\n{} --key-file {} --offset {} --limit {} read {} {}",quote(&env::args().next().unwrap_or_else(||"atlas-agent".into())),quote(&key_path),next,limit,quote(&args[i+1]),quote(&args[i+2]));}}}Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("{e}");std::process::exit(1)}}

#[cfg(test)]
mod tests {
 use super::*;
 use std::os::unix::fs::{PermissionsExt, symlink};
 #[cfg(feature="agent-upload")]
 #[test] fn uploaded_file_notice_matches_server_status(){
  assert_eq!(upload_notice(&json!({"text_status":"unavailable","index_notice":"Текст файла превышает предел индексации 300 МиБ"})),Some("Текст файла превышает предел индексации 300 МиБ"));
  assert!(upload_notice(&json!({"text_status":"too_large"})).unwrap().contains("300 МиБ"));
  assert!(upload_notice(&json!({"text_status":"available"})).is_none());
 }
 #[test] fn search_scope_distinguishes_all_from_none(){
  let all:Value=serde_json::from_str(&search_payload("q",None,None,0,10)).unwrap();assert!(all.get("object_ids").is_none());
  let empty:Value=serde_json::from_str(&search_payload("q",Some("hybrid"),Some(&[]),10,10)).unwrap();assert_eq!(empty["object_ids"],json!([]));assert_eq!(empty["mode"],"hybrid");assert_eq!(empty["offset"],10);
 }
 #[cfg(feature="agent-write")]
 #[test] fn write_request_uses_real_private_socket_without_test_hooks(){
  use std::os::unix::net::UnixListener;
  let dir=std::env::temp_dir().join(format!("atlas-cli-write-socket-{}",std::process::id()));std::fs::create_dir(&dir).unwrap();std::fs::set_permissions(&dir,std::fs::Permissions::from_mode(0o700)).unwrap();let path=dir.join("agent.sock");let listener=UnixListener::bind(&path).unwrap();std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600)).unwrap();
  let worker=std::thread::spawn(move||{let(mut peer,_)=listener.accept().unwrap();let mut data=Vec::new();let mut buffer=[0u8;1024];loop{let n=peer.read(&mut buffer).unwrap();assert!(n>0);data.extend_from_slice(&buffer[..n]);if let Some(i)=data.windows(4).position(|w|w==b"\r\n\r\n"){let h=std::str::from_utf8(&data[..i]).unwrap();let n:usize=h.lines().find_map(|v|v.strip_prefix("Content-Length: ")).unwrap().parse().unwrap();if data.len()>=i+4+n{assert!(h.starts_with("POST /api/agent/default/notes HTTP/1.1"));assert_eq!(serde_json::from_slice::<Value>(&data[i+4..]).unwrap()["request_id"],"request-1");break}}}peer.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").unwrap();});
  let file=dir.join("note.md");std::fs::write(&file,"text").unwrap();let args=vec!["create".into(),"default".into(),"title".into(),file.to_string_lossy().to_string(),"request-1".into()];let(method,route,body)=write_command(&args).unwrap();let stream=connect_agent_at(&dir).unwrap();let (status,result)=request_on_stream(method,&route,body,"key",Duration::from_secs(5),false,stream).unwrap();assert_eq!(status,201);assert_eq!(result["ok"],true);worker.join().unwrap();std::fs::remove_file(path).unwrap();std::fs::remove_file(file).unwrap();std::fs::remove_dir(dir).unwrap();
 }
 #[cfg(any(feature="agent-write",feature="agent-upload"))]
 #[test] fn request_id_errors_explain_allowed_characters(){
  assert!(validate_request_id("request-1_ok").is_ok());
  for id in ["", "with spaces", "не-латиница"] {let error=validate_request_id(id).unwrap_err();assert!(error.contains("1–128")&&error.contains("латинских"),"{error}");}
  assert!(validate_request_id(&"a".repeat(129)).is_err());
 }
 #[cfg(feature="agent-write")]
 #[test] fn write_commands_explain_invalid_arguments(){
  for args in [vec!["create".into(),"default".into()],vec!["edit".into(),"default".into(),"wrong-id".into(),"acquire".into()]] {
   let error=write_command(&args).unwrap_err();assert!(error.contains("аргумент")&&error.contains("--help"),"{error}");
  }
 }
 #[cfg(feature="agent-write")]
 #[test] fn write_commands_build_expected_payloads_without_test_hooks(){
  let file=std::env::temp_dir().join(format!("atlas-cli-write-{}",std::process::id()));std::fs::write(&file,"Текст\n").unwrap();
  let args=vec!["create".into(),"default".into(),"Тема".into(),file.to_string_lossy().to_string(),"request-1".into()];let (method,path,body)=write_command(&args).unwrap();assert_eq!(method,"POST");assert_eq!(path,"/api/agent/default/notes");assert_eq!(serde_json::from_str::<Value>(&body.unwrap()).unwrap(),json!({"title":"Тема","text":"Текст\n","request_id":"request-1"}));
  let args=vec!["edit".into(),"default".into(),"abcd".into(),"acquire".into()];assert_eq!(serde_json::from_str::<Value>(&write_command(&args).unwrap().2.unwrap()).unwrap(),json!({"action":"acquire"}));
  let args=vec!["edit".into(),"default".into(),"abcd".into(),"commit".into(),"lease-1".into(),"Title".into(),file.to_string_lossy().to_string()];let (_,route,body)=write_command(&args).unwrap();assert_eq!(route,"/api/agent/default/notes/abcd/edit");assert_eq!(serde_json::from_str::<Value>(&body.unwrap()).unwrap(),json!({"action":"commit","lease":"lease-1","title":"Title","text":"Текст\n"}));std::fs::remove_file(file).unwrap();
 }
 #[test] fn human_results_show_sources_and_text(){
  let value=json!({"results":[{"id":"abc","title":"Тема","chunks":[{"text":"Фрагмент"}]}]});let text=human_output("search",&value);assert!(text.contains("Тема  [abc]"));assert!(text.contains("Фрагмент"));
  assert_eq!(human_output("read",&json!({"title":"Заметка","text":"Содержимое"})),"Заметка\n\nСодержимое");
 }
 #[test] fn server_errors_explain_recovery(){
  assert!(agent_error(403,&json!({"error":"Forbidden"})).contains("ключ"));
  assert!(agent_error(503,&json!({"error":"Workspace unavailable"})).contains("Откройте пространство"));
  assert!(agent_error(503,&json!({"error":"Search mode disabled"})).contains("настройках поиска"));
  assert!(agent_error(503,&json!({"error":"other"})).contains("other"));
 }
 #[test] fn connection_errors_explain_recovery(){
  use std::io::{Error,ErrorKind};
  let missing=connection_error("socket",&Error::from(ErrorKind::NotFound));assert!(missing.contains("Запустите Atlas"));assert!(connection_error("connect",&Error::from(ErrorKind::NotFound)).contains("Запустите Atlas"));
  let denied=connection_error("connect",&Error::from_raw_os_error(libc::EPERM));assert!(denied.contains("огранич")&&denied.contains("агента"));
  let refused=connection_error("connect",&Error::from(ErrorKind::ConnectionRefused));assert!(refused.contains("перезапуст"));
  let other=connection_error("connect",&Error::from_raw_os_error(libc::EMFILE));assert!(other.contains("24"));
 }
 #[test] fn connect_checks_private_socket_and_directory(){
  use std::os::unix::net::UnixListener;
  let dir=std::env::temp_dir().join(format!("atlas-cli-peer-{}",std::process::id()));std::fs::create_dir(&dir).unwrap();std::fs::set_permissions(&dir,std::fs::Permissions::from_mode(0o700)).unwrap();
  let path=dir.join("agent.sock");let listener=UnixListener::bind(&path).unwrap();std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600)).unwrap();
  let stream=connect_agent_at(&dir).unwrap();let (_peer,_)=listener.accept().unwrap();drop(stream);std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o666)).unwrap();assert!(connect_agent_at(&dir).is_err());drop(listener);std::fs::remove_file(path).unwrap();std::fs::remove_dir(dir).unwrap();
 }
 #[test] fn secret_file_rejects_public_permissions_and_symlinks(){
  let dir=std::env::temp_dir().join(format!("atlas-cli-key-test-{}",std::process::id()));std::fs::create_dir(&dir).unwrap();
  let key=dir.join("key");std::fs::write(&key,"secret").unwrap();std::fs::set_permissions(&key,std::fs::Permissions::from_mode(0o600)).unwrap();
  assert_eq!(secure_file(key.to_str().unwrap()).unwrap(),"secret");
  let link=dir.join("link");symlink(&key,&link).unwrap();assert!(secure_file(link.to_str().unwrap()).is_err());
  std::fs::set_permissions(&key,std::fs::Permissions::from_mode(0o644)).unwrap();assert!(secure_file(key.to_str().unwrap()).is_err());
  std::fs::remove_dir_all(dir).unwrap();
 }
}
