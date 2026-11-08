#!/bin/sh
set -eu
if [ "$#" -ne 2 ]; then echo 'Usage: build-whisper.sh WHISPER_SOURCE BUILD_DIR' >&2; exit 2; fi
source_dir=$1
build_dir=$2
revision=$(git -C "$source_dir" rev-parse HEAD)
if [ "$revision" != f92e1e4d1056700c39d4e86c1140f0a088fe1ab6 ]; then echo 'Unexpected whisper.cpp revision' >&2; exit 2; fi
cmake -S "$source_dir" -B "$build_dir" -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 -DGGML_NATIVE=OFF -DGGML_METAL=ON -DWHISPER_BUILD_TESTS=OFF
cmake --build "$build_dir" --target whisper-cli -j 4
