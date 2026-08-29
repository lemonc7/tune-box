FROM rust:1.95-slim-trixie AS chef

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        curl \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef


FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json


FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .

RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall dioxus-cli --root /.cargo -y --force
ENV PATH="/.cargo/bin:$PATH"
ENV SQLX_OFFLINE=true
RUN dx bundle --package tune-box --web --release

FROM gcr.io/distroless/cc-debian13:latest AS runtime

WORKDIR /app
ENV RUST_LOG=info,dioxus_server=warn,hyper_util=warn
# 下载和打标签是突发式的大块分配。glibc 默认会动态抬高 mmap 阈值（最高 32MB），
# 超过之后这些内存 free 只回到堆的空闲链表、不还给内核，容器里看到的 RSS 就只升不降。
#   MALLOC_ARENA_MAX=2           减少 per-thread arena 各自囤积的内存
#   MALLOC_TRIM_THRESHOLD_=128K  显式设置会同时关闭阈值的动态调整，
#                                让堆顶空闲内存能被回收，大块分配继续走 mmap
ENV MALLOC_ARENA_MAX=2
ENV MALLOC_TRIM_THRESHOLD_=131072

COPY --from=builder --chown=1000:1000 /app/packages/app/dist/ /app/

USER 1000:1000
EXPOSE 8080
ENTRYPOINT ["/app/server"]
