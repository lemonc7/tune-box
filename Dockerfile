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
COPY . .

# BuildKit 缓存 Cargo 注册表和编译产物；将最终二进制复制到缓存目录之外。
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release \
    && install -s -Dm755 target/release/tune-box /out/tune-box

FROM m.daocloud.io/gcr.io/distroless/cc-debian12:latest

WORKDIR /app
COPY --from=builder --chown=1000:1000 /out/tune-box /app/tune-box

EXPOSE 8080
USER 1000:1000
ENTRYPOINT ["/app/tune-box"]
