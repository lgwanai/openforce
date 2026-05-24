# OpenForce Agent 设计改进方案

> 综合分析 PI、OpenCode、OpenHuman 三个项目的 Agent 设计，对比 OpenForce 当前实现，提供具体修改方案。
> 分析日期: 2026-05-23

---

## 一、OpenForce 当前实现的核心问题

通过代码审查，发现 OpenForce 在任务分解和执行层面存在以下根本性问题：

### 1.1 文本解析式 Tool Calling（最严重问题）

**现状**：Worker 使用纯文本解析来识别 Agent 动作，而非 LLM 原生 function calling：

```rust
// worker/src/main.rs — 用字符串匹配解析动作
if upper.starts_with("DONE ") || upper.starts_with("DONE:") { ... }
else if upper.starts_with("READ:") || upper.starts_with("READ ") { ... }
else if upper.starts_with("SKILL:") || upper.starts_with("SKILL ") { ... }
else if upper.starts_with("SHELL:") || upper.starts_with("SHELL ") { ... }
```

**影响**：
- 动作识别不可靠（LLM 可能不按格式输出）
- 无法利用 LLM 的结构化输出能力
- 无法进行参数校验（类型、必填等）
- 无法利用并行 tool calling
- 错误处理极其脆弱

**对比 OpenCode**：使用 AI SDK `streamText()` + `tool()` 定义，每个工具都有 JSON Schema 定义、参数验证、错误格式化，LLM 原生调用工具，系统只执行。

**对比 PI**：使用 `executeToolCalls()` 支持 sequential/parallel 模式，每个工具有 `execute → prepare → finalize` 生命周期。

### 1.2 任务分解过于简陋

**现状**：RoundTable 使用 3 个固定角色 Agent（交互/数据/实施架构师），各自给一个 prompt 生成候选方案：

```rust
// planner_roundtable.rs
let i_prompt = format!("{base}\n(从交互/用户体验维度分析)");
let d_prompt = format!("{base}\n(从数据/系统集成维度分析)");
let imp_prompt = format!("{base}\n(从实施可行性/时间顺序维度分析)");
```

**影响**：
- 3 个固定维度无法覆盖所有任务类型
- 没有追问机制（发现信息缺口后直接返回 `needs_info` 但无交互循环）
- 分解粒度粗（只有 role + title + objective + steps + acceptance_criteria）
- 没有任务依赖关系的智能推断
- 每个 Agent 只做一轮推理，没有多轮 refinement

**对比 OpenCode**：
- Plan Agent 模式下 LLM 可以自主调用 explore/plan subagents 进行研究
- 5 阶段工作流（Initial Understanding → Design → Review → Final Plan → ExitPlanMode）
- 实验性 plan mode 允许 LLM 写入 plan file 并递进式完善

**对比 PI**：
- Plan Mode 有状态机（Normal ↔ Plan），可切换
- 支持 `/plan toggle` 切换模式
- 计划步骤可追踪（`[DONE:n]` 机制）

### 1.3 Worker 执行循环过于原始

**现状**：Worker 是一个简单的顺序 while 循环：
- 每次只标记一个 subtask 为 in_progress
- 没有并行工具调用
- 没有上下文压缩（只有简单的 de-duplication）
- 没有上下文溢出检测
- 没有智能重试机制
- 文本格式错误会导致卡住

**对比 OpenCode**：
```
runLoop:
  while true:
    1. 过滤已压缩消息
    2. 检查退出条件
    3. 溢出检测 → 自动压缩
    4. 构建 System Prompt（env + instructions + skills）
    5. 解析 Tool → Schema 转换
    6. LLM.stream() → 流式处理
    7. 处理结果（break/continue/compact）
    8. Prune 旧工具输出
```

### 1.4 上下文管理缺失

**现状**：
- Worker 的 AgentMemory 只有简单压缩（merge old decisions, keep top 5 findings）
- 没有 Token 级别的上下文预算管理
- 没有基于模型的 context limit 检测
- 压缩逻辑基于字符数估算（`len() / 4`），不精确
- 没有结构化摘要（Summary Template）

**对比 OpenCode**：
- 三层上下文管理：Overflow 检测 → Compaction 压缩 → Prune 修剪
- Compaction 使用专门的 compaction Agent + LLM 生成结构化摘要
- 7 部分摘要模板（Goal, Constraints, Progress, Key Decisions, Next Steps, Critical Context, Relevant Files）
- Tail 保留策略（默认保留最近 2 个 turn + 25% token budget）

### 1.5 没有 Agent 角色体系

**现状**：Worker 只有一个 `profile_name` 参数，所有任务用同一个 system prompt 模板。

**对比 OpenCode**：
- 8 种内置 Agent（build/plan/explore/general/scout/compaction/title/summary）
- 每个 Agent 有独立权限、独立 System Prompt 文件、独立模式（primary/subagent）
- Agent 可生成（Agent.generate() 通过 LLM 自动创建）
- Agent 权限继承（子 Agent 默认 deny task + deny todowrite）

### 1.6 System Prompt 工程缺失

**现状**：Worker 的 system prompt 是简单的字符串拼接。

**对比 PI**：System Prompt 结构清晰：
1. 角色声明 → 2. 可用工具列表 → 3. Guidelines → 4. 文档索引 → 5. 扩展注入 → 6. 项目上下文 → 7. Skills → 8. 环境信息

**对比 OpenCode**：
- 模型特定的 System Prompt（8 个文件：anthropic.txt, beast.txt, gemini.txt, default.txt 等）
- Instructions 系统分层加载（全局 → 项目 → 配置 URL）
- 动态指令附着（read 文件时自动注入 AGENTS.md/CLAUDE.md）

### 1.7 架构文档与实现脱节

架构文档（`architecture_v5.0_dynamic_orchestration.md`）描述了一个极其完善的分布式系统：
- Event-Sourced Session with append-only event log
- Lease + Fencing Token
- Worker Spec 冻结执行规格
- Effect Gateway
- Three-plane architecture (Control/Data/Evolutionary)
- Kill Switch、熔断、Break-Glass
- 多租户治理

但当前代码实现的是一个**单机 CLI 工具**，几乎没有任何文档描述的高级特性。

---

## 二、三个参考项目的优秀设计提炼

### 2.1 PI 的核心优势

| 特性 | 实现 | 对 OpenForce 的价值 |
|------|------|-------------------|
| **Plan Mode 状态机** | Normal ↔ Plan，工具限制，安全命令检查 | 提供安全规划阶段 |
| **SKILL.md 标准** | frontmatter 验证、名称冲突解决、跨 harness 兼容 | 已有类似实现，可增强 |
| **Prompt Templates** | bash 风格参数替换、自动补全渲染 | 可复用的 prompt 片段 |
| **Progressive Disclosure** | System Prompt 只注名称+描述，完整指令按需加载 | 节省上下文窗口 |
| **Context File 遍历** | AGENTS.md/CLAUDE.md 多层发现 | 项目/全局指令注入 |

### 2.2 OpenCode 的核心优势

| 特性 | 实现 | 对 OpenForce 的价值 |
|------|------|-------------------|
| **Event-Driven Session** | 25 种事件类型 → 消息投影 | 可审计、可恢复 |
| **Agent 系统** | 8 种内置 Agent，权限继承，反嵌套控制 | 多角色协作 |
| **三层上下文管理** | Overflow → Compaction → Prune | 长任务不丢上下文 |
| **子 Agent 委托** | Task Tool + Background Task + 权限继承 | 真正的多 Agent 协作 |
| **Todo 系统** | Session 级任务追踪，持久化 SQLite | 任务进度可视化 |
| **Route 架构** | Protocol + Endpoint + Auth + Transport + Framing | LLM 提供商抽象 |
| **Cache Policy** | 自动缓存断点放置 | 降低 API 成本 |
| **模型特化 Prompt** | 8 个模型专用 system prompt 文件 | 更好的模型适配 |

### 2.3 OpenHuman 的核心优势

| 特性 | 实现 | 对 OpenForce 的价值 |
|------|------|-------------------|
| **Memory 模块** | Embedding 语义搜索 | 长期记忆检索 |
| **Subconscious** | 后台思考/处理 | 持续性 Agent 能力 |
| **Learning 模块** | 经验积累和模式提取 | 自我改进 |
| **Tree Summarizer** | 树形上下文压缩 | 更好的压缩策略 |
| **Safety/Security** | 独立安全模块 | 内容/操作安全检查 |
| **多 Channel** | Discord/Telegram/WhatsApp | 多渠道接入 |

---

## 三、改进方案（分阶段实施）

### Phase 1：修复核心执行缺陷（1-2 周）

#### 3.1.1 改造 Worker 为原生 Tool Calling

**当前问题**：文本解析式动作识别。

**改造方案**：
```
Worker 改造为使用 LLM Native Function Calling:

1. 定义 Tool Schema（Rust struct → JSON Schema）：
   - read_file(path: String) → String
   - write_file(path: String, content: String)
   - shell_exec(command: String, workdir?: String) → ShellResult
   - skill_invoke(name: String, params?: Object)
   - task_done(subtask_id: u32, result: String)
   - task_all_done()
   - record_finding(text: String)

2. LLMClient 增加 tool calling 支持：
   - OpenAI: tools 参数 + tool_choice
   - Anthropic: tools 参数

3. Worker Loop 改造：
   loop:
     response = llm.chat_with_tools(system, messages, tools)
     if response.tool_calls:
       for call in response.tool_calls:
         result = execute_tool(call)
         messages.push(ToolResult { call_id, result })
       continue
     if response.finish_reason == "stop":
       break
```

**关键文件改造**：
- `crates/llm-client/src/unified.rs` — 增加 `chat_with_tools()` 方法
- `crates/llm-client/src/openai.rs` — 支持 tools 参数
- `crates/llm-client/src/anthropic.rs` — 支持 tools 参数
- `crates/worker/src/main.rs` — 重写执行循环

#### 3.1.2 增加上下文窗口管理

**方案**（借鉴 OpenCode）：
```rust
// 新增 crate: crates/context-manager/

struct ContextManager {
    model_limit: usize,         // 模型上下文限制
    reserved_tokens: usize,     // 保留给输出的 token
    tail_turns: usize,          // 保留最近 N 个 turn
    preserve_recent_tokens: usize,
}

impl ContextManager {
    // 检测是否溢出
    fn is_overflow(&self, total_tokens: usize) -> bool {
        let usable = self.model_limit - self.reserved_tokens;
        total_tokens >= usable
    }

    // 选择要压缩的消息范围
    fn select_compaction_target(&self, messages: &[Message]) -> (Vec<Message>, Vec<Message>) {
        // head: 要压缩的, tail: 保留的
    }
}
```

#### 3.1.3 实现结构化摘要（Compaction）

**模板**（借鉴 OpenCode 的 SUMMARY_TEMPLATE）：
```markdown
## Goal
- [单句任务摘要]

## Progress
### Done
- [已完成的任务]
### In Progress
- [正在进行的任务]
### Blocked
- [阻塞项]

## Key Decisions
- [决策及原因]

## Next Steps
- [下一步行动]

## Critical Context
- [重要技术事实、错误信息、文件路径]
```

---

### Phase 2：增强任务分解能力（2-3 周）

#### 3.2.1 改造 Planner 为多轮交互式

**方案**：
```
改进后的 Planner 工作流：

Phase 1: 信息收集
  - Planner Agent 分析用户输入
  - 识别信息缺口
  - 通过 AskUserQuestion tool 与用户交互
  - 通过 Explore Agent 研究代码库

Phase 2: 任务分解（多 Agent 并行）
  - 根据任务类型动态选择分析维度
  - 后台任务识别（独立的 Agent 识别哪些可以后台执行）
  - 每个 Agent 可调用 explore subagent 深入研究

Phase 3: DAG 构建
  - 依赖关系推断（基于文件、接口、数据流）
  - 并行度优化
  - 关键路径识别

Phase 4: Review & Refine
  - 交叉验证 Agent 审查计划
  - 用户确认
```

#### 3.2.2 引入 Plan Mode

**方案**（借鉴 PI + OpenCode）：
```
Plan Mode:
  - 只读工具：read, grep, glob, ls, ask
  - 禁止：write, edit, shell(mutating), task(delegation)
  - 输出：plan.md 文件
  - 需要用户批准后才切换到 Build Mode
  - Plan Mode 下的 shell 命令安全检查（借鉴 PI 的 22 个破坏性模式 + 47 个安全模式白名单）
```

#### 3.2.3 任务粒度控制

**方案**：
```
任务分解原则：
  - 每个子任务必须在 1 个 Worker 的能力范围内
  - 每个子任务必须有明确的输入/输出
  - 每个子任务必须有可验证的验收标准
  - 支持任务拆分（大任务 → 小任务）
  - 支持任务合并（过度拆分 → 合并）

借鉴 OpenCode Todo 系统：
  - pending → in_progress → completed
  - 同时只有 1 个 in_progress（或按并行度限制）
  - completed 需要实际验证，不是 LLM 自己声明
```

---

### Phase 3：多 Agent 协作体系（3-4 周）

#### 3.3.1 建立 Agent 角色体系

**方案**（借鉴 OpenCode）：
```rust
// crates/domain/src/agent.rs

struct AgentInfo {
    name: String,              // 唯一标识
    description: String,       // 用途描述（用于 LLM 选择）
    mode: AgentMode,           // Primary / Subagent
    system_prompt: String,     // Agent 专属 System Prompt
    allowed_tools: Vec<String>, // 允许的工具列表
    denied_tools: Vec<String>,  // 禁止的工具列表
    max_steps: usize,          // 最大步数限制
    model: Option<ModelRef>,   // 可选专属模型
}

enum AgentMode {
    Primary,   // 主 Agent，可与用户交互
    Subagent,  // 子 Agent，只能被委托
}

// 内置 Agent 列表
const BUILTIN_AGENTS: &[AgentInfo] = &[
    // build — 默认主 Agent，完整权限
    // plan — 只读规划 Agent
    // explore — 代码探索子 Agent
    // review — 代码审查子 Agent
    // test — 测试子 Agent
    // fix — Bug 修复子 Agent
];
```

#### 3.3.2 子 Agent 委托机制

**方案**（借鉴 OpenCode Task Tool）：
```rust
// Task Tool 参数
struct TaskToolParams {
    description: String,       // 3-5 words
    prompt: String,            // 子 Agent 完整指令
    subagent_type: String,     // Agent 类型名
    task_id: Option<String>,   // 可选：恢复已有子任务
    background: bool,          // 是否后台执行
}

// 执行流程
async fn execute_subagent(params: TaskToolParams) -> Result<TaskResult> {
    // 1. 权限检查
    // 2. 验证 Agent 存在
    // 3. 创建子 Session（parent_id = current_session）
    // 4. 权限继承（Plan Mode 穿透、反嵌套）
    // 5. 运行子 Agent
    // 6. 返回结果
}
```

#### 3.3.3 并发 Worker 执行

**方案**：
```
当前：DAG 计算 wave 后顺序执行每个 wave 内的任务
改进：
  1. Wave 内任务并行执行（每个任务独立 Worker）
  2. Worker Pool 管理（限制并发数）
  3. 共享文件通过 Patch 协议提交（Worker 不直接写共享文件）
  4. Merge Service 处理并发写入冲突
```

---

### Phase 4：生产级基础设施（4-8 周）

#### 3.4.1 Event-Sourced Session

**方案**（执行架构文档的设计）：
```
Session Store 改造：
  - 所有状态变更为 Event（append-only）
  - Session 状态通过 Event Projection 重建
  - 支持时间旅行审计
  - 支持故障恢复

核心事件类型：
  - SessionCreated, PlanProposed, PlanCompiled
  - TaskLeased, TaskStarted, HeartbeatReceived
  - ArtifactSubmitted, PatchSubmitted, FindingSubmitted
  - TaskSucceeded, TaskFailed, TaskTimedOut
  - SessionCompleted, SessionAborted
```

#### 3.4.2 System Prompt 工程化

**方案**（借鉴 PI + OpenCode）：
```
1. 模型特定 Prompt 文件：
   prompts/
   ├── anthropic.txt    — Claude 专用
   ├── openai.txt       — GPT 专用
   ├── gemini.txt       — Gemini 专用
   └── default.txt      — 通用默认

2. Instructions 加载链：
   全局 (~/.openforce/AGENTS.md)
   → 项目 (AGENTS.md / CLAUDE.md 向上遍历)
   → 配置 URL

3. System Prompt 模板引擎：
   支持变量替换 ($MODEL, $DATE, $CWD, $SKILLS)
   支持条件块 (if model == "claude" { ... })
```

#### 3.4.3 LLM Provider 抽象层

**方案**（借鉴 OpenCode Route 架构）：
```
Route = Protocol + Endpoint + Auth + Transport + Framing

统一抽象：
trait LlmProtocol {
    fn build_request(&self, messages: &[Message], tools: &[Tool]) -> Request;
    fn parse_stream_event(&self, event: &Event) -> LlmEvent;
    fn map_finish_reason(&self, reason: &str) -> FinishReason;
}

支持 Provider：
  - OpenAI (Chat Completions + Responses)
  - Anthropic (Messages)
  - Google (Gemini)
  - AWS Bedrock
  - Azure OpenAI
  - 兼容 OpenAI 协议的第三方
```

---

## 四、关键对比总结

| 维度 | OpenForce 当前 | PI | OpenCode | OpenHuman | 改进优先级 |
|------|-------------|-----|----------|-----------|----------|
| **Tool Calling** | 文本解析 | ✅ Native | ✅ Native + Schema | ✅ Native | **P0** |
| **任务分解** | 3 Agent 一轮 | ✅ Plan Mode | ✅ 5-Phase Plan | ✅ Agent模块 | **P0** |
| **上下文管理** | 简单压缩 | ✅ Compaction | ✅ 3层(Overflow/Compaction/Prune) | ✅ Tree Summarizer | **P0** |
| **Agent 体系** | 无 | ✅ 基础 | ✅ 8种Agent+权限继承 | ✅ Agent模块 | **P1** |
| **多 Agent 协作** | 顺序Wave | ❌ | ✅ Task Tool+Background | ✅ Team模块 | **P1** |
| **System Prompt** | 字符串拼接 | ✅ 结构化组装 | ✅ 模型特化+分层加载 | ✅ 模块化 | **P1** |
| **Plan Mode** | 无 | ✅ 状态机+安全检查 | ✅ Agent切换+权限控制 | ❌ | **P1** |
| **Skill 系统** | 基础3层加载 | ✅ 完整验证+冲突解决 | ✅ Progressive Disclosure | ✅ Skills模块 | P2 |
| **Event Sourcing** | JSON文件 | ❌ | ✅ 25种事件类型 | ❌ | P2 |
| **LLM Provider抽象** | 简单Client | ✅ 多Provider | ✅ Route架构 | ✅ Providers模块 | P2 |
| **权限系统** | 无 | ✅ Bash安全检查 | ✅ Session级Rule | ✅ Safety模块 | P2 |
| **缓存策略** | 无 | ❌ | ✅ 自动Cache Point | ❌ | P3 |
| **后台任务** | 无 | ❌ | ✅ BackgroundJob+auto-resume | ✅ Subconscious | P3 |
| **记忆系统** | 无 | ✅ 文件记忆 | ✅ 3层记忆 | ✅ Memory+Embeddings | P3 |

---

## 五、推荐实施路线图

```
Week 1-2:  [P0] Worker Native Tool Calling + 上下文管理
Week 3-4:  [P0] 结构化 Compaction + Planner 多轮交互
Week 5-6:  [P1] Agent 角色体系 + Plan Mode
Week 7-8:  [P1] 子 Agent 委托 + 并发 Worker
Week 9-10: [P1] System Prompt 工程化 + 模型特化
Week 11-12:[P2] Event-Sourced Session
Week 13-14:[P2] LLM Provider Route 架构
Week 15-16:[P2] 权限系统 + 安全模块
Week 17+:  [P3] 后台任务 + 记忆系统 + 缓存策略
```

---

## 六、核心设计原则（来自三个项目的共识）

1. **渐进式信息披露 (Progressive Disclosure)**：System Prompt 只包含 Skill 名称+描述，完整指令按需加载。PI 和 OpenCode 都采用此模式。

2. **LLM 建议，系统决策**：LLM 提出计划和候选方案，确定性系统组件（Scheduler）负责推进状态机。Planner 只产出候选方案，Scheduler 编译并执行。

3. **只读规划，可写执行**：规划阶段工具受限（只读），用户批准后才切换到执行模式。PI 和 OpenCode 都有独立的 Plan Mode。

4. **权限继承和边界控制**：子 Agent 必须继承父 Agent 的安全约束（Plan Mode 穿透），默认禁止嵌套委托。OpenCode 的 `deriveSubagentSessionPermission` 是最佳实践。

5. **事件溯源作为真相源**：所有状态变更必须可追溯、可回放。OpenCode 的 25 种 Session Event 和 OpenForce 架构文档中描述的 Event Log 都指向这个方向。

6. **上下文管理三层策略**：Overflow 检测 → Compaction 压缩 → Prune 修剪。OpenCode 的实现最完善。

7. **模型特化 Prompt**：不同模型需要不同的 System Prompt。OpenCode 有 8 个模型专用 prompt 文件，这是提升效果的关键。

8. **冻结执行上下文**：任务一旦开始，其 Prompt、Tool Policy、Model 配置必须冻结。OpenForce 架构文档的 Worker Spec 是这个思想的完整表达。
