#!/usr/bin/env python3
"""Exercise the real downloader with tiny local HTTP fixtures; no user files touched.
Usage: python3 atlas-nemotron-loader-tests.py DOWNLOADER_RS RUSTC LIBC_RLIB
Requires a local loopback listener and system curl/shasum (macOS) or sha256sum (Linux).
"""
import hashlib, http.server, os, pathlib, subprocess, sys, tempfile, threading, time
from socketserver import ThreadingMixIn

source, rustc, libc = map(pathlib.Path, sys.argv[1:])
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self, *args): pass
 def do_GET(self):
  content = b'bad' if self.path == '/bad' else b'abc'
  self.send_response(200); self.send_header('Content-Length', str(len(content))); self.end_headers()
  try:
   if self.path in ('/slow',):
    self.wfile.write(content[:1]); self.wfile.flush(); time.sleep(1)
    self.wfile.write(content[1:]); self.wfile.flush()
   else: self.wfile.write(content)
  except (BrokenPipeError,ConnectionResetError): pass
class Server(ThreadingMixIn, http.server.HTTPServer): daemon_threads=True
server=Server(('127.0.0.1', 0),Handler); threading.Thread(target=server.serve_forever, daemon=True).start()
try:
 with tempfile.TemporaryDirectory(prefix='atlas-loader-test-') as tmp:
  root=pathlib.Path(tmp); text=source.read_text()
  import re
  text,n=re.subn(r'const URL: &str = "[^"]+";', 'fn test_url() -> String { std::env::var("ATLAS_TEST_DOWNLOAD_URL").unwrap() }',text); assert n==1
  text,n=re.subn(r'const SHA: &str = "[0-9a-f]+";', 'const SHA: &str = "'+hashlib.sha256(b'abc').hexdigest()+'";',text); assert n==1
  text,n=re.subn(r'const SIZE: u64 = [0-9_]+;', 'const SIZE: u64 = 3;',text); assert n==1
  old='let mut child = Command::new("/usr/bin/curl").args(["--fail", "--location", "--silent", "--show-error", "--max-time", "21600", URL])'
  new='let url = test_url(); let mut child = Command::new("/usr/bin/curl").args(["--fail", "--location", "--silent", "--show-error", "--max-time", "21600", url.as_str()])'
  assert text.count(old)==1; text=text.replace(old,new)
  text+='''\nfn main() {
 let args: Vec<String> = std::env::args().collect();
 let mode=&args[1]; let root=std::path::PathBuf::from(&args[2]);
 start(root.clone()).expect("start");
 if mode=="cancel" || mode=="conflict" {
  let until=std::time::Instant::now()+std::time::Duration::from_secs(10);
  loop {
   let ready=temp().lock().unwrap().as_ref().and_then(|p|std::fs::metadata(p).ok()).is_some_and(|m|m.len()>0);
   if ready { break; } assert!(std::time::Instant::now()<until,"download not started"); std::thread::sleep(std::time::Duration::from_millis(5));
  }
  if mode=="cancel" { cleanup(); } else { std::fs::write(root.join("models/nemotron-3.5-bolt-e4b-q4.gguf"),b"existing").unwrap(); }
 }
 let until=std::time::Instant::now()+std::time::Duration::from_secs(10);
 loop {let s=status(root.clone()); if s=="complete" || s.starts_with("error:") {println!("{s}");break} assert!(std::time::Instant::now()<until,"download timeout: {s}");std::thread::sleep(std::time::Duration::from_millis(10));}
}\n'''
  rs=root/'fixture.rs'; rs.write_text(text); binary=root/'fixture'
  subprocess.run([str(rustc),'--edition=2021',str(rs),'--extern',f'libc={libc}','-L',f'dependency={libc.parent}','-o',str(binary)],check=True)
  for mode,endpoint in [('success','/ok'),('bad_sha','/bad'),('cancel','/slow'),('conflict','/slow')]:
   directory=root/mode; directory.mkdir()
   env=os.environ.copy(); env.update(ATLAS_TEST_DOWNLOAD_URL=f'http://127.0.0.1:{server.server_port}{endpoint}',NO_PROXY='127.0.0.1',no_proxy='127.0.0.1')
   result=subprocess.run([str(binary),mode,str(directory)],env=env,text=True,capture_output=True,timeout=20,check=True)
   status=result.stdout.strip(); output=directory/'models/nemotron-3.5-bolt-e4b-q4.gguf'
   if mode=='success': assert status=='complete' and output.read_bytes()==b'abc', (mode,status)
   elif mode=='conflict': assert status.startswith('error:') and output.read_bytes()==b'existing',(mode,status)
   else: assert status.startswith('error:') and not output.exists(),(mode,status)
   assert not list((directory/'models').glob('.nemotron-e4b-download-*')),mode
   print('PASS',mode,status,flush=True)
finally:
 server.shutdown(); server.server_close()
