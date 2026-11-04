#!/bin/sh
set -eu
root="${1:?path to Atlas root required}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/src"
for name in model_download q8_model_download vision_model_download rag_model_download rag_e4b_model_download nemotron_model_download windows_model_download; do
    cp "$root/ui/src-tauri/src/$name.rs" "$tmp/src/"
done
cat > "$tmp/Cargo.toml" <<'TOML'
[package]
name = "atlas-windows-downloader-check"
version = "0.1.0"
edition = "2021"
[dependencies]
libc = "=0.2.191"
sha2 = "=0.10.9"
TOML
cat > "$tmp/src/main.rs" <<'RS'
mod windows_model_download;
mod model_download;
mod q8_model_download;
mod vision_model_download;
mod rag_model_download;
mod rag_e4b_model_download;
mod nemotron_model_download;
fn main() {}
RS
cat >> "$tmp/src/windows_model_download.rs" <<'RS'
#[cfg(test)]
mod windows_tests {
    use super::*;
    #[test]
    fn digest_and_cancellation() {
        let path = std::env::temp_dir().join(format!("atlas-hash-windows-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let cancelled = AtomicBool::new(false);
        assert!(verify_sha256(&path, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", &cancelled).unwrap());
        assert!(!verify_sha256(&path, "wrong", &cancelled).unwrap());
        cancelled.store(true, Ordering::SeqCst);
        assert!(verify_sha256(&path, "wrong", &cancelled).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
RS
(cd "$tmp" && cargo test --offline --target x86_64-pc-windows-gnu --no-run)
cp "$tmp"/target/x86_64-pc-windows-gnu/debug/deps/atlas_windows_downloader_check-*.exe "${2:?output directory required}/atlas-windows-downloader-tests.exe"
