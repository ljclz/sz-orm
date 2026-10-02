#!/bin/bash
# deploy-122.sh — 在本机（Windows/Git Bash）执行：把三个仓库 + soak-kit 推送到
# 第二服务器（122.51.216.76），全程 SSH，不经过 GitHub。
# 用法: bash deploy-122.sh [all|sz-orm|szrsql|sz-rust|kit]
set -euo pipefail

KEY="$HOME/.ssh/szorm_server_122"
HOST="root@122.51.216.76"
SSH_OPTS="-i $KEY -o BatchMode=yes -o StrictHostKeyChecking=no"
SZORM="/e/vue/test/鲜视达/rust/sz-orm"
SZRSQL="/e/vue/test/鲜视达/rust/szrsql"
SZRUST="/e/vue/test/sz-pay/server/sz-rust"
KIT="$SZORM/scripts/soak-kit"
TARGET="/www/rust"

WHAT="${1:-all}"

ssh_cmd() { ssh $SSH_OPTS $HOST "$@"; }

sync_szorm() {
    echo "=== 同步 sz-orm（tar，排除 target/.git）==="
    tar -czf /tmp/szorm-deploy.tar.gz -C "$(dirname "$SZORM")" \
        --exclude='target' --exclude='.git' --exclude='*.log' \
        --exclude='soak-reports' sz-orm
    scp $SSH_OPTS /tmp/szorm-deploy.tar.gz $HOST:/tmp/
    ssh_cmd "rm -rf $TARGET/sz-orm.new && mkdir -p $TARGET/sz-orm.new && \
        tar -xzf /tmp/szorm-deploy.tar.gz -C $TARGET/sz-orm.new --strip-components=1 && \
        rm -rf $TARGET/sz-orm.old && mv $TARGET/sz-orm $TARGET/sz-orm.old 2>/dev/null; \
        mv $TARGET/sz-orm.new $TARGET/sz-orm && rm /tmp/szorm-deploy.tar.gz"
    echo "sz-orm 已同步 $(git -C "$SZORM" rev-parse --short HEAD)"
}

sync_szrsql() {
    echo "=== 同步 szrsql（git bundle，服务器端 fetch 保持 git 历史）==="
    git -C "$SZRSQL" bundle create /tmp/szrsql.bundle --all
    scp $SSH_OPTS /tmp/szrsql.bundle $HOST:/tmp/
    ssh_cmd "cd $TARGET/szrsql_src && git fetch /tmp/szrsql.bundle 'refs/heads/*:refs/remotes/local/*' 2>/dev/null || \
        (git init -q $TARGET/szrsql_src && cd $TARGET/szrsql_src && git fetch /tmp/szrsql.bundle 'refs/heads/*:refs/remotes/local/*'); \
        cd $TARGET/szrsql_src && git checkout -f -B main local/main 2>/dev/null || git checkout -f -B master local/master; \
        rm /tmp/szrsql.bundle"
    echo "szrsql 已同步 $(git -C "$SZRSQL" rev-parse --short HEAD)"
}

sync_szrust() {
    echo "=== 同步 sz-rust（tar）==="
    tar -czf /tmp/szrust-deploy.tar.gz -C "$(dirname "$SZRUST")" \
        --exclude='target' --exclude='.git' --exclude='*.log' \
        --exclude='audit_out.txt' --exclude='clippy_*' --exclude='build_*.txt' sz-rust
    scp $SSH_OPTS /tmp/szrust-deploy.tar.gz $HOST:/tmp/
    ssh_cmd "rm -rf $TARGET/sz-rust.new && mkdir -p $TARGET/sz-rust.new && \
        tar -xzf /tmp/szrust-deploy.tar.gz -C $TARGET/sz-rust.new --strip-components=1 && \
        rm -rf $TARGET/sz-rust.old && mv $TARGET/sz-rust $TARGET/sz-rust.old 2>/dev/null; \
        mv $TARGET/sz-rust.new $TARGET/sz-rust && rm /tmp/szrust-deploy.tar.gz"
    echo "sz-rust 已同步 $(git -C "$SZRUST" rev-parse --short HEAD)"
}

sync_kit() {
    echo "=== 部署 soak-kit 到 $TARGET/soak-toolkit/projects ==="
    ssh_cmd "mkdir -p $TARGET/soak-toolkit/projects"
    scp $SSH_OPTS "$KIT"/*.sh $HOST:$TARGET/soak-toolkit/projects/
    ssh_cmd "chmod +x $TARGET/soak-toolkit/projects/*.sh $TARGET/soak-toolkit/*.sh"
}

case "$WHAT" in
    all)     sync_szorm; sync_szrsql; sync_szrust; sync_kit ;;
    sz-orm)  sync_szorm ;;
    szrsql)  sync_szrsql ;;
    sz-rust) sync_szrust ;;
    kit)     sync_kit ;;
    *) echo "用法: deploy-122.sh [all|sz-orm|szrsql|sz-rust|kit]"; exit 1 ;;
esac
echo "=== 同步完成 ==="
