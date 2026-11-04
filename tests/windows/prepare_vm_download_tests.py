from pathlib import Path
import argparse,hashlib,re
parser=argparse.ArgumentParser(description='Prepare standalone Windows downloader test with synthetic data'); parser.add_argument('atlas_root',type=Path); parser.add_argument('output',type=Path); args=parser.parse_args()
root=args.atlas_root.resolve()/'ui/src-tauri/src'; out=args.output.resolve(); out.mkdir(parents=True, exist_ok=False); target=out/'src'; target.mkdir()
names=['model_download','q8_model_download','vision_model_download','rag_model_download','rag_e4b_model_download','nemotron_model_download']
good=b'atlas-downloader-synthetic-123'; digest=hashlib.sha256(good).hexdigest()
(target/'windows_model_download.rs').write_text((root/'windows_model_download.rs').read_text())
for name in names:
 s=(root/(name+'.rs')).read_text();s,n=re.subn(r'const URL: &str = "[^\"]*";', 'const URL: &str = "http://127.0.0.1:47877/test";',s);assert n==1
 s,n=re.subn(r'const SHA: &str = "[a-f0-9]+";',f'const SHA: &str = "{digest}";',s);assert n==1
 (target/(name+'.rs')).write_text(s)
(out/'Cargo.toml').write_text('[package]\nname="atlas-download-vm-test"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nlibc="=0.2.191"\nsha2="=0.10.9"\n')
code='mod windows_model_download;\n'+'\n'.join(f'mod {n};' for n in names)+r'''
use std::{env,path::PathBuf,time::{Duration,Instant},io::{Read,Write},net::TcpListener,sync::{Arc,atomic::{AtomicBool,Ordering}}};
fn main(){
 let a:Vec<String>=env::args().collect();let dir=PathBuf::from(&a[1]);let n=&a[2];let case=&a[3];
 let (start,status,cleanup):(fn(PathBuf)->Result<(),String>,fn(PathBuf)->String,fn())=match n.as_str(){
'''+ '\n'.join(f'  "{n}" => ({n}::start,{n}::status,{n}::cleanup),' for n in names)+r'''
 _=>panic!("unknown downloader")};
 let name=match n.as_str(){"model_download"=>"embeddinggemma-2.gguf","q8_model_download"=>"embeddinggemma-2-q8.gguf","vision_model_download"=>"embeddinggemma-2-mmproj-q8.gguf","rag_model_download"=>"gemma-4-e2b-it-q4.gguf","rag_e4b_model_download"=>"gemma-4-e4b-it-q4.gguf","nemotron_model_download"=>"nemotron-3.5-bolt-e4b-q4.gguf",_=>unreachable!()};
 let dest=dir.join("models").join(name);
 if case=="existing"{std::fs::create_dir_all(dest.parent().unwrap()).unwrap();std::fs::write(&dest,b"do-not-replace").unwrap();assert!(start(dir.clone()).is_err());assert_eq!(std::fs::read(&dest).unwrap(),b"do-not-replace");return}
 let listener=TcpListener::bind("127.0.0.1:47877").unwrap();let scenario=case.to_owned();
 let received=Arc::new(AtomicBool::new(false));let sent=Arc::new(AtomicBool::new(false));let release=Arc::new(AtomicBool::new(false));
 let r=received.clone();let t=sent.clone();let ready=release.clone();
 std::thread::spawn(move || {let (mut stream,_)=listener.accept().unwrap();let mut req=[0u8;4096];let n=stream.read(&mut req).unwrap();assert!(n>0);r.store(true,Ordering::SeqCst);if scenario=="race" {while !ready.load(Ordering::SeqCst){std::thread::sleep(Duration::from_millis(5));}} if scenario=="cancel" {std::thread::sleep(Duration::from_secs(3));return;} let body:&[u8]=if scenario=="bad" {b"bad-checksum"} else {b"atlas-downloader-synthetic-123"};write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();stream.write_all(body).unwrap();t.store(true,Ordering::SeqCst);});
 start(dir.clone()).unwrap();
 let request_deadline=Instant::now()+Duration::from_secs(15);while !received.load(Ordering::SeqCst){assert!(Instant::now()<request_deadline,"curl did not send the HTTP request");std::thread::sleep(Duration::from_millis(5));}
 if case=="cancel"{cleanup();}
 if case=="race"{std::fs::write(&dest,b"do-not-replace").unwrap();release.store(true,Ordering::SeqCst);}
 let deadline=Instant::now()+Duration::from_secs(20);
 loop{let s=status(dir.clone());if s=="complete"||s.starts_with("error:"){
  match case.as_str(){"good"=>{assert_eq!(s,"complete");assert_eq!(std::fs::read(&dest).unwrap(),b"atlas-downloader-synthetic-123")},"bad"=>{assert_eq!(s,"error:Загруженный файл не прошёл проверку");assert!(sent.load(Ordering::SeqCst));assert!(!dest.exists())},"cancel"=>{assert_eq!(s,"error:Не удалось загрузить модель. Проверьте подключение к интернету.");assert!(!dest.exists())},"race"=>{assert!(sent.load(Ordering::SeqCst),"valid body not sent");assert!(s.contains("os error 183")||s.contains("os error 80"),"unexpected failure: {s}");assert_eq!(std::fs::read(&dest).unwrap(),b"do-not-replace")},_=>panic!("unknown case")};
  let leftovers:Vec<_>=std::fs::read_dir(dest.parent().unwrap()).unwrap().filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().contains("-download-")).collect();assert!(leftovers.is_empty(),"temporary files remain");break}
  assert!(Instant::now()<deadline,"timeout: {s}");std::thread::sleep(Duration::from_millis(20));
 }
}
'''
(target/'main.rs').write_text(code)
print('harness prepared')
