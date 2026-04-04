# Moltis 项目架构深度分析

**项目地址**: https://github.com/moltis-org/moltis

---

## 1. 项目整体结构

Moltis 是一个 Rust 工作区项目，采用分层架构设计：

```
┌────────────────────────────────────────────────────────────────────┐
│                          应用层 (Applications)                       │
│  apps/courier - 主应用入口                                          │
│  apps/macos, apps/ios - 原生移动端应用                              │
├────────────────────────────────────────────────────────────────────┤
│                         网关层 (Gateway)                             │
│  crates/gateway - 核心业务逻辑、协议分发、状态管理                    │
│  crates/protocol - WebSocket/RPC 协议定义 (v4)                      │
├────────────────────────────────────────────────────────────────────┤
│                        Agent 核心 (Agents)                           │
│  crates/agents - LLM 运行时、模型选择、Prompt 构建、工具执行          │
│  crates/chat - Chat 执行引擎                                        │
│  crates/tools - 工具实现和策略执行                                   │
│  crates/skills - 技能系统                                           │
├────────────────────────────────────────────────────────────────────┤
│                        提供者层 (Providers)                          │
│  crates/providers - LLM 提供者实现 (Anthropic, OpenAI, Ollama等)    │
├────────────────────────────────────────────────────────────────────┤
│                        支撑层 (Infrastructure)                       │
│  crates/sessions - 会话存储 (JSONL)                                 │
│  crates/memory - 长期记忆系统                                        │
│  crates/config - 配置管理                                           │
│  crates/common - 共享类型和 Hook 系统                               │
└────────────────────────────────────────────────────────────────────┘
```

---

## 2. Agent Loop 核心实现

### 2.1 入口函数

Agent Loop 的核心实现位于 `crates/agents/src/runner.rs`（约 6200+ 行）：

```rust
// 主要入口点
pub async fn run_agent_loop(
    provider: Arc<dyn LlmProvider>,
    tools: &ToolRegistry,
    system_prompt: &str,
    user_content: &UserContent,
    on_event: Option<&OnEvent>,
    history: Option<Vec<ChatMessage>>,
) -> Result<AgentRunResult, AgentRunError>

// 带上下文的完整版本
pub async fn run_agent_loop_with_context(
    provider: Arc<dyn LlmProvider>,
    tools: &ToolRegistry,
    system_prompt: &str,
    user_content: &UserContent,
    on_event: Option<&OnEvent>,
    history: Option<Vec<ChatMessage>>,
    tool_context: Option<serde_json::Value>,    // 注入工具参数
    hook_registry: Option<Arc<HookRegistry>>,   // 钩子注册表
) -> Result<AgentRunResult, AgentRunError>

// 流式版本 - 实时 UI 更新
pub async fn run_agent_loop_streaming(...) -> Result<AgentRunResult, AgentRunError>
```

### 2.2 Agent Loop 执行流程

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         AGENT LOOP 执行流程                              │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  1. 初始化                                                               │
│     - 加载配置: max_iterations (默认 25, Lazy 模式 3x)                   │
│     - 构建消息列表: [System] + [History] + [User]                        │
│     - 提取 session_key 用于 Hook                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  2. 迭代循环 (LOOP)                                                      │
│     iterations += 1                                                      │
│     if iterations > max_iterations → ERROR                               │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  3. BeforeLLMCall Hook 分发                                              │
│     - 可阻塞 LLM 调用 (Block)                                            │
│     - 可修改参数 (ModifyPayload)                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  4. LLM API 调用                                                         │
│     provider.complete(&messages, tool_schemas)                           │
│                                                                          │
│     错误处理:                                                             │
│     - Context Window Error → 返回错误                                    │
│     - Rate Limit → 指数退避重试 (最多 5 次)                               │
│     - Server Error (5xx) → 重试 (最多 1 次)                              │
│     - Billing Error → 直接返回                                           │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  5. 响应处理                                                             │
│     a. 工具调用从文本解析 (fallback):                                     │
│        - Fenced blocks: ```tool_call\n{...}\n```                         │
│        - XML: <function=name><parameter=key>value</parameter>            │
│        - XML invoke: <invoke name="..."><arg name="...">value</arg>      │
│        - Bare JSON: {"tool": "name", "arguments": {...}}                 │
│                                                                          │
│     b. 恢复畸形工具调用                                                   │
│     c. 强制执行显式 /sh 命令                                              │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  6. AfterLLMCall Hook 分发                                               │
│     - 可阻塞工具执行                                                      │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  7. 无工具调用? → 返回文本结果                                            │
│     return AgentRunResult { text, iterations, tool_calls_made, usage }   │
└─────────────────────────────────────────────────────────────────────────┘
                                 │ 有工具调用
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  8. 工具并发执行                                                         │
│     futures::future::join_all(tool_futures)                              │
│                                                                          │
│     每个工具调用:                                                         │
│     a. BeforeToolCall Hook → Block/ModifyPayload/Continue               │
│     b. tool.execute(args)                                                │
│     c. AfterToolCall Hook                                                │
│     d. 结果截断 (max_tool_result_bytes)                                  │
│     e. 追加 Tool 消息到 messages                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
                         继续下一轮迭代
```

### 2.3 关键事件类型

```rust
pub enum RunnerEvent {
    Thinking,                          // LLM 开始思考
    ThinkingDone,                      // LLM 思考完成
    ToolCallStart {                    // 工具调用开始
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolCallEnd {                      // 工具调用结束
        id: String,
        name: String,
        success: bool,
        error: Option<String>,
        result: Option<serde_json::Value>,
    },
    ThinkingText(String),              // 推理文本增量
    TextDelta(String),                 // 输出文本增量
    Iteration(usize),                  // 迭代计数
    SubAgentStart {                    // 子 Agent 启动
        task: String,
        model: String,
        depth: u64,
    },
    SubAgentEnd {                      // 子 Agent 结束
        task: String,
        model: String,
        depth: u64,
        iterations: usize,
        tool_calls_made: usize,
    },
    RetryingAfterError {               // 错误后重试
        error: String,
        delay_ms: u64,
    },
}
```

---

## 3. 工具系统架构

### 3.1 AgentTool Trait

```rust
// crates/agents/src/tool_registry.rs
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    async fn warmup(&self) -> Result<()> { Ok(()) }  // 启动预热

    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value>;
}
```

### 3.2 ToolRegistry 设计

```rust
pub struct ToolRegistry {
    tools: HashMap<String, ToolEntry>,      // 静态注册的工具
    activated: ActivatedTools,              // 运行时激活的工具 (Lazy 模式)
}

pub struct ToolEntry {
    tool: Arc<dyn AgentTool>,
    source: ToolSource,
}

pub enum ToolSource {
    Builtin,                               // 内置工具
    Mcp { server: String },                // MCP 服务器提供
    Wasm { component_hash: [u8; 32] },     // WASM 组件
}
```

### 3.3 工具来源分类

| 来源 | 说明 | 示例 |
|------|------|------|
| **Builtin** | 编译时内置 | `exec`, `web_fetch`, `spawn_agent` |
| **MCP** | MCP 服务器动态提供 | `mcp__github__search`, `mcp__memory__store` |
| **WASM** | 预编译组件 | 自定义扩展 |

### 3.4 核心工具实现

```
crates/tools/src/
├── spawn_agent.rs      # 子 Agent 生成 (最大嵌套深度 3)
├── exec.rs             # Shell 命令执行
├── browser.rs          # 浏览器自动化
├── web_search.rs       # Web 搜索
├── web_fetch.rs        # Web 内容获取 (SSRF 防护)
├── sessions_communicate.rs  # 会话间通信
├── sessions_manage.rs  # 会话管理
├── cron_tool.rs        # 定时任务
└── memory/tools.rs     # 长期记忆操作
```

---

## 4. LLM 提供者抽象

### 4.1 LlmProvider Trait

```rust
// crates/agents/src/model.rs
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;                    // 提供者名称
    fn id(&self) -> &str;                      // 模型 ID

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
    ) -> anyhow::Result<CompletionResponse>;

    fn supports_tools(&self) -> bool { false }     // 是否支持工具调用
    fn context_window(&self) -> u32 { 200_000 }    // 上下文窗口大小
    fn supports_vision(&self) -> bool { false }    // 是否支持图像输入

    fn tool_mode(&self) -> Option<ToolMode> { None }

    // 流式 API
    fn stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>>;

    fn stream_with_tools(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<serde_json::Value>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>>;

    // 扩展思考支持
    fn reasoning_effort(&self) -> Option<ReasoningEffort> { None }
    fn with_reasoning_effort(self: Arc<Self>, effort: ReasoningEffort) -> Option<Arc<dyn LlmProvider>>;
}
```

### 4.2 消息类型

```rust
pub enum ChatMessage {
    System { content: String },
    User { content: UserContent },
    Assistant { content: Option<String>, tool_calls: Vec<ToolCall> },
    Tool { tool_call_id: String, content: String },
}

pub enum UserContent {
    Text(String),
    Multimodal(Vec<ContentPart>),
}

pub enum ContentPart {
    Text(String),
    Image { media_type: String, data: String },
}
```

---

## 5. Hook 系统

### 5.1 Hook 事件类型

```rust
// crates/common/src/hooks.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    // Agent 生命周期
    BeforeAgentStart,
    AgentEnd,

    // LLM 调用
    BeforeLLMCall,
    AfterLLMCall,

    // 上下文压缩
    BeforeCompaction,
    AfterCompaction,

    // 消息处理
    MessageReceived,
    MessageSending,
    MessageSent,

    // 工具调用
    BeforeToolCall,      // 可阻塞/修改
    AfterToolCall,       // 只读
    ToolResultPersist,   // 持久化

    // 会话生命周期
    SessionStart,
    SessionEnd,

    // 网关生命周期
    GatewayStart,
    GatewayStop,

    Command,
}
```

### 5.2 Hook Payload

```rust
pub enum HookPayload {
    BeforeToolCall {
        session_key: String,
        tool_name: String,
        arguments: Value,
    },
    AfterToolCall {
        session_key: String,
        tool_name: String,
        success: bool,
        result: Option<Value>,
    },
    BeforeLLMCall {
        session_key: String,
        provider: String,
        model: String,
        messages: Value,
        tool_count: usize,
        iteration: usize,
    },
    // ... 其他变体
}
```

### 5.3 Hook 动作

```rust
pub enum HookAction {
    Continue,                    // 继续正常执行
    ModifyPayload(Value),        // 修改参数
    Block(String),               // 阻塞并返回原因
}
```

### 5.4 Hook Registry 特性

- **优先级排序**: 高优先级先执行，可短路阻塞
- **读写分离**: 只读事件并行分发，修改事件顺序分发
- **熔断器**: 连续失败 3 次自动禁用，60 秒冷却后恢复
- **Dry-run 模式**: 仅记录不执行

---

## 6. 子 Agent 系统

### 6.1 SpawnAgentTool 核心逻辑

```rust
// crates/tools/src/spawn_agent.rs
pub struct SpawnAgentTool {
    provider_registry: Arc<RwLock<ProviderRegistry>>,
    default_provider: Arc<dyn LlmProvider>,
    tool_registry: Arc<ToolRegistry>,
    agents_config: Option<Arc<RwLock<AgentsConfig>>>,
    on_event: Option<OnSpawnEvent>,
    session_deps: Option<SessionDeps>,
}

const MAX_SPAWN_DEPTH: u64 = 3;  // 最大嵌套深度
```

### 6.2 子 Agent 执行流程

```
┌──────────────────────────────────────────────────────────────┐
│                    SPAWN AGENT 执行流程                       │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  1. 解析参数                                                  │
│     - task (必须): 任务描述                                   │
│     - context: 上下文信息                                     │
│     - preset: 预设名称 (如 researcher, coder)                 │
│     - model: 模型 ID 覆盖                                     │
│     - allow_tools / deny_tools: 工具策略                      │
│     - delegate_only: 仅代理模式                               │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  2. 深度检查                                                  │
│     if depth >= MAX_SPAWN_DEPTH (3) → ERROR                  │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  3. 构建子工具注册表                                          │
│     - 移除 spawn_agent (防止无限递归)                         │
│     - 应用 allow_tools / deny_tools 策略                     │
│     - delegate_only: 仅保留代理工具集                         │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  4. 构建系统 Prompt                                          │
│     - 注入身份: "You are {name} ({emoji})"                   │
│     - 注入风格: "Your style is {theme}"                      │
│     - 加载持久记忆: MEMORY.md 内容                            │
│     - 任务和上下文                                            │
│     - system_prompt_suffix                                   │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  5. 运行子 Agent Loop                                        │
│     run_agent_loop_with_context(                             │
│         provider,                                            │
│         &sub_tools,                                          │
│         &system_prompt,                                      │
│         &UserContent::text(task),                            │
│         ...,                                                 │
│         Some(json!({ "_spawn_depth": depth + 1 })),          │
│         None,  // 无 hooks                                   │
│     )                                                        │
│                                                              │
│     可选超时: preset.timeout_secs                            │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  6. 返回结果                                                  │
│     {                                                        │
│       "text": "子 Agent 输出",                               │
│       "iterations": 3,                                       │
│       "tool_calls_made": 5,                                  │
│       "model": "claude-sonnet-4-20250514",                   │
│       "preset": "researcher"                                 │
│     }                                                        │
└──────────────────────────────────────────────────────────────┘
```

### 6.3 Agent Preset 配置

```rust
pub struct AgentPreset {
    pub identity: AgentIdentity,        // 名称、emoji、风格
    pub model: Option<String>,          // 指定模型
    pub tools: PresetToolPolicy,        // allow/deny 工具列表
    pub delegate_only: bool,            // 仅代理模式
    pub reasoning_effort: Option<ReasoningEffort>,  // 思考深度
    pub system_prompt_suffix: Option<String>,       // 额外指令
    pub memory: Option<PresetMemoryConfig>,         // 记忆配置
    pub sessions: Option<SessionAccessPolicy>,      // 会话访问策略
    pub timeout_secs: Option<u64>,                  // 超时
}
```

---

## 7. 会话与状态管理

### 7.1 会话存储

```rust
// crates/sessions/src/store.rs
pub struct SessionStore {
    pub base_dir: PathBuf,
}

// JSONL 格式存储，每行一条消息
// ~/.moltis/sessions/{session_key}/messages.jsonl
```

### 7.2 消息类型

```rust
pub enum PersistedMessage {
    System { content: String, created_at: Option<u64> },
    Notice { content: String, created_at: Option<u64> },
    User {
        content: MessageContent,
        created_at: Option<u64>,
        audio: Option<String>,
        channel: Option<Value>,
        seq: Option<u64>,
        run_id: Option<String>,
    },
    Assistant {
        content: String,
        created_at: Option<u64>,
        model: Option<String>,
        provider: Option<String>,
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        tool_calls: Option<Vec<PersistedToolCall>>,
        reasoning: Option<String>,
    },
    Tool { tool_call_id: String, content: String, created_at: Option<u64> },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        arguments: Option<Value>,
        success: bool,
        result: Option<Value>,
        error: Option<String>,
        reasoning: Option<String>,
        run_id: Option<String>,
    },
}
```

---

## 8. 网关与协议层

### 8.1 WebSocket 协议 (v4)

```rust
// crates/protocol/src/lib.rs
pub const PROTOCOL_VERSION: u32 = 4;
pub const MAX_PAYLOAD_BYTES: usize = 524_288;  // 512 KB
pub const TICK_INTERVAL_MS: u64 = 30_000;       // 30s 心跳

// 帧类型
pub struct RequestFrame {          // 客户端 → 网关 RPC
    pub id: u32,
    pub method: String,
    pub params: Value,
}

pub struct ResponseFrame {         // 网关 → 客户端 结果
    pub id: u32,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

pub struct EventFrame {            // 网关 → 客户端 推送
    pub topic: String,
    pub payload: Value,
}
```

### 8.2 ChatRuntime Trait

```rust
// crates/chat/src/runtime.rs
#[async_trait]
pub trait ChatRuntime: Send + Sync {
    // 广播
    async fn broadcast(&self, topic: &str, payload: Value);

    // Channel 回复队列
    async fn push_channel_reply(&self, session_key: &str, target: ChannelReplyTarget);
    async fn drain_channel_replies(&self, session_key: &str) -> Vec<ChannelReplyTarget>;

    // 运行错误追踪
    async fn set_run_error(&self, run_id: &str, error: String);

    // 连接映射
    async fn active_session_key(&self, conn_id: &str) -> Option<String>;
    async fn active_project_id(&self, conn_id: &str) -> Option<String>;

    // 服务访问
    fn sandbox_router(&self) -> Option<&Arc<SandboxRouter>>;
    fn memory_manager(&self) -> Option<&Arc<MemoryManager>>;
    fn tts_service(&self) -> &dyn TtsService;
    fn project_service(&self) -> &dyn ProjectService;
    fn mcp_service(&self) -> &dyn McpService;

    // 远程节点
    async fn connected_nodes(&self) -> Vec<ConnectedNodeSummary>;
}
```

---

## 9. 安全机制

### 9.1 SSRF 防护

```rust
// crates/tools/src/web_fetch.rs
// 阻止访问:
// - Loopback 地址 (127.0.0.1, ::1)
// - 私有地址 (10.x, 172.16-31.x, 192.168.x)
// - 链路本地地址 (169.254.x.x)
// - CGNAT 地址 (100.64-127.x.x)
```

### 9.2 Secret 管理

```rust
// 使用 secrecy::Secret<String> 包装敏感数据
// Debug 实现显示 [REDACTED]
// expose_secret() 仅在消费点调用
```

### 9.3 WebSocket Origin 验证

```rust
// crates/gateway/src/server.rs
// 拒绝跨源 WebSocket 升级 (403)
// Loopback 变体等价处理
```

---

## 10. 配置要点

| 配置项 | 说明 | 默认值 |
|--------|------|--------|
| `agent_max_iterations` | Agent Loop 最大迭代次数 | 25 |
| `max_tool_result_bytes` | 工具结果大小限制 | - |
| `tool_mode` | Native/Text/Off/Auto | Auto |
| `registry_mode` | Lazy/Eager 工具加载 | Eager |
| `MAX_SPAWN_DEPTH` | 子 Agent 最大嵌套深度 | 3 |

---

## 11. 核心文件索引

| 模块 | 文件路径 | 说明 |
|------|----------|------|
| Agent Loop | `crates/agents/src/runner.rs` | Agent 循环核心实现 |
| Tool Registry | `crates/agents/src/tool_registry.rs` | 工具注册表 |
| LLM Provider | `crates/agents/src/model.rs` | LLM 提供者抽象 |
| Hook System | `crates/common/src/hooks.rs` | Hook 事件系统 |
| Spawn Agent | `crates/tools/src/spawn_agent.rs` | 子 Agent 生成 |
| Chat Engine | `crates/chat/src/lib.rs` | Chat 执行引擎 |
| Chat Runtime | `crates/chat/src/runtime.rs` | 运行时抽象 |
| Gateway | `crates/gateway/src/chat.rs` | 网关适配器 |
| Protocol | `crates/protocol/src/lib.rs` | WebSocket 协议 |
| Sessions | `crates/sessions/src/store.rs` | 会话存储 |

---

这个架构设计体现了现代 AI Agent 系统的核心特征：**异步事件驱动、工具并发执行、Hook 可观测性、子 Agent 委托、以及多层安全防护**。
