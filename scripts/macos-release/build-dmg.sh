#!/bin/zsh
# Package a previously built Atlas.app without changing its signature.
set -eu
setopt pipefail
if (( $# != 2 )); then
  print -u2 'Usage: build-dmg.sh ATLAS_APP OUTPUT_DMG'
  exit 2
fi
app=${1:A}; output=${2:A}; script_dir=${0:A:h}
[[ $(uname -s) == Darwin ]] || { print -u2 'macOS is required'; exit 2; }
[[ -d "$app" && ! -L "$app" && -f "$app/Contents/Info.plist" ]] || { print -u2 'Atlas.app not found'; exit 2; }
[[ -f "$script_dir/Как установить Atlas.rtf" ]] || { print -u2 'Installation instructions not found'; exit 2; }
[[ -d "${output:h}" && ! -e "$output" && ! -L "$output" ]] || { print -u2 'Output already exists or its parent does not exist'; exit 2; }
[[ $(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist") == local.atlas.desktop ]] || { print -u2 'Not an Atlas application'; exit 2; }
codesign --verify --deep --strict "$app"
# The enclosed instructions promise Apple Silicon and macOS 14 or later.
check_min_version() {
  local version=$1
  [[ "$version" =~ '^[0-9]+(\.[0-9]+){1,2}$' ]] || { print -u2 "Unknown minimum macOS version: $version"; return 1; }
  local -a parts
  parts=("${(@s:.:)version}")
  (( parts[1] < 14 || (parts[1] == 14 && parts[2] == 0 && ${parts[3]:-0} == 0) )) || { print -u2 "Requires macOS $version, instructions promise macOS 14"; return 1; }
}
check_min_version "$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$app/Contents/Info.plist")"
[[ -f "$app/Contents/MacOS/atlas" ]] || { print -u2 'Atlas executable not found'; exit 2; }
while IFS= read -r -d '' binary; do
  if /usr/bin/file -b "$binary" | /usr/bin/grep -q 'Mach-O'; then
    /usr/bin/lipo "$binary" -verify_arch arm64 || { print -u2 "Missing arm64: $binary"; exit 2; }
    minimum=$(/usr/bin/otool -arch arm64 -l "$binary" | /usr/bin/awk '/cmd LC_BUILD_VERSION/{b=1;v=0;next}/cmd LC_VERSION_MIN_MACOSX/{v=1;b=0;next} b && $1=="minos"{print $2;b=0} v && $1=="version"{print $2;v=0}')
    check_min_version "$minimum" || { print -u2 "Incompatible binary: $binary"; exit 2; }
  fi
done < <(/usr/bin/find "$app/Contents" -type f -print0)
work=$(mktemp -d "${output:h}/.atlas-dmg.XXXXXXXX")
cleanup() { rm -rf "$work"; }
trap cleanup EXIT
mkdir "$work/stage"
ditto "$app" "$work/stage/Atlas.app"
ln -s /Applications "$work/stage/Applications"
cp "$script_dir/Как установить Atlas.rtf" "$work/stage/Как установить Atlas.rtf"
hdiutil create -volname 'Atlas — установка' -fs HFS+ -srcfolder "$work/stage" -format UDZO "$work/Atlas.dmg"
# Hard-link publication on the same volume fails if another file appeared at output.
/bin/link "$work/Atlas.dmg" "$output"
print "DMG created: $output"
