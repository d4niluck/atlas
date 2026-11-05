#!/bin/bash
# Build the same FAISS revision as Linux and a portable OpenMP runtime.
set -euo pipefail
if [ "$#" -ne 3 ]; then
  echo 'Usage: build-faiss.sh FAISS_SOURCE LLVM_SOURCE OUTPUT_DIR' >&2
  exit 2
fi
faiss_source=$(cd "$1" && pwd -P)
llvm_source=$(cd "$2" && pwd -P)
output=$3
faiss_commit=75c755c5a1fc854756aaf3bd1131213aeb21d8d1
llvm_commit=2078da43e25a4623cab2d0d60decddf709aaea28
[ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ] || { echo 'Apple Silicon macOS required' >&2; exit 2; }
[ "$(git -C "$faiss_source" rev-parse HEAD)" = "$faiss_commit" ] || { echo 'Unexpected FAISS revision' >&2; exit 2; }
[ "$(git -C "$llvm_source" rev-parse HEAD)" = "$llvm_commit" ] || { echo 'Unexpected LLVM revision' >&2; exit 2; }
[ ! -e "$output" ] || { echo 'Output already exists' >&2; exit 2; }
work=$(mktemp -d "${TMPDIR:-/tmp}/atlas-faiss-build.XXXXXXXX")
output_id=''
complete=0
cleanup() {
  if [ "$complete" = 0 ] && [ -n "$output_id" ] && [ -d "$output" ] && [ ! -L "$output" ] && [ "$(stat -f '%u:%d:%i' "$output" 2>/dev/null || true)" = "$output_id" ]; then
    rm -rf "$output"
  fi
  rm -rf "$work"
}
trap cleanup EXIT
mkdir "$work/faiss" "$work/llvm"
# Copy tracked, unchanged source files; sparse checkouts need no additional network.
[ -z "$(git -C "$faiss_source" status --porcelain --untracked-files=no)" ] || { echo 'FAISS checkout has tracked changes' >&2; exit 2; }
[ -z "$(git -C "$llvm_source" status --porcelain --untracked-files=no -- openmp cmake llvm/cmake)" ] || { echo 'LLVM checkout has tracked changes' >&2; exit 2; }
git -C "$faiss_source" ls-files -z | tar -C "$faiss_source" --null -T - -cf - | tar -x -C "$work/faiss"
git -C "$llvm_source" ls-files -z -- openmp cmake llvm/cmake | tar -C "$llvm_source" --null -T - -cf - | tar -x -C "$work/llvm"
GIT_OPTIONAL_LOCKS=0 git -C "$faiss_source" --work-tree="$work/faiss" diff --quiet "$faiss_commit" -- || { echo 'FAISS snapshot changed during copy' >&2; exit 2; }
GIT_OPTIONAL_LOCKS=0 git -C "$llvm_source" --work-tree="$work/llvm" diff --quiet "$llvm_commit" -- openmp cmake llvm/cmake || { echo 'LLVM snapshot changed during copy' >&2; exit 2; }
echo 'Building OpenMP for macOS 14…'
cmake -S "$work/llvm/openmp" -B "$work/openmp-build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 -DLIBOMP_ENABLE_SHARED=ON -DLIBOMP_OMPT_SUPPORT=OFF -DLIBOMP_OMPD_SUPPORT=OFF -DLIBOMP_INSTALL_ALIASES=OFF
cmake --build "$work/openmp-build" --target omp -j 2
omp="$work/openmp-build/runtime/src"
echo 'Building FAISS for macOS 14…'
cmake -S "$work/faiss" -B "$work/faiss-build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 -DBUILD_SHARED_LIBS=ON -DFAISS_ENABLE_GPU=OFF -DFAISS_ENABLE_PYTHON=OFF -DFAISS_ENABLE_C_API=OFF -DBUILD_TESTING=OFF -DFAISS_OPT_LEVEL=generic -DBLA_VENDOR=Apple -DOpenMP_CXX_FLAGS="-Xpreprocessor -fopenmp -I$omp" -DOpenMP_CXX_LIB_NAMES=omp -DOpenMP_omp_LIBRARY="$omp/libomp.dylib"
cmake --build "$work/faiss-build" --target faiss -j 2
install_name_tool -change @rpath/libomp.dylib @loader_path/.dylibs/libomp.dylib "$work/faiss-build/faiss/libfaiss.dylib"
# Publish output only once the complete build has succeeded.
mkdir "$output"
output_id=$(stat -f '%u:%d:%i' "$output")
mkdir "$output/.dylibs"
cp "$work/faiss-build/faiss/libfaiss.dylib" "$output/libfaiss.dylib"
cp "$omp/libomp.dylib" "$output/.dylibs/libomp.dylib"
cp "$work/llvm/openmp/LICENSE.TXT" "$output/OPENMP-LICENSE.txt"
complete=1
echo "FAISS libraries built in $output"
