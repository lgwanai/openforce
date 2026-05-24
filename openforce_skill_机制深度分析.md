# OpenForce SKILL 机制深度分析

> 研究日期: 2026-05-22 | 分支: master | 基于实际代码分析

---

## 1. 架构总览

OpenForce 的 SKILL 机制是一个**三层渐进式披露（Progressive Disclosure）系统**，兼容 Claude Code / Open Claw 的 SKILL.md 规范。整个机制横跨 Rust 和 Python 两套子系统，通过 CLI → Worker 的数据通道实现端到端的技能注入和执行。

```
┌─────────────────────────────────────────────────────────────────┐
│                        skills/ 目录                              │
│  ├── deerflow/                                                   │
│  │   ├── SKILL.md          ← 技能定义（frontmatter + body）       │
│  │   ├── adapter.py        ← 工具适配器（stdin/stdout JSON）      │
│  │   ├── scripts/          ← 可执行脚本                           │
│  │   ├── references/       ← 参考文档                             │
│  │   └── ...                                                     │
│  ├── .skills-config.json   ← 可选：启用/禁用配置                  │
│  └── <new-skill>/          ← 放入即生效（零配置热加载）            │
│      └── SKILL.md                                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│               openforce-skill crate (crates/skill/)              │
│                                                                   │
│  discovery.rs  ──→ 扫描 skills/ 下所有 SKILL.md                   │
│  parser.rs     ──→ 解析 YAML frontmatter + body                   │
│  config.rs     ──→ 加载 .skills-config.json 启用/禁用             │
│  executor.rs   ──→ Level 3: 执行脚本、加载引用、调用 adapter      │
└─────────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
┌──────────────────────────┐ ┌──────────────────────────┐
│   CLI (openforce-cli)    │ │   Worker (openforce-worker)│
│                          │ │                            │
│  SkillRunner::discover() │ │  SkillRegistry::discover() │
│       ↓                  │ │       ↓                    │
│  metadata_prompt()       │ │  load_skill_body()  L2    │
│       ↓                  │ │  execute_script()   L3    │
│  传递给 Worker            │ │  invoke_tool()      L3    │
│  (通过 JSON task file)    │ │  load_reference()   L3    │
└──────────────────────────┘ └──────────────────────────┘
```

---

## 2. 三层渐进式加载机制

### Level 1 — 启动时（~100 token/skill）

**触发时机**: CLI 启动时，`SkillRunner::discover("skills")` 扫描目录

**加载内容**: 仅解析 SKILL.md 的 YAML frontmatter 中的 `name` 和 `description`

**注入位置**:
- **Planner RoundTable**: `planner_roundtable.rs:51` — 注入到三个规划代理（交互/数据/实施）的提示词中
- **Worker System Prompt**: `worker/main.rs:211-212` — 注入到 Worker Agent 的系统提示词

**注入格式**:
```
AVAILABLE SKILLS:
  deerflow-skill — DeerFlow agent orchestration for complex tasks...
  → If relevant to your task, use SKILL: <name> to load its full instructions.
```

**代码路径**:
```
discovery.rs:87-101  → metadata_prompt() 生成 Level 1 元数据字符串
main.rs:193-194      → CLI 调用 discover() 获取 skill_metadata
main.rs:446-462      → 通过 JSON task file 传递给 Worker
worker/main.rs:218   → Worker 从 task.skill_metadata 读取
worker/main.rs:396   → 注入到每个 Agent 循环的 execute_prompt 中
```

### Level 2 — 匹配时（完整 body）

**触发时机**: Worker Agent 在执行循环中输出 `SKILL: <name>` 动作

**加载内容**: SKILL.md 的完整 body（frontmatter 之后的所有 Markdown 内容）

**代码路径**:
```
worker/main.rs:491-510  → Agent 循环检测 SKILL: 关键字
discovery.rs:114-122    → load_skill_body() 从 HashMap 返回完整 body
worker/main.rs:503      → 将 body 注入 active_file_content，带入下一轮 LLM 上下文
```

**Worker 处理逻辑** (worker/main.rs:491-510):
```rust
} else if upper.starts_with("SKILL:") || upper.starts_with("SKILL ") {
    let skill_name = last_line.strip_prefix("SKILL:").or(...).trim();
    if let Some(body) = skill_registry.load_skill_body(skill_name) {
        active_file_content = Some((
            format!("SKILL:{skill_name}"),
            format!("{}{dirs_info}", body)
        ));
    }
}
```

### Level 3 — 执行时（按需加载）

**三种操作**:

| 操作 | 指令格式 | 功能 | 代码位置 |
|------|---------|------|---------|
| LOAD_REF | `LOAD_REF: <skill>/<path>` | 加载 references/templates/assets 下的文件 | worker:511-533, executor:18-54 |
| EXEC_SCRIPT | `EXEC_SCRIPT: <skill>/scripts/<path> [args]` | 执行技能脚本（.py→python3, .sh→bash, .js→node） | worker:534-561, executor:58-139 |
| INVOKE_TOOL | `INVOKE_TOOL: <skill>/<tool> {json}` | 调用 adapter.py 工具 | worker:562-590, executor:145-229 |

**安全措施**:
- 路径遍历防护（canonicalize 检查）
- allowed-tools 白名单过滤
- 脚本输出截断为 4000 字符
- 仅允许 `scripts/`, `references/`, `templates/`, `assets/` 子目录

---

## 3. 核心组件

### 3.1 `openforce-skill` crate (`crates/skill/`)

独立 Rust crate，是 SKILL 系统的核心基础设施。

| 文件 | 职责 | 关键方法 |
|------|------|---------|
| `lib.rs` | 核心类型定义 | `SkillFrontmatter`, `Skill` |
| `discovery.rs` | 目录扫描、注册表 | `discover()`, `metadata_prompt()`, `load_skill_body()` |
| `parser.rs` | YAML frontmatter 解析 | `parse_frontmatter()`, `extract_body()` |
| `config.rs` | 配置管理 | `load_from_skills_dir()`, `is_enabled()` |
| `executor.rs` | Level 3 执行 | `load_reference()`, `execute_script()`, `invoke_tool()`, `resolve_search()` |

**核心类型**:
```rust
pub struct SkillFrontmatter {
    pub name: String,                          // 必需，≤64 字符
    pub description: String,                   // 必需，≤1024 字符
    pub allowed_tools: Option<Vec<String>>,    // 可选工具白名单
    pub version: Option<String>,
    pub author: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub license: Option<String>,
}

pub struct Skill {
    pub frontmatter: SkillFrontmatter,
    pub body: String,        // SKILL.md body（Markdown）
    pub dir: PathBuf,        // 技能目录绝对路径
    pub enabled: bool,       // 是否启用
}
```

**发现逻辑** (`walk_skills_dir`):
1. 扫描 `skills/` 下的所有顶级子目录
2. 跳过隐藏目录（`.` 开头）
3. 检查目录是否包含 `SKILL.md`
4. 也检查一级嵌套子目录
5. 解析 frontmatter 并构建 `HashMap<String, Skill>`

**配置系统** (`.skills-config.json`):
```json
{
  "disabled_skills": ["risky-skill"],
  "skill_overrides": {
    "deerflow-skill": {
      "enabled": true,
      "allowed_tools": ["web_search", "web_fetch"]
    }
  }
}
```
优先级: skill_overrides > disabled_skills > 默认启用

### 3.2 CLI 集成 (`crates/openforce-cli/`)

**skill_runner.rs** — 向后兼容包装器，对 `openforce-skill` crate 的薄封装。

**main.rs 传递流程**:
1. `SkillRunner::discover("skills")` → 获取 metadata
2. `skill_metadata` 字符串写入 Worker 的 JSON task file (第 461 行)
3. Worker 进程启动时从 task file 读取并初始化自己的 SkillRegistry

**planner_roundtable.rs 使用**:
- Round 1: 三个规划代理的提示词包含 skill metadata（第 51 行）
- 检测到 `[需网络检索: ...]` 时，调用 `resolve_search()` 执行实时搜索（第 84-91 行）
- 搜索结果反馈到 Round 2 审查和 Round 3 综合

### 3.3 Worker 集成 (`crates/worker/src/main.rs`)

**初始化** (第 211-227 行):
```rust
let skill_config = SkillConfig::load_from_skills_dir(skills_dir);
let skill_registry = SkillRegistry::discover_with_config(skills_dir, Some(&skill_config));
let skill_executor = SkillExecutor::new(&skill_registry);
```

**Agent 执行循环** (第 286-669 行) — 每轮检查 LLM 响应的最后一行:

```
SKILL: <name>           → Level 2 加载 skill body
LOAD_REF: <s>/<path>    → Level 3 加载引用文件
EXEC_SCRIPT: <s>/<path> → Level 3 执行脚本
INVOKE_TOOL: <s>/<tool> → Level 3 调用 adapter 工具
READ: <file>            → 读取项目文件
DONE <id>: <result>     → 标记子任务完成
ALL_DONE                → 全部完成
```

**WorkerTask 输入结构** (第 140-159 行):
```rust
struct WorkerTask {
    // ...
    skill_metadata: Option<String>,  // Level 1 元数据（由 CLI 传入）
    skills_dir: Option<String>,      // skills 目录路径
}
```

### 3.4 Python (DeerFlow) 子系统

位于 `skills/deerflow/deerflow/skills/`，是更丰富的技能管理系统：

| 模块 | 功能 |
|------|------|
| `types.py` | `Skill`, `SkillCategory` 数据类 |
| `parser.py` | pyyaml 解析 SKILL.md |
| `loader.py` | 扫描 public/custom 目录 |
| `manager.py` | CRUD、原子写入、历史记录 |
| `installer.py` | .skill ZIP 安装、安全检查 |
| `validation.py` | 严格字段验证 |
| `security_scanner.py` | LLM 驱动安全审核 |
| `tool_policy.py` | allowed-tools 合并与过滤 |

**注意**: DeerFlow 的 skills 子系统和 Rust `openforce-skill` crate 是**两套独立实现**，通过 `adapter.py` 的 stdin/stdout JSON 协议桥接。

---

## 4. Skill 选择机制分析

### 4.1 当前选择方式：LLM 语义理解

目前 skill 选择**完全依赖 LLM 的语义理解**：

1. Worker 的 system prompt 和 execute_prompt 中注入 skill metadata
2. LLM 读取 skill 的 `name` 和 `description`
3. LLM 判断任务是否与 skill 描述匹配
4. 如果匹配，LLM 在响应最后一行输出 `SKILL: <name>`
5. Worker 解析该指令并加载 skill body

**Prompt 注入格式**:
```
AVAILABLE SKILLS:
  deerflow-skill — DeerFlow agent orchestration for complex tasks requiring
  multi-step reasoning, web search, tool orchestration, or parallel subagent
  delegation.
  → If relevant to your task, use SKILL: <name> to load its full instructions.
```

### 4.2 Planner 中的技能感知

```
RoundTable (3 agents × 3 rounds)
  │
  ├── Round 1: 每个规划代理收到 skill metadata
  │   - 交互架构师: 从 UX 维度分析
  │   - 数据架构师: 从数据/集成维度分析
  │   - 实施架构师: 从可行性/时序维度分析
  │   ↓
  │   检测 [需网络检索: ...] → resolve_search() 调用 deerflow-skill
  │
  ├── Round 2: 交叉审查（含实时搜索结果）
  │
  └── Round 3: 输出 TaskTree (JSON)
      └── DecomposedTask { role, title, objective, files, steps, acceptance_criteria, dependencies }
```

**关键问题**: `DecomposedTask` 结构体中没有 `skill` 字段，Planner 不会将匹配的技能绑定到具体子任务。

### 4.3 Worker 中的技能使用

Worker 在每个执行周期都注入 skill metadata，但**不会主动选择技能**——完全等待 LLM 输出 `SKILL:` 指令。

---

## 5. 能力矩阵：已实现 vs 期望

### 5.1 已实现 ✅

| 能力 | 状态 | 说明 |
|------|------|------|
| 零配置热加载 | ✅ | 放入 `skills/` 目录即自动发现 |
| 三层渐进式加载 | ✅ | Level 1 (meta) → Level 2 (body) → Level 3 (execute) |
| Planner 感知技能 | ✅ | RoundTable 提示词包含 skill metadata |
| Worker 执行技能 | ✅ | SKILL/LOAD_REF/EXEC_SCRIPT/INVOKE_TOOL 完整支持 |
| 配置启用/禁用 | ✅ | `.skills-config.json` |
| allowed-tools 过滤 | ✅ | frontmatter + config override |
| 路径安全防护 | ✅ | canonicalize 防遍历 |
| adapter.py 协议 | ✅ | stdin/stdout JSON |
| 兼容 Claude Code 规范 | ✅ | SKILL.md frontmatter 格式 |

### 5.2 未实现 ❌ — 阻止"自动选择"的关键缺口

| 缺口 | 严重度 | 说明 |
|------|--------|------|
| **无技能-任务绑定** | 🔴 高 | `DecomposedTask` 没有 `skill` 字段，Planner 分解任务后不绑定技能 |
| **纯被动选择** | 🔴 高 | Worker 不主动加载技能，完全等 LLM 手动输出 `SKILL:` |
| **无程序化匹配** | 🟡 中 | 没有 embedding 或关键词匹配作为 LLM 的辅助 |
| **无技能优先级** | 🟡 中 | 多技能可能适用同一任务时无选择策略 |
| **Python/Rust 技能系统割裂** | 🟡 中 | DeerFlow 的 Python skills 目录和 Rust `skills/` 是两套独立系统 |

---

## 6. 改进方案：实现真正的语义自动选择

### 6.1 高优先级：Planner 自动绑定技能到任务

**目标**: Planner 在任务分解时，根据语义自动匹配技能并绑定到 `DecomposedTask`。

**步骤 1**: `DecomposedTask` 新增 `skill` 字段:
```rust
pub struct DecomposedTask {
    // ... 现有字段
    pub skill: Option<String>,  // 绑定的技能名称
}
```

**步骤 2**: Planner 的 Round 1 prompt 增加技能匹配指令:
```
对于每个子任务，判断是否需要使用 AVAILABLE SKILLS 中的技能：
- 分析子任务的核心需求（需要网络搜索？多步推理？工具编排？）
- 匹配最合适的技能（根据 description 语义判断）
- 在输出 JSON 中添加 "skill": "<name>" 或 "skill": null
```

**步骤 3**: Worker 收到绑定了 skill 的任务时，在首轮自动加载:
```rust
if let Some(ref skill_name) = current_subtask.skill {
    if let Some(body) = skill_registry.load_skill_body(skill_name) {
        active_file_content = Some((format!("SKILL:{skill_name}"), body));
    }
}
```

### 6.2 高优先级：Worker 主动技能加载

**当前流程**:
```
LLM 看到 task → LLM 看到 skill list → LLM 决定是否用 → 输出 SKILL: xxx
```

**改进流程**:
```
Worker 收到 task + bound_skill → Worker 主动加载 skill body → LLM 直接使用 skill 指令
```

优势:
- 减少一轮 LLM 交互（不需要 LLM 先输出 SKILL: 指令）
- 确定性更强（不依赖 LLM 是否注意到 skill）
- LLM 可以在 skill 指令的指导下更高效地执行

### 6.3 中优先级：程序化 Skill Matcher

作为 LLM 语义匹配的辅助，提供确定性的 fallback:

```rust
pub struct SkillMatcher;

impl SkillMatcher {
    /// 根据任务描述推荐合适的技能（关键词 + 规则）
    pub fn recommend(task: &str, registry: &SkillRegistry) -> Vec<String> {
        registry.enabled_skill_names()
            .into_iter()
            .filter(|name| {
                let skill = registry.get(name);
                // 简单的关键词匹配作为 baseline
                skill.map(|s| task_contains_keywords(task, &s.frontmatter.description))
                    .unwrap_or(false)
            })
            .collect()
    }
}
```

---

## 7. 完整的 Skill 生命周期（改进后）

```
1. 开发 Skill
   创建 skills/<name>/SKILL.md
   ├── frontmatter: name, description, allowed-tools, ...
   ├── body: 给 LLM 的指令
   ├── adapter.py (可选)
   ├── scripts/ (可选)
   └── references/ (可选)

2. 自动发现 (Level 1)
   CLI 启动 → SkillRegistry::discover("skills")
   → 扫描所有 SKILL.md → 解析 frontmatter → 构建 HashMap
   → 生成 metadata_prompt() → 注入 Planner + Worker

3. 语义匹配 (Planner) ← 【改进点】
   RoundTable 3 agents 读取 skill metadata
   → 分析子任务需求，语义匹配技能
   → 将匹配的技能绑定到 DecomposedTask.skill

4. 主动加载 (Worker) ← 【改进点】
   Worker 收到任务 → 检查 bound_skill
   → 主动加载 skill body 到上下文
   → LLM 直接基于 skill 指令工作

5. 技能执行 (Level 3)
   LLM 按 skill body 指令行动:
   → LOAD_REF / EXEC_SCRIPT / INVOKE_TOOL

6. 配置管理
   .skills-config.json + allowed-tools 控制权限
```

---

## 8. 关键文件索引

### Rust 核心 crate
| 文件 | 说明 |
|------|------|
| `crates/skill/src/lib.rs` | SkillFrontmatter, Skill 类型定义 |
| `crates/skill/src/discovery.rs` | SkillRegistry, walk_skills_dir, metadata_prompt |
| `crates/skill/src/parser.rs` | YAML frontmatter 手动解析器 |
| `crates/skill/src/config.rs` | SkillConfig, .skills-config.json 加载 |
| `crates/skill/src/executor.rs` | SkillExecutor, 所有 Level 3 操作 |

### CLI 集成
| 文件 | 说明 |
|------|------|
| `crates/openforce-cli/src/skill_runner.rs` | 向后兼容包装器 |
| `crates/openforce-cli/src/main.rs:190-197, 446-462` | Skill 发现 + 传递给 Worker |
| `crates/openforce-cli/src/planner_roundtable.rs:51, 84-91` | Planner 技能使用 |

### Worker 集成
| 文件 | 说明 |
|------|------|
| `crates/worker/src/main.rs:211-227` | Worker 初始化 skill 组件 |
| `crates/worker/src/main.rs:396-410` | 注入 skill_metadata + 动作列表 |
| `crates/worker/src/main.rs:491-590` | SKILL/LOAD_REF/EXEC_SCRIPT/INVOKE_TOOL 处理 |

### 技能定义
| 文件 | 说明 |
|------|------|
| `skills/deerflow/SKILL.md` | deerflow-skill 定义 |
| `skills/deerflow/adapter.py` | Tavily + Jina 适配器 |

---

## 9. 总结

### "放入即生效" — 已实现 ✅

将 `SKILL.md` 放入 `skills/<name>/` 目录，启动时自动被发现，skill metadata 自动注入 Planner 和 Worker 的提示词。这是通过 `walk_skills_dir()` 启动时扫描 + `metadata_prompt()` 注入实现的。

### "语义理解后自动选择" — 部分实现 ⚠️

**已实现的部分**: LLM 确实通过读取 skill description 进行语义理解和选择，在 Worker 循环中输出 `SKILL:` 指令触发加载。

**未实现的部分**:
1. Planner 分解任务后**不绑定**技能到具体子任务（`DecomposedTask` 无 `skill` 字段）
2. Worker **不主动**加载技能，完全被动等待 LLM 手动输出 `SKILL:` 指令
3. 没有程序化的技能匹配作为辅助

### 最小改动方案

要让 "planner 和 worker 都能根据实际任务语义理解后自动选择合适的 skill 执行"，最少需要两步改动：

1. **`planner_roundtable.rs`**: 在 Round 1 prompt 中增加技能匹配思考步骤，在输出 JSON 格式中增加 `skill` 字段
2. **`worker/src/main.rs`**: 在开始执行子任务时，检查 `bound_skill` 并主动调用 `load_skill_body()`

这两处改动触及的核心代码不超过 50 行，但能从根本上改变 skill 选择从"被动"到"主动"的模式。

---

*本文档基于 openforce master 分支 (commit: 43edc59) 的实际代码分析生成。*
