use std::{fs,process::Command,time::{SystemTime,UNIX_EPOCH},path::{Path,PathBuf}};
use serde_json::{json,Value};
fn path()->PathBuf {std::env::temp_dir().join(format!("atlas-create-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))}
fn run(p:&Path,args:&[&str])->Value {let out=Command::new(env!("CARGO_BIN_EXE_atlas-native-storage-contract")).arg(args[0]).arg(p).args(&args[1..]).output().unwrap(); assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));serde_json::from_slice(&out.stdout).unwrap()}
#[test] fn create_workspace_note_and_board_roundtrip(){
 let p=path();run(&p,&["create-workspace"]);assert!(p.join("atlas.sqlite3").is_file());assert!(p.join("notes").is_dir());assert!(p.join("files").is_dir());
 let note=run(&p,&["create-note","Привет","Содержимое"]);let id=note["id"].as_str().expect("id");assert_eq!(run(&p,&["get-note",id])["text"],"Содержимое");
 let board=json!({"objects":[{"id":id,"x":41,"y":-5}],"edges":[],"stacks":[]});run(&p,&["put-board",&board.to_string()]);let saved=run(&p,&["get-board"]);assert_eq!(saved["objects"][0]["x"],41);assert_eq!(saved["objects"][0]["y"],-5);
 fs::remove_dir_all(p).unwrap();
}
#[test] fn image_display_state_survives_board_roundtrip(){
 let p=path();run(&p,&["create-workspace"]);let note=run(&p,&["create-note","test",""]);let id=note["id"].as_str().unwrap();
 let board=json!({"objects":[{"id":id,"x":1,"y":2,"imageExpanded":true}],"edges":[],"stacks":[]});run(&p,&["put-board",&board.to_string()]);
 assert_eq!(run(&p,&["get-board"])["objects"][0]["imageExpanded"],true);fs::remove_dir_all(p).unwrap();
}
