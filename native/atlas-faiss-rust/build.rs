use std::{env, path::PathBuf, process::Command};
fn main() {
 if env::var("CARGO_CFG_TARGET_OS").as_deref()==Ok("windows") {
  assert_eq!(env::var("CARGO_CFG_TARGET_ENV").as_deref(),Ok("gnu"),"Windows build requires the GNU toolchain");
  assert_eq!(env::var("CARGO_CFG_TARGET_ARCH").as_deref(),Ok("x86_64"),"Windows FAISS build currently supports x86_64 only");
  let root=PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
  let out=PathBuf::from(env::var_os("OUT_DIR").unwrap());
  let prefix=PathBuf::from(env::var_os("ATLAS_FAISS_PREFIX").expect("set ATLAS_FAISS_PREFIX to the Windows FAISS build prefix"));
  let object=out.join("shim.o");let archive=out.join("libatlas_faiss_shim.a");
  let cxx=env::var_os("CXX").unwrap_or_else(||"x86_64-w64-mingw32-g++".into());
  let ar=env::var_os("AR").unwrap_or_else(||"x86_64-w64-mingw32-ar".into());
  let status=Command::new(cxx).args(["-std=c++20","-DFAISS_MAIN_LIB","-fopenmp","-c"]).arg(root.join("shim.cpp")).arg("-I").arg(prefix.join("include")).arg("-o").arg(&object).status().unwrap();assert!(status.success());
  let status=Command::new(ar).arg("crs").arg(&archive).arg(&object).status().unwrap();assert!(status.success());
  println!("cargo:rustc-link-search=native={}",out.display());
  println!("cargo:rustc-link-search=native={}",prefix.join("lib").display());
  println!("cargo:rustc-link-lib=static=atlas_faiss_shim");
  println!("cargo:rustc-link-lib=static=faiss");
  println!("cargo:rustc-link-lib=dylib=openblas");
  println!("cargo:rustc-link-lib=dylib=gomp");
  println!("cargo:rustc-link-lib=dylib=stdc++");
  println!("cargo:rerun-if-changed={}",prefix.join("include").display());
  println!("cargo:rerun-if-changed={}",prefix.join("lib").display());
  println!("cargo:rerun-if-env-changed=ATLAS_FAISS_PREFIX");
  println!("cargo:rerun-if-env-changed=CXX");println!("cargo:rerun-if-env-changed=AR");
  println!("cargo:rerun-if-changed=shim.cpp");
  return;
 }
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
