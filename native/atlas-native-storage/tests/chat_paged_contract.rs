use atlas_native_storage_contract::chat_state;
use rusqlite::Connection;
use serde_json::json;

#[test]
fn legacy_history_migrates_paginates_and_appends_without_rewriting() {
 let mut db=Connection::open_in_memory().unwrap();
 let messages=(0..85).map(|i|json!({"role":if i%2==0{"user"}else{"assistant"},"content":format!("message {i}")})).collect::<Vec<_>>();
 let legacy=json!({"messages":messages,"summary":"early facts","summarized":80,"context_start":0,"scope":null});
 chat_state::set(&mut db,0,&legacy,||Ok(())).unwrap();
 let page=chat_state::page(&mut db,None,40,||Ok(())).unwrap();
 assert_eq!(page["messages"].as_array().unwrap().len(),40);assert_eq!(page["messages"][0]["seq"],46);assert_eq!(page["next_before"],46);
 let page=chat_state::page(&mut db,Some(46),40,||Ok(())).unwrap();assert_eq!(page["messages"][0]["seq"],6);assert_eq!(page["next_before"],6);
 let page=chat_state::page(&mut db,Some(6),40,||Ok(())).unwrap();assert_eq!(page["messages"].as_array().unwrap().len(),5);assert!(page["next_before"].is_null());
 let c=chat_state::context(&mut db,||Ok(())).unwrap();assert_eq!(c["revision"],1);assert_eq!(c["last_message_seq"],85);assert_eq!(c["messages"].as_array().unwrap().len(),5);assert_eq!(c["summarized_seq"],80);
 let user=json!({"role":"user","content":"next"});let assistant=json!({"role":"assistant","content":"reply"});
 assert_eq!(chat_state::append(&mut db,1,&user,&assistant,"facts",85,0,&json!(null),||Ok(())).unwrap(),2);
 assert!(chat_state::append(&mut db,1,&user,&assistant,"facts",85,0,&json!(null),||Ok(())).is_err());
 let c=chat_state::context(&mut db,||Ok(())).unwrap();assert_eq!(c["last_message_seq"],87);assert_eq!(c["messages"].as_array().unwrap().len(),2);
 chat_state::clear_paged(&mut db,||Ok(())).unwrap();let c=chat_state::context(&mut db,||Ok(())).unwrap();assert_eq!(c["revision"],3);assert_eq!(c["context_start_seq"],87);assert!(c["messages"].as_array().unwrap().is_empty());
 assert_eq!(chat_state::page(&mut db,None,40,||Ok(())).unwrap()["messages"].as_array().unwrap().len(),40);
}

#[test]
fn migration_failure_rolls_back_legacy_and_context_guard() {
 let mut db=Connection::open_in_memory().unwrap();let legacy=json!({"messages":[{"role":"user","content":"x"}],"summary":"","summarized":0,"context_start":0});
 chat_state::set(&mut db,0,&legacy,||Ok(())).unwrap();
 assert!(chat_state::context(&mut db,||Err("closed".into())).is_err());
 assert_eq!(chat_state::get(&db).unwrap()["state"],legacy);
 let result=chat_state::context(&mut db,||Ok(())).unwrap();assert_eq!(result["messages"][0]["content"],"x");
}

#[test]
fn long_history_appends_without_rewriting_old_messages_and_pages_without_gaps(){
 let mut db=Connection::open_in_memory().unwrap();let user=json!({"role":"user","content":"u"});let assistant=json!({"role":"assistant","content":"a"});
 chat_state::append(&mut db,0,&user,&assistant,"",0,0,&json!(null),||Ok(())).unwrap();
 db.execute_batch("CREATE TRIGGER no_rewrite BEFORE UPDATE ON chat_messages BEGIN SELECT RAISE(ABORT,'old history rewritten'); END; CREATE TRIGGER no_delete BEFORE DELETE ON chat_messages BEGIN SELECT RAISE(ABORT,'old history deleted'); END;").unwrap();
 for i in 1..1000 {chat_state::append(&mut db,i,&user,&assistant,"",(i*2) as i64,0,&json!(null),||Ok(())).unwrap();}
 let c=chat_state::context(&mut db,||Ok(())).unwrap();assert_eq!(c["last_message_seq"],2000);assert_eq!(c["messages"].as_array().unwrap().len(),2);
 let mut before=None;let mut total=0;loop{let p=chat_state::page(&mut db,before,40,||Ok(())).unwrap();total+=p["messages"].as_array().unwrap().len();before=p["next_before"].as_i64();if before.is_none(){break}}
 assert_eq!(total,2000);
}
