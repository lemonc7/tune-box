FROM rust:1.95-slim-trixie AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        curl \
        musl-tools \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown x86_64-unknown-linux-musl

ENV CC_x86_64_unknown_linux_musl=musl-gcc
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_LINKER=musl-gcc

RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
    | bash \
    && cargo binstall dioxus-cli@0.7.10 -y --force

ENV SQLX_OFFLINE=true

COPY . .
RUN dx bundle --package tune-box --release \
    @client --platform web \
    @server --platform server --target x86_64-unknown-linux-musl

    
FROM alpine:3.23 AS runtime

WORKDIR /app

ENV RUST_LOG=info,dioxus_server=warn,hyper_util=warn

# ChronoLocal 需要 IANA 时区数据才能根据运行时 TZ（如 Asia/Shanghai）格式化日志。
RUN apk add --no-cache tzdata \
    && addgroup -g 1000 tune-box \
    && adduser -D -H -u 1000 -G tune-box tune-box

COPY --from=builder --chown=tune-box:tune-box /app/packages/app/dist/ /app/

USER tune-box
EXPOSE 8080
ENTRYPOINT ["/app/server"]
