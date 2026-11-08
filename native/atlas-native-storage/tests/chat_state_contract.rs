use atlas_native_storage_contract::chat_state;
use rusqlite::Connection;
use serde_json::json;

#[test]
fn chat_state_compare_and_swap_clear_and_guard_are_atomic() {
 let mut db=Connection::open_in_memory().unwrap();
 let empty=json!({"messages":[],"summary":"","summarized":0,"context_start":0});
 assert_eq!(chat_state::get(&db).unwrap(), json!({"state":empty,"revision":0}));
 let value=json!({"messages":[{"role":"user","content":"hello"}],"summary":"","summarized":0,"context_start":0});
 assert_eq!(chat_state::set(&mut db,0,&value,||Ok(())).unwrap(),1);
 assert!(chat_state::set(&mut db,0,&empty,||Ok(())).is_err());
 assert!(chat_state::set(&mut db,1,&empty,||Err("cancelled".into())).is_err());
 assert_eq!(chat_state::get(&db).unwrap(),json!({"state":value,"revision":1}));
 assert_eq!(chat_state::clear(&mut db,||Ok(())).unwrap(),2);
 assert!(chat_state::set(&mut db,1,&value,||Ok(())).is_err());
 let cleared=json!({"messages":[{"role":"user","content":"hello"}],"summary":"","summarized":1,"context_start":1});
 assert_eq!(chat_state::get(&db).unwrap(),json!({"state":cleared,"revision":2}));
}

#[test]
fn invalid_state_and_failed_clear_keep_previous_history() {
 let mut db=Connection::open_in_memory().unwrap();
 let state=json!({"messages":[{"role":"user","content":"first"},{"role":"assistant","content":"reply"}],"summary":"saved facts","summarized":2,"context_start":0});
 chat_state::set(&mut db,0,&state,||Ok(())).unwrap();
 assert!(chat_state::set(&mut db,1,&json!({"messages":[],"summary":"x","summarized":9,"context_start":0}),||Ok(())).is_err());
 assert!(chat_state::clear(&mut db,||Err("workspace closed".into())).is_err());
 assert_eq!(chat_state::get(&db).unwrap(),json!({"state":state,"revision":1}));
 chat_state::clear(&mut db,||Ok(())).unwrap();
 let result=chat_state::get(&db).unwrap();assert_eq!(result["state"]["summary"],"");
 assert_eq!(result["state"]["messages"].as_array().unwrap().len(),2);
 assert_eq!(result["state"]["context_start"],2);assert_eq!(result["state"]["summarized"],2);
}
