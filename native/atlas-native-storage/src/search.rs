use rusqlite::{params_from_iter, Connection};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};

fn one_edit(a: &[char], b: &[char]) -> bool {
    if a.len().abs_diff(b.len()) > 1 { return false; }
    let (mut i, mut j, mut edits) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] { i += 1; j += 1; continue; }
        edits += 1;
        if edits > 1 { return false; }
        if a.len() == b.len() {
            if i + 1 < a.len() && a[i] == b[j+1] && a[i+1] == b[j] { i += 2; j += 2; }
            else { i += 1; j += 1; }
        } else if a.len() > b.len() { i += 1; } else { j += 1; }
    }
    edits += usize::from(i < a.len() || j < b.len());
    edits == 1
}

fn variants(db: &Connection, words: &[String]) -> rusqlite::Result<Vec<String>> {
    db.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS temp.atlas_search_vocabulary USING fts5vocab(main, search_chunks, row)")?;
    let mut candidates = Vec::new();
    let mut stmt = db.prepare("SELECT term, doc FROM temp.atlas_search_vocabulary WHERE term >= ?1 AND term < ?2")?;
    for word in words.iter().take(16) {
        let word = word.to_lowercase();
        let w: Vec<char> = word.chars().collect();
        if !(3..=64).contains(&w.len()) { continue; }
        // Only two-character prefix ranges are read: one typo can alter at most
        // the first two characters. This avoids scanning the whole vocabulary.
        let alphabet = "abcdefghijklmnopqrstuvwxyzабвгдеёжзийклмнопрстуфхцчшщъыьэюя0123456789";
        let mut prefixes = BTreeSet::new();
        prefixes.insert(format!("{}{}", w[0], w[1]));
        prefixes.insert(format!("{}{}", w[1], w[0]));
        prefixes.insert(format!("{}{}", w[1], w[2]));
        prefixes.insert(format!("{}{}", w[0], w[2]));
        for c in alphabet.chars() {
            prefixes.insert(format!("{}{}", c, w[0]));
            prefixes.insert(format!("{}{}", c, w[1]));
            prefixes.insert(format!("{}{}", w[0], c));
        }
        let mut matches: Vec<(i64, String)> = Vec::new();
        for prefix in prefixes {
            let upper = format!("{}{}", prefix, char::MAX);
            let mut rows = stmt.query([&prefix, &upper])?;
            while let Some(row) = rows.next()? {
                let term: String = row.get(0)?;
                let chars: Vec<char> = term.chars().collect();
                if one_edit(&w, &chars) {
                    matches.push((row.get(1)?, term));
                }
            }
        }
        matches.sort_unstable_by(|a,b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        matches.dedup_by(|a,b| a.1 == b.1);
        candidates.extend(matches.into_iter().take(12).map(|(_,term)| term));
    }
    Ok(candidates)
}

pub fn lexical(db: &Connection, query: &str, ids: Option<&[String]>, limit: usize) -> rusqlite::Result<Vec<Value>> {
    let words: Vec<String> = query.split(|c:char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).map(str::to_owned).collect();
    if words.is_empty() || limit == 0 || ids.is_some_and(|v| v.is_empty()) { return Ok(vec![]); }
    let mut sql = String::from("SELECT objects.id,objects.title,search_chunks.start,search_chunks.field,search_chunks.content,bm25(search_chunks) FROM search_chunks JOIN objects ON objects.id=search_chunks.object_id WHERE search_chunks MATCH ? AND objects.id NOT IN (SELECT id FROM trash)");
    if let Some(ids) = ids { sql.push_str(" AND objects.id IN ("); sql.push_str(&vec!["?";ids.len()].join(",")); sql.push(')'); }
    sql.push_str(" ORDER BY bm25(search_chunks),objects.id,CAST(search_chunks.start AS INTEGER) LIMIT ?");
    let run = |terms: &[String]| -> rusqlite::Result<Vec<Value>> {
        if terms.is_empty() { return Ok(vec![]); }
        let mut args: Vec<rusqlite::types::Value> = vec![terms.iter().map(|x|format!("\"{}\"", x)).collect::<Vec<_>>().join(" OR ").into()];
        if let Some(ids) = ids { args.extend(ids.iter().cloned().map(Into::into)); }
        args.push((limit.min(128) as i64).into());
        let mut stmt = db.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), |r| {
            let id:String=r.get(0)?; let title:String=r.get(1)?; let start:i64=r.get(2)?; let field:String=r.get(3)?; let text:String=r.get(4)?; let score=-r.get::<_,f64>(5)?;
            let chunks=if start>=0 {vec![json!({"text":text,"start":start,"end":start+text.chars().count() as i64,"field":field,"score":score})]} else {vec![]};
            Ok(json!({"id":id,"title":title,"chunks":chunks,"score":score}))
        })?;
        rows.collect()
    };
    let mut exact = run(&words)?;
    if words.len() > 1 || exact.len() < limit.min(128) {
        let alternatives = variants(db, &words)?;
        let mut seen: HashSet<(String,String,i64)> = exact.iter().map(key).collect();
        for row in run(&alternatives)? { if seen.insert(key(&row)) { exact.push(row); } }
        if words.len() > 1 {
            // A rare corrected word must outrank unrelated hits for a common
            // exact word; among equal coverage exact spelling wins.
            let normalized:Vec<Vec<char>>=words.iter().map(|w|w.to_lowercase().chars().collect()).collect();
            exact.sort_by_cached_key(|row| {
                let text=format!("{} {}",row["title"].as_str().unwrap_or(""),row["chunks"][0]["text"].as_str().unwrap_or(""));
                let tokens:Vec<Vec<char>>=text.split(|c:char|!c.is_alphanumeric()).filter(|s|!s.is_empty()).map(|s|s.to_lowercase().chars().collect()).collect();
                let mut covered=0usize;let mut exact_words=0usize;
                for word in &normalized {
                    if tokens.iter().any(|t|t==word){covered+=1;exact_words+=1;}
                    else if (3..=64).contains(&word.len()) && tokens.iter().any(|t|one_edit(word,t)){covered+=1;}
                }
                (std::cmp::Reverse(covered),std::cmp::Reverse(exact_words))
            });
        }
        exact.truncate(limit.min(128));
    }
    Ok(exact)
}
fn key(row:&Value)->(String,String,i64) { let chunk=&row["chunks"][0]; (row["id"].as_str().unwrap_or("").into(),chunk["field"].as_str().unwrap_or("title").into(),chunk["start"].as_i64().unwrap_or(-1)) }

pub fn fuse(lexical: Vec<Value>, semantic: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<(f64,usize,Value)> = Vec::new();
    for (list_no, list) in [semantic,lexical].into_iter().enumerate() {
        for (rank, row) in list.into_iter().enumerate() {
            let row_key=key(&row);
            if let Some(item)=out.iter_mut().find(|x|key(&x.2)==row_key) { item.0 += 1.0/(60.0+rank as f64+1.0); }
            else {out.push((1.0/(60.0+rank as f64+1.0),list_no,row));}
        }
    }
    out.sort_by(|a,b|b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    out.into_iter().take(128).map(|(_,_,row)|row).collect()
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn edits() {for (a,b) in [("превет","привет"),("привт","привет"),("приивет","привет"),("пирвет","привет"),("zearch","search")] {assert!(one_edit(&a.chars().collect::<Vec<_>>(),&b.chars().collect::<Vec<_>>()));} assert!(!one_edit(&['a','b','c'],&['x','y','z']));}
 #[test] fn vocabulary_typo_positions() {let db=Connection::open_in_memory().unwrap();db.execute_batch("CREATE TABLE objects(id TEXT,title TEXT); CREATE TABLE trash(id TEXT); CREATE VIRTUAL TABLE search_chunks USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content); INSERT INTO objects VALUES('1','one'); INSERT INTO search_chunks VALUES('1',0,'body','индексация');").unwrap();for q in ["йндексация","нидексация","индксация","индексацияя","индексацич"] {assert_eq!(lexical(&db,q,None,128).unwrap().len(),1,"{q}");}}
 #[test] fn typo_with_common_word_is_not_lost() {let db=Connection::open_in_memory().unwrap();db.execute_batch("CREATE TABLE objects(id TEXT,title TEXT); CREATE TABLE trash(id TEXT); CREATE VIRTUAL TABLE search_chunks USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content);").unwrap();for i in 0..160 {let id=i.to_string(); db.execute("INSERT INTO objects VALUES(?1,?1)",[&id]).unwrap();db.execute("INSERT INTO search_chunks VALUES(?1,0,'body','как')",[&id]).unwrap();}for (id,text) in [("matched","как индексация"),("exact","как индексацтя")] {db.execute("INSERT INTO objects VALUES(?1,?1)",[id]).unwrap();db.execute("INSERT INTO search_chunks VALUES(?1,0,'body',?2)",rusqlite::params![id,text]).unwrap();}let rows=lexical(&db,"как индексацтя",None,128).unwrap();assert_eq!(rows[0]["id"],"exact");assert!(rows.iter().any(|r|r["id"]=="matched"),"relevant corrected word omitted");assert!(rows.iter().position(|r|r["id"]=="matched").unwrap()<10);}
 #[test] fn lexical_typo_and_exact_first() {let db=Connection::open_in_memory().unwrap(); db.execute_batch("CREATE TABLE objects(id TEXT,title TEXT); CREATE TABLE trash(id TEXT); CREATE VIRTUAL TABLE search_chunks USING fts5(object_id UNINDEXED,start UNINDEXED,field UNINDEXED,content); INSERT INTO objects VALUES('1','one'),('2','two'); INSERT INTO search_chunks VALUES('1',0,'body','привет мир'),('2',0,'body','превет мир');").unwrap(); let r=lexical(&db,"превет",None,128).unwrap(); assert_eq!(r[0]["id"],"2");assert!(r.iter().any(|x|x["id"]=="1")); let r=lexical(&db,"зривет",Some(&["1".into()]),128).unwrap();assert_eq!(r.len(),1);}
}
