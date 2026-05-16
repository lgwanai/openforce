# OpenForce

[![License](https://img.shields.io/badge/license-OpenForce%20Learning%20v1.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://rust-lang.org)
[![Tests](https://img.shields.io/badge/tests-44%20passed-green.svg)](#测试)

> **道生一，一生二，二生三，三生万物。**
> *— 道德经·第四十二章*

新一代 **Agent OS** — 蜂群式 AI Agent 编排平台。一个任务进入，分解为千百个子任务，每个子任务由不同角色、不同模型的 Worker 独立执行，跨物理机动态创建，完成后瞬间销毁。

## 实际案例：从一行指令到完整交付

假设你输入：

```
"帮我做一个图书管理系统，包含前端借阅界面、后端 API、数据库设计和安全审计"
```

OpenForce 的执行过程：

```
┌──────────────────────────────────────────────────────────────────┐
│ 用户: "帮我做一个图书管理系统..."                                   │
└──────────────────────────┬───────────────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────────────┐
│                    Planner (claude-opus)                         │
│                                                                  │
│  检索知识库 → 匹配 SOP → 生成执行计划:                             │
│                                                                  │
│  Task 1: [架构师角色] 设计数据库 Schema 和 API 契约                │
│  Task 2: [后端专家]   实现图书 CRUD + 借阅逻辑                     │
│  Task 3: [前端专家]   实现 Vue 借阅界面 + 读者管理                  │
│  Task 4: [安全专家]   审计认证模块 + SQL 注入检查                   │
│  Task 5: [测试专家]   编写集成测试 + E2E 用例                       │
│                                                                  │
│  每个 Task 动态分配: 角色 Profile + 模型 + 工具策略                 │
└──────────────────────────┬───────────────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────────────┐
│                       Scheduler (确定性)                          │
│                                                                  │
│  编译 DAG:                                                        │
│    Task1 ──┬──→ Task2 ──→ Task5                                  │
│            └──→ Task3 ──→ Task5                                   │
│            └──→ Task4 ──→ Task5                                   │
│                                                                  │
│  发放 5 个独立 Lease (Fencing Token: 1~5)                         │
│  分发到 3 台物理机的 Node Daemon                                  │
└──────────────────────────┬───────────────────────────────────────┘
                           │
     ┌─────────────────────┼─────────────────────┐
     │                     │                     │
┌────▼─────┐  ┌───────▼────┐  ┌───────▼────┐
│ 物理机 A  │  │  物理机 B   │  │  物理机 C   │
│           │  │            │  │            │
│ Worker-1  │  │  Worker-2  │  │  Worker-4  │
│ 架构师    │  │  后端专家   │  │  安全专家   │
│ claude-   │  │  claude-   │  │  claude-   │
│ opus      │  │  sonnet    │  │  sonnet    │
│           │  │            │  │            │
│ Worker-3  │  │  Worker-5  │  │            │
│ 前端专家   │  │  测试专家   │  │            │
│ deepseek  │  │  deepseek  │  │            │
└────┬─────┘  └───────┬────┘  └───────┬────┘
     │                │               │
     └────────────────┼───────────────┘
                      │
┌─────────────────────▼────────────────────────────────────────────┐
│                         产出 (万物)                               │
│                                                                  │
│  Worker-1 → Artifact: api-schema.json + db-schema.sql            │
│  Worker-2 → Artifact: book-service.rs + borrow-handler.rs        │
│  Worker-3 → Artifact: BookList.vue + ReaderManage.vue            │
│  Worker-4 → Finding:  安全审计报告 (发现 3 个问题，建议修复)       │
│  Worker-5 → Finding:  测试报告 (42 unit + 8 E2E, 覆盖 87%)       │
│                                                                  │
│  Merge Agent 合并所有 Patch → 完整项目交付                         │
│  Event Log: 47 个事件，完整审计链可回放                            │
└──────────────────────────────────────────────────────────────────┘
```

**关键特征**：

| 特征 | 体现 |
|------|------|
| **不同角色** | 架构师 / 后端 / 前端 / 安全 / 测试 — 5 个 Worker 5 种角色 |
| **不同模型** | claude-opus 做架构设计, claude-sonnet 做代码生成, deepseek 做前端和测试 |
| **跨物理机** | 3 台机器并行执行，Worker 之间通过 Project Workspace 共享产物 |
| **动态创建销毁** | 每个 Worker 用完即焚，临时凭证任务结束后立即失效 |
| **Session 独立** | 本次项目的 47 个事件仅属于此 Session，不污染其他项目 |
| **经验解耦** | Observer 采集执行数据进入进化面，但不会反向影响正在运行的任务 |

## 核心设计原则

### 三层解耦

```
┌──────────────────────────────────────────────┐
│              进化面 (Evolution Plane)          │
│  Observer → Evaluator → Evolver              │
│  旁路采集 · 离线评估 · 版本化发布               │
│  绝不直接修改正在运行的 Session                 │
└──────────────────────────────────────────────┘
                      ↕ 单向读
┌──────────────────────────────────────────────┐
│             控制面 (Control Plane)            │
│  Planner → Scheduler → Session Store         │
│  计划编译 · 确定性调度 · 事件溯源              │
└──────────────────────────────────────────────┘
                      ↕
┌──────────────────────────────────────────────┐
│              数据面 (Data Plane)              │
│  Node Daemon → Workers (Agent Space)         │
│  Project Workspace (VFS)                     │
│  Execution Sandbox (测试/编译)                │
└──────────────────────────────────────────────┘
```

**三面严格解耦**：进化面不能触及控制面的状态机，控制面不能直接操作数据面的沙箱，数据面不能绕过控制面执行副作用。

### 三层空间隔离

```
Worker 写代码             项目共享文件            运行测试
    │                         │                     │
┌───▼──────────┐    ┌─────────▼──────┐    ┌────────▼──────────┐
│ Agent Space  │    │Project Workspace│    │ Execution Sandbox │
│              │    │                 │    │                   │
│ ReAct 循环    │───▶│ VFS + 对象存储   │───▶│ 完整运行时         │
│ 调用 LLM     │    │ 所有 Worker 共享  │    │ Node + DB + Go    │
│ 生成 Patch   │    │ 唯一真相源        │    │ 集成测试           │
│              │    │                 │    │                   │
│ 极轻量       │    │ CAS 版本控制     │    │ 测试完立即销毁      │
│ 只能写 Patch  │    │ 不可裸写          │    │                   │
└──────────────┘    └─────────────────┘    └───────────────────┘
```

**为什么必须三层分离**：单个写 Vue 页面的 Worker 无法在自己的沙箱里跑通整个集成测试。Agent Space 只负责"写代码"，Execution Sandbox 负责"跑测试"，两者通过 Project Workspace 解耦。

### 角色与模型：动态拉取，非硬编码

每个 Worker 的角色 (Role Profile)、Prompt (Prompt Bundle)、工具策略 (Tool Policy) 和执行模型 **不是在代码中写死的**，而是 由 Planner 在分解任务时根据 SOP 和知识库动态决策：

```
Planner 分解任务
  │
  ├── 匹配知识库 → "这是 CRUD 后端任务，使用 backend_expert 角色"
  │     └── Worker-2: role=backend_expert_v9, model=claude-sonnet, prompt=bundle_crud_v21
  │
  ├── 匹配知识库 → "这是前端页面任务，使用 vue3_frontend 角色"
  │     └── Worker-3: role=vue3_frontend_v9, model=deepseek-v4-flash, prompt=bundle_vue_v15
  │
  └── 匹配知识库 → "这是安全审计，需要深度推理"
        └── Worker-4: role=security_auditor_v4, model=claude-sonnet, prompt=bundle_audit_v7
```

**角色 Profile、Prompt Bundle、Tool Policy、Sandbox Image 全部版本化**。即使后台发布了新版本，正在运行中的 Worker 也不会被影响——Worker Spec 一旦生成即冻结。

### Session 与经验解耦

```
Session A (图书管理系统)           Session B (电商平台)
        │                                  │
        │ 完整 Event Log                    │ 完整 Event Log
        │ 47 events                         │ 89 events
        │                                  │
        └──────────┬───────────────────────┘
                   │
        ┌──────────▼───────────┐
        │   进化面 (旁路)       │
        │                      │
        │ Observer: 采集指标    │  ← 只读，不影响 Session
        │ Evaluator: 评分      │  ← 在历史数据上回放
        │ Evolver: 生成候选版   │  ← 版本化 + 灰度 + 可回滚
        │                      │
        │ 只接受"审计确认合法     │
        │ 成功"的样本            │
        └──────────────────────┘
```

**Session 是独立的** — 每个项目的 Event Log 完全隔离，不会互相污染。  
**经验是跨 Session 提炼的** — 但只能从已审计确认的合法成功样本中学习。  
**进化面的更新必须版本化、灰度化、可回滚** — 绝不允许静默全量替换。

### 事件溯源 (Event Sourcing)

```
SessionCreated → PlanCompiled → TaskReadied → TaskLeased
→ TaskStarted → HeartbeatReceived → ArtifactSubmitted
→ TaskSucceeded → SessionCompleted
```

所有状态变更以 append-only Event Log 持久化到 PostgreSQL。任何时刻的系统状态都可以从事件日志完整重放。26 种事件类型覆盖完整生命周期。

### 确定性调度 (Deterministic Scheduling)

- **CAS 版本控制** — `pg_advisory_xact_lock` 串行化写入，杜绝脑裂
- **Lease + Fencing Token** — 每次任务重试递增 Token，旧 Worker 迟到写入被自动拒绝
- **Plan Epoch** — 重规划时旧计划任务显式 Inherited/Frozen/Invalidated
- **Command 幂等** — `command_dedup` 表确保同一命令不会重复执行

### HITL 审批

```
Worker 提交敏感 Patch
  → PatchClassifier (9 PCR 规则) 评估风险
    → Sensitive → ApprovalRequest → 人工 Approve/Reject
      → ApprovalToken (强绑定 7 个字段) → 消费后立即失效
```

### Effect Gateway — 副作用隔离

所有部署、迁移、通知、计费等真实副作用必须经过 Effect Gateway。Worker 只能提交 EffectRequest，Gateway 强制幂等 + 审批 + Outbox 分发。

## 对比：v1.0 三省六部 vs v5.1 蜂群式

| 维度 | v1.0 三省六部制 | v5.1 蜂群式 |
|------|----------------|------------|
| 架构理念 | 模拟古代官制，固定角色 | 道家哲学，动态蜂群 |
| 角色分配 | 六部各司其职，硬编码 | Planner 根据任务类型动态匹配 Role Profile |
| 模型选择 | 单一模型 | 按角色动态分配不同模型 |
| 并发模型 | 六部串行协作 | 千百 Worker 跨物理机并行 |
| 空间模型 | 单体 | 三层隔离 (Agent/Workspace/Execution) |
| 状态管理 | 黑板模式 | Event-Sourced Session |
| 经验系统 | 无 | 旁路进化面，Session 与经验解码 |
| 可靠性 | 单点脆弱 | CAS + Lease + Fencing Token |
| Worker 生命周期 | 持久存在 | 瞬时创建，用完即焚 |
| 部署 | 单机 Python | 跨物理机 Rust 二进制 |

## 对比：OpenForce vs Claude Code

Claude Code 是优秀的单智能体编程助手，但受限于单进程架构。OpenForce 是为**复杂工程任务**设计的多智能体编排系统。

| 维度 | Claude Code | OpenForce |
|------|------------|-----------|
| 架构模型 | 单 Agent，一体式 | 多 Agent，蜂群式 |
| 任务分解 | 一次性理解，隐含 | Planner 显式分解为带类型的子任务 |
| Worker | 自身 (单向) | N 个独立 Worker，每个独立进程/沙箱 |
| 角色系统 | 固定 System Prompt | 动态 Role Profile + Prompt Bundle，按任务匹配 |
| 模型选择 | 单一模型 | 按角色分配不同模型 (opus/sonnet/haiku/deepseek) |
| 物理执行 | 单机单进程 | 跨物理机，Node Daemon 管理 Worker 生命周期 |
| 状态持久化 | 会话结束时丢失 | Event-Sourced Session (26 种事件类型，完整审计) |
| 并发模式 | 串行 | 真正并行 — N 个 Worker 同时执行 |
| 可靠性 | 依赖进程存活 | CAS + Lease + Fencing Token，Worker 崩溃不污染状态 |
| 副作用控制 | 无 | Effect Gateway 统一入口，幂等 + 审批 + Outbox |
| 安全审批 | 无 | HITL 审批流，Patch 风险分级 (9 PCR)，签名 Token |
| 经验学习 | 会话内上下文 | Observer → Evaluator → Evolver 版本化旁路进化 |
| 空间隔离 | 本地文件系统 | 三层隔离 (Agent/Workspace/Execution) |
| 多租户 | 不适用 | 租户级策略、配额、BYOK、Kill Switch |
| 适用场景 | 对话式编程辅助 | 企业级 AI 编排生产平台 |

**本质区别**：Claude Code 是一个"人" — 聪明但有限。OpenForce 是一个"组织" — Planner 像 CEO 分配任务，Scheduler 像 COO 确保执行，多个 Worker 像不同部门的专家各司其职。一个人再强也不可能同时写前端、写后端、做安全审计、跑集成测试；但一个好的组织可以。

**互补关系**：你可以用 Claude Code 来开发 OpenForce，然后用 OpenForce 来编排成百上千个 Claude Code 完成任务。Claude Code 是锤子，OpenForce 是工厂。

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
│   :50060     │  (多实例，跨物理机)
└──────────────┘
```

## 项目结构

```
openforce/
├── openforce.toml              # 角色/模型配置
├── crates/
│   ├── proto/                  # Protobuf 定义 (6 文件)
│   ├── domain/                 # 领域类型 (26 EventPayload, 12 CommandType)
│   ├── session-store/          # Event Store (CAS + 命令处理器 + 投影)
│   ├── scheduler/              # 确定性调度 (DAG + Lease + Fencing)
│   ├── policy-engine/          # 策略引擎
│   ├── path-acl/               # 路径 ACL (glob 模式)
│   ├── patch-classifier/       # Patch 风险分级 (9 PCR 规则)
│   ├── project-tools/          # Project Tools + HITL 审批
│   ├── effect-gateway/         # 副作用网关 (幂等 + Outbox)
│   ├── gateway/                # REST → gRPC 代理
│   ├── space-manager/          # 三层隔离 + 热池
│   ├── tenant-governance/      # 多租户治理
│   ├── evolution/              # 进化面 (Observer/Evaluator/Canary)
│   ├── launch-checker/         # 上线验证 (8 Gate + 5 Red-Team)
│   ├── node-daemon/            # Worker 生命周期
│   ├── llm-client/             # LLM 客户端 (Anthropic + OpenAI)
│   ├── tui-dashboard/          # 终端监控面板
│   └── openforce-cli/          # CLI 入口
└── version1.0/                 # 旧版归档
```

## 快速开始

> 📖 详细部署: [DEPLOYMENT.md](DEPLOYMENT.md)

```bash
export OPENAI_API_KEY="sk-..."

# 启动服务
cargo run -p openforce-session-store   # :50051
cargo run -p openforce-scheduler       # :50052

# 提交任务
cargo run -p openforce-cli -- -w /path/to/project "审查项目"

# TUI 监控
cargo run -p openforce-tui-dashboard
```

## 技术栈

| 组件 | 技术 |
|------|------|
| 语言 | Rust 2024 Edition |
| 传输 | gRPC (tonic) + REST (axum) |
| 存储 | PostgreSQL (Event Sourcing + JSONB) |
| LLM | Anthropic / OpenAI-compatible (DeepSeek) |
| TUI | Ratatui + Crossterm |

---

> **夫物芸芸，各复归其根。**
> 千百 Worker，不同角色，不同模型，完成使命，归于虚无。唯有 Event Log，永存不灭。
