# OpenForce 标准 SKILL 渐进式调用机制

## 现状分析

### 已有实现
1. **Rust 侧 `skill_runner.rs`**: 已实现渐进式发现（frontmatter 解析 + body 按需加载 + adapter.py 工具调用），但：
   - 仅在 `openforce-cli` crate 中，**未被 Worker Agent 使用**
   - 不支持标准 SKILL.md 目录结构（`scripts/`, `references/`, `assets/`）
   - 无 frontmatter 标准验证（allowed-tools, metadata, version, author 等）
   - 无 Skill 启用/禁用配置

2. **Python 侧 DeerFlow**: 完整的 Skill 管理（loader, parser, validation, installer, security_scanner, tool_policy），但：
   - 绑定 DeerFlow 框架，非平台级通用
   - Python 运行时，无法被 Rust Worker 直接调用

3. **Rust Worker (`worker/main.rs`)**: 无任何 Skill 支持，仅有硬编码的 `READ/DONE/FINDING` 工具

4. **Domain/Proto**: 无 Skill 相关领域模型或 gRPC 定义

### 缺口
- **Worker Agent 无 Skill**: Worker 的 LLM 循环中无法触发任何 Skill
- **Planner 部分支持**: `planner_roundtable.rs` 调用了 `SkillRunner::discover("skills")` 但不传递给 Worker
- **无标准目录约定**: 不符合 `.claude/skills/` 或 `skills/` 的社区标准
- **无 Skill 配置层**: 没有 enable/disable、allowed-tools 过滤

## 目标

建立**平台级标准 SKILL 系统**：
1. 任何 Skill 目录放入 `skills/` 即自动生效（零配置热加载）
2. 三层渐进式调用（元数据→指令→附加资源）
3. Worker Agent + CLI + DeerFlow 均可通过统一接口调用
4. 兼容 Claude Code / Open Claw SKILL.md 标准
5. 安全审计 + allowed-tools 过滤

## 架构设计

### Skill 目录标准

```
skills/                           # OpenForce Skill 根目录
├── .skills-config.json           # 可选: Skill 启用/禁用配置
├── deerflow/                     # 现有 DeerFlow Skill
│   ├── SKILL.md                  # 标准格式 (name + description frontmatter)
│   ├── adapter.py                # 可选: 工具适配器
│   ├── scripts/                  # 可选: 可执行脚本
│   ├── references/               # 可选: 参考文档 (按需加载)
│   ├── templates/                # 可选: 模板文件
│   └── assets/                   # 可选: 静态资源
├── code-review/                  # 新增: 代码审查 Skill
│   └── SKILL.md
└── deployment/                   # 新增: 部署 Skill
    ├── SKILL.md
    └── scripts/
        └── deploy.sh
```

### 三层渐进式调用流程

```
Level 1 [启动时] ─→ 扫描 skills/ 下所有 SKILL.md 的 frontmatter
                    仅加载 name + description (约100 token/skill)
                    注入 Planner/Worker 系统提示词

Level 2 [匹配时] ─→ LLM 判断任务匹配某 Skill description
                    读取完整 SKILL.md body (≤5000 token)
                    注入当前对话上下文

Level 3 [执行时] ─→ Skill body 引用 scripts/ 或 references/ 中的文件
                    按需加载指定文件
                    执行 scripts/ 下的可执行文件
```

## 实施任务

### Task 1: 提取 `openforce-skill` 独立 crate

从 `openforce-cli/src/skill_runner.rs` 提取为独立 crate `crates/skill/`:

**文件**: `crates/skill/Cargo.toml`, `crates/skill/src/lib.rs`, `crates/skill/src/discovery.rs`, `crates/skill/src/parser.rs`, `crates/skill/src/config.rs`, `crates/skill/src/executor.rs`

核心类型:
```rust
// crates/skill/src/lib.rs
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    pub allowed_tools: Option<Vec<String>>,   // 新增
    pub version: Option<String>,               // 新增
    pub author: Option<String>,                // 新增
    pub metadata: Option<serde_json::Value>,   // 新增
}

pub struct Skill {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    pub dir: PathBuf,
    pub enabled: bool,                         // 新增
}
```

**discovery.rs**: 扫描 `skills/` 目录，解析 frontmatter，构建 Skill 注册表
**parser.rs**: YAML frontmatter 解析 + body 提取 + 标准字段验证
**config.rs**: `.skills-config.json` 加载，Skill 启用/禁用
**executor.rs**: 按需加载 body、references、执行 scripts、调用 adapter

### Task 2: 重构 `openforce-cli` 使用新 crate

**文件**: `crates/openforce-cli/src/skill_runner.rs` → 替换为 `use openforce_skill::*`
**文件**: `crates/openforce-cli/src/main.rs` → 传递 Skill 上下文给 Worker
**文件**: `crates/openforce-cli/Cargo.toml` → 添加 `openforce-skill` 依赖

关键变更:
- `SkillRunner::discover("skills")` → `SkillRegistry::discover("skills")`
- `skill_summary()` → `registry.metadata_prompt()` (Level 1)
- `load_body()` → `registry.load_skill_body()` (Level 2)
- `invoke_tool()` → `registry.execute_script()` (Level 3)

### Task 3: Worker Agent 集成 Skill 支持

**文件**: `crates/worker/src/main.rs`

变更:
1. WorkerTask 新增 `skill_metadata: String` 字段（Level 1 元数据由 CLI 传入）
2. Worker 的 system prompt 中注入 `skill_metadata`（约 100 token/skill）
3. Agent 执行循环新增 `SKILL: <name>` 动作关键词
4. 匹配到 SKILL 后，Worker 读取完整 SKILL.md body（Level 2）
5. Skill 指令中的 `scripts/` 引用由 Worker 按需执行（Level 3）

```rust
// Worker 执行循环中新增 SKILL 动作
} else if upper.starts_with("SKILL:") {
    let skill_name = last_line.strip_prefix("SKILL:").unwrap_or("").trim();
    if let Some(body) = skill_registry.load_skill_body(skill_name) {
        ctx.push_str(&format!("\n── SKILL: {skill_name} ──\n{body}\n"));
        memory.add_decision(cycles, "SKILL", &format!("loaded {skill_name}"));
    }
}
```

### Task 4: Skill 上下文从 CLI → Worker 传递

**文件**: `crates/openforce-cli/src/main.rs`

变更:
- Worker 构建时，通过 `SkillRegistry::discover()` 获取 Level 1 元数据
- 将 `registry.metadata_prompt()` 写入 WorkerTask 的 `skill_metadata` 字段
- Worker 的 system prompt 中包含 `skill_metadata`

### Task 5: 兼容 DeerFlow Python Skill 适配

**文件**: `skills/deerflow/SKILL.md` → 更新 frontmatter 添加标准字段

不变更:
- DeerFlow Python 层的 `skills/` 模块保持现有功能
- Rust 侧通过 `adapter.py` 调用 Python Skill（Level 3 执行）
- 不需要在 Rust 中重写 DeerFlow 的 Skill 管理

### Task 6: Skill 配置与安全

**文件**: `crates/skill/src/config.rs`

`.skills-config.json` 格式:
```json
{
  "disabled_skills": ["some-risky-skill"],
  "skill_overrides": {
    "deerflow-skill": { "enabled": true, "allowed_tools": ["web_search", "web_fetch"] }
  }
}
```

安全措施:
- `allowed_tools` 前端过滤（兼容 DeerFlow tool_policy）
- scripts/ 执行前校验路径合法性（禁止 `../` 逃逸）
- adapter.py 调用受 allowed_tools 约束

## 文件变更汇总

| 操作 | 文件 |
|------|------|
| 新建 | `crates/skill/Cargo.toml` |
| 新建 | `crates/skill/src/lib.rs` |
| 新建 | `crates/skill/src/discovery.rs` |
| 新建 | `crates/skill/src/parser.rs` |
| 新建 | `crates/skill/src/config.rs` |
| 新建 | `crates/skill/src/executor.rs` |
| 修改 | `Cargo.toml` (workspace 添加 skill crate) |
| 修改 | `crates/openforce-cli/Cargo.toml` |
| 修改 | `crates/openforce-cli/src/main.rs` |
| 修改 | `crates/worker/Cargo.toml` |
| 修改 | `crates/worker/src/main.rs` |
| 删除 | `crates/openforce-cli/src/skill_runner.rs` (逻辑迁移到 skill crate) |
| 修改 | `skills/deerflow/SKILL.md` (添加标准 frontmatter 字段) |

## 验证标准

1. `cargo build` 零错误
2. `cargo test` 全部通过
3. 将新 Skill 目录放入 `skills/` 后自动被发现
4. Worker 执行循环能响应 `SKILL: <name>` 动作
5. `.skills-config.json` 能正确禁用 Skill
6. DeerFlow adapter.py 调用仍正常工作
