# SZ-ORM 部署镜像
#
# 用途：将 SZ-ORM 项目及其示例打包为可分发的容器镜像，用于：
#   - 在测试/生产环境一键部署 SZ-ORM 应用
#   - 在 CI 流水线中作为运行环境
#   - 作为依赖镜像被下游项目继承
#
# 构建：docker build -t sz-orm:0.2.0 .
# 运行示例：docker run --rm sz-orm:0.2.0 production_dtx
# 运行测试：docker run --rm sz-orm:0.2.0 cargo test --workspace
#
# 多阶段构建以最小化最终镜像体积

# ---------- 阶段 1：构建 ----------
FROM rust:1.82-slim AS builder

WORKDIR /build

# 安装构建所需的系统依赖（MySQL/PG 客户端库等）
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        && rm -rf /var/lib/apt/lists/*

# 先复制 Cargo 配置以利用 Docker 层缓存
COPY Cargo.toml Cargo.lock ./
COPY packages/ ./packages/
COPY cli/ ./cli/
COPY examples/ ./examples/

# 构建示例二进制（release 优化）
RUN cargo build --release -p sz-orm-examples --bins

# ---------- 阶段 2：运行时 ----------
FROM debian:bookworm-slim AS runtime

# 安装最小运行时依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        && rm -rf /var/lib/apt/lists/* \
        && useradd -m -u 1000 szorm

WORKDIR /app

# 从构建阶段复制编译产物
COPY --from=builder /build/target/release/quick_start        /app/bin/quick_start
COPY --from=builder /build/target/release/model_definition  /app/bin/model_definition
COPY --from=builder /build/target/release/transaction       /app/bin/transaction
COPY --from=builder /build/target/release/migration         /app/bin/migration
COPY --from=builder /build/target/release/hooks_soft_delete /app/bin/hooks_soft_delete
COPY --from=builder /build/target/release/multi_tenant     /app/bin/multi_tenant
COPY --from=builder /build/target/release/production_app    /app/bin/production_app
COPY --from=builder /build/target/release/production_dtx    /app/bin/production_dtx

# 切换到非 root 用户
USER szorm

# 默认入口：列出所有可用示例
ENTRYPOINT ["/app/bin/production_dtx"]
CMD []
