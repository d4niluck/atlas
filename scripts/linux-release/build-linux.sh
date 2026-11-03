#!/bin/bash
# Build the committed Atlas snapshot for Ubuntu 26.04 x86-64 on macOS Docker.
set -euo pipefail
if [ "$#" -ne 2 ]; then echo "Usage: $0 PROJECT_ROOT OUTPUT.tar.gz" >&2; exit 2; fi
project=$(cd "$1" && pwd -P)
parent=$(cd "$(dirname "$2")" && pwd -P)
output="$parent/$(basename "$2")"
[ "$(git -C "$project" rev-parse --show-toplevel)" = "$project" ] || { echo 'Invalid repository root' >&2; exit 2; }
[ ! -e "$output" ] && [ ! -L "$output" ] || { echo 'Output already exists' >&2; exit 2; }
commit=$(git -C "$project" rev-parse --verify 'HEAD^{commit}')
work=$(mktemp -d "$parent/.atlas-linux-build.XXXXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir "$work/context" "$work/result"
printf 'Preparing committed snapshot %s\n' "$commit"
git -C "$project" archive "$commit" | tar -xf - -C "$work/context"
dockerfile="$work/context/scripts/linux-release/Dockerfile"
[ -f "$dockerfile" ] || { echo 'Dockerfile is absent from committed snapshot' >&2; exit 2; }
printf 'Building for linux/amd64; this may take significant time under emulation.\n'
docker buildx build --platform linux/amd64 --progress plain \
  --build-arg "SOURCE_COMMIT=$commit" --build-arg BUILD_JOBS=2 \
  --file "$dockerfile" --output "type=local,dest=$work/result" "$work/context"
printf 'Packaging...\n'
tar -C "$work/result" -czf "$work/atlas.tar.gz" Atlas
# Hard-link within one directory gives exclusive publication, without replacing another file.
ln "$work/atlas.tar.gz" "$output"
printf 'Created %s\n' "$output"
