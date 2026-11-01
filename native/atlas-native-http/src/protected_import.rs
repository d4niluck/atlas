use std::{fs::{self,File,OpenOptions},io::{Read,Seek,SeekFrom,Write},os::unix::fs::OpenOptionsExt,path::Path};
use sha2::{Digest,Sha256};
type R<T> = Result<T,Box<dyn std::error::Error>>;

// The source directory is pinned as the transfer worker's current directory.
pub fn copy(destination:&Path)->R<()> {
 let marker=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(".atlas-protected")?;
 if !marker.metadata()?.is_file(){return Err("invalid protected marker".into())}
 if unsafe{libc::flock(std::os::fd::AsRawFd::as_raw_fd(&marker),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("workspace already active".into())}
 let mut value=String::new(); (&marker).take(128).read_to_string(&mut value)?;
 if value!="atlas-protected-v1"{return Err("invalid protected marker".into())}
 for name in ["workspace.atlas-journal","workspace.atlas-wal","workspace.atlas-shm"] {match fs::symlink_metadata(name){Ok(_)=>return Err("workspace has pending database changes".into()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(e.into())}}
 let mut src=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open("workspace.atlas")?;
 if !src.metadata()?.is_file()||src.metadata()?.len()<4096{return Err("invalid protected container".into())}
 let mut header=[0u8;16];src.read_exact(&mut header)?;if &header==b"SQLite format 3\0"{return Err("unencrypted container".into())}src.seek(SeekFrom::Start(0))?;
 let meta=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(".workspace.json")?;if !meta.metadata()?.is_file(){return Err("invalid protected metadata".into())}
 let metadata:serde_json::Value=serde_json::from_reader(meta.take(4096))?;if metadata["protected"]!=true{return Err("invalid protected metadata".into())}
 fs::create_dir(destination)?;
 let mut dst=OpenOptions::new().write(true).create_new(true).mode(0o600).open(destination.join("workspace.atlas"))?;
 let mut hash=Sha256::new();let mut buffer=[0u8;65536];loop{let n=src.read(&mut buffer)?;if n==0{break}hash.update(&buffer[..n]);dst.write_all(&buffer[..n])?;}dst.sync_all()?;drop(dst);
 let mut dst=File::open(destination.join("workspace.atlas"))?;let mut check=Sha256::new();loop{let n=dst.read(&mut buffer)?;if n==0{break}check.update(&buffer[..n]);}let expected=hash.finalize();if expected!=check.finalize(){return Err("protected copy verification failed".into())}
 src.seek(SeekFrom::Start(0))?;let mut again=Sha256::new();loop{let n=src.read(&mut buffer)?;if n==0{break}again.update(&buffer[..n]);}if expected!=again.finalize(){return Err("source changed during copy".into())}
 for (name,bytes) in [(".atlas-protected",b"atlas-protected-v1".as_slice()),(".workspace.json",br#"{"name":"Protected workspace","protected":true}"#.as_slice())] {let mut f=OpenOptions::new().write(true).create_new(true).mode(0o600).open(destination.join(name))?;f.write_all(bytes)?;f.sync_all()?;}
 File::open(destination)?.sync_all()?;Ok(())
}
