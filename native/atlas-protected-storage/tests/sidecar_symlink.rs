#[cfg(unix)]
#[test]
fn sqlite_does_not_follow_journal_symlink() {
 use atlas_sqlcipher_proof::Vault;
 use std::os::unix::fs::symlink;
 let root=std::env::temp_dir().join(format!("atlas-sidecar-{}",uuid::Uuid::new_v4()));
 std::fs::create_dir(&root).unwrap();
 let db=root.join("workspace.atlas"); let target=root.join("sentinel");
 {let vault=Vault::create(&db,"test-password").unwrap();drop(vault);}
 std::fs::write(&target,b"unchanged").unwrap();
 symlink(&target,root.join("workspace.atlas-journal")).unwrap();
 let result=Vault::open(&db,"test-password").and_then(|mut v|v.put_note("id","title","body"));
 assert!(result.is_err(),"journal symlink should not permit a successful write");
 assert_eq!(std::fs::read(&target).unwrap(),b"unchanged");
 std::fs::remove_dir_all(root).unwrap();
}
