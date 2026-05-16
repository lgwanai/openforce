# OpenForce 部署指南

## 环境要求

| 组件 | 版本 | 说明 |
|------|------|------|
| Rust | 1.85+ | 编译 |
| PostgreSQL | 16+ | Event Store (JSONB) |
| Protobuf | 3.25+ | proto 编译 |
| OpenSSL | 1.1+ | TLS |
| pkg-config | — | sqlx 编译 |

```bash
# macOS
brew install rustup postgresql@17 protobuf openssl pkg-config

# Ubuntu
apt install build-essential libssl-dev pkg-config protobuf-compiler postgresql-16

# LLM API Key
export OPENAI_API_KEY="sk-..."
```

---

## 单服务器部署

所有服务在一台机器，通过 localhost gRPC 通信。

```
:8080  Gateway ──→ :50052 Scheduler ──→ :50051 SessionStore ──→ :5432 PostgreSQL
                   :50053 ProjectTools
                   :50054 EffectGateway
                   :50060 NodeDaemon × N
```

### 启动

```bash
# 1. 数据库
createdb openforce

# 2. 按依赖顺序启动（每个服务一个终端）
DATABASE_URL="postgres://localhost:5432/openforce" \
    cargo run --release -p openforce-session-store

SESSION_STORE_ADDR="127.0.0.1:50051" \
    cargo run --release -p openforce-scheduler

DATABASE_URL="postgres://localhost:5432/openforce" \
    cargo run --release -p openforce-project-tools

DATABASE_URL="postgres://localhost:5432/openforce" \
    cargo run --release -p openforce-effect-gateway

OPENAI_API_KEY="sk-..." \
    cargo run --release -p openforce-node-daemon

HTTP_ADDR="0.0.0.0:8080" \
    cargo run --release -p openforce-gateway

# 3. 验证
curl http://localhost:8080/health
cargo run -p openforce-cli -- "test"
```

### systemd 管理

```ini
# /etc/systemd/system/openforce-session-store.service
[Unit]
Description=OpenForce Session Store
After=network.target postgresql.service

[Service]
Type=simple
User=openforce
WorkingDirectory=/opt/openforce
Environment="DATABASE_URL=postgres://localhost:5432/openforce"
Environment="GRPC_ADDR=0.0.0.0:50051"
Environment="RUST_LOG=session_store=info"
ExecStart=/opt/openforce/target/release/session-store
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable --now openforce-session-store
sudo systemctl enable --now openforce-scheduler
```

---

## 多服务器部署

### 架构

```
┌─────────────────────┐    ┌─────────────────────┐
│  Control Plane      │    │  Data Plane          │
│  172.16.0.10        │    │  172.16.0.20~30      │
│                     │    │                      │
│  SessionStore:50051 │    │  NodeDaemon:50060 ×N │
│  Scheduler:50052    │    │  (Worker 进程管理)    │
│  ProjectTools:50053 │    └─────────────────────┘
│  EffectGateway:50054│
│  Gateway:8080       │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│  Data Store         │
│  172.16.0.40        │
│  PostgreSQL :5432   │
└─────────────────────┘
```

### 控制面 (172.16.0.10)

```bash
# Session Store
DATABASE_URL="postgres://172.16.0.40:5432/openforce" \
GRPC_ADDR="0.0.0.0:50051" \
    cargo run --release -p openforce-session-store

# Scheduler
SESSION_STORE_ADDR="172.16.0.10:50051" \
GRPC_ADDR="0.0.0.0:50052" \
    cargo run --release -p openforce-scheduler

# Gateway (对外)
HTTP_ADDR="0.0.0.0:8080" \
    cargo run --release -p openforce-gateway
```

### 数据面 (172.16.0.20~30, 多台)

```bash
# 每台机器运行 Node Daemon
GRPC_ADDR="0.0.0.0:50060" \
OPENAI_API_KEY="sk-..." \
    cargo run --release -p openforce-node-daemon
```

### 网络策略

| 源 | 目标 | 端口 | 协议 |
|----|------|------|------|
| Scheduler | SessionStore | 50051 | gRPC |
| Scheduler | NodeDaemon | 50060 | gRPC |
| Gateway | 所有后端 | 50051-50054 | gRPC |
| 所有服务 | PostgreSQL | 5432 | TCP |
| 外部 | Gateway | 8080 | HTTP |

---

## Docker Compose

最小化 `docker-compose.yml`:

```yaml
services:
  postgres:
    image: postgres:17-alpine
    environment:
      POSTGRES_DB: openforce
      POSTGRES_USER: openforce
      POSTGRES_PASSWORD: openforce
    volumes: [pgdata:/var/lib/postgresql/data]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U openforce"]
      interval: 5s

  session-store:
    build: { context: ., dockerfile: Dockerfile }
    environment:
      DATABASE_URL: postgres://openforce:openforce@postgres:5432/openforce
      GRPC_ADDR: 0.0.0.0:50051
    ports: ["50051:50051"]
    depends_on: { postgres: { condition: service_healthy } }

  scheduler:
    build: { context: ., dockerfile: Dockerfile }
    environment:
      SESSION_STORE_ADDR: session-store:50051
      GRPC_ADDR: 0.0.0.0:50052
    ports: ["50052:50052"]

  node-daemon:
    build: { context: ., dockerfile: Dockerfile }
    environment:
      GRPC_ADDR: 0.0.0.0:50060
      OPENAI_API_KEY: ${OPENAI_API_KEY}
    ports: ["50060:50060"]
    deploy: { replicas: 3 }

volumes:
  pgdata:
```

```bash
export OPENAI_API_KEY="sk-..."
docker compose up -d
```

---

## Kubernetes

核心资源:

```yaml
apiVersion: v1
kind: Namespace
metadata: { name: openforce }
---
apiVersion: apps/v1
kind: Deployment
metadata: { name: session-store, namespace: openforce }
spec:
  replicas: 1  # CAS 锁需要单实例
  selector: { matchLabels: { app: session-store } }
  template:
    spec:
      containers:
        - name: session-store
          image: openforce/session-store:latest
          ports: [{ containerPort: 50051 }]
          env:
            - { name: DATABASE_URL, value: "postgres://openforce:${DB_PASSWORD}@postgres:5432/openforce" }
            - { name: GRPC_ADDR, value: "0.0.0.0:50051" }
---
apiVersion: apps/v1
kind: Deployment
metadata: { name: node-daemon, namespace: openforce }
spec:
  replicas: 5  # 水平扩展
  selector: { matchLabels: { app: node-daemon } }
  template:
    spec:
      containers:
        - name: node-daemon
          image: openforce/node-daemon:latest
          ports: [{ containerPort: 50060 }]
          env:
            - { name: OPENAI_API_KEY, valueFrom: { secretKeyRef: { name: openforce-secrets, key: api-key } } }
            - { name: GRPC_ADDR, value: "0.0.0.0:50060" }
```

---

## 配置参考

```toml
# openforce.toml
[llm]
provider = "openai"                         # 或 "anthropic"
api_base = "https://api.deepseek.com"

[planner]
model = "deepseek-v4-flash"
max_tokens = 16000
temperature = 0.2

[workers.default]
model = "deepseek-v4-flash"
max_tokens = 8000
temperature = 0.1

[workers.profiles.code_review]
model = "deepseek-v4-flash"
system_prompt = "...code review expert prompt..."

[session_store]
database_url = "postgres://localhost:5432/openforce"

[scheduler]
tick_interval_ms = 500
default_lease_ttl_secs = 300
max_retries = 3
```

---

## 监控

```bash
# gRPC 健康检查
grpcurl -plaintext localhost:50051 grpc.health.v1.Health/Check

# REST 健康检查
curl http://localhost:8080/health

# TUI 监控面板
cargo run -p openforce-tui-dashboard

# 数据库维护
psql -d openforce -c "SELECT state, COUNT(*) FROM session_projection GROUP BY state;"
psql -d openforce -c "SELECT pg_size_pretty(pg_total_relation_size('event_log'));"
```

## 故障排查

| 症状 | 原因 | 解决 |
|------|------|------|
| `connect refused` | 服务未启动 | 先启动 PostgreSQL，再 SessionStore，最后其他 |
| `VersionConflict` | 并发写入 | 正常行为，CAS 自动处理，调用方重试 |
| `FencingTokenStale` | 旧 Worker 迟到提交 | 系统自动拒绝，无需处理 |
| 连接池耗尽 | 并发过多 | 增加 max_connections 或加 PgBouncer |
| Worker 超时 | NodeDaemon 资源不足 | 增加 NodeDaemon 副本数 |
