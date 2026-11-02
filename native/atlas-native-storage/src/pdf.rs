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
pub fn extract_pdf_path(path:&Path)->R<String>{extract_doc(lopdf::Document::load(path)?)}
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
