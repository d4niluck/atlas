use atlas_llama_probe::index_keys_compatible;

const Q8_CPU: &str = "242feaef290d2510c2074d7276184c6a6e2026fbdd50f592dabe7213a9cfdd9e";
const Q8_METAL: &str = "6f0854163647a583811c00bd0ca24eb8ff49f9b5898b2311804ff1871d90b4d6";
const BF16_CPU: &str = "4aafee636d8617521f67f926b2addc41797394d3eefd3927dbd66fcb4a4e47dd";
const BF16_METAL: &str = "d0e160e8253cf51d313fd886df6f0af258239e39366e7f33a9baa37fbe289302";

#[test]
fn tested_variants_reuse_existing_indexes_on_both_devices() {
 for left in [Q8_CPU,Q8_METAL,BF16_CPU,BF16_METAL] {
  for right in [Q8_CPU,Q8_METAL,BF16_CPU,BF16_METAL] {
   assert!(index_keys_compatible(left,right), "{left} != {right}");
   assert!(index_keys_compatible(&format!("{left}:vision-q8-v1"),&format!("{right}:vision-q8-v1")));
  }
 }
}

#[test]
fn unknown_weights_and_different_image_modes_remain_distinct() {
 let unknown="0000000000000000000000000000000000000000000000000000000000000000";
 assert!(index_keys_compatible(unknown,unknown));
 assert!(!index_keys_compatible(Q8_CPU,unknown));
 assert!(!index_keys_compatible(unknown,BF16_CPU));
 assert!(!index_keys_compatible(Q8_CPU,&format!("{BF16_CPU}:vision-q8-v1")));
 assert!(!index_keys_compatible(&format!("{Q8_CPU}:vision-q8-v1"),BF16_CPU));
 assert!(!index_keys_compatible(&format!("{Q8_CPU}:other"),&format!("{BF16_CPU}:other")));
}
