use atlas_sqlcipher_proof::Vault;
use rusqlite::Connection;
use serde_json::json;
#[test]
fn export_preserves_objects_board_trash_history_and_search() {
 let root=std::env::temp_dir().join(format!("atlas-export-test-{}",uuid::Uuid::new_v4()));
 std::fs::create_dir(&root).unwrap();
 let mut vault=Vault::create(&root.join("source.atlas"),"test-password").unwrap();
 vault.set_workspace_name("Example").unwrap();
 let note="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";let file="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
 vault.put_note(note,"note","apple and pear").unwrap();
 let bytes=vec![77u8;3*1024*1024+13];vault.put_file(file,"example.bin",bytes.as_slice(),None).unwrap();
 let board=json!({"objects":[{"id":note,"x":1,"y":2},{"id":file,"x":4,"y":8}],"edges":[{"id":"edge1","from":note,"to":file}],"stacks":[]});
 vault.put_board(&board.to_string()).unwrap();
 vault.record_history(json!({"query":"apple","mode":"lexical","results":[],"created_at":7.0,"scope_known":true})).unwrap();
 vault.trash(file,123).unwrap();
 let out=root.join("result");vault.export_plain(&out).unwrap();
 assert_eq!(std::fs::read(out.join("files").join(file).join("example.bin")).unwrap(),bytes);
 assert_eq!(std::fs::read_to_string(out.join("notes").join(format!("{note}.md"))).unwrap(),"apple and pear");
 let db=Connection::open(out.join("atlas.sqlite3")).unwrap();
 assert_eq!(db.query_row("select count(*) from objects",[],|r|r.get::<_,i64>(0)).unwrap(),2);
 assert_eq!(db.query_row("select count(*) from trash",[],|r|r.get::<_,i64>(0)).unwrap(),1);
 assert_eq!(db.query_row("select count(*) from pending_edges",[],|r|r.get::<_,i64>(0)).unwrap(),1);
 assert_eq!(db.query_row("select count(*) from search_history",[],|r|r.get::<_,i64>(0)).unwrap(),1);
 assert_eq!(db.query_row("select count(*) from search_index where search_index match 'apple'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
 drop(db);
 atlas_native_storage_contract::restore_objects(&out,&[file.to_owned()]).unwrap();
 let restored=Vault::import_plain(&out,&root.join("restored.atlas"),"test-password").unwrap();
 let mut file_out=Vec::new();restored.read(file,&mut file_out).unwrap();assert_eq!(file_out,bytes);assert!(restored.board().unwrap().contains("edge1"));
 drop(restored);drop(vault);std::fs::remove_dir_all(root).unwrap();
}
