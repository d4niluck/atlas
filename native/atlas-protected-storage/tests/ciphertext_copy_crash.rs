#![cfg(feature="test-hook")]
//! Verify durable copy markers against actual process termination.
use atlas_sqlcipher_proof::ciphertext_copy::{begin_recorded_copy,prepare_recorded_copy,open_recorded_copy};
use std::{fs, path::PathBuf, process::{Command,Stdio}, time::{Duration,Instant}};
const GEN:&str="0123456789abcdef0123456789abcdef";
const PAYLOAD:&[u8]=b"ciphertext test bytes, no user material";
#[test] fn copy_child(){
 let Some(dir)=std::env::var_os("ATLAS_COPY_TEST_DIR") else{return};let root=PathBuf::from(dir);
 let db=rusqlite::Connection::open(root.join("primary.sqlite")).unwrap();let destination=root.join("search.atlas");
 begin_recorded_copy(&db,&destination,GEN).unwrap();fs::write(root.join("begin-ready"),b"ready").unwrap();
 if std::env::var_os("ATLAS_COPY_STOP_AT_BEGIN").is_some(){loop{std::thread::sleep(Duration::from_secs(1))}}
 let source=fs::File::open(root.join("source")).unwrap();
 prepare_recorded_copy(&source,&db,&destination,GEN,||Ok(()),|f|f()).unwrap();
}
#[test] fn crash_at_each_copy_boundary_fails_closed_or_opens_verified(){
 for point in ["begin","created","identified","copied","ready"] {
  let root=std::env::temp_dir().join(format!("atlas-copy-crash-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();fs::write(root.join("source"),PAYLOAD).unwrap();
  for other in ["created","identified","copied","ready"] {if other!=point{fs::write(root.join(format!("{other}-go")),b"go").unwrap();}}
  let mut command=Command::new(std::env::current_exe().unwrap());command.args(["--exact","copy_child","--nocapture"]).env("ATLAS_COPY_TEST_DIR",&root).env("ATLAS_TEST_COPY_BARRIER",&root).stdout(Stdio::null()).stderr(Stdio::null());if point=="begin"{command.env("ATLAS_COPY_STOP_AT_BEGIN","1");}
  let mut child=command.spawn().unwrap();let deadline=Instant::now()+Duration::from_secs(30);let ready=root.join(format!("{point}-ready"));
  while !ready.exists()&&Instant::now()<deadline {if let Some(s)=child.try_wait().unwrap(){panic!("{point}: child exited {s}")}std::thread::sleep(Duration::from_millis(10));}
  if !ready.exists(){let _=child.kill();let _=child.wait();panic!("{point}: barrier timed out")};child.kill().unwrap();child.wait().unwrap();
  let db=rusqlite::Connection::open(root.join("primary.sqlite")).unwrap();let destination=root.join("search.atlas");
  if point=="ready" {
   assert!(open_recorded_copy(&db,&destination,GEN).is_ok(),"{point}");
   fs::write(&destination,b"changed").unwrap();assert!(open_recorded_copy(&db,&destination,GEN).is_err(),"tampered content accepted");
   fs::remove_file(&destination).unwrap();fs::write(&destination,PAYLOAD).unwrap();assert!(open_recorded_copy(&db,&destination,GEN).is_err(),"replaced inode accepted");
  }else{assert!(open_recorded_copy(&db,&destination,GEN).is_err(),"{point}: incomplete copy accepted");}
  assert_eq!(fs::read(root.join("source")).unwrap(),PAYLOAD);drop(db);fs::remove_dir_all(root).unwrap();
 }
}
