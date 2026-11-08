use rusqlite::{Connection, OptionalExtension};
use serde_json::{json,Value};

/// Compare saved excerpts and reuse identical reads within the page.
pub(crate) fn enrich_chat_page_with(mut page:Value,mut load:impl FnMut(&str,&str,i64,i64)->Result<Option<String>,String>)->Result<Value,String>{
 let messages=page["messages"].as_array_mut().ok_or("invalid chat page")?;let mut cache:std::collections::HashMap<(String,String,i64,i64),Option<String>>=std::collections::HashMap::new();
 for message in messages {for key in ["sources","found_sources"] {if let Some(items)=message.get_mut(key){if items.is_null(){continue}let items=items.as_array_mut().ok_or("invalid chat sources")?;for source in items {let id=source["id"].as_str().ok_or("invalid source id")?;let field=if source["field"]=="description"{"description"}else{"body"};let start=source["start"].as_i64().filter(|v|*v>=0).ok_or("invalid source start")?;let end=source["end"].as_i64().filter(|v|*v>=start && *v-start<=65536).ok_or("invalid source end")?;let key=(id.to_owned(),field.to_owned(),start,end-start);let current=if let Some(value)=cache.get(&key){value.clone()}else{let value=load(id,field,start,end-start)?;cache.insert(key,value.clone());value};let stale=current.as_deref()!=source["text"].as_str();source["missing"]=json!(current.is_none());source["stale"]=json!(stale);}}}}
 Ok(page)
}
pub(crate) fn enrich_chat_page(db:&Connection,page:Value)->Result<Value,String>{enrich_chat_page_with(page,|id,field,start,len|db.query_row("SELECT substr(CASE WHEN ?2='description' THEN objects.description ELSE COALESCE(search_index.body,'') END,?3+1,?4) FROM objects JOIN search_index ON objects.id=search_index.object_id WHERE objects.id=?1 AND objects.id NOT IN (SELECT id FROM trash)",rusqlite::params![id,field,start,len],|r|r.get(0)).optional().map_err(|e|e.to_string()))}

/// Read each source once, even if it appears in many saved searches.
pub(crate) fn read(db:&Connection)->Result<Vec<Value>,String>{
 let mut stmt=db.prepare("SELECT query,mode,results,created_at,object_ids,scope_known,answer,sources,error,agentic FROM search_history ORDER BY id DESC").map_err(|e|e.to_string())?;
 let mut rows=stmt.query([]).map_err(|e|e.to_string())?;let mut items=Vec::new();
 while let Some(row)=rows.next().map_err(|e|e.to_string())?{
  let raw:String=row.get(2).map_err(|e|e.to_string())?;let scope:Option<String>=row.get(4).map_err(|e|e.to_string())?;let sources:Option<String>=row.get(7).map_err(|e|e.to_string())?;
  let results:Vec<Value>=serde_json::from_str(&raw).map_err(|e|e.to_string())?;
  items.push(json!({"query":row.get::<_,String>(0).map_err(|e|e.to_string())?,"mode":row.get::<_,String>(1).map_err(|e|e.to_string())?,"results":results,"created_at":row.get::<_,f64>(3).map_err(|e|e.to_string())?,"object_ids":scope.map(|v|serde_json::from_str::<Value>(&v)).transpose().map_err(|e|e.to_string())?,"scope_known":row.get::<_,bool>(5).map_err(|e|e.to_string())?,"answer":row.get::<_,Option<String>>(6).map_err(|e|e.to_string())?,"sources":sources.map(|v|serde_json::from_str::<Value>(&v)).transpose().map_err(|e|e.to_string())?,"error":row.get::<_,Option<String>>(8).map_err(|e|e.to_string())?,"agentic":row.get::<_,Option<bool>>(9).map_err(|e|e.to_string())?,"order_exact":true}));
 }
 drop(rows);drop(stmt);
 crate::protected::enrich_history_with(items,|id|db.query_row("SELECT objects.title, COALESCE(search_index.body,''),objects.description FROM objects JOIN search_index ON objects.id=search_index.object_id WHERE objects.id=? AND objects.id NOT IN (SELECT id FROM trash)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string()))
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn corrupt_history_results_are_errors_not_panics(){
  for entry in [json!({"results":[42]}),json!({"results":[{"id":"a","chunks":[42]}]}),json!({"sources":[42]})] {
   assert!(crate::protected::enrich_history_with(vec![entry],|_|Ok(None)).is_err());
  }
 }
 #[test] fn each_source_is_loaded_once_and_stale_results_are_preserved(){
  let (title,text,description)=("Book","ёж📘 text","");let hash=crate::source_hash(title,text,description);
  let matching=json!({"id":"a","title":title,"_source_hash":hash,"chunks":[{"field":"body","start":0,"end":3,"text":"ёж📘"}]});
  let old=json!({"id":"a","title":title,"_source_hash":"old","chunks":[{"field":"body","start":0,"end":3,"text":"ёж📘"}]});
  let missing=json!({"id":"missing","title":"gone","chunks":[{"field":"body","start":0,"end":1,"text":"x"}]});
  let mut calls=std::collections::HashMap::<String,usize>::new();
  let entries=crate::protected::enrich_history_with(vec![json!({"results":[matching,old,missing],"sources":[{"id":"a","field":"body","start":0,"end":3,"text":"ёж📘"}]})],|id|{*calls.entry(id.to_string()).or_default()+=1;Ok(if id=="a"{Some((title.into(),text.into(),description.into()))}else{None})}).unwrap();
  assert_eq!(calls.get("a"),Some(&1));assert_eq!(calls.get("missing"),Some(&1));
  assert_eq!(entries[0]["results"][0]["stale"],false);assert_eq!(entries[0]["results"][0]["chunks"][0]["stale"],false);
  assert_eq!(entries[0]["results"][1]["stale"],true);assert_eq!(entries[0]["results"][2]["missing"],true);assert_eq!(entries[0]["sources"][0]["stale"],false);
  assert!(entries[0]["results"][0].get("_source_hash").is_none());
 }
 #[test] fn chat_sources_and_found_sources_track_changes_and_deletion(){let page=json!({"messages":[{"role":"assistant","content":"answer","sources":[{"id":"a","field":"body","start":0,"end":3,"text":"ёж📘"}],"found_sources":[{"id":"a","field":"body","start":0,"end":3,"text":"ёж📘"},{"id":"gone","field":"body","start":0,"end":1,"text":"x"}]}],"next_before":null});let mut calls=0;let checked=super::enrich_chat_page_with(page.clone(),|id,_,_,_|{calls+=1;Ok(if id=="a"{Some("ёж📘".into())}else{None})}).unwrap();assert_eq!(calls,2);assert_eq!(checked["messages"][0]["sources"][0]["stale"],false);assert_eq!(checked["messages"][0]["found_sources"][0]["stale"],false);assert_eq!(checked["messages"][0]["found_sources"][1]["missing"],true);let changed=super::enrich_chat_page_with(page,|id,_,_,_|Ok(if id=="a"{Some("cha".into())}else{None})).unwrap();assert_eq!(changed["messages"][0]["sources"][0]["stale"],true);assert_eq!(changed["messages"][0]["found_sources"][0]["stale"],true);}
}
