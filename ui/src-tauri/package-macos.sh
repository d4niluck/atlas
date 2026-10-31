#!/bin/zsh
# Build-time tools are needed only by the developer, never by the app user.
set -eu
if (( $# != 2 )); then print -u2 'Usage: package-macos.sh LLAMA_RUNTIME_DIR OUTPUT_APP'; exit 2; fi
here=${0:A:h}
project=${here:h:h}
llama_dir=${1:A}
output=${2:A}
[[ ! -e "$output" ]] || { print -u2 'Output already exists'; exit 2; }
[[ -x "$llama_dir/llama-server" ]] || { print -u2 'llama-server missing'; exit 2; }
print 'Building interface…'
(cd "$project/ui" && npm run build -- --configLoader runner)
print 'Building native backend…'
(cd "$project/native/atlas-native-http" && cargo build --release --offline)
print 'Building local proxy…'
(cd "$project/native/atlas-native-proxy" && cargo build --release --offline)
resources="$here/resources"
rm -rf "$resources/ui" "$resources/llama"
mkdir -p "$resources/ui" "$resources/.dylibs"
cp -R "$project/ui/dist/." "$resources/ui/"
cp "$project/native/atlas-native-http/target/release/atlas-native-http" "$resources/atlas-backend"
cp "$project/native/atlas-faiss-rust/vendor/lib/libfaiss.dylib" "$resources/libfaiss.dylib"
mkdir -p "$resources/licenses/faiss"
cp "$project/native/atlas-faiss-rust/vendor/LICENSE" "$project/native/atlas-faiss-rust/vendor/THIRD_PARTY_NOTICES" "$resources/licenses/faiss/"
cp "$project/native/atlas-faiss-rust/vendor/lib/.dylibs/libomp.dylib" "$resources/.dylibs/libomp.dylib"
cp "$project/native/atlas-native-proxy/target/release/atlas-native-proxy" "$resources/atlas-proxy"
mkdir -p "$resources/llama"
cp -R "$llama_dir/." "$resources/llama/"
print 'Building app…'
(cd "$here" && cargo build --release --offline)
mkdir -p "$output/Contents/MacOS" "$output/Contents/Resources/resources"
cp "$here/target/release/atlas" "$output/Contents/MacOS/atlas"
cp -R "$resources/." "$output/Contents/Resources/resources/"
cat > "$output/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>atlas</string><key>CFBundleIdentifier</key><string>local.atlas.desktop</string><key>CFBundleName</key><string>Atlas</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>1</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PLIST
plutil -lint "$output/Contents/Info.plist"
print "App created: $output"
