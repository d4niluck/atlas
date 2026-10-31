use super::*;
use serde_json::{json, Value};
fn item(id: &str) -> Value { json!({"id":id,"title":id,"score":0.9,"chunks":[{"field":"body","start":0,"end":1,"text":"x"}]}) }
#[test]
fn all_subquestions_get_early_context_and_originals_stay_available() {
 let originals:Vec<Value>=(0..8).map(|i|item(&format!("old{i}"))).collect();
 let groups=vec![vec![item("a0"),item("a1")],vec![item("b0"),item("b1")],vec![item("c0"),item("c1")]];
 let merged=merge_agentic(&groups,originals);
 let first:Vec<_>=merged.iter().take(8).map(|x|x["id"].as_str().unwrap()).collect();
 assert!(first.contains(&"a0") && first.contains(&"b0") && first.contains(&"c0"),"{first:?}");
 for i in 0..8 {assert!(merged.iter().any(|x|x["id"]==format!("old{i}")));}
}
#[test]
fn identical_span_is_not_duplicated_across_queries() {
 let merged=merge_agentic(&[vec![item("a"),item("b")],vec![item("a"),item("c")]],vec![item("a"),item("d")]);
 assert_eq!(merged.iter().filter(|x|x["id"]=="a").count(),1);
}
#[test]
fn failed_retry_preserves_answer_source_pair() {
 let mut answer=Some("Ответ [1]".to_string());
 let mut sources=vec![item("original")];
 let old_answer=answer.clone();let old_sources=sources.clone();
 assert!(!accept_answer(&mut answer,&mut sources,vec![item("new")],Err("generator failed".into())));
 assert_eq!(answer,old_answer);assert_eq!(sources,old_sources);
}
#[test]
fn successful_retry_replaces_answer_and_sources_together() {
 let mut answer=Some("Старый [1]".to_string());let mut sources=vec![item("old")];
 let proposed=vec![item("new")];
 assert!(accept_answer(&mut answer,&mut sources,proposed.clone(),Ok("Новый [1]".into())));
 assert_eq!(answer.as_deref(),Some("Новый [1]"));assert_eq!(sources,proposed);
}
