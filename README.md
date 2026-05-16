# OpenForce

[![License](https://img.shields.io/badge/license-OpenForce%20Learning%20v1.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://rust-lang.org)
[![Tests](https://img.shields.io/badge/tests-44%20passed-green.svg)](#测试)

> **道生一，一生二，二生三，三生万物。**
> *— 道德经·第四十二章*

新一代 **Agent OS** — 蜂群式 AI Agent 编排平台。一个任务进入，分解为千百个子任务，跨物理机动态创建 Worker 并行执行，完成后瞬间销毁，不留痕迹。

## 哲学与架构

```
                      ┌─────────────────────────────────┐
                      │          用户指令 (道)            │
                      └──────────────┬──────────────────┘
                                     │
                      ┌──────────────▼──────────────────┐
                      │      Planner (一)               │
                      │   理解意图 · 检索知识库           │
                      │   生成执行计划 · 分配专家模型     │
                      └──────────────┬──────────────────┘
                                     │
                      ┌──────────────▼──────────────────┐
                      │     Scheduler (二)              │
                      │   确定性调度 · DAG 推进          │
                      │   Lease 发放 · Fencing Token    │
                      └──────────────┬──────────────────┘
                                     │
          ┌──────────────┬───────────┼───────────┬──────────────┐
          ▼              ▼           ▼           ▼              ▼
    ┌─────────┐   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐
    │Worker-1 │   │Worker-2 │  │Worker-3 │  │Worker-4 │  │Worker-N │
    │(三)     │   │(三)     │  │(三)     │  │(三)     │  │(三)     │
    │独立进程  │   │独立进程  │  │独立沙箱  │  │独立物理机│  │用完即焚  │
    └────┬────┘   └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘
         │             │            │            │            │
         └─────────────┴────────────┴────────────┴────────────┘
                                     │
                      ┌──────────────▼──────────────────┐
                      │       万物 (产出)                │
                      │   Artifacts · Patches · Findings │
                      │   完整审计链 · Event-Sourced     │
                      └─────────────────────────────────┘
```

**"一生三"** — Planner 将用户意图分解为执行计划，Scheduler 将计划编译为 DAG，Worker 将 DAG 节点转化为实际产出。

**"三生万物"** — 一个 Scheduler 管理成千上万个 Worker，每个 Worker 在独立物理机或沙箱中运行，动态创建、完成任务后立即销毁。

### 对比旧版：从"三省六部"到"蜂群式"

| 维度 | v1.0 三省六部制 | v5.1 蜂群式 |
|------|----------------|------------|
| 架构理念 | 模拟古代官制，固定角色 | 道家哲学，动态蜂群 |
| 任务分配 | 六部各司其职，固定职能 | Planner→Scheduler→Workers，按需生成 |
| 并发模型 | 六部串行协作 | 千百 Worker 跨机并行 |
| 状态管理 | 黑板模式 | Event-Sourced Session |
| 可靠性 | 单点脆弱 | CAS + Lease + Fencing Token |
| 可审计性 | 有限 | 26 种事件类型，完整因果链 |
| Worker 生命周期 | 持久存在 | 瞬时创建，用完即焚 |
| 物理部署 | 单机 Python | 跨物理机 Rust 二进制 |

## 核心能力

### 事件溯源 (Event Sourcing)

```
SessionCreated → PlanCompiled → TaskReadied → TaskLeased
→ TaskStarted → HeartbeatReceived → ArtifactSubmitted
→ TaskSucceeded → SessionCompleted
```

所有状态变更以 append-only Event Log 持久化到 PostgreSQL。任何时刻的系统状态都可以从事件日志完整重放。

### 确定性调度 (Deterministic Scheduling)

- **CAS 版本控制** — `pg_advisory_xact_lock` 串行化写入，杜绝脑裂
- **Lease + Fencing Token** — 每次任务重试递增 Token，旧 Worker 迟到写入被自动拒绝
- **Plan Epoch** — 重规划时旧计划任务显式 Inherited/Frozen/Invalidated
- **Command 幂等** — `command_dedup` 表确保同一命令不会重复执行

### HITL 审批 (Human-in-the-Loop)

```
Worker 提交敏感 Patch
  → PatchClassifier 评估风险 (Safe/Moderate/Sensitive/Reject)
    → Sensitive → 自动升级为 ApprovalRequest
      → 人工 Approve/Reject
        → ApprovalToken 强绑定 (session+task+lease+fencing+snapshot+hash)
          → 消费后立即失效
```

### Effect Gateway — 副作用隔离

所有真实副作用（部署、迁移、通知、计费）必须经过 Effect Gateway。Worker 只能提交 EffectRequest，不能直接执行。Gateway 强制幂等 (idempotency_key UNIQUE) + 审批 + Outbox 分发。

### 三层空间隔离

| 空间 | 定位 | 环境 |
|------|------|------|
| **Agent Space** | Worker 运行 ReAct 循环 | 极轻量 Python/Node |
| **Project Workspace** | 全局项目产物 | VFS + 对象存储 |
| **Execution Sandbox** | 集成测试/编译 | 完整运行时 (Node+DB+Go) |

### 多 LLM 后端支持

```
[planner]          → 固定模型 (config 配置)
[workers.profiles] → Planner 按任务类型分配合适模型
  ├── code_review    → deepseek-v4-flash / claude-sonnet
  ├── security_audit → deepseek-v4-flash / claude-sonnet
  ├── architecture   → claude-opus
  └── code_generation → claude-sonnet
```

同时支持 Anthropic API 和 OpenAI-compatible API (DeepSeek 等)。

## 服务拓扑

```
                    ┌─────────────┐
                    │ REST Gateway │
                    │   :8080      │
                    └──────┬───────┘
                           │
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                   ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│Scheduler     │  │Project Tools │  │Effect Gateway│
│   :50052     │  │   :50053     │  │   :50054     │
└──────┬───────┘  └──────────────┘  └──────────────┘
       │
       ▼
┌──────────────┐     ┌──────────────┐
│Session Store │────▶│ PostgreSQL   │
│   :50051     │     │ Event Log    │
└──────┬───────┘     └──────────────┘
       │
       ▼
┌──────────────┐
│Node Daemon   │
│   :50060     │
│ spawn Worker │
│  进程/沙箱    │
└──────────────┘
```

## 项目结构

```
openforce/
├── openforce.toml              # 配置文件 (Planner/Worker 模型)
├── crates/
│   ├── proto/                  # Protobuf 定义 (6 文件)
│   ├── domain/                 # 领域类型 (26 EventPayload, 8 TaskState, 12 CommandType)
│   ├── session-store/          # Event Store (CAS + 12 命令处理器 + 投影)
│   ├── scheduler/              # 确定性调度 (DAG + Lease + Fencing)
│   ├── policy-engine/          # 策略引擎 (租户/预算/熔断)
│   ├── path-acl/               # 路径 ACL (glob 模式)
│   ├── patch-classifier/       # Patch 风险分级 (9 PCR 规则)
│   ├── project-tools/          # Project Tools + HITL 审批
│   ├── effect-gateway/         # 副作用网关 (幂等 + Outbox)
│   ├── gateway/                # REST → gRPC 代理
│   ├── space-manager/          # 三层隔离 + 热池
│   ├── tenant-governance/      # 多租户治理 (保留/离职/BYOK)
│   ├── evolution/              # 进化面 (Observer/Evaluator/Canary)
│   ├── launch-checker/         # 上线验证 (8 Gate + 5 Red-Team)
│   ├── node-daemon/            # Worker 生命周期管理
│   ├── llm-client/             # LLM 客户端 (Anthropic + OpenAI)
│   ├── tui-dashboard/          # 终端监控面板
│   └── openforce-cli/          # CLI 入口
└── version1.0/                 # 旧版"三省六部"归档
```

## 快速开始

> 📖 详细部署指南: [DEPLOYMENT.md](DEPLOYMENT.md)

```bash
export OPENAI_API_KEY="sk-..."

# 2. 初始化数据库
createdb openforce_test
DATABASE_URL="postgres://localhost:5432/openforce_test" cargo run -p openforce-session-store

# 3. 提交任务 (CLI 模式)
cargo run -p openforce-cli -- -w /path/to/project "审查项目安全性"

# 4. TUI 监控面板
cargo run -p openforce-tui-dashboard

# 5. 启动全部服务
# Session Store (:50051) → cargo run -p openforce-session-store
# Scheduler (:50052)      → cargo run -p openforce-scheduler
# Project Tools (:50053)  → cargo run -p openforce-project-tools
# Effect Gateway (:50054) → cargo run -p openforce-effect-gateway
# Node Daemon (:50060)    → cargo run -p openforce-node-daemon
# REST Gateway (:8080)    → cargo run -p openforce-gateway
```

## 技术栈

| 组件 | 技术 |
|------|------|
| 语言 | Rust 2024 Edition |
| 传输 | gRPC (tonic) + REST (axum) |
| 存储 | PostgreSQL (Event Sourcing + JSONB) |
| 沙箱 | 进程隔离 + 腾讯 Cube |
| LLM | Anthropic API / OpenAI-compatible (DeepSeek) |
| TUI | Ratatui + Crossterm |

## 测试

```
cargo test --workspace
# 44 tests: 15 domain + 14 lifecycle + 4 classifier + 5 ACL + 6 PostgreSQL integration
```

---

> **夫物芸芸，各复归其根。**
> 千百 Worker，完成使命，归于虚无。唯有 Event Log，永存不灭。
