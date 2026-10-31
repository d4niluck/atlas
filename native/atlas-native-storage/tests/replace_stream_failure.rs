use atlas_native_storage_contract::{create_workspace,add_file,replace_file_stream};
use std::{fs,io::{self,Read},time::{SystemTime,UNIX_EPOCH}};
use rusqlite::Connection;
struct Broken{remaining:usize}
impl Read for Broken {fn read(&mut self,b:&mut[u8])->io::Result<usize>{if self.remaining==0{return Err(io::Error::new(io::ErrorKind::UnexpectedEof,"interrupted"))}let n=self.remaining.min(b.len()).min(8192); b[..n].fill(42); self.remaining-=n;Ok(n)}}
fn row(p:&std::path::Path,id:&str)->String{Connection::open(p.join("atlas.sqlite3")).unwrap().query_row("SELECT path FROM objects WHERE id=?",[id],|r|r.get(0)).unwrap()}
fn root()->std::path::PathBuf{std::env::temp_dir().join(format!("atlas-stream-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))}
#[test] fn broken_stream_keeps_original(){let p=root();create_workspace(&p).unwrap();let id=add_file(&p,"a.bin",b"original").unwrap();let before=row(&p,&id);assert!(replace_file_stream(&p,&id,"a.bin",&mut Broken{remaining:32768}).is_err());assert_eq!(row(&p,&id),before);assert_eq!(fs::read(p.join(before)).unwrap(),b"original");fs::remove_dir_all(p).unwrap();}
#[test] fn large_stream_is_read_incrementally(){let p=root();create_workspace(&p).unwrap();let id=add_file(&p,"a.bin",b"old").unwrap();let mut input=io::repeat(19).take(32*1024*1024);replace_file_stream(&p,&id,"a.bin",&mut input).unwrap();assert_eq!(fs::metadata(p.join(row(&p,&id))).unwrap().len(),32*1024*1024);fs::remove_dir_all(p).unwrap();}
