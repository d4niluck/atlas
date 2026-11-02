use std::{fs::OpenOptions,io::{self,BufRead,BufReader,Read,Write},net::{Ipv4Addr,TcpStream},path::Path,time::Duration};

#[cfg(target_os="macos")]
fn publish(from:&Path,to:&Path)->io::Result<()>{use std::{ffi::CString,os::unix::ffi::OsStrExt};unsafe extern "C" {fn renamex_np(from:*const i8,to:*const i8,flags:u32)->i32;}let from=CString::new(from.as_os_str().as_bytes())?;let to=CString::new(to.as_os_str().as_bytes())?;if unsafe{renamex_np(from.as_ptr(),to.as_ptr(),4)}==0{Ok(())}else{Err(io::Error::last_os_error())}}
#[cfg(not(target_os="macos"))]
fn publish(from:&Path,to:&Path)->io::Result<()>{std::fs::hard_link(from,to)}
fn valid_id(s:&str)->bool {!s.is_empty()&&s.len()<=64&&s.bytes().all(|b|b.is_ascii_hexdigit()||b==b'-')}
pub fn download(port:u16,token:&str,workspace:&str,id:&str,path:&Path)->Result<(),String>{
 if !(workspace=="default"||valid_id(workspace))||!valid_id(id)||!path.is_absolute(){return Err("Некорректный файл".into())}
 let mut socket=TcpStream::connect((Ipv4Addr::LOCALHOST,port)).map_err(|_|"Не удалось связаться с Atlas")?;
 socket.set_read_timeout(Some(Duration::from_secs(300))).map_err(|e|e.to_string())?;
 write!(socket,"GET /api/w/{workspace}/files/{id}/content HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Atlas-Token: {token}\r\nConnection: close\r\n\r\n").map_err(|e|e.to_string())?;
 let mut reader=BufReader::new(socket);let mut status=String::new();reader.read_line(&mut status).map_err(|e|e.to_string())?;if !status.starts_with("HTTP/1.1 200 ")&&!status.starts_with("HTTP/1.0 200 "){return Err("Не удалось получить файл из пространства".into())}
 let mut length=None;let mut header_bytes=0;loop{let mut line=String::new();let n=reader.read_line(&mut line).map_err(|e|e.to_string())?;header_bytes+=n;if n==0||header_bytes>32768{return Err("Некорректный ответ Atlas".into())}if line=="\r\n"{break}if let Some((key,value))=line.split_once(':'){if key.eq_ignore_ascii_case("content-length"){length=Some(value.trim().parse::<u64>().map_err(|_|"Некорректный размер файла")?);}}}
 let length=length.ok_or("Размер файла неизвестен")?;
 let parent=path.parent().ok_or("Некорректная папка")?;
 use std::os::unix::fs::OpenOptionsExt;let mut tmp=None;
 for attempt in 0..1000u32 {let name=format!(".atlas-save-{}-{attempt}",std::process::id());let candidate=parent.join(name);match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&candidate){Ok(f)=>{tmp=Some((candidate,f));break},Err(e) if e.kind()==io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(format!("Не удалось создать временный файл: {e}"))}}
 let (temp,mut file)=tmp.ok_or("Не удалось создать временный файл")?;
 use std::os::unix::fs::MetadataExt;let created=file.metadata().map_err(|e|e.to_string())?;
 let result=(||->Result<(),String>{let n=io::copy(&mut reader.take(length),&mut file).map_err(|e|e.to_string())?;if n!=length{return Err("Файл передан не полностью".into())}file.sync_all().map_err(|e|e.to_string())?;publish(&temp,path).map_err(|e|if e.kind()==io::ErrorKind::AlreadyExists{"Файл с таким именем уже существует. Выберите другую папку.".to_owned()}else{format!("Не удалось сохранить файл: {e}")})?;Ok(())})();drop(file);
 if let Ok(m)=std::fs::symlink_metadata(&temp){if m.file_type().is_symlink()||m.dev()!=created.dev()||m.ino()!=created.ino(){return Err(format!("Временный файл изменился; проверьте {}",temp.display()))}if let Err(error)=std::fs::remove_file(&temp){return Err(format!("Не удалось удалить временный файл {}: {error}",temp.display()))}}result

}
#[cfg(test)]mod tests {use super::*;use std::{net::TcpListener,thread};
fn run(payload:&'static [u8],declared:usize,existing:bool)->(Result<(),String>,Vec<u8>){let listener=TcpListener::bind("127.0.0.1:0").unwrap();let port=listener.local_addr().unwrap().port();let worker=thread::spawn(move||{let(mut socket,_)=listener.accept().unwrap();let mut req=[0;512];let _=socket.read(&mut req);write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\n\r\n").unwrap();socket.write_all(payload).unwrap()});let path=std::env::temp_dir().join(format!("atlas-save-file-{}-{}",std::process::id(),port));if existing{std::fs::write(&path,b"keep").unwrap()}let result=download(port,"token","default","abcd",&path);worker.join().unwrap();let bytes=std::fs::read(&path).unwrap_or_default();if path.exists(){std::fs::remove_file(path).unwrap();}(result,bytes)}
#[test]fn success(){let(r,b)=run(b"content",7,false);assert!(r.is_ok());assert_eq!(b,b"content")}
#[test]fn preserve_existing(){let(r,b)=run(b"content",7,true);assert!(r.is_err());assert_eq!(b,b"keep")}
#[test]fn incomplete(){let(r,b)=run(b"short",10,false);assert!(r.is_err());assert!(b.is_empty())}
}
