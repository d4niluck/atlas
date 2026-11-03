use rusqlite::{Connection, OptionalExtension};
use serde_json::{json,Value};

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
}
