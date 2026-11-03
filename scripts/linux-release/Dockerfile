# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e
# The immutable amd64 manifest is Ubuntu 26.04, published 2026-10-24.
FROM --platform=linux/amd64 ubuntu@sha256:157dc3d6e4a3e4f2c68d12df9df9ed6e808b59af6d4c3757b432b4f519916725 AS builder
ARG APT_SNAPSHOT=20261103T000000Z
ARG BUILD_JOBS=2
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get -o APT::Update::Error-Mode=any update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && apt-get -o APT::Update::Error-Mode=any -S "$APT_SNAPSHOT" update \
 && apt-get -S "$APT_SNAPSHOT" install -y --no-install-recommends --allow-downgrades \
    ca-certificates curl git file build-essential clang cmake ninja-build pkg-config \
    nodejs npm cargo rustc python3 libwebkit2gtk-4.1-dev libgtk-3-dev \
    libayatana-appindicator3-dev librsvg2-dev libssl-dev libopenblas-dev \
    liblapack-dev libomp-dev patchelf \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /build
# Tags are checked against exact commits to prevent moving references changing a build.
RUN git clone --depth 1 --branch v1.15.1 https://github.com/facebookresearch/faiss.git faiss \
 && test "$(git -C faiss rev-parse HEAD)" = 75c755c5a1fc854756aaf3bd1131213aeb21d8d1 \
 && cmake -S faiss -B faiss-build -G Ninja -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=ON -DFAISS_ENABLE_GPU=OFF -DFAISS_ENABLE_PYTHON=OFF \
    -DFAISS_ENABLE_C_API=OFF -DBUILD_TESTING=OFF -DFAISS_OPT_LEVEL=generic \
 && cmake --build faiss-build --target faiss -j "$BUILD_JOBS"
RUN git clone --depth 1 --branch b12001 https://github.com/ggml-org/llama.cpp.git llama \
 && test "$(git -C llama rev-parse HEAD)" = eb4e244be77a906a183cd4a00fd51d9ad3200955 \
 && cmake -S llama -B llama-build -G Ninja -DCMAKE_BUILD_TYPE=Release \
    -DGGML_NATIVE=OFF -DGGML_SSE42=ON -DGGML_AVX=ON -DGGML_AVX2=ON \
    -DGGML_BMI2=ON -DGGML_FMA=ON -DGGML_F16C=ON -DLLAMA_CURL=OFF -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=ON \
 && cmake --build llama-build --target llama-server -j "$BUILD_JOBS"
# Immutable PDFium release, verified against the exact file tested on Ubuntu.
RUN curl -fsSL --retry 3 -o pdfium.tgz 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8125/pdfium-linux-x64.tgz' \
 && echo 'e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659  pdfium.tgz' | sha256sum -c - \
 && mkdir pdfium && python3 -c 'import tarfile; tarfile.open("pdfium.tgz", "r:gz").extractall("pdfium", filter="data")'
COPY . /src/
WORKDIR /src/ui
RUN npm ci --no-audit --no-fund && npm run build
WORKDIR /src
RUN mkdir -p ui/src-tauri/resources \
 && LIBRARY_PATH=/build/faiss-build/faiss CARGO_BUILD_JOBS="$BUILD_JOBS" cargo build --locked --release --manifest-path native/atlas-native-http/Cargo.toml \
 && CARGO_BUILD_JOBS="$BUILD_JOBS" cargo build --locked --release --manifest-path native/atlas-native-proxy/Cargo.toml \
 && CARGO_BUILD_JOBS="$BUILD_JOBS" cargo build --locked --release --manifest-path ui/src-tauri/Cargo.toml
ARG SOURCE_COMMIT=unknown
RUN set -eu; \
 out=/out/Atlas; res="$out/lib/Atlas/resources"; \
 mkdir -p "$out/bin" "$res/llama" "$res/ui" "$res/licenses/faiss" "$res/licenses/llama" "$res/licenses/pdfium"; \
 cp ui/src-tauri/target/release/atlas "$out/bin/atlas"; \
 cp native/atlas-native-http/target/release/atlas-native-http "$res/atlas-backend"; \
 cp native/atlas-native-proxy/target/release/atlas-native-proxy "$res/atlas-proxy"; \
 cp /build/faiss-build/faiss/libfaiss.so "$res/"; \
 cp -a /build/llama-build/bin/. "$res/llama/"; \
 for f in "$res/llama/llama-server" "$res"/llama/*.so*; do \
   if [ -f "$f" ] && [ ! -L "$f" ]; then patchelf --set-rpath '$ORIGIN' "$f"; fi; \
 done; \
 mv /build/llama-build/bin /build/llama-build/bin-hidden; \
 cp -a ui/dist/. "$res/ui/"; \
 cp /build/pdfium/lib/libpdfium.so "$res/"; \
 cc -O2 -pthread -I/build/pdfium/include pdfium/extract.c -L/build/pdfium/lib -lpdfium -Wl,-rpath,'$ORIGIN' -o "$res/atlas-pdfium-extract"; \
 cp /build/faiss/LICENSE "$res/licenses/faiss/"; \
 cp /build/llama/LICENSE "$res/licenses/llama/"; \
 cp /build/pdfium/LICENSE "$res/licenses/pdfium/"; \
 cp -a /build/pdfium/licenses/. "$res/licenses/pdfium/"; \
 ln -s Atlas "$out/lib/atlas"; \
 printf 'Atlas commit: %s\nUbuntu snapshot: %s\nFAISS: 75c755c5a1fc854756aaf3bd1131213aeb21d8d1\nllama.cpp: eb4e244be77a906a183cd4a00fd51d9ad3200955\nPDFium: 158.0.8125.0 (SHA-256 e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659)\n' "$SOURCE_COMMIT" "$APT_SNAPSHOT" > "$out/BUILD-INFO.txt"; \
 rustc --version >> "$out/BUILD-INFO.txt"; node --version >> "$out/BUILD-INFO.txt"; \
 dpkg-query -W -f='${Package} ${Version}\n' | LC_ALL=C sort > "$out/BUILD-PACKAGES.txt"; \
 for f in "$out/bin/atlas" "$res/atlas-backend" "$res/atlas-proxy" "$res/atlas-pdfium-extract" "$res/llama/llama-server"; do \
   echo "Checking $f"; file "$f" | grep -q 'x86-64'; \
   ldd "$f" > /tmp/atlas-ldd; cat /tmp/atlas-ldd; ! grep -q 'not found' /tmp/atlas-ldd; \
 done; \
 "$res/llama/llama-server" --version; \
 "$res/atlas-pdfium-extract" ui/tests/fixtures/two-pages.pdf > /tmp/pdf-text; \
 grep -q 'Atlas first page' /tmp/pdf-text; grep -q 'Atlas second page' /tmp/pdf-text
FROM scratch AS artifact
COPY --from=builder /out/ /
