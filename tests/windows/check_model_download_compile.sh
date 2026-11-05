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
(cd "$tmp" && cargo check --offline --target x86_64-pc-windows-gnu)
