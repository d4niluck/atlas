use atlas_faiss_probe::{Index, overlay::FlatOverlay};

fn v(first: f32, second: f32) -> [f32; 512] {
    let mut x = [0.0; 512];
    x[0] = first;
    x[1] = second;
    x
}
fn fixture() -> FlatOverlay {
    let mut base = Index::new().unwrap();
    base.add(&[v(1.0, 0.0), v(0.9, 0.0), v(0.8, 0.0), v(0.7, 0.0)]).unwrap();
    FlatOverlay::new(base, vec!["a".into(), "b".into(), "a".into(), "c".into()]).unwrap()
}
fn ids(rows: &[(String, usize, f32)]) -> Vec<(&str, usize)> {
    rows.iter().map(|(id, position, _)| (id.as_str(), *position)).collect()
}

#[test]
fn searches_original_index_with_fragment_positions_and_filter_before_limit() {
    let idx = fixture();
    let all = idx.search(&v(1.0,0.0), None, 4).unwrap();
    assert_eq!(ids(&all), vec![("a",0),("b",0),("a",1),("c",0)]);
    let allowed = vec![String::from("c")];
    let filtered = idx.search(&v(1.0,0.0), Some(&allowed), 1).unwrap();
    assert_eq!(ids(&filtered), vec![("c",0)]);
    assert!(idx.search(&v(1.0,0.0), Some(&[]), 10).unwrap().is_empty());
    assert!(idx.search(&v(1.0,0.0), None, 0).unwrap().is_empty());
}

#[test]
fn replacing_object_excludes_all_old_fragments_before_top_k() {
    let mut idx = fixture();
    idx.replace("a", &[v(0.95,0.0),v(0.10,0.0)]).unwrap();
    let all = idx.search(&v(1.0,0.0), None, 5).unwrap();
    assert_eq!(ids(&all), vec![("a",0),("b",0),("c",0),("a",1)]);
    assert!((all[0].2-0.95).abs() < 1e-6);
    let allowed = vec![String::from("a")];
    assert_eq!(ids(&idx.search(&v(1.0,0.0), Some(&allowed), 3).unwrap()), vec![("a",0),("a",1)]);
}

#[test]
fn repeated_edits_deletion_and_restoration_do_not_resurrect_stale_fragments() {
    let mut idx = fixture();
    idx.replace("a", &[v(0.95,0.0)]).unwrap();
    idx.replace("a", &[v(0.30,0.0),v(0.20,0.0),v(0.10,0.0)]).unwrap();
    idx.remove("b").unwrap();
    assert_eq!(ids(&idx.search(&v(1.0,0.0),None,10).unwrap()), vec![("c",0),("a",0),("a",1),("a",2)]);
    idx.replace("b", &[v(0.9,0.0)]).unwrap();
    assert_eq!(ids(&idx.search(&v(1.0,0.0),None,2).unwrap()), vec![("b",0),("c",0)]);
    idx.remove("a").unwrap();
    assert_eq!(ids(&idx.search(&v(1.0,0.0),None,10).unwrap()), vec![("b",0),("c",0)]);
}

#[test]
fn new_object_and_empty_replacement_work_with_filter() {
    let mut idx = fixture();
    idx.replace("new", &[v(1.1,0.0)]).unwrap();
    assert_eq!(ids(&idx.search(&v(1.0,0.0),None,1).unwrap()),vec![("new",0)]);
    idx.replace("new", &[]).unwrap();
    let only_new = vec![String::from("new")];
    assert!(idx.search(&v(1.0,0.0),Some(&only_new),3).unwrap().is_empty());
}

#[test]
fn mixed_base_and_edits_match_exact_full_rebuild_for_many_queries() {
    use std::collections::BTreeMap;
    let mut base = Index::new().unwrap();
    let mut vectors = Vec::new();
    let mut keys = Vec::new();
    let mut truth: BTreeMap<String, Vec<[f32;512]>> = BTreeMap::new();
    for i in 0..31 {
        let key=format!("item-{i:02}");
        let vv=vec![v((i as f32+1.0)/100.0, ((i*13%31) as f32+1.0)/100.0)];
        keys.push(key.clone()); vectors.extend_from_slice(&vv); truth.insert(key,vv);
    }
    base.add(&vectors).unwrap();
    let mut idx=FlatOverlay::new(base,keys).unwrap();
    for i in [0,3,8,12,17,24,30] {
        let key=format!("item-{i:02}");
        let vv=vec![v((i as f32+37.0)/100.0,((i*17%31) as f32+51.0)/100.0), v((i as f32+71.0)/100.0,((i*7%31) as f32+101.0)/100.0)];
        idx.replace(&key,&vv).unwrap(); truth.insert(key,vv);
    }
    for i in [2,11,23] {let key=format!("item-{i:02}");idx.remove(&key).unwrap();truth.remove(&key);}
    for qi in 1..21 {
        let q=v(0.137*qi as f32 + 0.011,0.081*qi as f32 + 0.017);
        for filtered in [false,true] {
            let allow:Vec<String>=truth.keys().filter(|k|k.ends_with('0')||k.ends_with('3')||k.ends_with('7')).cloned().collect();
            let filter=if filtered {Some(allow.as_slice())}else{None};
            let mut expected:Vec<(String,usize,f32)>=truth.iter().filter(|(key,_)|filter.is_none_or(|ids|ids.contains(key))).flat_map(|(key,vs)|vs.iter().enumerate().map(|(i,x)|(key.clone(),i,x[0]*q[0]+x[1]*q[1]))).collect();
            expected.sort_by(|a,b|b.2.total_cmp(&a.2));expected.truncate(7);
            let got=idx.search(&q,filter,7).unwrap();
            assert_eq!(ids(&got),ids(&expected),"query {qi}, filtered={filtered}");
            for (a,b) in got.iter().zip(&expected) {assert!((a.2-b.2).abs()<1e-6);}
        }
    }
}

fn base_for_snapshot() -> (Index,Vec<String>) {
    let mut base=Index::new().unwrap();
    base.add(&[v(1.0,0.0),v(0.9,0.0),v(0.8,0.0),v(0.7,0.0)]).unwrap();
    (base,vec!["a".into(),"b".into(),"a".into(),"c".into()])
}
#[test]
fn snapshot_roundtrip_retains_edits_removals_empty_and_filtered_results() {
    let mut idx=fixture();
    idx.apply_batch(vec![("a".into(),Some(vec![v(0.33,0.0),v(0.21,0.0)])),("b".into(),None),("empty".into(),Some(vec![])),("new".into(),Some(vec![v(0.99,0.0)]))]).unwrap();
    let snap=idx.snapshot_delta().unwrap();let (base,keys)=base_for_snapshot();
    let restored=FlatOverlay::from_snapshot(base,keys,snap).unwrap();
    for filter in [None,Some(vec!["a".into(),"new".into()]),Some(vec!["b".into()])]{
        let a=idx.search(&v(1.0,0.0),filter.as_deref(),10).unwrap();let b=restored.search(&v(1.0,0.0),filter.as_deref(),10).unwrap();
        assert_eq!(a,b);
    }
}
#[test]
fn snapshot_rejects_corrupt_or_inconsistent_metadata() {
    let mut idx=fixture();idx.replace("a",&[v(0.3,0.0),v(0.4,0.0)]).unwrap();idx.remove("b").unwrap();
    macro_rules! reject {($mutate:expr)=>{{let mut snap=idx.snapshot_delta().unwrap();$mutate(&mut snap);let (base,keys)=base_for_snapshot();assert!(FlatOverlay::from_snapshot(base,keys,snap).is_err());}}}
    reject!(|s:&mut atlas_faiss_probe::overlay::DeltaSnapshot|s.index.clear());
    reject!(|s:&mut atlas_faiss_probe::overlay::DeltaSnapshot|{s.records.pop();});
    reject!(|s:&mut atlas_faiss_probe::overlay::DeltaSnapshot|s.changed_keys.push("a".into()));
    reject!(|s:&mut atlas_faiss_probe::overlay::DeltaSnapshot|s.changed_keys.clear());
    reject!(|s:&mut atlas_faiss_probe::overlay::DeltaSnapshot|s.records[0].1=1);
    reject!(|s:&mut atlas_faiss_probe::overlay::DeltaSnapshot|s.records.reverse());
    reject!(|s:&mut atlas_faiss_probe::overlay::DeltaSnapshot|s.removed.push("a".into()));
    reject!(|s:&mut atlas_faiss_probe::overlay::DeltaSnapshot|s.removed.push("b".into()));
}
#[test]
fn batch_last_write_wins_and_matches_sequential_operations() {
    let mut batch=fixture();let mut serial=fixture();
    let changes=vec![("a".into(),Some(vec![v(0.4,0.0)])),("b".into(),None),("a".into(),Some(vec![v(0.6,0.0),v(0.5,0.0)])),("new".into(),Some(vec![]))];
    batch.apply_batch(changes.clone()).unwrap();
    for (key,value) in changes {match value{Some(vectors)=>serial.replace(&key,&vectors).unwrap(),None=>serial.remove(&key).unwrap()}}
    assert_eq!(batch.search(&v(1.0,0.0),None,10).unwrap(),serial.search(&v(1.0,0.0),None,10).unwrap());
}

#[test]
fn stable_ties_at_top_k_boundary_across_base_and_delta() {
    let mut base=Index::new().unwrap();let vectors=vec![v(1.0,0.0);8];base.add(&vectors).unwrap();
    let mut idx=FlatOverlay::new(base,["h","g","f","e","d","c","b","a"].iter().map(|x|x.to_string()).collect()).unwrap();
    idx.replace("j",&[v(1.0,0.0)]).unwrap();idx.replace("i",&[v(1.0,0.0)]).unwrap();
    assert_eq!(ids(&idx.search(&v(1.0,0.0),None,3).unwrap()),vec![("a",0),("b",0),("c",0)]);
}

#[test]
fn materialized_index_preserves_search_and_mapping_above_one_batch() {
    let mut base=Index::new().unwrap();let mut vectors=Vec::new();let mut keys=Vec::new();
    for i in 0..330 {vectors.push(v((i as f32+1.0)/1000.0,((i*47%337) as f32+1.0)/1000.0));keys.push(format!("item-{i:03}"));}
    base.add(&vectors).unwrap();let mut idx=FlatOverlay::new(base,keys.clone()).unwrap();
    idx.apply_batch(vec![("item-000".into(),None),("item-001".into(),Some(vec![])),("item-002".into(),Some(vec![v(0.75,0.81),v(0.76,0.82)])),("new".into(),Some(vec![v(0.90,0.91)]))]).unwrap();
    let snap=idx.snapshot_delta().unwrap();let mut base2=Index::new().unwrap();base2.add(&vectors).unwrap();let restored=FlatOverlay::from_snapshot(base2,keys,snap).unwrap();
    for overlay in [idx,restored]{let (flat,mapping)=overlay.materialize().unwrap();assert_eq!(flat.count(),mapping.len() as i64);assert_eq!(mapping.len(),330);
      for filter in [None,Some(vec!["item-002".to_string(),"new".to_string(),"item-300".to_string()])]{
        let q=v(0.21,0.51);let expected=overlay.search(&q,filter.as_deref(),10).unwrap();
        let allowed_ids:Vec<i64>=mapping.iter().enumerate().filter(|(_,r)|filter.as_ref().is_none_or(|allowed|allowed.contains(&r.0))).map(|(i,_)|i as i64).collect();
        let actual:Vec<(String,usize,f32)>=flat.search(&q,&allowed_ids,10).unwrap().into_iter().map(|(id,s)|{let r=&mapping[id as usize];(r.0.clone(),r.1,s)}).collect();
        assert_eq!(ids(&actual),ids(&expected));for (a,b) in actual.iter().zip(expected.iter()){assert!((a.2-b.2).abs()<1e-6)}
      }
    }
}

#[test]
fn invalid_vectors_rejected_without_partial_state_changes() {
    let mut idx=fixture();idx.replace("a",&[v(0.3,0.0)]).unwrap();let before=idx.search(&v(1.0,0.0),None,10).unwrap();
    for invalid in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY]{
      assert!(idx.apply_batch(vec![("b".into(),None),("c".into(),Some(vec![v(invalid,0.0)]))]).is_err(),"must reject non-finite vector");
      assert_eq!(idx.search(&v(1.0,0.0),None,10).unwrap(),before);
    }
}
#[test]
fn restore_rejects_non_finite_vector_in_serialized_index() {
    let mut idx=fixture();idx.replace("a",&[v(0.3,0.0)]).unwrap();let mut snap=idx.snapshot_delta().unwrap();
    let mut invalid=Index::new().unwrap();invalid.add(&[v(f32::NAN,0.0)]).unwrap();snap.index=invalid.to_memory().unwrap();
    let (base,keys)=base_for_snapshot();assert!(FlatOverlay::from_snapshot(base,keys,snap).is_err());
}

#[test]
fn hidden_edited_object_survives_snapshot_and_restores_without_reencoding() {
 let mut idx=fixture();idx.replace("a",&[v(0.99,0.0),v(0.88,0.0)]).unwrap();
 let only_a=vec!["a".to_owned()];let before=idx.search(&v(1.0,0.0),Some(&only_a),10).unwrap();
 idx.hide("a").unwrap();assert!(idx.has_hidden());assert!(idx.search(&v(1.0,0.0),Some(&only_a),10).unwrap().is_empty());
 let snap=idx.snapshot_delta().unwrap();let (base,keys)=base_for_snapshot();let mut reopened=FlatOverlay::from_snapshot(base,keys,snap).unwrap();
 assert!(reopened.has_hidden());assert!(reopened.search(&v(1.0,0.0),Some(&only_a),10).unwrap().is_empty());
 reopened.revive("a").unwrap();assert!(!reopened.has_hidden());assert_eq!(reopened.search(&v(1.0,0.0),Some(&only_a),10).unwrap(),before);
}
#[test]
fn purge_hidden_edited_object_never_revives_stale_base_or_delta() {
 let mut idx=fixture();idx.replace("a",&[v(0.99,0.0)]).unwrap();let only_a=vec!["a".to_owned()];
 idx.hide("a").unwrap();idx.purge("a").unwrap();assert!(!idx.has_hidden());assert!(idx.search(&v(1.0,0.0),Some(&only_a),10).unwrap().is_empty());
 idx.revive("a").unwrap();assert!(idx.search(&v(1.0,0.0),Some(&only_a),10).unwrap().is_empty());
 let snap=idx.snapshot_delta().unwrap();let (base,keys)=base_for_snapshot();let mut reopened=FlatOverlay::from_snapshot(base,keys,snap).unwrap();reopened.revive("a").unwrap();assert!(reopened.search(&v(1.0,0.0),Some(&only_a),10).unwrap().is_empty());
 reopened.replace("a",&[v(0.42,0.0)]).unwrap();let after=reopened.search(&v(1.0,0.0),Some(&only_a),10).unwrap();assert_eq!(after.len(),1);assert!((after[0].2-0.42).abs()<1e-6);
}
#[test]
fn hidden_key_metadata_and_materialize_are_consistent() {
 let mut idx=fixture();assert!(idx.has_key("a"));assert!(!idx.has_key("never"));assert!(idx.hide("never").is_err());
 idx.replace("new",&[v(0.99,0.0)]).unwrap();idx.hide("new").unwrap();idx.hide("b").unwrap();
 assert!(idx.has_key("new")&&idx.has_key("b"));assert_eq!(idx.hidden_keys(),vec!["b","new"]);
 let (flat,rows)=idx.materialize().unwrap();assert!(flat.count()>0);assert!(!rows.iter().any(|r|r.0=="new"||r.0=="b"));
 idx.purge("new").unwrap();assert!(!idx.has_key("new"));let snap=idx.snapshot_delta().unwrap();assert!(!snap.removed.iter().any(|k|k=="new"));assert_eq!(snap.hidden,vec!["b"]);
 let (base,keys)=base_for_snapshot();let mut reopened=FlatOverlay::from_snapshot(base,keys,snap).unwrap();assert!(reopened.has_key("b"));assert_eq!(reopened.hidden_keys(),vec!["b"]);reopened.purge("b").unwrap();assert!(!reopened.has_key("b"));assert!(!reopened.has_hidden());
}
#[test]
fn physical_reconstruction_bounds_and_hidden_survival() {
 let mut idx=fixture();idx.replace("a",&[v(0.42,0.51)]).unwrap();idx.hide("a").unwrap();assert_eq!(idx.reconstruct_base(0).unwrap(),v(1.0,0.0));assert_eq!(idx.reconstruct_delta(0).unwrap(),v(0.42,0.51));
 for i in [-1,4,i64::MAX]{assert!(idx.reconstruct_base(i).is_err())}for i in [-1,1,i64::MAX]{assert!(idx.reconstruct_delta(i).is_err())}
 let snap=idx.snapshot_delta().unwrap();let (base,keys)=base_for_snapshot();let mut reopened=FlatOverlay::from_snapshot(base,keys,snap).unwrap();assert_eq!(reopened.reconstruct_delta(0).unwrap(),v(0.42,0.51));reopened.revive("a").unwrap();assert_eq!(reopened.search(&v(1.0,0.0),Some(&["a".into()]),1).unwrap()[0].2,0.42);
}
#[test]
fn materialize_reconstructs_every_vector_including_duplicates_and_hidden() {
 use std::collections::HashMap;
 let mut base=Index::new().unwrap();let mut keys=Vec::new();let mut vectors=Vec::new();let mut truth:HashMap<(String,usize),[f32;512]>=HashMap::new();let mut counts=HashMap::<String,usize>::new();
 for i in 0..333{let key=format!("k{:03}",i%300);let pos=counts.entry(key.clone()).or_default();let x=if i%4==0 {v(0.6,0.8)} else {v(i as f32*0.001+0.001,1.0/(i as f32+1.0))};truth.insert((key.clone(),*pos),x);*pos+=1;keys.push(key);vectors.push(x)}
 base.add(&vectors).unwrap();let mut overlay=FlatOverlay::new(base,keys).unwrap();
 overlay.replace("k002",&[v(0.1,0.2),v(0.1,0.2)]).unwrap();truth.retain(|(k,_),_|k!="k002");truth.insert(("k002".into(),0),v(0.1,0.2));truth.insert(("k002".into(),1),v(0.1,0.2));
 overlay.hide("k003").unwrap();overlay.purge("k004").unwrap();truth.retain(|(k,_),_|k!="k003"&&k!="k004");
 let (index,rows)=overlay.materialize().unwrap();assert_eq!(rows.len(),truth.len());assert_eq!(index.count(),rows.len() as i64);
 for (i,key) in rows.iter().enumerate(){let expected=truth.remove(key).expect("missing or duplicated mapping");assert_eq!(index.reconstruct(i as i64).unwrap(),expected,"vector mismatch {key:?}")}
 assert!(truth.is_empty());
}
#[test]
fn compact_physically_removes_expired_but_preserves_other_hidden_vectors() {
 let mut idx=fixture();idx.replace("a",&[v(0.99,0.0)]).unwrap();idx.hide("a").unwrap();idx.hide("b").unwrap();idx.purge("c").unwrap();
 let hidden=idx.hidden_keys();assert_eq!(hidden,vec!["a","b"]);
 let (flat,mapping)=idx.materialize_including_hidden().unwrap();assert_eq!(mapping.len(),2);assert!(!mapping.iter().any(|r|r.0=="c"));
 for (i,(key,pos)) in mapping.iter().enumerate(){assert_eq!(*pos,0);assert_eq!(flat.reconstruct(i as i64).unwrap(),if key=="a"{v(0.99,0.0)}else{v(0.9,0.0)});}
 let mut compacted=FlatOverlay::new(flat,mapping.iter().map(|r|r.0.clone()).collect()).unwrap();for key in &hidden{compacted.hide(key).unwrap();}
 let snap=compacted.snapshot_delta().unwrap();let (flat,mapping)=idx.materialize_including_hidden().unwrap();let mut restored=FlatOverlay::from_snapshot(flat,mapping.iter().map(|r|r.0.clone()).collect(),snap).unwrap();
 assert!(restored.search(&v(1.0,0.0),None,10).unwrap().is_empty());restored.revive("a").unwrap();restored.revive("b").unwrap();
 assert_eq!(ids(&restored.search(&v(1.0,0.0),None,10).unwrap()),vec![("a",0),("b",0)]);assert!(!restored.has_key("c"));
 assert!(idx.has_hidden());assert!(idx.search(&v(1.0,0.0),None,10).unwrap().is_empty());
}
#[test]
fn cancellation_during_compaction_keeps_base_delta_and_hidden_unchanged() {
 use std::cell::Cell;
 let mut idx=fixture();idx.replace("a",&[v(0.99,0.0)]).unwrap();idx.hide("a").unwrap();
 let before=idx.search(&v(1.0,0.0),None,10).unwrap();let saved=idx.snapshot_delta().unwrap();let calls=Cell::new(0);
 assert!(idx.materialize_cancellable(true,||{calls.set(calls.get()+1);calls.get()<4}).is_err());assert!(calls.get()>=4);
 assert_eq!(idx.search(&v(1.0,0.0),None,10).unwrap(),before);let after=idx.snapshot_delta().unwrap();assert_eq!(after.index,saved.index);assert_eq!(after.changed_keys,saved.changed_keys);assert_eq!(after.hidden,saved.hidden);assert_eq!(after.removed,saved.removed);
 let (flat,records)=idx.materialize_cancellable(true,||true).unwrap();assert_eq!(flat.count(),records.len() as i64);assert!(records.iter().any(|r|r.0=="a"));
}
#[test]
fn incremental_materialize_is_bounded_restartable_and_rejects_stale_or_foreign_owner(){
 let mut idx=fixture();idx.replace("a",&[v(0.91,0.0),v(0.92,0.0)]).unwrap();idx.hide("a").unwrap();
 let expected=idx.materialize_including_hidden().unwrap();let before=idx.snapshot_delta().unwrap();
 let incomplete=idx.start_materialize(true).unwrap();assert!(incomplete.finish(&idx).is_err());
 let mut cursor=idx.start_materialize(true).unwrap();let mut steps=0;loop{steps+=1;assert!(steps<=20);if cursor.step(&idx,1).unwrap(){break}}assert!(steps>=6,"budget of 1 must inspect every base/delta entry");
 let (index,rows)=cursor.finish(&idx).unwrap();assert_eq!(rows,expected.1);for i in 0..index.count(){assert_eq!(index.reconstruct(i).unwrap(),expected.0.reconstruct(i).unwrap())}
 let mut stale=idx.start_materialize(true).unwrap();stale.step(&idx,1).unwrap();idx.revive("a").unwrap();assert!(stale.step(&idx,1).is_err());assert!(stale.finish(&idx).is_err());
 let mut first=fixture();let mut second=fixture();first.replace("a",&[v(0.5,0.0)]).unwrap();second.replace("a",&[v(0.2,0.0)]).unwrap();let mut foreign=first.start_materialize(true).unwrap();assert!(foreign.step(&second,1).is_err());
 let after=idx.snapshot_delta().unwrap();assert_eq!(after.index,before.index);assert_eq!(after.changed_keys,before.changed_keys);assert_eq!(after.hidden,Vec::<String>::new());
}

#[test]
fn preloaded_delta_preserves_search_and_checks_metadata_without_serialized_buffer() {
    let mut original=fixture();
    original.apply_batch(vec![("a".into(),Some(vec![v(0.33,0.0),v(0.21,0.0)])),("b".into(),None), ("new".into(),Some(vec![v(0.99,0.0)]))]).unwrap();
    let mut snap=original.snapshot_delta().unwrap();
    let delta=Index::from_memory(&snap.index).unwrap();
    snap.index.clear();
    let (base,keys)=base_for_snapshot();
    let loaded=FlatOverlay::from_snapshot_index(base,keys,snap,delta).unwrap();
    for filter in [None,Some(vec!["a".into(),"new".into()]),Some(vec!["b".into()])] {
        assert_eq!(loaded.search(&v(1.0,0.0),filter.as_deref(),10).unwrap(),original.search(&v(1.0,0.0),filter.as_deref(),10).unwrap());
    }
    let mut bad=original.snapshot_delta().unwrap();
    let delta=Index::from_memory(&bad.index).unwrap();
    bad.index.clear();
    bad.records[0].1=1;
    let (base,keys)=base_for_snapshot();
    assert!(FlatOverlay::from_snapshot_index(base,keys,bad,delta).is_err());
}
