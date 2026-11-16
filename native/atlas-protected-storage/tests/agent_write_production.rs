#![cfg(feature="agent-write")]
use atlas_sqlcipher_proof::Vault;
#[test]
fn protected_creation_replays_without_test_hooks() {
 let root=std::env::temp_dir().join(format!("atlas-vault-write-prod-{}",uuid::Uuid::new_v4()));
 std::fs::create_dir(&root).unwrap();let path=root.join("vault");
 let mut vault=Vault::create(&path,"test-password").unwrap();
 let first=vault.agent_create_note("agent","request","Title","secret body",||Ok(())).unwrap();
 let again=vault.agent_create_note("agent","request","Title","secret body",||Ok(())).unwrap();
 assert_eq!(first["id"],again["id"]);assert_eq!(again["replayed"],true);
 assert!(vault.agent_create_note("agent","request","Title","different",||Ok(())).is_err());
 drop(vault);assert!(!std::fs::read(path).unwrap().windows(11).any(|v|v==b"secret body"));
 std::fs::remove_dir_all(root).unwrap();
}
