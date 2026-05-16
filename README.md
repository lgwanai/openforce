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

## 核心设计原则：四层解耦

OpenForce 的理论基础是**四层递进解耦**，每一层解决一个特定的耦合问题。层层递进，构建出可调度、可复用的 Agent 编排系统。

### 第一层：大脑与双手解耦

**解决的问题**：模型（大脑）的规划决策与代码执行（双手）的环境耦合在一起。一次执行失败污染整个上下文，环境问题影响全局决策。

```
┌────────────────────────────┐      ┌────────────────────────────┐
│      大脑 (Planner)         │      │      双手 (Sandbox)         │
│                            │      │                            │
│  LLM 模型                  │──①──▶│  独立沙箱 (Docker/进程)     │
│  生成指令序列               │  指令 │  执行代码                   │
│  不执行任何代码             │      │  运行测试                   │
│                            │◀──②──│                            │
│  上下文安全                 │  结果 │  无状态                    │
│  不受执行环境影响           │      │  崩溃即销毁，新建即干净     │
│                            │      │  不同任务绝对隔离           │
└────────────────────────────┘      └────────────────────────────┘
```

**设计要点**：
- Planner 只生成指令和计划，不执行任何代码
- 代码执行在完全隔离的沙盒中进行
- 沙盒是无状态的——崩溃即销毁，新建即干净
- 不同的子任务跑在不同的沙盒中，绝不复用

### 第二层：协调者与执行者分离

**解决的问题**：单个 Agent 处理长程复杂任务时，上下文窗口被无限拉长、记忆不断压缩，导致智能退化。一个人不可能同时精通后端、前端、安全、测试——一个 Agent 也不行。

```
┌─────────────────────────────────────────────────────┐
│           协调者 (Orchestrator / Planner)            │
│                                                     │
│  • 拆解复杂任务                                     │
│  • 分派给专业子 Agent                                │
│  • 不执行具体工作                                    │
│  • 上下文窗口保持精简                                │
└────────┬──────────────┬──────────────┬──────────────┘
         │              │              │
┌────────▼────┐  ┌───────▼─────┐  ┌───▼──────────┐
│ Worker-1    │  │  Worker-2   │  │  Worker-3     │
│ 后端专家    │  │  前端专家    │  │  安全审计     │
│             │  │             │  │               │
│ 独立 Prompt │  │ 独立 Prompt  │  │ 独立 Prompt   │
│ 独立工具集  │  │ 独立工具集   │  │ 独立工具集    │
│ 独立 MCP    │  │ 独立 MCP     │  │ 独立 MCP      │
│             │  │             │  │               │
│ 只做 CRUD   │  │ 只写 Vue    │  │ 只做审计      │
└─────────────┘  └─────────────┘  └───────────────┘
```

**设计要点**：
- 协调者只拆任务、分任务，不干具体活
- 每个 Worker 有独立的提示词、工具集和 MCP 服务器
- 每个 Worker 只专注于被分配的子任务，职责单一
- 避免了单一 Agent 上下文爆炸和注意力分散

### 第三层：状态与会话解耦

**解决的问题**：Agent 的身份定义与它的运行时状态耦合在一起，使得 Agent 无法被灵活管理和复用。同一个"角色"应该能同时执行多个不同的具体任务。

```
┌─────────────────────────────────┐     ┌─────────────────────────────┐
│   Agent 定义 (静态模板)           │     │   Session (动态实例)          │
│                                 │     │                             │
│   "我是谁"                      │     │   "我此刻在干什么"            │
│                                 │     │                             │
│   • 模型 (claude-sonnet)        │     │   • 对话历史                  │
│   • 系统提示词                   │     │   • 执行线程                  │
│   • 可用工具集                   │     │   • 挂载的文件系统             │
│   • 权限边界                     │     │   • 当前上下文快照             │
│   • 角色 Profile                 │     │   • 租约/Fencing Token       │
│                                 │     │                             │
│   = 配置文件, 不变               │     │   = 运行时数据, 不断变化       │
│   一份模板可启动 N 个 Session     │     │   每个 Session 完全独立        │
└─────────────────────────────────┘     └─────────────────────────────┘

  同一份 "后端专家 v9" Agent 模板
      │
      ├──→ Session A: 正在写图书 CRUD
      ├──→ Session B: 正在写订单 CRUD  
      └──→ Session C: 正在修 API bug
```

**设计要点**：
- Agent 定义是静态配置文件，Session 是动态运行实例
- 同一 Agent 模板可同时启动多个独立的 Session
- Session 之间完全隔离，一个崩溃不影响其他
- 实现了 **Worker 的池化和复用**——不需要为每个任务"造一个新的 Agent"

### 第四层：上下文与运行环境解耦

**解决的问题**：Agent 的"记忆"依赖本地进程。如果 Worker 崩溃，记忆丢失，只能重来。而且无法把"最聪明的状态"复制给其他 Worker。

```
┌──────────────────────────────────────────────────┐
│              上下文外置存储                        │
│                                                  │
│  Session 完整上下文 (对话历史 + 执行轨迹)           │
│  存储于外部高速存储 (PostgreSQL / Redis)           │
│  不依赖本地进程内存                               │
└──────────────────────┬───────────────────────────┘
                       │
        ┌──────────────┼──────────────┐
        │              │              │
   ┌────▼────┐   ┌─────▼─────┐  ┌────▼────┐
   │快照     │   │克隆       │  │回滚     │
   │         │   │           │  │         │
   │保存当前 │   │复制一份    │  │退回到   │
   │最优状态 │   │给新 Worker │  │之前的   │
   │         │   │           │  │检查点   │
   └─────────┘   └───────────┘  └─────────┘
```

**实际场景**：一个 Worker 刚刚完成了"搜索并理解整个代码库"的高成本操作，处于最聪明状态。协调者将这个状态**快照克隆 10 份**，分派 10 个不同的编码任务——每个 Worker 都从"已理解代码库"的状态开始，不用重复搜索。

**设计要点**：
- Session 完整上下文存储于外部，不依赖本地
- 协调者可对 Session 进行**快照、克隆、回滚**
- 克隆"最聪明状态" → 并行分派不同任务
- 回滚到检查点 → 从失败中快速恢复

---

### 四层递进总结

```
第四层  上下文与运行环境解耦  →  快照、克隆、回滚、调度到最优状态
  ↑
第三层  状态与会话解耦        →  同一 Agent 模板，N 个独立 Session
  ↑
第二层  协调者与执行者分离    →  一人拆任务，多人各司其职
  ↑
第一层  大脑与双手解耦        →  大脑出主意，双手在沙箱执行
```

这套解耦体系使得 OpenForce 不仅是"多个 Agent 一起工作"，而是**一个可调度、可复用、可回滚的 Agent 编排系统**。

### 其他核心机制

**事件溯源** — 所有状态变更以 append-only Event Log 持久化，26 种事件类型覆盖完整生命周期。

**确定性调度** — CAS 版本控制 + Lease + Fencing Token，杜绝脑裂和双写。

**HITL 审批** — PatchClassifier (9 PCR 规则) → ApprovalRequest → ApprovalToken (7 字段强绑定)。

**Effect Gateway** — 所有副作用（部署、迁移、通知）统一入口，幂等 + 审批 + Outbox。

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
