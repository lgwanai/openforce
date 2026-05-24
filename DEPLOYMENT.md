# OpenForce 部署指南

## 环境要求

| 组件 | 版本 | 说明 |
|------|------|------|
| Rust | 1.85+ | 编译 |
| Redis | 6+ | Session + Todo 持久化 (可选) |
| API Key | — | LLM 调用 |

```bash
# macOS
brew install rustup redis

# Ubuntu
apt install build-essential pkg-config redis

# LLM API Key
export API_KEY="sk-..."
```

---

## 单机部署

```bash
# 1. 启动 Redis (可选)
redis-server --daemonize yes

# 2. 编译 CLI + Worker
cargo build --release -p openforce-cli -p openforce-worker

# 3. 运行
openforce new "分析项目安全性"

# 4. 斜杠指令
openforce /graphify 分析 src 目录
openforce /code-review
openforce /tdd

# 5. 多轮 Session
openforce continue
openforce approve
openforce reject "加强认证"
openforce skills      # 列出 83 Skill
openforce sessions    # 列出所有 Session
```

---

## 配置

```toml
# openforce.toml (可选，有默认值)
[llm]
provider = "openai"
api_base = "https://api.deepseek.com"

[planner]
provider = "openai"
model = "deepseek-v4-flash"
max_tokens = 16000
temperature = 0.2

[workers.default]
provider = "openai"
model = "deepseek-v4-flash"
max_tokens = 8000
temperature = 0.1
```

---

## 项目目录

```
openforce/
├── openforce.toml          # LLM 配置
├── skills/                 # 83 Skills (渐进式披露)
├── experts/                # 专家库
├── crates/
│   ├── openforce-cli/      # CLI + Planner
│   │   ├── agents/         # 198 Agent 角色
│   │   └── src/
│   ├── llm-client/         # LLM 客户端 (OpenAI+Anthropic)
│   ├── worker/             # Worker 二进制
│   ├── skill/              # Skill 系统
│   └── redis-session/      # Redis Session+Todo
└── README.md
```

---

## 环境变量

| 变量 | 必需 | 说明 |
|------|------|------|
| `API_KEY` | 是 | LLM API Key |
| `LLM_BASE_URL` | 否 | LLM endpoint |
| `REDIS_URL` | 否 | Redis 连接 |
| `WORKER_MAX_SECS` | 否 | Worker 超时 (默认 1200) |
| `WORKER_MAX_CYCLES` | 否 | 最大 cycle (默认 40) |
| `WORKER_MAX_TOKENS` | 否 | Token 预算 (默认 150000) |

---

## Worker 控制

```bash
WORKER_MAX_CYCLES=20 WORKER_MAX_SECS=600 openforce new "快速任务"
export WORKER_MAX_NO_PROGRESS=5   # 停滞检测
export WORKER_MAX_TOKENS=100000   # Token 预算
```

---

## Redis Session (可选)

```bash
export REDIS_URL="redis://localhost:6379"

# Keys:
#   session:{id}              — Session 状态
#   session:{id}:todos        — Todo 追踪
#   session:{id}:results      — 阶段结果
#   session:{id}:worker:{wid} — Worker snapshot
```

---

## 监控

```bash
cargo check
openforce sessions
openforce skills
redis-cli keys "session:*"
```

---

## 故障排查

| 症状 | 原因 | 解决 |
|------|------|------|
| `API_KEY not set` | 缺 Key | `export API_KEY="sk-..."` |
| Worker spawn error | 未编译 | `cargo build --release -p openforce-worker` |
| Redis connect failed | 未启动 | `redis-server --daemonize yes` |
| RoundTable failed | LLM 失败 | 检查 Key/endpoint |
| Token budget exceeded | 任务复杂 | 增加 WORKER_MAX_TOKENS |
| Conversation overflow | 历史过长 | 自动 compaction |
