#![cfg(feature="agent-write")]
use atlas_native_storage_contract as storage;
#[test]
fn creation_replay_conflict_and_reopen_without_test_hooks() {
 let root=std::env::temp_dir().join(format!("atlas-agent-write-prod-{}",uuid::Uuid::new_v4()));
 storage::execute(&["atlas".into(),"create-workspace".into(),root.to_string_lossy().into_owned()]).unwrap();
 let first=storage::agent_create_note(&root,"agent","request","Title","body").unwrap();
 let again=storage::agent_create_note(&root,"agent","request","Title","body").unwrap();
 assert_eq!(first["id"],again["id"]); assert_eq!(again["replayed"],true);
 assert!(storage::agent_create_note(&root,"agent","request","Title","other").is_err());
 std::fs::remove_dir_all(root).unwrap();
}
