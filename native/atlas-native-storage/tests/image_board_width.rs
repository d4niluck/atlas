use std::{fs,path::PathBuf};
use atlas_native_storage_contract::{add_file,execute};
use serde_json::{json,Value};

#[test]
fn image_width_survives_board_roundtrip_and_invalid_width_is_ignored(){
 let root:PathBuf=std::env::temp_dir().join(format!("atlas-image-width-{}",uuid::Uuid::new_v4()));
 let root_arg=root.to_string_lossy().into_owned();
 let args=|action:&str,extra:Vec<String>|{let mut v=vec!["atlas".into(),action.into(),root_arg.clone()];v.extend(extra);v};
 execute(&args("create-workspace",vec![])).unwrap();
 let id=add_file(&root,"picture.png",b"image").unwrap();
 let board=|width:Value|json!({"objects":[{"id":id,"kind":"file","title":"picture.png","x":0,"y":0,"imageExpanded":true,"imageWidth":width}],"edges":[],"stacks":[]});
 execute(&args("put-board",vec![board(json!(460)).to_string()])).unwrap();
 let read=execute(&args("get-board",vec![])).unwrap();
 assert_eq!(read["objects"][0]["imageWidth"],json!(460.0));
 execute(&args("put-board",vec![board(json!(9999)).to_string()])).unwrap();
 let read=execute(&args("get-board",vec![])).unwrap();
 assert!(read["objects"][0]["imageWidth"].is_null());
 fs::remove_dir_all(root).unwrap();
}
