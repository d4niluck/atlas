/// Sum resident memory of the app process and its descendants; shared pages may be counted repeatedly.
pub fn process_tree_rss_kib(text:&str,root:u32,exclude:u32)->Option<u64>{
 let rows:Vec<(u32,u32,u64)>=text.lines().filter_map(|line|{let mut x=line.split_whitespace();Some((x.next()?.parse().ok()?,x.next()?.parse().ok()?,x.next()?.parse().ok()?))}).collect();
 if !rows.iter().any(|r|r.0==root){return None}
 let mut ids=std::collections::HashSet::from([root]);
 loop{let old=ids.len();for (pid,parent,_) in &rows{if ids.contains(parent){ids.insert(*pid);}}if old==ids.len(){break}}
 Some(rows.iter().filter(|r|r.0!=exclude&&ids.contains(&r.0)).map(|r|r.2).sum())
}
#[cfg(test)] mod tests{
 use super::*;
 #[test]fn includes_descendants_and_excludes_other_apps_and_ps(){let ps="10 1 100\n11 10 200\n12 11 300\n13 10 20\n20 1 500\n";assert_eq!(process_tree_rss_kib(ps,10,13),Some(600));}
 #[test]fn missing_root_is_not_zero(){assert_eq!(process_tree_rss_kib("2 1 0\n",10,0),None)}
}
