# syntax=docker/dockerfile:1.7
#
# Pinchora image: cargo-chef dependency caching, distroless runtime.
#
#   docker build -t pinchora:dev .
#   docker run --rm --env-file .env pinchora:dev migrate
#   docker run --rm --env-file .env -p 9000:9000 pinchora:dev gateway
#
# RUST_VERSION must match rust-toolchain.toml. The toolchain file itself is kept
# out of the build context (.dockerignore): the base image tag is the pin here,
# and rustup would otherwise try to download a second "1.99" toolchain.

ARG RUST_VERSION=1.99
ARG CARGO_CHEF_VERSION=0.1.78
ARG NODE_VERSION=22

# --- web: the dashboard SPA (web/), embedded into the binary by pinchora-web ------
FROM node:${NODE_VERSION}-alpine AS web
# The Reef asset pages are cut from the sprite atlas and the look sheets by a stdlib-only Python script.
RUN apk add --no-cache python3
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci --no-audit --no-fund
COPY assets/sprites/atlas.png assets/sprites/atlas.json /src/assets/sprites/
COPY assets/reef/looks/ /src/assets/reef/looks/
COPY crates/pinchora-render/src/character/palette.rs /src/crates/pinchora-render/src/character/
COPY content/accessories.json /src/content/
COPY web/ ./
RUN npm run build

# --- chef: Rust toolchain + cargo-chef -------------------------------------------
FROM rust:${RUST_VERSION}-bookworm AS chef
ARG CARGO_CHEF_VERSION
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo install cargo-chef --locked --version "${CARGO_CHEF_VERSION}"
WORKDIR /app

# --- planner: reduce the workspace to a dependency recipe ------------------------
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- builder: compile dependencies (cached layer), then the workspace ------------
FROM chef AS builder
# aws-lc-sys (rustls) and tikv-jemalloc-sys compile C. The rust image already has
# gcc and make; cmake is the only build tool aws-lc-sys may additionally ask for.
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
# The dependency layer is deliberately kept in the image layers (not in a target/
# cache mount) so `cache-to type=gha` can export it. Only the downloaded crates
# use cache mounts.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo chef cook --release --locked -p pinchora --recipe-path recipe.json

# Full source. assets/fonts (include_bytes!) and migrations (sqlx::migrate!) are
# embedded at compile time, so both must be in the build context. So is the built
# dashboard: pinchora-web's build.rs bakes web/dist into the binary.
COPY . .
COPY --from=web /src/web/dist web/dist
ARG GIT_SHA=dev
ENV GIT_SHA=${GIT_SHA}
# The thin-LTO link of the final binary runs one worker per job; with all VM cores it
# outgrows an 8 GB Docker VM.
ARG CARGO_BUILD_JOBS=4
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release --locked -p pinchora \
    && install -D target/release/pinchora /out/pinchora

# --- runtime: distroless, no shell, nonroot ---------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
COPY --from=builder /out/pinchora /usr/local/bin/pinchora

# 8080 interactions endpoint, 8090 web dashboard, 9000 ops (/metrics /healthz /readyz).
EXPOSE 8080 8090 9000
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/pinchora"]
CMD ["gateway"]
