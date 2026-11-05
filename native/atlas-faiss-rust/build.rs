use std::{env, path::PathBuf, process::Command};
fn main() {
 let root=PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
 let out=PathBuf::from(env::var_os("OUT_DIR").unwrap());
 let object=out.join("shim.o"); let archive=out.join("libatlas_faiss_shim.a");
 let status=Command::new("clang++").arg("-std=c++17").arg("-c").arg(root.join("shim.cpp")).arg("-I").arg(root.join("vendor/include")).arg("-o").arg(&object).status().unwrap(); assert!(status.success());
 let status=Command::new("ar").arg("crs").arg(&archive).arg(&object).status().unwrap(); assert!(status.success());
 println!("cargo:rustc-link-search=native={}",out.display());
 println!("cargo:rustc-link-search=native={}",root.join("vendor/lib").display());
 println!("cargo:rustc-link-lib=static=atlas_faiss_shim"); println!("cargo:rustc-link-lib=dylib=faiss");
 if env::var("CARGO_CFG_TARGET_OS").as_deref()==Ok("macos") {
  println!("cargo:rustc-link-lib=c++");
  println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
 } else { println!("cargo:rustc-link-lib=stdc++"); }
 println!("cargo:rerun-if-changed=shim.cpp");
}
