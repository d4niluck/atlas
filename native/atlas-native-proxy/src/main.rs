use std::{env, fs, io::{self, BufRead, Read, Write}, path::PathBuf, sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}}};
use tiny_http::{Server, Response, Header, StatusCode};
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
 let args: Vec<String> = env::args().collect();
 if args.len()!=4 { return Err("usage: proxy UI_DIR BACKEND_PORT LISTEN_PORT".into()); }
 let root = fs::canonicalize(&args[1])?; let backend: u16=args[2].parse()?; let port:u16=args[3].parse()?;
 let mut lines=io::stdin().lock().lines(); let bootstrap=lines.next().ok_or("no bootstrap")??; let token=lines.next().ok_or("no token")??;
 if bootstrap.len()<32 || token.len()<32 {return Err("short secret".into());}
 let mut random=[0u8;32];fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
 let cookie=random.iter().map(|n|format!("{n:02x}")).collect::<String>();
 let consumed=Arc::new(AtomicBool::new(false)); let listener=Server::http(format!("127.0.0.1:{port}"))?;
 let port=listener.server_addr().to_ip().ok_or("non-IP listener")?.port();
 println!("{port}"); io::stdout().flush()?;
 let agent=ureq::AgentBuilder::new().redirects(0).timeout_connect(std::time::Duration::from_secs(5)).timeout_read(std::time::Duration::from_secs(30)).timeout_write(std::time::Duration::from_secs(30)).build();
 let rag_agent=ureq::AgentBuilder::new().redirects(0).timeout_connect(std::time::Duration::from_secs(5)).timeout_read(std::time::Duration::from_secs(180)).timeout_write(std::time::Duration::from_secs(30)).build();
 let active=Arc::new(AtomicUsize::new(0));
 for mut req in listener.incoming_requests() {
  if active.fetch_add(1,Ordering::SeqCst)>=16 { active.fetch_sub(1,Ordering::SeqCst); let _=req.respond(Response::empty(503)); continue; }
  let active=active.clone();
  let root=root.clone(); let bootstrap=bootstrap.clone(); let token=token.clone(); let cookie=cookie.clone(); let consumed=consumed.clone(); let agent=agent.clone(); let rag_agent=rag_agent.clone();
  std::thread::spawn(move || {
   struct Guard(Arc<AtomicUsize>); impl Drop for Guard { fn drop(&mut self){self.0.fetch_sub(1,Ordering::SeqCst);} } let _guard=Guard(active);
  let host=format!("127.0.0.1:{port}"); let origin=format!("http://{host}");
  let headers=req.headers(); let values=|name:&'static str| headers.iter().filter(|h|h.field.equiv(name)).map(|h|h.value.as_str()).collect::<Vec<_>>();
  let hosts=values("Host"); let origins=values("Origin"); let cookies=values("Cookie");
  let allowed=hosts.len()==1 && hosts[0]==host && origins.len()<=1 && origins.first().is_none_or(|o|*o==origin);
  if !allowed {let _=req.respond(Response::empty(403));return;}
  let path=req.url().to_owned();
  if path==format!("/bootstrap/{bootstrap}") && req.method()==&tiny_http::Method::Get {
   if consumed.swap(true,Ordering::SeqCst) {let _=req.respond(Response::empty(403));return;}
   let response=Response::empty(303).with_header(Header::from_bytes("Location","/").unwrap()).with_header(Header::from_bytes("Set-Cookie",format!("atlas_session={cookie}; HttpOnly; SameSite=Strict; Path=/")).unwrap()).with_header(Header::from_bytes("Cache-Control","no-store").unwrap());let _=req.respond(response);return;
  }
  if cookies.len()!=1 || !cookies[0].split(';').any(|v| v.trim()==format!("atlas_session={cookie}")) {let _=req.respond(Response::empty(403));return;}
  if path.starts_with("/api/") || path=="/api" {
   let target=format!("http://127.0.0.1:{backend}{path}"); let selected=if path.split('?').next().unwrap_or("").ends_with("/rag/search") {&rag_agent} else {&agent}; let mut call=selected.request(req.method().as_str(),&target).set("X-Atlas-Token",&token);
   for h in req.headers() { if h.field.equiv("Content-Type") || h.field.equiv("Content-Length") {call=call.set(h.field.as_str().as_str(),h.value.as_str());} }
   let response=match call.send(req.as_reader()) {Ok(r)=>r,Err(ureq::Error::Status(_,r))=>r,Err(_)=>{let _=req.respond(Response::empty(502));return;}};
   let status=response.status(); let mut hs=Vec::new();
   for name in response.headers_names() { if ["content-length","transfer-encoding","connection","set-cookie"].contains(&name.to_ascii_lowercase().as_str()) { continue; } if let Some(v)=response.header(&name) {if let Ok(h)=Header::from_bytes(name.as_str(),v){hs.push(h);}} }
   let len=response.header("Content-Length").and_then(|s|s.parse::<usize>().ok()); let _=req.respond(Response::new(StatusCode(status),hs,response.into_reader(),len,None));return;
  }
  let relative=path.split('?').next().unwrap_or("").trim_start_matches('/'); let relative=if relative.is_empty(){"index.html"}else{relative};
  let candidate=match fs::canonicalize(root.join(PathBuf::from(relative))) {Ok(p) if p.starts_with(&root) && p.is_file()=>p,_=>{let _=req.respond(Response::empty(404));return;}};
  let file=match fs::File::open(&candidate){Ok(f)=>f,Err(_)=>{let _=req.respond(Response::empty(404));return;}};
  let mime=match candidate.extension().and_then(|s|s.to_str()).unwrap_or(""){"html"=>"text/html; charset=utf-8","js"|"mjs"=>"text/javascript; charset=utf-8","css"=>"text/css; charset=utf-8","svg"=>"image/svg+xml","png"=>"image/png","woff2"=>"font/woff2","woff"=>"font/woff","ttf"=>"font/ttf","wasm"=>"application/wasm",_=>"application/octet-stream"};
  let response=Response::from_file(file).with_header(Header::from_bytes("Content-Type",mime).unwrap()).with_header(Header::from_bytes("X-Content-Type-Options","nosniff").unwrap()).with_header(Header::from_bytes("Referrer-Policy","no-referrer").unwrap()).with_header(Header::from_bytes("Content-Security-Policy","default-src 'self'; connect-src 'self'; img-src 'self' data: blob:; media-src 'self' blob:; frame-src 'self' blob:; style-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; worker-src 'self' blob:").unwrap());let _=req.respond(response);
  });
 }
 Ok(())
}
