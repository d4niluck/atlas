#!/usr/bin/env bash
# Build the Windows helper using a caller-supplied, verified PDFium release.
set -euo pipefail
if [[ $# != 2 ]]; then echo "Usage: $0 PDFIUM_DIR OUTPUT_DIR" >&2; exit 2; fi
pdfium=$1
output=$2
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
source_file="$script_dir/../../pdfium/extract_windows.c"
[[ -f "$pdfium/include/fpdfview.h" && -f "$pdfium/include/fpdf_text.h" && -f "$pdfium/lib/pdfium.dll.lib" && -f "$pdfium/bin/pdfium.dll" && -f "$pdfium/LICENSE" && -d "$pdfium/licenses" && -f "$source_file" ]] || { echo 'Missing PDFium headers, import library, DLL or helper source' >&2; exit 1; }
command -v x86_64-w64-mingw32-gcc >/dev/null || { echo 'MinGW x86-64 compiler not found' >&2; exit 1; }
# Exclusive creation preserves any existing output; incomplete output is retained on errors.
mkdir -- "$output"
complete=0
cleanup(){ if [[ "$complete" == 0 ]]; then printf "Incomplete output retained for inspection: %s\n" "$output" >&2; fi; }
trap cleanup EXIT
x86_64-w64-mingw32-gcc -std=c11 -O2 -Wall -Wextra -I"$pdfium/include" "$source_file" "$pdfium/lib/pdfium.dll.lib" -o "$output/atlas-pdfium-extract.exe"
cp -- "$pdfium/bin/pdfium.dll" "$output/pdfium.dll"
mkdir -p -- "$output/licenses/pdfium"
cp -- "$pdfium/LICENSE" "$output/licenses/pdfium/"
cp -R -- "$pdfium/licenses/." "$output/licenses/pdfium/"
complete=1
printf 'Windows PDFium helper: %s\n' "$output/atlas-pdfium-extract.exe"
