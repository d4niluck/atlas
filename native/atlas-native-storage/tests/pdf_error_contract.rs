use atlas_native_storage_contract::{add_file,create_workspace,replace_file};
use std::{fs,io::Write,path::PathBuf,process::Command};

// The child test executable and its helper live in an exclusively created directory.
struct TestDir(PathBuf);
impl Drop for TestDir { fn drop(&mut self){let _=fs::remove_dir_all(&self.0);} }
#[test] fn errors_are_isolated_and_preserve_files(){
 let parent=std::env::temp_dir();let dir=parent.join(format!("atlas-pdf-contract-{}",uuid::Uuid::new_v4()));fs::create_dir(&dir).unwrap();let guard=TestDir(dir);
 let exe=guard.0.join("runner");fs::copy(std::env::current_exe().unwrap(),&exe).unwrap();
 let status=Command::new(exe).args(["--exact","isolated_child","--nocapture"]).env("ATLAS_PDF_ISOLATED_TEST","1").status().unwrap();assert!(status.success());
}
#[test] fn isolated_child(){
 if std::env::var_os("ATLAS_PDF_ISOLATED_TEST").is_none(){return}
 use std::os::unix::fs::{OpenOptionsExt,PermissionsExt};
 let root=std::env::current_exe().unwrap().parent().unwrap().to_path_buf();let helper=root.join("atlas-pdfium-extract");
 let mut f=fs::OpenOptions::new().write(true).create_new(true).mode(0o700).open(&helper).unwrap();f.write_all(b"#!/bin/sh\ncase \"$1\" in */pass.pdf) exit 15;; *) exit 13;; esac\n").unwrap();drop(f);fs::set_permissions(&helper,fs::Permissions::from_mode(0o700)).unwrap();
 let workspace=root.join("workspace");create_workspace(&workspace).unwrap();let password=b"protected original";let id=add_file(&workspace,"pass.pdf",password).unwrap();let db=rusqlite::Connection::open(workspace.join("atlas.sqlite3")).unwrap();let(path,status,body):(String,String,String)=db.query_row("select objects.path,objects.text_status,search_index.body from objects join search_index on objects.id=search_index.object_id where objects.id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();assert_eq!(status,"unavailable");assert!(body.is_empty());assert_eq!(fs::read(workspace.join(path)).unwrap(),password);
 let id=add_file(&workspace,"old.txt",b"evidence unique").unwrap();let before:(String,String)=db.query_row("select path,body from objects join search_index on objects.id=search_index.object_id where objects.id=?",[&id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();assert!(replace_file(&workspace,&id,"new.pdf",b"broken").is_err());let after:(String,String)=db.query_row("select path,body from objects join search_index on objects.id=search_index.object_id where objects.id=?",[&id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();assert_eq!(before,after);assert_eq!(fs::read(workspace.join(before.0)).unwrap(),b"evidence unique");
}
