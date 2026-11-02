use super::{Path,R};
use pdf_extract::{OutputDev,OutputError,Transform,MediaBox,PlainTextOutput,ConvertToFmt};
use std::{cell::RefCell,rc::Rc,fmt};
#[derive(Clone)] struct Writer(Rc<RefCell<String>>);
impl fmt::Write for Writer{fn write_str(&mut self,s:&str)->fmt::Result{self.0.borrow_mut().push_str(s);Ok(())}}
impl ConvertToFmt for Writer{type Writer=Self;fn convert(self)->Self{self}}
struct Pages{current:Writer,output:PlainTextOutput<Writer>,text:String,first:bool}
impl Pages{fn new()->Self{let current=Writer(Rc::new(RefCell::new(String::new())));Self{output:PlainTextOutput::new(current.clone()),current,text:String::new(),first:true}}}
impl OutputDev for Pages{
fn begin_page(&mut self,n:u32,b:&MediaBox,a:Option<(f64,f64,f64,f64)>)->Result<(),OutputError>{self.current.0.borrow_mut().clear();self.output=PlainTextOutput::new(self.current.clone());self.output.begin_page(n,b,a)}
fn end_page(&mut self)->Result<(),OutputError>{self.output.end_page()?;if !self.first{self.text.push('\n')}self.first=false;self.text.push_str(&self.current.0.borrow());Ok(())}
fn output_character(&mut self,t:&Transform,w:f64,s:f64,f:f64,c:&str)->Result<(),OutputError>{self.output.output_character(t,w,s,f,c)}
fn begin_word(&mut self)->Result<(),OutputError>{self.output.begin_word()}
fn end_word(&mut self)->Result<(),OutputError>{self.output.end_word()}
fn end_line(&mut self)->Result<(),OutputError>{self.output.end_line()}}
// The prior page-by-page API returns all completed pages when a page fails.
fn extract_doc(mut doc:lopdf::Document)->R<String>{if doc.is_encrypted(){doc.decrypt("")?;}let mut pages=Pages::new();let _=pdf_extract::output_doc(&doc,&mut pages);Ok(pages.text)}
fn same_source(path:&Path, before:&std::fs::Metadata)->R<()>{use std::os::unix::fs::MetadataExt;let after=std::fs::symlink_metadata(path)?;if !after.is_file()||before.dev()!=after.dev()||before.ino()!=after.ino()||before.len()!=after.len()||before.mtime()!=after.mtime()||before.mtime_nsec()!=after.mtime_nsec()||before.ctime()!=after.ctime()||before.ctime_nsec()!=after.ctime_nsec(){return Err("PDF source changed".into())}Ok(())}
fn fallback_pdf(path:&Path,before:&std::fs::Metadata)->R<String>{same_source(path,before)?;let doc=match lopdf::Document::load(path){Ok(v)=>v,Err(lopdf::Error::IO(e))=>return Err(e.into()),Err(_)=>{same_source(path,before)?;return Ok(String::new())}};let text=extract_doc(doc).unwrap_or_default();same_source(path,before)?;Ok(text)}
pub fn extract_pdf_path(path:&Path)->R<String>{
 use std::io::Read;use std::process::{Command,Stdio};use std::time::{Duration,Instant};use std::os::unix::process::CommandExt;
 let before=std::fs::symlink_metadata(path)?;if !before.is_file(){return Err("invalid PDF file".into())}
 let helper=std::env::current_exe()?.with_file_name("atlas-pdfium-extract");
 let mut child=match Command::new(&helper).process_group(0).arg(path).arg("--parent-watch").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn(){Ok(v)=>v,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return fallback_pdf(path,&before),Err(e)=>return Err(e.into())};
 let stdout=child.stdout.take().ok_or("missing PDF extractor output")?;let (tx,rx)=std::sync::mpsc::channel();let reader=std::thread::spawn(move||{let mut output=Vec::new();let result=stdout.take(300 * 1024 * 1024 + 1).read_to_end(&mut output).and_then(|_|if output.len()>300 * 1024 * 1024 {Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"PDF text too large"))} else {Ok(output)});let _=tx.send(result);});let started=Instant::now();let mut output=None;
 loop{if output.is_none(){if let Ok(v)=rx.try_recv(){match v{Ok(bytes)=>output=Some(bytes),Err(e)=>{unsafe{libc::kill(-(child.id() as i32),libc::SIGKILL);};let _=child.kill();let _=child.wait();let _=reader.join();if e.kind()==std::io::ErrorKind::InvalidData{return fallback_pdf(path,&before)}return Err(e.into())}}}}let state=match child.try_wait(){Ok(v)=>v,Err(e)=>{unsafe{libc::kill(-(child.id() as i32),libc::SIGKILL);};let _=child.kill();let _=child.wait();let _=reader.join();return Err(e.into())}};if let Some(status)=state{reader.join().map_err(|_|"PDF reader failed")?;let bytes=match output{Some(v)=>v,None=>match rx.recv().map_err(|_|"PDF reader failed")?{Ok(v)=>v,Err(e) if e.kind()==std::io::ErrorKind::InvalidData=>return fallback_pdf(path,&before),Err(e)=>return Err(e.into())}};if !status.success(){if matches!(status.code(),Some(3|15)){same_source(path,&before)?;return Ok(String::new())}return Err("PDF extraction failed or file changed".into())}same_source(path,&before)?;return Ok(String::from_utf8(bytes)?)}if started.elapsed()>Duration::from_secs(1200){unsafe{libc::kill(-(child.id() as i32),libc::SIGKILL);};let _=child.kill();let _=child.wait();let _=reader.join();return Err("PDF extraction timed out".into())}std::thread::sleep(Duration::from_millis(25));}
}

pub fn extract_pdf_bytes(bytes:&[u8])->R<String>{extract_doc(lopdf::Document::load_mem(bytes)?)}
#[cfg(test)] mod tests {
 use super::*;
 use lopdf::{Document,Object,Stream,dictionary};
 #[test] fn page_output_matches_previous_implementation(){
  let mut d=Document::with_version("1.5");let pages=d.new_object_id();
  let font=d.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
  let resources=d.add_object(dictionary!{"Font"=>dictionary!{"F1"=>font}});
  let mut kids=Vec::new();for word in ["first page","second page",""] {let stream=d.add_object(Stream::new(dictionary!{},format!("BT /F1 12 Tf 10 20 Td ({word}) Tj ET").into_bytes()));let page=d.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"Resources"=>resources,"MediaBox"=>vec![0.into(),0.into(),200.into(),200.into()],"Contents"=>stream});kids.push(Object::Reference(page));}
  d.objects.insert(pages,dictionary!{"Type"=>"Pages","Kids"=>kids,"Count"=>3}.into());let catalog=d.add_object(dictionary!{"Type"=>"Catalog","Pages"=>pages});d.trailer.set("Root",catalog);
  let mut bytes=Vec::new();d.save_to(&mut bytes).unwrap();let before=pdf_extract::extract_text_from_mem_by_pages(&bytes).unwrap().join("\n");assert_eq!(extract_pdf_bytes(&bytes).unwrap(),before);
 }
}
