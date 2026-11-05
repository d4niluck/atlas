#!/usr/bin/env python3
"""Exercise all six Atlas downloaders with a local fake server and temporary files only.
Usage: python3 check.py /path/to/atlas
Run on macOS/Linux; for a Windows port adapt only the fixture hooks, not assertions.
"""
import hashlib,http.server,pathlib,re,subprocess,tempfile,threading,time,os,sys,shutil
ROOT=pathlib.Path(sys.argv[1]).resolve();NAMES=['model_download','q8_model_download','vision_model_download','rag_model_download','rag_e4b_model_download','nemotron_model_download']
GOOD=b'atlas-downloader-synthetic-123'; BAD=b'bad-checksum'; HASH=hashlib.sha256(GOOD).hexdigest()
mode='good'
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  self.send_response(200);self.end_headers()
  try:
   if mode=='delay':time.sleep(.7);self.wfile.write(GOOD)
   elif mode=='slow':
    for _ in range(100):self.wfile.write(b'x'*8192);self.wfile.flush();time.sleep(.025)
   else:self.wfile.write(BAD if mode=='bad' else GOOD)
  except (BrokenPipeError,ConnectionResetError):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='atlas-download-contract-',dir=None) as tmp:
 p=pathlib.Path(tmp);(p/'src').mkdir();(p/'Cargo.toml').write_text('[package]\nname="atlas-download-contract"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nlibc="=0.2.191"\n')
 for name in NAMES:
  s=(ROOT/'ui/src-tauri/src'/f'{name}.rs').read_text()
  s,n=re.subn(r'const URL: &str = "[^\"]*";',f'const URL: &str = "http://127.0.0.1:{server.server_port}/{name}";',s);assert n==1,name
  s,n=re.subn(r'const SHA: &str = "[a-f0-9]+";',f'const SHA: &str = "{HASH}";',s);assert n==1,name
  (p/'src'/f'{name}.rs').write_text(s)
 code='\n'.join(f'mod {n};' for n in NAMES)+r'''
use std::{env,path::PathBuf,time::{Duration,Instant}};
fn main(){
 let a:Vec<String>=env::args().collect();let dir=PathBuf::from(&a[1]);let n=&a[2];let case=&a[3];
 let (start,status,cleanup):(fn(PathBuf)->Result<(),String>,fn(PathBuf)->String,fn())=match n.as_str(){
'''+'\n'.join(f'  "{n}" => ({n}::start,{n}::status,{n}::cleanup),' for n in NAMES)+r'''
 _=>panic!("unknown downloader")};
 let name=match n.as_str(){"model_download"=>"embeddinggemma-2.gguf","q8_model_download"=>"embeddinggemma-2-q8.gguf","vision_model_download"=>"embeddinggemma-2-mmproj-q8.gguf","rag_model_download"=>"gemma-4-e2b-it-q4.gguf","rag_e4b_model_download"=>"gemma-4-e4b-it-q4.gguf","nemotron_model_download"=>"nemotron-3.5-bolt-e4b-q4.gguf",_=>unreachable!()};
 let dest=dir.join("models").join(name);
 if case=="existing"{std::fs::create_dir_all(dest.parent().unwrap()).unwrap();std::fs::write(&dest,b"do-not-replace").unwrap();assert!(start(dir.clone()).is_err());assert_eq!(std::fs::read(&dest).unwrap(),b"do-not-replace");return}
 start(dir.clone()).unwrap();
 if case=="cancel"{std::thread::sleep(Duration::from_millis(120));cleanup();}
 if case=="race"{std::thread::sleep(Duration::from_millis(120));std::fs::write(&dest,b"do-not-replace").unwrap();}
 let deadline=Instant::now()+Duration::from_secs(15);
 loop{let s=status(dir.clone());if s=="complete"||s.starts_with("error:"){
  match case.as_str(){"good"=>{assert_eq!(s,"complete");assert_eq!(std::fs::read(&dest).unwrap(),b"atlas-downloader-synthetic-123")},"bad"|"cancel"=>{assert!(s.starts_with("error:"),"{s}");assert!(!dest.exists())},"race"=>{assert!(s.starts_with("error:"),"{s}");assert_eq!(std::fs::read(&dest).unwrap(),b"do-not-replace")},_=>panic!("unknown case")};
  let leftovers:Vec<_>=std::fs::read_dir(dest.parent().unwrap()).unwrap().filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().contains("-download-")).collect();assert!(leftovers.is_empty(),"temporary files remain");break}
  assert!(Instant::now()<deadline,"timeout: {s}");std::thread::sleep(Duration::from_millis(20));
 }
}
'''
 (p/'src/main.rs').write_text(code)
 env=os.environ.copy()
 subprocess.run(['cargo','build','--offline','--quiet'],cwd=p,env=env,check=True)
 for name in NAMES:
  for case in ['good','bad','existing','cancel','race']:
   mode={'bad':'bad','cancel':'slow','race':'delay'}.get(case,'good')
   with tempfile.TemporaryDirectory(prefix='download-case-',dir=tmp) as d:
    result=subprocess.run([str(p/'target/debug/atlas-download-contract'),d,name,case],capture_output=True,text=True)
    print(('PASS' if result.returncode==0 else 'FAIL'),name,case,flush=True)
    if result.returncode:print(result.stderr,flush=True);sys.exit(1)
server.shutdown()
