# Compile-check only: no Atlas backend, UI bundle, release artifact, or GUI test.
FROM rust:1.90-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev build-essential pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /atlas
COPY ui/src-tauri/ ui/src-tauri/
# Tauri checks frontendDist exists at compile time; this is not an application UI.
RUN mkdir -p ui/dist && printf '<!doctype html><title>Build check only</title>' > ui/dist/index.html
RUN cargo check --locked --manifest-path ui/src-tauri/Cargo.toml
