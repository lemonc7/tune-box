# syntax=docker/dockerfile:1.7

FROM rust:1.95-slim-bookworm AS builder

ARG DEBIAN_MIRROR=https://mirrors.ustc.edu.cn
ENV SQLX_OFFLINE=true

WORKDIR /build
RUN sed -i \
        -e "s|http://deb.debian.org|${DEBIAN_MIRROR}|g" \
        -e "s|https://deb.debian.org|${DEBIAN_MIRROR}|g" \
        -e "s|http://security.debian.org|${DEBIAN_MIRROR}|g" \
        -e "s|https://security.debian.org|${DEBIAN_MIRROR}|g" \
        /etc/apt/sources.list.d/debian.sources \
    && apt-get update \
    && apt-get install -y --no-install-recommends build-essential cmake \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown \
    && cargo install dioxus-cli --version 0.7.10 --locked

COPY . .

# Dioxus Fullstack 的发布产物包含 server 可执行文件和 public 前端资源。
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    dx bundle --package app --web --release \
    && cp -a target/dx/app/release/web/. /out/ \
    && install -Dm644 config.example.toml /out/config/config.toml \
    && install -d /out/data /out/downloads

FROM m.daocloud.io/gcr.io/distroless/cc-debian12:latest

WORKDIR /app
COPY --from=builder --chown=1000:1000 /out/ /app/

EXPOSE 8080
USER 1000:1000
ENTRYPOINT ["/app/server"]
