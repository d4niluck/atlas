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
 for name in ["workspace.atlas-journal","workspace.atlas-wal","workspace.atlas-shm","workspace.search.atlas-journal","workspace.search.atlas-wal","workspace.search.atlas-shm"] {match fs::symlink_metadata(name){Ok(_)=>return Err("workspace has pending database changes".into()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(e.into())}}
 let mut src=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open("workspace.atlas")?;
 if !src.metadata()?.is_file()||src.metadata()?.len()<4096{return Err("invalid protected container".into())}
 let mut header=[0u8;16];src.read_exact(&mut header)?;if &header==b"SQLite format 3\0"{return Err("unencrypted container".into())}src.seek(SeekFrom::Start(0))?;
 let meta=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(".workspace.json")?;if !meta.metadata()?.is_file(){return Err("invalid protected metadata".into())}
 let metadata:serde_json::Value=serde_json::from_reader(meta.take(4096))?;if metadata["protected"]!=true{return Err("invalid protected metadata".into())}
 let mut indexes=Vec::new();for entry in fs::read_dir(".")?{let entry=entry?;let name=entry.file_name();let Some(name)=name.to_str() else{continue};let base=name.strip_suffix("-journal").or_else(||name.strip_suffix("-wal")).or_else(||name.strip_suffix("-shm"));if let Some(base)=base{if let Some(g)=base.strip_prefix("workspace.search.").and_then(|v|v.strip_suffix(".atlas")){if atlas_sqlcipher_proof::search_index_name(g).ok().as_deref()==Some(base){return Err("search index has pending database changes".into())}}}let Some(generation)=name.strip_prefix("workspace.search.").and_then(|v|v.strip_suffix(".atlas")) else{continue};if atlas_sqlcipher_proof::search_index_name(generation).ok().as_deref()==Some(name){indexes.push(name.to_owned());}}
 if indexes.len()>1{return Err("Найдено несколько файлов поискового индекса; требуется проверка перед переносом".into())}
 fs::create_dir(destination)?;
 let mut dst=OpenOptions::new().write(true).create_new(true).mode(0o600).open(destination.join("workspace.atlas"))?;
 let mut hash=Sha256::new();let mut buffer=[0u8;65536];loop{let n=src.read(&mut buffer)?;if n==0{break}hash.update(&buffer[..n]);dst.write_all(&buffer[..n])?;}dst.sync_all()?;drop(dst);
 let mut dst=File::open(destination.join("workspace.atlas"))?;let mut check=Sha256::new();loop{let n=dst.read(&mut buffer)?;if n==0{break}check.update(&buffer[..n]);}let expected=hash.finalize();if expected!=check.finalize(){return Err("protected copy verification failed".into())}
 src.seek(SeekFrom::Start(0))?;let mut again=Sha256::new();loop{let n=src.read(&mut buffer)?;if n==0{break}again.update(&buffer[..n]);}if expected!=again.finalize(){return Err("source changed during copy".into())}
 if let Some(name)=indexes.first(){let mut src=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(name)?;
  if !src.metadata()?.is_file()||src.metadata()?.len()<4096{return Err("invalid protected search index".into())}
  let mut head=[0u8;16];src.read_exact(&mut head)?;if &head==b"SQLite format 3\0"{return Err("unencrypted search index".into())}src.seek(SeekFrom::Start(0))?;
  let path=destination.join(name);let mut dst=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;let mut hash=Sha256::new();loop{let n=src.read(&mut buffer)?;if n==0{break}hash.update(&buffer[..n]);dst.write_all(&buffer[..n])?;}dst.sync_all()?;drop(dst);
  let expected=hash.finalize();let mut check=Sha256::new();let mut dst=File::open(path)?;loop{let n=dst.read(&mut buffer)?;if n==0{break}check.update(&buffer[..n]);}if expected!=check.finalize(){return Err("protected search copy verification failed".into())}
  src.seek(SeekFrom::Start(0))?;let mut again=Sha256::new();loop{let n=src.read(&mut buffer)?;if n==0{break}again.update(&buffer[..n]);}if expected!=again.finalize(){return Err("search source changed during copy".into())}
 }
 let meta_bytes=serde_json::to_vec(&metadata)?;for (name,bytes) in [(".atlas-protected",b"atlas-protected-v1".as_slice()),(".workspace.json",meta_bytes.as_slice())] {let mut f=OpenOptions::new().write(true).create_new(true).mode(0o600).open(destination.join(name))?;f.write_all(bytes)?;f.sync_all()?;}
 File::open(destination)?.sync_all()?;Ok(())
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn copy_pair_child(){
  let Ok(root)=std::env::var("ATLAS_TEST_PROTECTED_COPY_ROOT") else{return};let root=Path::new(&root);std::env::set_current_dir(root.join("source")).unwrap();if std::env::var_os("ATLAS_TEST_EXPECT_MULTIPLE").is_some(){assert!(copy(&root.join("copy")).unwrap_err().to_string().contains("несколько"));assert!(!root.join("copy").exists());return}copy(&root.join("copy")).unwrap();for name in ["workspace.atlas","workspace.search.0123456789abcdef0123456789abcdef.atlas"]{assert_eq!(fs::read(name).unwrap(),fs::read(root.join("copy").join(name)).unwrap());}
 }
 #[test] fn refuses_multiple_search_indexes_before_copy(){
  let root=std::env::temp_dir().join(format!("atlas-protected-multiple-{}",uuid::Uuid::new_v4()));let source=root.join("source");fs::create_dir_all(&source).unwrap();fs::write(source.join(".atlas-protected"),"atlas-protected-v1").unwrap();fs::write(source.join(".workspace.json"),r#"{"protected":true}"#).unwrap();for name in ["workspace.atlas","workspace.search.0123456789abcdef0123456789abcdef.atlas","workspace.search.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.atlas"]{let vault=atlas_sqlcipher_proof::Vault::create(&source.join(name),"secret").unwrap();drop(vault);}let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","protected_import::tests::copy_pair_child","--nocapture"]).env("ATLAS_TEST_PROTECTED_COPY_ROOT",&root).env("ATLAS_TEST_EXPECT_MULTIPLE","1").output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));fs::remove_dir_all(root).unwrap();
 }
 #[test] fn copies_both_encrypted_files_without_rebuilding(){
  let root=std::env::temp_dir().join(format!("atlas-protected-copy-{}",uuid::Uuid::new_v4()));let source=root.join("source");fs::create_dir_all(&source).unwrap();fs::write(source.join(".atlas-protected"),"atlas-protected-v1").unwrap();fs::write(source.join(".workspace.json"),r#"{"protected":true}"#).unwrap();for name in ["workspace.atlas","workspace.search.0123456789abcdef0123456789abcdef.atlas"]{let vault=atlas_sqlcipher_proof::Vault::create(&source.join(name),"copy-test-password").unwrap();vault.set_workspace_name("test").unwrap();drop(vault);}let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","protected_import::tests::copy_pair_child","--nocapture"]).env("ATLAS_TEST_PROTECTED_COPY_ROOT",&root).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));fs::remove_dir_all(root).unwrap();
 }
}
