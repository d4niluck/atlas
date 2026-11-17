//! Crash contract for the unpublished encrypted search stage.
use atlas_sqlcipher_proof::{search_stage::{build_search_stage,cleanup_abandoned_stages}, Vault};
use std::{fs, path::PathBuf, process::{Command,Stdio}, time::{Duration,Instant}};

const SECRET:&str="ATLAS_STAGE_SECRET_15a8143c";
#[test]
fn stage_child() {
 let Some(dir)=std::env::var_os("ATLAS_STAGE_TEST_DIR") else{return};
 let root=PathBuf::from(dir);
 let source=Vault::open(&root.join("source.atlas"),"test-password").unwrap();
 let stage=build_search_stage(&source,&root,"test-password").unwrap();
 fs::write(root.join("ready"),stage.path().file_name().unwrap().as_encoded_bytes()).unwrap();
 loop {std::thread::sleep(Duration::from_secs(1))}
}

#[test]
fn killed_stage_stays_encrypted_and_source_is_unchanged() {
 let root=std::env::temp_dir().join(format!("atlas-stage-crash-{}",uuid::Uuid::new_v4()));
 fs::create_dir(&root).unwrap();
 let mut vault=Vault::create(&root.join("source.atlas"),"test-password").unwrap();
 vault.enable_search_journal().unwrap();vault.put_note("n","test",SECRET).unwrap();drop(vault);
 let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","stage_child","--nocapture"]).env("ATLAS_STAGE_TEST_DIR",&root).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
 let until=Instant::now()+Duration::from_secs(30);
 while !root.join("ready").exists() && Instant::now()<until {if let Some(status)=child.try_wait().unwrap(){panic!("stage child exited: {status}")}std::thread::sleep(Duration::from_millis(20));}
 if !root.join("ready").exists(){let _=child.kill();let _=child.wait();panic!("stage child did not reach barrier")}
 let name=fs::read_to_string(root.join("ready")).unwrap();assert!(name.starts_with(".atlas-search-stage-"));
 let stage=root.join(&name);let lock=root.join(format!("{name}.lock"));
 let foreign=root.join(format!(".atlas-search-stage-{}",uuid::Uuid::new_v4().simple()));fs::write(&foreign,b"foreign").unwrap();
 cleanup_abandoned_stages(&root).unwrap();assert!(stage.exists()&&lock.exists(),"live stage deleted");assert!(foreign.exists());
 child.kill().unwrap();child.wait().unwrap();
 let bytes=fs::read(&stage).unwrap();assert!(!bytes.windows(SECRET.len()).any(|x|x==SECRET.as_bytes()));
 let source=Vault::open(&root.join("source.atlas"),"test-password").unwrap();assert_eq!(source.get_note("n").unwrap(),SECRET);drop(source);
 assert!(Vault::open(&stage,"wrong-password").is_err());
 // A replaced stage path must never cause deletion of an unrelated target.
 let saved=root.join("saved-stage");fs::rename(&stage,&saved).unwrap();
 std::os::unix::fs::symlink(&foreign,&stage).unwrap();cleanup_abandoned_stages(&root).unwrap();
 assert!(fs::symlink_metadata(&stage).unwrap().file_type().is_symlink());assert_eq!(fs::read(&foreign).unwrap(),b"foreign");
 fs::remove_file(&stage).unwrap();fs::rename(&saved,&stage).unwrap();
 cleanup_abandoned_stages(&root).unwrap();assert!(!stage.exists()&&!lock.exists());assert_eq!(fs::read(&foreign).unwrap(),b"foreign");
 let source=Vault::open(&root.join("source.atlas"),"test-password").unwrap();assert_eq!(source.get_note("n").unwrap(),SECRET);drop(source);
 fs::remove_dir_all(root).unwrap();
}
