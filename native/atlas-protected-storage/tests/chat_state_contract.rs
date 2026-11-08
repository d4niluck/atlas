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

#[test]
fn existing_vault_without_chat_table_is_migrated_when_first_used() {
 let root=std::env::temp_dir().join(format!("atlas-chat-old-{}",uuid::Uuid::new_v4()));
 fs::create_dir(&root).unwrap();
 let path=root.join("vault");let mut vault=Vault::create(&path,"password").unwrap();
 assert_eq!(vault.chat_get().unwrap()["revision"],0);
 let state=json!({"messages":[{"role":"user","content":"private test"}],"summary":"","summarized":0,"context_start":0});
 assert!(vault.chat_set_guarded(0,&state,||Err("closed".into())).is_err());
 assert_eq!(vault.chat_get().unwrap()["revision"],0);
 vault.chat_set_guarded(0,&state,||Ok(())).unwrap();drop(vault);
 assert_eq!(Vault::open(&path,"password").unwrap().chat_get().unwrap()["state"],state);
 fs::remove_dir_all(root).unwrap();
}

#[test]
fn paged_history_survives_protection_roundtrip_and_stays_encrypted() {
 let root=std::env::temp_dir().join(format!("atlas-chat-paged-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();
 let plain=root.join("plain");create_workspace(&plain).unwrap();let mut db=rusqlite::Connection::open(plain.join("atlas.sqlite3")).unwrap();
 let user=json!({"role":"user","content":"PAGED_PRIVATE_SECRET_48917"});let assistant=json!({"role":"assistant","content":"answer"});
 chat_state::append(&mut db,0,&user,&assistant,"",0,0,&json!(null),||Ok(())).unwrap();drop(db);
 let encrypted=root.join("encrypted");let mut vault=Vault::import_plain(&plain,&encrypted,"password").unwrap();vault.set_workspace_name("test").unwrap();
 assert_eq!(vault.chat_context_guarded(||Ok(())).unwrap()["messages"][0]["content"],"PAGED_PRIVATE_SECRET_48917");
 let page=vault.chat_page_guarded(None,40,||Ok(())).unwrap();assert_eq!(page["messages"].as_array().unwrap().len(),2);
 vault.chat_clear_paged_guarded(||Ok(())).unwrap();assert_eq!(vault.chat_context_guarded(||Ok(())).unwrap()["context_start_seq"],2);
 let exported=root.join("exported");vault.export_plain(&exported).unwrap();drop(vault);
 let mut db=rusqlite::Connection::open(exported.join("atlas.sqlite3")).unwrap();let result=chat_state::page(&mut db,None,40,||Ok(())).unwrap();assert_eq!(result["messages"],page["messages"]);assert_eq!(chat_state::context(&mut db,||Ok(())).unwrap()["revision"],2);drop(db);
 let bytes=fs::read(encrypted).unwrap();assert!(!bytes.windows(26).any(|w|w==b"PAGED_PRIVATE_SECRET_48917"));fs::remove_dir_all(root).unwrap();
}
