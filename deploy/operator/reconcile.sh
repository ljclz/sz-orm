#!/bin/bash
# SZ-ORM K8s Operator 调谐脚本
# 
# 监听 SzOrmInstance CRD 变更，调谐 Deployment 副本数至期望值。
# 调谐逻辑：
#   1. 读取 SzOrmInstance spec.replicas
#   2. 查询关联 Deployment 当前副本数
#   3. 若不一致，更新 Deployment 副本数
#   4. 更新 SzOrmInstance status.readyReplicas 和 status.phase
#
# 约束：仅管理命名空间级资源，禁止修改集群级 ConfigMap/Secret

set -euo pipefail

RECONCILE_INTERVAL="${RECONCILE_INTERVAL:-30}"
WATCH_NAMESPACE="${WATCH_NAMESPACE:-default}"

echo "[sz-orm-operator] starting controller, namespace=${WATCH_NAMESPACE}, interval=${RECONCILE_INTERVAL}s"

while true; do
    # 获取所有 SzOrmInstance CR
    INSTANCES=$(kubectl get szorminstances -n "${WATCH_NAMESPACE}" -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}' 2>/dev/null || true)

    for INSTANCE in ${INSTANCES}; do
        echo "[sz-orm-operator] reconciling SzOrmInstance: ${INSTANCE}"

        # 读取期望副本数
        DESIRED_REPLICAS=$(kubectl get szorminstance "${INSTANCE}" -n "${WATCH_NAMESPACE}" -o jsonpath='{.spec.replicas}' 2>/dev/null || echo "2")
        
        # 读取当前 Deployment 副本数
        CURRENT_REPLICAS=$(kubectl get deployment "${INSTANCE}" -n "${WATCH_NAMESPACE}" -o jsonpath='{.spec.replicas}' 2>/dev/null || echo "0")

        if [ "${DESIRED_REPLICAS}" != "${CURRENT_REPLICAS}" ]; then
            echo "[sz-orm-operator] scaling ${INSTANCE}: ${CURRENT_REPLICAS} -> ${DESIRED_REPLICAS}"
            kubectl scale deployment "${INSTANCE}" -n "${WATCH_NAMESPACE}" --replicas="${DESIRED_REPLICAS}"
        fi

        # 更新 status
        READY_REPLICAS=$(kubectl get deployment "${INSTANCE}" -n "${WATCH_NAMESPACE}" -o jsonpath='{.status.readyReplicas}' 2>/dev/null || echo "0")
        PHASE="Pending"
        if [ "${READY_REPLICAS}" = "${DESIRED_REPLICAS}" ]; then
            PHASE="Ready"
        fi

        kubectl patch szorminstance "${INSTANCE}" -n "${WATCH_NAMESPACE}" --type=merge -p "{\"status\":{\"readyReplicas\":${READY_REPLICAS},\"phase\":\"${PHASE}\",\"lastReconcileTime\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}}" 2>/dev/null || true

        echo "[sz-orm-operator] reconciled ${INSTANCE}: desired=${DESIRED_REPLICAS}, ready=${READY_REPLICAS}, phase=${PHASE}"
    done

    sleep "${RECONCILE_INTERVAL}"
done