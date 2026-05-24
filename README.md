# OpenForce

[![License](https://img.shields.io/badge/license-OpenForce%20Learning%20v1.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://rust-lang.org)

> **道生一，一生二，二生三，三生万物。** — 道德经·第四十二章

**Agent OS** — 蜂群式 AI Agent 编排平台。一个任务进入，Planner 语义理解 → RoundTable 分解 → DAG 调度 → 198 个角色中匹配 N 个 Worker 并行执行。

---

## 快速开始

```bash
export API_KEY="sk-..."
export REDIS_URL="redis://localhost:6379"  # 可选

# 普通语义输入
openforce new "分析项目架构并找出安全问题"

# 斜杠指令激活 Skill
openforce /graphify 分析 src 目录
openforce /code-review
openforce /tdd

# 多轮 Session
openforce approve                        # 通过 Gate
openforce reject "加强认证模块"          # 拒绝 + 重规划
openforce continue                       # 恢复最新 Session
openforce skills                         # 列出 83 个 Skill
openforce sessions                       # 列出所有 Session
```

---

## 核心架构

```
用户输入 (语义 or /skill:name)
    │
    ▼
┌──────────────────────────────────────────┐
│  Planner — 语义分类 + RoundTable 分解      │
│  ├── Semantic Classification (LLM)        │
│  ├── Agent Catalog: 198 角色渐进式披露      │
│  ├── Skill Catalog: 83 Skill Level 1 XML   │
│  ├── Round 1: N 维度并行分析               │
│  ├── Round 2: 交叉审查 MECE                │
│  └── Round 3: JSON 合成 → TaskTree         │
└──────────────┬───────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────┐
│  Scheduler — DAG 构建 + Wave 调度          │
│  ├── build_dag(): 依赖推断 + 优先级        │
│  ├── compute_waves(): 拓扑分层             │
│  └── max_concurrent=8: 分片执行            │
└──────────────┬───────────────────────────┘
               │
     ┌─────────┼─────────┐
     ▼         ▼         ▼
┌─────────┐┌─────────┐┌─────────┐
│Worker-1 ││Worker-2 ││Worker-3 │
│Agent:   ││Agent:   ││Agent:   │
│Backend  ││Frontend ││Security │
│Architect││Designer ││Auditor  │
│         ││         ││         │
│Native   ││Native   ││Native   │
│Tool Call││Tool Call││Tool Call│
│Loop     ││Loop     ││Loop     │
└────┬────┘└────┬────┘└────┬────┘
     │         │         │
     └─────────┼─────────┘
               ▼
┌──────────────────────────────────────────┐
│  Redis Session — Todo + Worker Snapshot   │
│  session:{id}:todos — 任务进度追踪         │
│  session:{id}:worker:{wid} — Worker 状态   │
└──────────────────────────────────────────┘
```

## 关键能力

| 能力 | 实现 |
|------|------|
| **Agent 角色库** | 198 角色 (来自 agency-agents)，20 领域，渐进式 XML 披露 |
| **Skill 系统** | 83 Skills，Level 1 XML → Level 2 on-demand |
| **斜杠指令** | `/graphify` `/code-review` `/tdd` 等，精确+模糊匹配 |
| **原生 Tool Calling** | OpenAI Function Calling + Anthropic Tool Use，8 工具 |
| **对话历史** | Worker 多轮累积 user→assistant(tool_calls)→tool_results |
| **RoundTable 规划** | N 维度分析 → 交叉审查 → JSON 合成 |
| **DAG 调度** | 拓扑分层 + 优先级 + max_concurrent=8 |
| **Todo 追踪** | Redis 持久化，[DONE:n] 标记，Resume 支持 |
| **上下文管理** | AgentMemory + Conversation 双重溢出检测 + 结构化压缩 |
| **渐进式披露** | Agent: name+desc → full profile; Skill: name+desc → body+scripts |

## Agent 角色 (198)

| 领域 | 数量 | 示例 |
|------|------|------|
| engineering | 29 | Backend Architect, AI Engineer, Code Reviewer, DevOps Automator |
| marketing | 30 | Content Creator, SEO Specialist, Bilibili Content Strategist |
| specialized | 41 | Blockchain Security Auditor, Compliance Auditor, Customer Service |
| design | 8 | UI Designer, UX Architect, Brand Guardian |
| testing | 8 | API Tester, Accessibility Auditor, Performance Benchmarker |
| sales | 8 | Account Strategist, Sales Engineer, Pipeline Analyst |
| paid-media | 7 | PPC Strategist, Programmatic Buyer, Creative Strategist |
| support | 6 | Legal Compliance Checker, Support Responder |
| spatial-computing | 6 | visionOS Spatial Engineer, XR Immersive Developer |
| project-management | 6 | Project Shepherd, Experiment Tracker |
| product | 5 | Product Manager, Behavioral Nudge Engineer |
| finance | 5 | Financial Analyst, Investment Researcher |
| game-development | 5 | Game Designer, Technical Artist, Narrative Designer |
| academic | 5 | Anthropologist, Historian, Psychologist |

## Worker Tool Calling

每个 Worker 使用 LLM 原生 Function Calling，8 个工具：

| 工具 | 用途 |
|------|------|
| `read_file` | 读取文件 (16K 截断) |
| `write_file` | 写入文件 |
| `shell_exec` | 执行 Shell (30s 超时，危险命令拦截) |
| `mark_all_done` | 标记所有子任务完成 |
| `record_finding` | 记录关键发现 |
| `skill_load` | 渐进式加载 Skill body |
| `skill_load_ref` | 加载 Skill 参考文件 |
| `skill_exec_script` | 执行 Skill 脚本 |

Worker 采用**验收标准驱动**的逐子任务执行：每个 subtask 有明确的 acceptance criterion，Worker 执行完自检，达标 (VERIFIED) 进入下一个，不达标 (FAILED) 最多 8 次重试。criteria met = done，不是死循环。

## Todo 追踪

```
session:{id}:todos → Redis JSON:
  items[] → {id, content, status, priority, agent,
             started_at, completed_at, output_ref}

[DONE:n] 标记: LLM 输出 "[DONE:3]" → 自动标记 todo#3 completed
Resume:     openforce continue → 加载 Redis Todo → 跳过已完成
```

## 项目结构 — 24 Crate / 137 源文件

```
┌──────────────────────────────────────────────────────────┐
│                    【AI 引擎】                             │
│  openforce-cli/  CLI + Planner + AgentRegistry + DAG     │
│  llm-client/     OpenAI + Anthropic Function Calling     │
│  knowledge-base/ 语义分类 (ExpertIndex)                   │
│  skill/          SKILL.md: discovery→parser→executor     │
├──────────────────────────────────────────────────────────┤
│                    【控制面】                              │
│  domain/         Session/Event(30)/Command(15)/Lease     │
│  session-store/  Event Sourcing + CAS + Projection       │
│  scheduler/      DAG + LeaseIssuer + CapabilityToken     │
│  policy-engine/  三层授权 (mTLS+Token+业务)               │
│  redis-session/  Redis Session + TodoList + Snapshot     │
│  proto/          gRPC 服务定义                             │
│  gateway/        REST→gRPC 代理                           │
├──────────────────────────────────────────────────────────┤
│                    【数据面】                               │
│  worker/         criteria-driven tool calling (583行)     │
│  node-daemon/    Worker 生命周期管理                      │
│  cube-sandbox/   Firecracker MicroVM (9 文件)             │
│  space-manager/  三层隔离 + WarmPool                      │
├──────────────────────────────────────────────────────────┤
│                    【安全基础设施】                         │
│  mtls/           Ed25519 CA + SPIFFE + 证书轮换          │
│  path-acl/       路径 ACL + 规范化防遍历                  │
│  patch-classifier/ Patch 风险分级 (9 PCR)                │
│  project-tools/  HITL 审批                               │
│  effect-gateway/ 副作用网关 (幂等 + Outbox)               │
├──────────────────────────────────────────────────────────┤
│                    【进化面 + 多租户 + 监控】               │
│  evolution/      Observer + Evaluator + Evolver          │
│  launch-checker/ 上线验证 (ReleaseGate + RedTeam)         │
│  tenant-governance/ BYOK + 配额 + 留存 + 离场             │
│  tui-dashboard/  Ratatui 终端面板                         │
└──────────────────────────────────────────────────────────┘
```

## 技术栈

| 组件 | 技术 |
|------|------|
| 语言 | Rust 2024 |
| LLM | OpenAI / Anthropic Native Function Calling |
| 存储 | Redis (Session + Todo) |
| 传输 | gRPC (tonic) + HTTP (reqwest) |
