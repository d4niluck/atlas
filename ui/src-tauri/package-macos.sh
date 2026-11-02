#!/bin/zsh
# Build-time tools are needed only by the developer, never by the app user.
set -eu
if (( $# < 2 || $# > 3 )); then print -u2 'Usage: package-macos.sh LLAMA_RUNTIME_DIR OUTPUT_APP [PDFIUM_DIR]'; exit 2; fi
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
rm -f "$resources/atlas-pdfium-extract" "$resources/libpdfium.dylib"
rm -rf "$resources/licenses/pdfium"
if (( $# == 3 )); then
  pdfium_dir=${3:A}
  [[ -f "$pdfium_dir/lib/libpdfium.dylib" && -f "$pdfium_dir/include/fpdfview.h" && -f "$pdfium_dir/LICENSE" && -d "$pdfium_dir/licenses" ]] || { print -u2 'Incomplete PDFium directory'; exit 2; }
  clang -O2 -pthread -I"$pdfium_dir/include" "$project/pdfium/extract.c" "$pdfium_dir/lib/libpdfium.dylib" -liconv -o "$resources/atlas-pdfium-extract"
  install_name_tool -change ./libpdfium.dylib @loader_path/libpdfium.dylib "$resources/atlas-pdfium-extract"
  cp "$pdfium_dir/lib/libpdfium.dylib" "$resources/libpdfium.dylib"
  mkdir -p "$resources/licenses/pdfium"
  cp "$pdfium_dir/LICENSE" "$resources/licenses/pdfium/"
  cp -R "$pdfium_dir/licenses/." "$resources/licenses/pdfium/"
fi
mkdir -p "$resources/llama"
cp -R "$llama_dir/." "$resources/llama/"
print 'Building app…'
(cd "$here" && cargo build --release --offline)
mkdir -p "$output/Contents/MacOS" "$output/Contents/Resources/resources"
cp "$here/target/release/atlas" "$output/Contents/MacOS/atlas"
cp -R "$resources/." "$output/Contents/Resources/resources/"
version=$(node -e 'process.stdout.write(JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")).version)' "$here/tauri.conf.json")
[[ "$version" =~ "^[0-9]+(\.[0-9]+)*$" ]] || { print -u2 'Invalid app version'; exit 2; }
cat > "$output/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>atlas</string><key>CFBundleIdentifier</key><string>local.atlas.desktop</string><key>CFBundleName</key><string>Atlas</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>$version</string><key>CFBundleVersion</key><string>$version</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PLIST
plutil -lint "$output/Contents/Info.plist"
codesign --force --deep --sign - "$output"
codesign --verify --deep --strict "$output"
print "App created: $output"
