use rusqlite::Connection;
use serde_json::{json,Value};
use std::collections::HashSet;

// Only titles are loaded; body text and embeddings are never read for catalog requests.
pub fn catalog(db:&Connection,query:Option<&str>,offset:usize,limit:usize,allowed:Option<&HashSet<String>>)->Result<Value,String>{
 if offset>i64::MAX as usize||limit==0||limit>128{return Err("Некорректный диапазон".into())}
 let words:Vec<String>=query.unwrap_or("").split(|c:char|!c.is_alphanumeric()&&c!='_').filter(|s|!s.is_empty()).map(|s|s.to_lowercase()).collect();
 if query.is_some()&&words.is_empty(){return Err("Введите слова для поиска по названию".into())}
 if query.is_none(){
  let total:i64=db.query_row("SELECT count(*) FROM objects WHERE id NOT IN (SELECT id FROM trash)",[],|r|r.get(0)).map_err(|e|e.to_string())?;
  let mut stmt=db.prepare("SELECT id,title,kind FROM objects WHERE id NOT IN (SELECT id FROM trash) ORDER BY id LIMIT ?1 OFFSET ?2").map_err(|e|e.to_string())?;
  let items=stmt.query_map(rusqlite::params![limit as i64,offset as i64],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
  let next=offset.saturating_add(items.len());return Ok(json!({"items":items,"total":total,"next_offset":if next<(total as usize){Some(next)}else{None}}))
 }
 let mut stmt=db.prepare("SELECT id,title,kind FROM objects WHERE id NOT IN (SELECT id FROM trash) ORDER BY id").map_err(|e|e.to_string())?;
 let rows=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?;
 // Keep only the best offset+limit rows; the heap root is the lowest ranked retained item.
 let mut best=std::collections::BinaryHeap::new();let mut total=0usize;let capacity=offset.saturating_add(limit);
 for row in rows {let(id,title,kind)=row.map_err(|e|e.to_string())?;if allowed.is_some_and(|ids|!ids.contains(&id)){continue}let folded=title.to_lowercase();let tokens=folded.split(|c:char|!c.is_alphanumeric()&&c!='_').filter(|s|!s.is_empty()).map(|s|s.chars().collect::<Vec<_>>()).collect::<Vec<_>>();let score=words.iter().map(|word|{if folded.contains(word.as_str()){2}else{let chars=word.chars().collect::<Vec<_>>();if (3..=64).contains(&chars.len())&&tokens.iter().any(|t|(3..=64).contains(&t.len())&&crate::search::one_edit(&chars,t)){1}else{0}}}).sum::<usize>();if score==0{continue}total+=1;let item=(std::cmp::Reverse(score),id,title,kind);if best.len()<capacity{best.push(item)}else if best.peek().is_some_and(|worst|&item<worst){best.pop();best.push(item)}}
 let mut best=best.into_vec();best.sort_by(|a,b|a.0.cmp(&b.0).then_with(||a.1.cmp(&b.1)));let items=best.into_iter().skip(offset).take(limit).map(|(_,id,title,kind)|json!({"id":id,"title":title,"kind":kind})).collect::<Vec<_>>();let next=offset.saturating_add(items.len());
 Ok(json!({"items":items,"total":total,"next_offset":if next<total{Some(next)}else{None}}))
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn catalogs_titles_without_trash_and_supports_unicode_search() {
  let db=rusqlite::Connection::open_in_memory().unwrap();
  db.execute_batch("CREATE TABLE objects(id TEXT PRIMARY KEY,title TEXT,kind TEXT); CREATE TABLE trash(id TEXT); INSERT INTO objects VALUES('1','Машинное обучение','note'),('2','Машинное зрение','file'),('3','Обучение удалено','note'); INSERT INTO trash VALUES('3');").unwrap();
  let list=catalog(&db,None,0,1,None).unwrap();assert_eq!(list["total"],2);assert_eq!(list["next_offset"],1);assert_eq!(list["items"][0]["id"],"1");
  let found=catalog(&db,Some("ОБУЧЕНИЕ"),0,10,None).unwrap();assert_eq!(found["total"],1);assert_eq!(found["items"][0]["id"],"1");
  let allowed=std::collections::HashSet::from(["2".to_string()]);assert_eq!(catalog(&db,Some("обучение"),0,10,Some(&allowed)).unwrap()["total"],0);
  let ranked=catalog(&db,Some("ОБУЧЕНИЕ зрение"),0,10,None).unwrap();assert_eq!(ranked["total"],2);
  for q in ["обучениея","обучние","обчуение"] {let typo=catalog(&db,Some(q),0,10,None).unwrap();assert_eq!(typo["items"][0]["id"],"1","{q}");}
  let first=catalog(&db,Some("МАШИННОЕ"),0,1,None).unwrap();let second=catalog(&db,Some("МАШИННОЕ"),1,1,None).unwrap();assert_eq!(first["items"][0]["id"],"1");assert_eq!(first["next_offset"],1);assert_eq!(second["items"][0]["id"],"2");assert!(second["next_offset"].is_null());
  let deep=catalog(&db,None,10001,1,None).unwrap();assert_eq!(deep["total"],2);assert!(deep["items"].as_array().unwrap().is_empty());
 }
}
