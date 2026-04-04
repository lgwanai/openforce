# 三省六部多智能体系统 - 代码审查与设计对比报告

**审查日期**: 2026-04-03
**项目地址**: https://github.com/moltis-org/moltis

---

## 一、代码健壮性与安全性问题

### 🔴 CRITICAL（必须修复）

| # | 问题 | 文件位置 | 风险描述 |
|---|------|----------|----------|
| 1 | **Shell 注入漏洞** | `src/tools/base.py:170-173` | 用户可控命令直接传给 `shell=True`，攻击者可执行任意系统命令 |
| 2 | **弱令牌生成** | `src/security/taint_engine.py:32-35` | SHA256 无密钥，审批令牌可被伪造 |
| 3 | **硬编码外部路径** | `src/tools/base.py:121` | `/Users/wuliang/skills/...` 无法移植，部署困难 |

#### 详细说明

**问题 1: Shell 注入漏洞**

```python
# base.py:170-173
full_cmd = f"mkdir -p /tmp/Library/Caches && ln -sf /Users/wuliang/Library/Caches/ms-playwright /tmp/Library/Caches/ms-playwright && export HOME=/tmp && {executable} {command}"
result = subprocess.run(full_cmd, shell=True, ...)
```

**修复建议**: 避免使用 `shell=True`，使用参数列表或命令白名单校验。

**问题 2: 弱令牌生成**

```python
# taint_engine.py:32-35
def generate_approval_token(...) -> str:
    raw = f"{owner_user_id}:{task_id}:{approval_id}:{action_hash}:{exp}:{nonce}:{channel_binding_hash}"
    return hashlib.sha256(raw.encode()).hexdigest()  # 无密钥，可预测
```

**修复建议**: 使用 HMAC 配合密钥生成签名。

**问题 3: 硬编码路径**

```python
# base.py:121
script_path = "/Users/wuliang/skills/baidu-search-openclaw/scripts/search.py"
```

**修复建议**: 使用环境变量或配置文件管理外部工具路径。

---

### 🟠 HIGH（强烈建议修复）

| # | 问题 | 文件位置 | 风险描述 |
|---|------|----------|----------|
| 4 | 裸 `except Exception` | `zhongshu.py:115-119` | 吞掉所有异常包括 KeyboardInterrupt，无日志记录 |
| 5 | 工具错误静默处理 | `zhongshu.py:162-175` | 丢失堆栈信息，调试困难 |
| 6 | 数据库连接无上下文管理器 | `db.py:67-82` | 异常时连接泄漏 |
| 7 | 竞态条件 | `db.py:95-112` | `get/set_active_task` 非原子操作，多进程并发问题 |
| 8 | SSRF 漏洞 | `base.py:103-113` | `fetch_webpage` 无 URL 校验，可访问内网 |
| 9 | 缺少类型注解 | `utils.py:5` | `invoke_llm_with_tools` 无类型提示 |
| 10 | `Dict[str, Any]` 滥用 | 多处 `plan` 字段 | 类型检查失效 |

#### 详细说明

**问题 4: 裸异常捕获**

```python
# zhongshu.py:115-119
try:
    response = invoke_llm_with_tools(llm, tools, messages)
except Exception as e:
    response = AIMessage(content=f"LLM Error: {str(e)}")
    return {"messages": [response], "intent": "Chat"}
```

**修复建议**: 捕获特定异常类型，添加日志记录。

**问题 7: 竞态条件**

```python
# cli.py:34-37 与 db.py 的配合
active_task = get_active_task(owner_user_id)  # 检查
if active_task:
    ...
# 此处另一进程可能修改
set_active_task(owner_user_id, task_id)  # 设置
```

**修复建议**: 使用单一原子操作，返回现有任务或成功设置。

**问题 8: SSRF 漏洞**

```python
# base.py:103-113
def fetch_webpage(url: str) -> str:
    resp = httpx.get(url, timeout=10.0)  # 无 URL 验证
```

**修复建议**: 校验 URL 协议和主机名，阻止访问内网地址。

---

### 🟡 MEDIUM（建议修复）

| # | 问题 | 文件位置 |
|---|------|----------|
| 11 | Debug print 残留 | `utils.py:117-131` |
| 12 | 重复 `load_prompt` 函数 | 三个 Agent 文件重复定义 |
| 13 | 魔法数字 `5000` | `base.py` 多处硬编码 |
| 14 | `TaintEngine.check_tool_call` 永远返回 True | `taint_engine.py:21-30` |
| 15 | 无污点传播实现 | `taint_engine.py` 仅有骨架 |

---

## 二、设计 vs 实现对比

### 已实现（符合设计）✓

| 功能 | 实现状态 | 文件位置 |
|------|----------|----------|
| 中书省/尚书省/户部架构 | ✓ 完整 | `agents/zhongshu.py`, `agents/shangshu.py`, `agents/hubu.py` |
| 中书省基础工具集 | ✓ 完整 | `tools/base.py` |
| 沙箱路径校验 | ✓ 完整 | `base.py:16-43` (realpath 归一化、禁止符号链接) |
| 调度工具骨架 | ✓ 骨架 | `tools/orchestration.py` |
| 多模型配置 | ✓ 完整 | `config/models.yaml` |
| Prompt 模板化 | ✓ 完整 | `prompts/*.md` |
| CLI 通道 | ✓ 完整 | `channels/cli.py` |
| 户部 ReAct 循环 | ✓ 完整 | `hubu.py` (含已访问 URL/关键词追踪) |
| 意图路由（语义） | ✓ 符合设计 | 使用 LLM 判断，无硬编码 |
| 尚书省无业务工具 | ✓ 符合设计 | 仅有调度工具 |

### 未实现或不完整 ✗

| 功能 | 设计要求 | 实现状态 | 优先级 |
|------|----------|----------|--------|
| **六部 Agent** | 兵部、吏部、工部、刑部、礼部、都察院 | 仅户部实现，其他全部缺失 | **P0** |
| **预算系统** | `token_budget`、`time_budget`、`cost_budget`、全局熔断 | 完全未实现 | **P0** |
| **Human-in-the-loop** | 高风险操作强制审批、审批卡片、回调续跑 | 仅有令牌生成/验证函数，无流程集成 | **P0** |
| **Barrier 并发屏障** | 带 SLA 的并发屏障、统一收集结果 | `spawn_agent` 仅返回 JSON，无真正并发控制 | **P1** |
| **状态机完整流转** | Created→Planned→Running→WaitingApproval→WaitingInput→WaitingExternalCall→Paused→Verifying→Succeeded/Failed/Escalated/Cancelled/Compensating | 仅 Running→Succeeded | **P1** |
| **L0 显式路由** | 简单回应直接返回 | 未区分 L0/L1 | **P1** |
| **验证闭环** | 硬性断言、断言防伪签名、沙箱退出码验证 | 未实现 | **P1** |
| **副作用一致性** | Outbox 模式、幂等键、三阶段提交 | 未实现 | **P2** |
| **内容压缩** | LLM summarize、Token 超限拦截 | 仅简单截断 | **P2** |
| **图快照恢复** | `graph_snapshot` 持久化，恢复优先使用快照 | 未实现 | **P2** |
| **Feishu/API 通道** | 多消息通道支持 | 仅 CLI 实现 | **P2** |
| **澄清轮次上限** | `max_clarification_rounds=3` | 未实现 | **P2** |
| **会话门控三输入类型** | 任务进行中仅接收审批回复/参数补全/固定指令 | 仅拦截新任务，未实现审批绑定和参数补全 | **P1** |

### 实现偏差（与设计不一致）

| 设计要求 | 实现现状 | 偏差程度 |
|----------|----------|----------|
| L0/L1 强制写 Memory（硬门禁） | 未体现此约束 | **中等偏差** |
| 审批快照校验（TOCTOU 防护） | 仅有 token 函数，无快照生成和校验 | **不完整** |
| ReAct 死循环熔断 | 未实现连续相同 Thought/Action 检测 | **缺失** |
| 指数退避重试 | 未实现 Backoff 策略 | **缺失** |

### 额外实现（设计未提及）

| 功能 | 文件位置 | 说明 |
|------|----------|------|
| Minimax/Kimi API 兼容 | `utils.py` | XML 工具调用解析，处理非标准 API 输出 |
| agent-browser 工具 | `base.py:143-183` | 浏览器自动化工具，设计仅提及 web_search/fetch_webpage |
| 百度搜索 Skill 集成 | `base.py:115-141` | 外部搜索服务集成 |

---

## 三、总体评估

### 完成度统计

| 维度 | 完成度 | 说明 |
|------|--------|------|
| 架构骨架 | 70% | 三省框架搭建完成，六部缺失 |
| 安全机制 | 30% | TaintEngine/审批仅有骨架，存在漏洞 |
| 功能完整性 | 40% | 核心流程可跑，关键功能缺失 |
| 代码健壮性 | 50% | 错误处理不足，类型安全弱 |
| 消息通道 | 33% | 仅 CLI，Feishu/API 未实现 |

### 风险矩阵

```
                    ┌─────────────────────────────────────┐
                    │           影响程度                   │
                    │   低        中        高        极高  │
        ┌───────────┼─────────────────────────────────────┤
   发   │   高      │  MEDIUM    HIGH     CRITICAL  BLOCK │
   生   │           │   (5)      (8)       (3)       -    │
   概   ├───────────┼─────────────────────────────────────┤
   率   │   中      │   LOW      MEDIUM    HIGH      -    │
        │           │   (2)      (5)        (8)           │
        ├───────────┼─────────────────────────────────────┤
        │   低      │   -        LOW       MEDIUM     -    │
        │           │            (2)        (5)           │
        └───────────┴─────────────────────────────────────┘
```

---

## 四、优先修复清单

### P0 - 阻塞级（必须立即修复）

| # | 问题 | 修复方案 | 预估工时 |
|---|------|----------|----------|
| 1 | Shell 注入漏洞 | 移除 `shell=True`，使用参数列表 + 白名单校验 | 4h |
| 2 | 弱令牌生成 | 使用 HMAC + 密钥生成签名 | 2h |
| 3 | 硬编码路径 | 抽取到配置文件或环境变量 | 1h |
| 4 | 预算系统 | 实现 `token_budget` 基础框架和熔断机制 | 8h |

### P1 - 重要级（近期修复）

| # | 问题 | 修复方案 | 预估工时 |
|---|------|----------|----------|
| 5 | 六部 Agent 缺失 | 先实现兵部用于代码执行 | 16h |
| 6 | Human-in-the-loop | 集成审批流程到高风险工具 | 12h |
| 7 | Barrier 并发屏障 | 实现并发 Agent 结果收集机制 | 8h |
| 8 | 状态机完善 | 补充 WaitingApproval/Paused 等状态 | 8h |
| 9 | TaintEngine 实现 | 实现真正的污点传播和校验逻辑 | 8h |

### P2 - 改进级（计划修复）

| # | 问题 | 修复方案 | 预估工时 |
|---|------|----------|----------|
| 10 | Feishu/API 通道 | 扩展消息通道适配器 | 16h |
| 11 | 图快照恢复 | 实现 `graph_snapshot` 持久化 | 8h |
| 12 | 副作用一致性 | 实现 Outbox 模式 | 12h |

---

## 五、代码质量改进建议

### 错误处理规范化

```python
# 推荐模式
import logging
import uuid

logger = logging.getLogger(__name__)

try:
    result = risky_operation()
except SpecificException as e:
    error_id = uuid.uuid4().hex[:8]
    logger.error(f"[{error_id}] Operation failed", exc_info=True)
    return {"error": True, "message": f"Operation failed (ref: {error_id})"}
```

### 数据库连接安全

```python
# 推荐模式
def save_task(task: TaskRecord):
    with sqlite3.connect(DB_PATH) as conn:
        cursor = conn.cursor()
        cursor.execute(...)
        conn.commit()
    # 自动关闭，异常安全
```

### 类型安全增强

```python
# 推荐模式
from typing import TypedDict, List, Optional

class Plan(TypedDict):
    steps: List[str]
    estimated_time: Optional[int]
    required_tools: List[str]

def process_plan(plan: Plan) -> str:
    ...
```

---

## 六、结论

**审查结论**: **BLOCK** - 存在 3 个 CRITICAL 安全漏洞，6 部 Agent 缺失，预算系统未实现。

**建议**: 
1. 先修复安全漏洞，确保基础安全
2. 补充预算系统，防止资源失控
3. 逐步实现六部 Agent，完善功能闭环
4. 持续改进代码质量，增强健壮性

---

*报告生成时间: 2026-04-03*
*审查工具: Claude Code + code-reviewer Agent + python-reviewer Agent*
