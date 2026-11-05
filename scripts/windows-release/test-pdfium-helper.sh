#!/usr/bin/env bash
# Build a smoke-test package; run test-pdfium-helper.cmd on Windows.
set -euo pipefail
if [[ $# != 2 ]]; then echo "Usage: $0 PDFIUM_DIR OUTPUT_DIR" >&2; exit 2; fi
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
root=$(cd -- "$script_dir/../.." && pwd)
fixture="$root/ui/tests/fixtures/two-pages.pdf"
[[ -f "$fixture" ]] || { echo 'Missing two-page PDF fixture' >&2; exit 1; }
"$script_dir/build-pdfium-helper.sh" "$1" "$2"
cp -- "$fixture" "$2/two-pages.pdf"
cp -- "$script_dir/test-pdfium-helper.cmd" "$2/"
printf 'Run on Windows: %s/test-pdfium-helper.cmd\n' "$2"
