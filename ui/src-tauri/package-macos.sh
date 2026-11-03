#!/bin/zsh
# Builds a local arm64 app from the committed Git snapshot without modifying it.
# The caller supplies existing runtime, PDFium, node_modules and offline Cargo cache.
# Their exact versions and provenance are not verified: this is not a clean CI build.
set -eu
setopt pipefail
if (( $# < 5 || $# > 7 )); then
  print -u2 'Usage: package-macos.sh PROJECT_ROOT LLAMA_DIR PDFIUM_DIR NODE_MODULES_DIR OUTPUT_APP [NATIVE_TARGET_DIR [APP_TARGET_DIR]]'
  exit 2
fi
project=${1:A}; llama_dir=${2:A}; pdfium_dir=${3:A}; node_modules_dir=${4:A}; output=${5:A}
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || { print -u2 'An Apple Silicon Mac is required'; exit 2; }
[[ -x "$llama_dir/llama-server" && -f "$llama_dir/LICENSE" && -f "$pdfium_dir/lib/libpdfium.dylib" && -f "$pdfium_dir/include/fpdfview.h" && -f "$pdfium_dir/LICENSE" && -d "$pdfium_dir/licenses" && -d "$node_modules_dir" ]] || { print -u2 'Missing project, runtime, PDFium, licenses or node_modules'; exit 2; }
[[ $(git -C "$project" rev-parse --show-toplevel 2>/dev/null) == "$project" ]] || { print -u2 'Project root must be a git working tree'; exit 2; }
[[ ! -e "$output" && ! -L "$output" && -d "${output:h}" ]] || { print -u2 'Output already exists or its parent directory is missing'; exit 2; }
for binary in "$llama_dir/llama-server" "$pdfium_dir/lib/libpdfium.dylib"; do
  file "$binary" | grep -q 'arm64' || { print -u2 "Not an arm64 binary: $binary"; exit 2; }
done
work=$(mktemp -d "${TMPDIR:-/tmp}/atlas-macos-build.XXXXXXXX")
output_id=''; output_complete=0
cleanup() {
  local result_code=$1 current_id=''
  if (( ! output_complete )) && [[ -n "$output_id" && -d "$output" && ! -L "$output" ]]; then
    current_id=$(stat -f '%u:%d:%i' "$output" 2>/dev/null || true)
    if [[ "$current_id" == "$output_id" && "${current_id%%:*}" == "$(id -u)" ]]; then rm -rf "$output"; fi
  fi
  rm -rf "$work"
  return $result_code
}
trap 'cleanup $?' EXIT
mkdir "$work/source"
# Freeze the committed snapshot before building; uncommitted files are excluded.
commit=$(git -C "$project" rev-parse --verify HEAD^{commit})
print "Building committed snapshot: $commit"
git -C "$project" archive "$commit" | (cd "$work/source" && tar -xf -)
src="$work/source"
file "$src/native/atlas-faiss-rust/vendor/lib/libfaiss.dylib" | grep -q 'arm64' || { print -u2 'Snapshot FAISS is not arm64'; exit 2; }
ln -s "$node_modules_dir" "$src/ui/node_modules"
native_target="$work/native-target"; app_target="$work/app-target"
if (( $# >= 6 )); then native_target=${6:A}; [[ -d "$native_target" ]] || { print -u2 'Native target directory does not exist'; exit 2; }; fi
if (( $# == 7 )); then app_target=${7:A}; [[ -d "$app_target" ]] || { print -u2 'App target directory does not exist'; exit 2; }; fi
print 'Building interface…'
(cd "$src/ui" && npm run build -- --configLoader runner)
print 'Building backend and proxy…'
cargo build --locked --release --offline --target-dir "$native_target" --manifest-path "$src/native/atlas-native-http/Cargo.toml"
cargo build --locked --release --offline --target-dir "$native_target" --manifest-path "$src/native/atlas-native-proxy/Cargo.toml"
mkdir -p "$src/ui/src-tauri/resources"
print 'Building app executable…'
cargo build --locked --release --offline --target-dir "$app_target" --manifest-path "$src/ui/src-tauri/Cargo.toml"
app="$work/Atlas.app"; res="$app/Contents/Resources/resources"
mkdir -p "$app/Contents/MacOS" "$res/.dylibs" "$res/licenses/faiss" "$res/licenses/pdfium" "$res/llama" "$res/ui"
cp "$app_target/release/atlas" "$app/Contents/MacOS/atlas"
cp "$native_target/release/atlas-native-http" "$res/atlas-backend"
cp "$native_target/release/atlas-native-proxy" "$res/atlas-proxy"
cp "$src/native/atlas-faiss-rust/vendor/lib/libfaiss.dylib" "$res/libfaiss.dylib"
cp "$src/native/atlas-faiss-rust/vendor/lib/.dylibs/libomp.dylib" "$res/.dylibs/libomp.dylib"
cp "$src/native/atlas-faiss-rust/vendor/LICENSE" "$src/native/atlas-faiss-rust/vendor/THIRD_PARTY_NOTICES" "$res/licenses/faiss/"
cp -R "$llama_dir/." "$res/llama/"
cp -R "$src/ui/dist/." "$res/ui/"
cp "$pdfium_dir/lib/libpdfium.dylib" "$res/libpdfium.dylib"
cp "$pdfium_dir/LICENSE" "$res/licenses/pdfium/"
cp -R "$pdfium_dir/licenses/." "$res/licenses/pdfium/"
clang -O2 -pthread -I"$pdfium_dir/include" "$src/pdfium/extract.c" "$pdfium_dir/lib/libpdfium.dylib" -liconv -o "$res/atlas-pdfium-extract"
install_name_tool -change ./libpdfium.dylib @loader_path/libpdfium.dylib "$res/atlas-pdfium-extract"
cp "$src/ui/src-tauri/icons/icon.icns" "$app/Contents/Resources/icon.icns"
version=$(node -e 'process.stdout.write(JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")).version)' "$src/ui/src-tauri/tauri.conf.json")
[[ "$version" =~ '^[0-9]+(\.[0-9]+)*$' ]] || { print -u2 'Invalid app version'; exit 2; }
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>atlas</string><key>CFBundleIdentifier</key><string>local.atlas.desktop</string><key>CFBundleName</key><string>Atlas</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleIconFile</key><string>icon.icns</string><key>CFBundleShortVersionString</key><string>$version</string><key>CFBundleVersion</key><string>$version</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
# Exclusive mkdir prevents replacing an existing app (including a race).
mkdir "$output"
output_id=$(stat -f '%u:%d:%i' "$output")
ditto "$app" "$output"
codesign --verify --deep --strict "$output"
output_complete=1
print "App created (ad hoc signature): $output"
