use atlas_native_storage_contract::{chat_state,create_workspace};
use atlas_sqlcipher_proof::Vault;
use serde_json::json;
use std::fs;

#[test]
fn chat_survives_protection_export_and_snapshot_without_plaintext() {
 let root=std::env::temp_dir().join(format!("atlas-chat-state-{}",uuid::Uuid::new_v4()));
 fs::create_dir(&root).unwrap();
 let plain=root.join("plain");create_workspace(&plain).unwrap();
 let secret="PRIVATE_CHAT_STATE_48917";
 let state=json!({"messages":[{"role":"user","content":secret}],"summary":"","summarized":0,"context_start":0});
 let mut db=rusqlite::Connection::open(plain.join("atlas.sqlite3")).unwrap();
 chat_state::set(&mut db,0,&state,||Ok(())).unwrap();drop(db);
 let encrypted=root.join("protected");
 let mut v=Vault::import_plain(&plain,&encrypted,"password").unwrap();
 v.set_workspace_name("test").unwrap();
 assert_eq!(v.chat_get().unwrap(),json!({"state":state,"revision":1}));
 v.chat_clear_guarded(||Ok(())).unwrap();
 let cleared=v.chat_get().unwrap();assert_eq!(cleared["state"]["messages"][0]["content"],secret);
 assert_eq!(cleared["state"]["context_start"],1);assert_eq!(cleared["revision"],2);
 assert!(v.chat_set_guarded(1,&state,||Ok(())).is_err());
 let encrypted_copy=root.join("protected-copy");v.snapshot_to(&encrypted_copy,"password",|_,_|{}).unwrap();
 assert_eq!(Vault::open(&encrypted_copy,"password").unwrap().chat_get().unwrap(),cleared);
 let output=root.join("exported");v.export_plain(&output).unwrap();drop(v);
 let db=rusqlite::Connection::open(output.join("atlas.sqlite3")).unwrap();
 assert_eq!(chat_state::get(&db).unwrap(),cleared);drop(db);
 for path in [&encrypted,&encrypted_copy] {let bytes=fs::read(path).unwrap();assert!(!bytes.windows(secret.len()).any(|w|w==secret.as_bytes()));}
 fs::remove_dir_all(root).unwrap();
}
