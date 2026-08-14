# syntax=docker/dockerfile:1.7

FROM rust:1.94-slim-bookworm AS builder

ENV SQLX_OFFLINE=true

WORKDIR /build
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential cmake \
    && rm -rf /var/lib/apt/lists/*
COPY . .

# BuildKit 缓存 Cargo 注册表和编译产物；将最终二进制复制到缓存目录之外。
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release \
    && install -s -Dm755 target/release/tune-box /out/tune-box

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o 1000 -g 1000 /app/config /app/data /app/downloads

WORKDIR /app
COPY --from=builder --chown=1000:1000 /out/tune-box ./tune-box

EXPOSE 8080
USER 1000:1000
ENTRYPOINT ["/app/tune-box"]
