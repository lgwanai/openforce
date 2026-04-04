# Phase 3: Human-in-the-loop - Research

**Researched:** 2026-04-04
**Domain:** Human-in-the-loop approval workflow, agent state persistence, secure token consumption
**Confidence:** HIGH

## Summary

Phase 3 implements a complete human-in-the-loop approval workflow for high-risk operations. The system must: (1) automatically block high-risk tool calls and request approval, (2) generate cryptographically secure action snapshots for TOCTOU prevention, (3) enforce one-time token consumption with replay attack prevention, (4) resume agent execution after approval, and (5) implement Bingbu Agent for code execution tasks.

The existing codebase provides a solid foundation: `ApprovalTokenManager` (HMAC-SHA256 tokens), `TaintEngine` (high/medium risk tool classification), `TaskRecord` (approval-related fields), and `consume_nonce` (atomic nonce consumption). The approval flow must integrate with LangGraph's state management and the CLI channel's existing approval callback mechanism.

**Primary recommendation:** Extend TaintEngine to raise `ApprovalRequest` exceptions for high-risk tools, implement approval workflow as a middleware layer between tool_node and agent nodes, and use LangGraph's interrupt/resume pattern for state persistence during approval.

## User Constraints

No CONTEXT.md exists for this phase. Research scope is defined by requirements HIL-01 through HIL-04 and AGT-01.

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| HIL-01 | 审批流程集成 - 高风险工具触发审批阻断 | TaintEngine.check_tool_call returns False for HIGH_RISK_TOOLS. Need to raise ApprovalRequest and integrate with graph flow. |
| HIL-02 | 审批快照生成 - TOCTOU 防护 | ApprovalTokenManager supports action_hash. Need to implement canonical JSON serialization of tool calls. |
| HIL-03 | 审批令牌消费 - 一次性原子消费 | consume_nonce() in db.py provides atomic nonce tracking. Need to integrate with token verification. |
| HIL-04 | 审批回调续跑 - 审批通过后恢复执行 | TaskRecord has approval_snapshot_id. Need LangGraph state persistence and resume mechanism. |
| AGT-01 | 兵部 Agent 实现 - 通用执行、代码编写能力 | Follow Shangshu/Hubu agent patterns. Need code execution tools with sandbox support. |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| langgraph | latest | Agent workflow orchestration | Already used in Zhongshu/Shangshu/Hubu agents |
| langchain-core | latest | Message types, tool decorators | Standard for LLM applications |
| pydantic | >=2.0.0 | State models, validation | Already used for TaskRecord |
| hashlib/hmac | stdlib | Token generation | Already implemented in ApprovalTokenManager |
| sqlite3 | stdlib | Nonce persistence, task state | Already used in db.py |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rich | current | CLI approval prompts | For interactive approval in CLI channel |
| asyncio | stdlib | Async state management | For async graph execution |

### Existing Infrastructure
| Component | Location | Purpose |
|-----------|----------|---------|
| ApprovalTokenManager | src/security/approval.py | HMAC token generation/verification |
| TaintEngine | src/security/taint_engine.py | Risk classification, taint tracking |
| TaskRecord | src/core/db.py | Task state persistence with approval fields |
| consume_nonce | src/core/db.py | Atomic nonce consumption |
| approval_callback | src/channels/cli.py | User approval prompts |

## Architecture Patterns

### Recommended Project Structure
```
src/
├── security/
│   ├── approval.py          # ApprovalTokenManager (exists)
│   ├── taint_engine.py      # TaintEngine (exists)
│   └── approval_flow.py     # NEW: Approval middleware
├── agents/
│   ├── zhongshu.py          # Main agent (exists)
│   ├── shangshu.py          # Orchestrator (exists)
│   ├── hubu.py              # Research agent (exists)
│   └── bingbu.py            # NEW: Code execution agent
├── tools/
│   ├── base.py              # Basic tools (exists)
│   └── code_executor.py     # NEW: Sandboxed code execution
└── core/
    └── db.py                # Task persistence (exists)
```

### Pattern 1: Approval Request Exception

**What:** Raise `ApprovalRequest` exception when high-risk tool is detected, containing all context needed for approval.

**When to use:** In tool_node before executing high-risk tools.

**Example:**
```python
# src/security/approval_flow.py

from dataclasses import dataclass
from typing import Dict, Any
import hashlib
import json

@dataclass
class ApprovalRequest(Exception):
    """Raised when an operation needs user approval."""
    tool_name: str
    tool_args: Dict[str, Any]
    action_hash: str
    approval_id: str
    task_id: str
    owner_user_id: str
    
    @classmethod
    def from_tool_call(cls, tool_name: str, tool_args: Dict[str, Any], 
                       task_id: str, owner_user_id: str) -> 'ApprovalRequest':
        """Create approval request with canonical action hash."""
        # Canonical JSON serialization for TOCTOU protection
        canonical = json.dumps({
            "tool": tool_name,
            "args": tool_args,
            "task_id": task_id
        }, sort_keys=True, separators=(',', ':'))
        
        action_hash = hashlib.sha256(canonical.encode()).hexdigest()
        approval_id = f"approval_{hashlib.sha256(f'{task_id}:{action_hash}'.encode()).hexdigest()[:16]}"
        
        return cls(
            tool_name=tool_name,
            tool_args=tool_args,
            action_hash=action_hash,
            approval_id=approval_id,
            task_id=task_id,
            owner_user_id=owner_user_id
        )
```

### Pattern 2: Approval Middleware Node

**What:** Add approval handling node to LangGraph that intercepts high-risk tool calls.

**When to use:** Between tool execution and agent nodes.

**Example:**
```python
# In zhongshu.py or similar

from src.security.approval_flow import ApprovalRequest
from src.security.taint_engine import TaintEngine

def tool_node_with_approval(state: ZhongshuState):
    """Execute tools with approval checking."""
    last_message = state["messages"][-1]
    
    if not hasattr(last_message, "tool_calls") or not last_message.tool_calls:
        return {"messages": []}
    
    for tool_call in last_message.tool_calls:
        tool_name = tool_call["name"]
        tool_args = tool_call["args"]
        
        # Check if high-risk tool
        if tool_name in TaintEngine.HIGH_RISK_TOOLS:
            raise ApprovalRequest.from_tool_call(
                tool_name=tool_name,
                tool_args=tool_args,
                task_id=state["task_id"],
                owner_user_id=state["owner_user_id"]
            )
        
        # Execute tool normally
        # ... existing tool execution code
```

### Pattern 3: Token Consumption with Nonce

**What:** Verify token and consume nonce atomically to prevent replay attacks.

**When to use:** When processing approval callback.

**Example:**
```python
# src/security/approval_flow.py

from src.core.db import consume_nonce
from src.security.approval import ApprovalTokenManager

def consume_approval_token(token: str, approval_request: ApprovalRequest) -> bool:
    """Verify and consume approval token atomically.
    
    Returns True if token is valid and consumed, False otherwise.
    Raises ValueError if token is reused (replay attack detected).
    """
    manager = ApprovalTokenManager()
    
    # Extract nonce from token
    parts = token.split(':')
    if len(parts) != 3:
        return False
    _, nonce, _ = parts
    
    # Verify token signature
    if not manager.verify_token(
        token=token,
        owner_user_id=approval_request.owner_user_id,
        task_id=approval_request.task_id,
        approval_id=approval_request.approval_id,
        action_hash=approval_request.action_hash
    ):
        return False
    
    # Atomic nonce consumption (prevents replay)
    if not consume_nonce(nonce):
        # Nonce already consumed - replay attack detected
        raise ValueError(f"Replay attack detected: nonce {nonce} already consumed")
    
    return True
```

### Pattern 4: State Persistence for Resume

**What:** Save agent state to TaskRecord when approval is pending, restore on callback.

**When to use:** When raising ApprovalRequest and when resuming.

**Example:**
```python
# src/security/approval_flow.py

import json
from src.core.db import TaskRecord, save_task

def save_pending_state(state: dict, approval_request: ApprovalRequest) -> str:
    """Save agent state for later resumption."""
    snapshot_id = f"snapshot_{approval_request.approval_id}"
    
    # Extract serializable state
    snapshot = {
        "task_id": state["task_id"],
        "owner_user_id": state["owner_user_id"],
        "messages": [
            {"type": type(m).__name__, "content": m.content, 
             "tool_calls": getattr(m, "tool_calls", None)}
            for m in state.get("messages", [])
        ],
        "pending_tool": {
            "name": approval_request.tool_name,
            "args": approval_request.tool_args,
            "id": "pending_tool_call_id"
        }
    }
    
    # Update task record
    task = get_task(state["task_id"])
    task.pending_approval_id = approval_request.approval_id
    task.approval_action_hash = approval_request.action_hash
    task.approval_snapshot_id = snapshot_id
    task.status = "WaitingApproval"
    task.checkpoints.append({
        "type": "approval_pending",
        "snapshot_id": snapshot_id,
        "approval_id": approval_request.approval_id
    })
    save_task(task)
    
    # Store snapshot (could use separate table or file)
    # For simplicity, store in checkpoints as JSON
    return snapshot_id

def restore_state(task_id: str) -> dict:
    """Restore agent state from saved snapshot."""
    task = get_task(task_id)
    
    for checkpoint in reversed(task.checkpoints):
        if checkpoint.get("type") == "approval_pending":
            # Restore state from checkpoint
            snapshot_id = checkpoint["snapshot_id"]
            # Load snapshot and reconstruct state
            # ...
            return restored_state
    
    return None
```

### Pattern 5: Bingbu Agent Structure

**What:** Code execution agent following Shangshu/Hubu patterns.

**When to use:** For code writing and execution tasks.

**Example:**
```python
# src/agents/bingbu.py

from typing import TypedDict, Annotated, List, Dict, Any
from langgraph.graph import StateGraph, END
from langgraph.graph.message import add_messages
from langchain_core.messages import BaseMessage, HumanMessage, AIMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool

class BingbuState(TypedDict):
    messages: Annotated[List[BaseMessage], add_messages]
    task_id: str
    goal: str
    files_created: List[str]
    files_modified: List[str]
    commands_executed: List[str]

@tool
def tool_create_file(filepath: str, content: str) -> str:
    """Create a new file with the given content."""
    # Use sandbox directory for safety
    from src.tools.base import write_file
    return write_file(filepath, content, sandbox_only=True)

@tool
def tool_execute_python(code: str) -> str:
    """Execute Python code in a sandboxed environment."""
    # Use RestrictedPython or similar for sandboxing
    # This is a high-risk tool requiring approval
    pass

@tool
def tool_install_package(package: str) -> str:
    """Install a Python package in the sandbox environment."""
    # High-risk tool requiring approval
    pass

# Build graph similar to Hubu/Shangshu
def build_bingbu_graph():
    workflow = StateGraph(BingbuState)
    # ... similar to Hubu/Shangshu pattern
    return workflow.compile()
```

### Anti-Patterns to Avoid

- **Don't use plain SHA256 for tokens:** Use HMAC with secret key (already implemented)
- **Don't store approval state in memory:** Use database for persistence across restarts
- **Don't use timestamp-only expiration:** Include nonce for uniqueness
- **Don't trust tool_args without hashing:** TOCTOU attacks can modify args between approval and execution
- **Don't execute approved tools without re-validating hash:** Always verify action_hash matches

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Token generation | Custom hash function | ApprovalTokenManager | HMAC-SHA256 with secret key, timing-attack resistant |
| Nonce tracking | Set/dict in memory | consume_nonce() in db.py | Atomic, persistent, prevents replay |
| Tool risk classification | Custom if/else | TaintEngine.check_tool_call | Already implemented, consistent |
| State persistence | Custom serialization | TaskRecord.checkpoints | Already has approval fields |
| Agent graph structure | New pattern | Shangshu/Hubu templates | Consistent architecture |

**Key insight:** The existing codebase has 80% of the infrastructure. The missing pieces are: (1) ApprovalRequest exception integration, (2) action_hash canonical serialization, (3) state save/restore helpers, and (4) Bingbu Agent implementation.

## Common Pitfalls

### Pitfall 1: Race Condition in Token Verification

**What goes wrong:** Token is verified but not consumed atomically, allowing replay attacks.

**Why it happens:** Separate verify() and consume() calls create a window for concurrent token reuse.

**How to avoid:** Combine verification and consumption in single atomic operation:
```python
# BAD
if verify_token(token, ...):  # Window of vulnerability
    consume_nonce(nonce)       # Can be called multiple times

# GOOD
if consume_approval_token(token, approval_request):  # Atomic
    execute_tool()
```

**Warning signs:** Multiple approval callbacks with same token succeeding.

### Pitfall 2: Non-Canonical JSON in Action Hash

**What goes wrong:** Different JSON serializations produce same logical content but different hashes.

**Why it happens:** Key ordering, whitespace, number representation differ between serializations.

**How to avoid:** Always use canonical JSON with sort_keys=True and specific separators:
```python
# BAD
json.dumps({"a": 1, "b": 2})  # May produce {"a":1,"b":2} or {"b":2,"a":1}

# GOOD
json.dumps({"a": 1, "b": 2}, sort_keys=True, separators=(',', ':'))
# Always produces: "a":1,"b":2
```

**Warning signs:** Approval fails with "hash mismatch" despite correct parameters.

### Pitfall 3: State Lost on Restart

**What goes wrong:** Agent state stored in memory is lost when process restarts, approval can't resume.

**Why it happens:** LangGraph doesn't automatically persist state to database.

**How to avoid:** Save complete state to TaskRecord.checkpoints before waiting for approval:
```python
# Save state before yielding to user
task.checkpoints.append({
    "type": "approval_pending",
    "state_snapshot": serialize_state(state),
    "approval_id": approval_id
})
save_task(task)
```

**Warning signs:** After restart, pending approvals show no context or fail to resume.

### Pitfall 4: Missing Tool Call ID

**What goes wrong:** ToolMessage requires tool_call_id but it's lost during approval flow.

**Why it happens:** Tool call context is discarded when raising ApprovalRequest.

**How to avoid:** Include tool_call_id in ApprovalRequest and restore it:
```python
@dataclass
class ApprovalRequest(Exception):
    tool_name: str
    tool_args: Dict[str, Any]
    tool_call_id: str  # Preserve this!
    # ...
```

**Warning signs:** "Missing tool_call_id" errors after approval resumes.

### Pitfall 5: Unbounded State Growth

**What goes wrong:** checkpoints list grows unbounded, slowing down task loading.

**Why it happens:** Each approval adds checkpoint but none are removed.

**How to avoid:** Prune old checkpoints after successful resume:
```python
def restore_state(task_id: str) -> dict:
    task = get_task(task_id)
    state = load_from_checkpoint(task.checkpoints[-1])
    
    # Keep only last N checkpoints
    task.checkpoints = task.checkpoints[-10:]
    save_task(task)
    
    return state
```

**Warning signs:** Task loading becomes slow, database grows large.

## Code Examples

### Complete Approval Flow Integration

```python
# src/security/approval_flow.py

import hashlib
import json
import secrets
from dataclasses import dataclass
from typing import Dict, Any, Optional

from src.security.approval import ApprovalTokenManager
from src.security.taint_engine import TaintEngine
from src.core.db import consume_nonce, get_task, save_task


@dataclass
class ApprovalRequest(Exception):
    """Raised when an operation needs user approval."""
    tool_name: str
    tool_args: Dict[str, Any]
    tool_call_id: str
    action_hash: str
    approval_id: str
    task_id: str
    owner_user_id: str
    snapshot: Optional[Dict[str, Any]] = None
    
    @classmethod
    def from_tool_call(
        cls,
        tool_name: str,
        tool_args: Dict[str, Any],
        tool_call_id: str,
        task_id: str,
        owner_user_id: str,
        state_snapshot: Optional[Dict[str, Any]] = None
    ) -> 'ApprovalRequest':
        """Create approval request with canonical action hash."""
        # Canonical JSON for TOCTOU protection
        canonical = json.dumps({
            "tool": tool_name,
            "args": tool_args,
            "task_id": task_id
        }, sort_keys=True, separators=(',', ':'))
        
        action_hash = hashlib.sha256(canonical.encode()).hexdigest()
        approval_id = f"apr_{secrets.token_urlsafe(8)}"
        
        return cls(
            tool_name=tool_name,
            tool_args=tool_args,
            tool_call_id=tool_call_id,
            action_hash=action_hash,
            approval_id=approval_id,
            task_id=task_id,
            owner_user_id=owner_user_id,
            snapshot=state_snapshot
        )
    
    def to_dict(self) -> Dict[str, Any]:
        """Serialize for API response or storage."""
        return {
            "tool_name": self.tool_name,
            "tool_args": self.tool_args,
            "action_hash": self.action_hash,
            "approval_id": self.approval_id,
            "task_id": self.task_id,
            "owner_user_id": self.owner_user_id
        }


def generate_approval_for_request(request: ApprovalRequest) -> Dict[str, Any]:
    """Generate approval token and data for user confirmation."""
    manager = ApprovalTokenManager()
    
    token = manager.generate_token(
        owner_user_id=request.owner_user_id,
        task_id=request.task_id,
        approval_id=request.approval_id,
        action_hash=request.action_hash,
        expires_in=3600  # 1 hour
    )
    
    # Save pending state to task
    task = get_task(request.task_id)
    if task:
        task.pending_approval_id = request.approval_id
        task.approval_action_hash = request.action_hash
        task.status = "WaitingApproval"
        task.checkpoints.append({
            "type": "approval_pending",
            "approval_id": request.approval_id,
            "tool_name": request.tool_name,
            "tool_args": request.tool_args,
            "action_hash": request.action_hash,
            "snapshot": request.snapshot
        })
        save_task(task)
    
    return {
        "approval_id": request.approval_id,
        "token": token,
        "tool_name": request.tool_name,
        "tool_args": request.tool_args,
        "action_hash": request.action_hash,
        "expires_in": 3600
    }


def verify_and_consume_approval(
    token: str,
    approval_id: str,
    task_id: str,
    owner_user_id: str,
    action_hash: str
) -> Dict[str, Any]:
    """Verify approval token and consume it atomically.
    
    Returns approval data if valid, raises ValueError if invalid or replayed.
    """
    manager = ApprovalTokenManager()
    
    # Extract nonce from token
    parts = token.split(':')
    if len(parts) != 3:
        raise ValueError("Invalid token format")
    
    exp_str, nonce, signature = parts
    
    # Verify token signature
    if not manager.verify_token(
        token=token,
        owner_user_id=owner_user_id,
        task_id=task_id,
        approval_id=approval_id,
        action_hash=action_hash
    ):
        raise ValueError("Invalid token signature")
    
    # Atomic nonce consumption
    if not consume_nonce(nonce):
        raise ValueError(f"Token already used (replay attack detected)")
    
    # Load and return pending approval data
    task = get_task(task_id)
    if not task:
        raise ValueError(f"Task {task_id} not found")
    
    for checkpoint in reversed(task.checkpoints):
        if checkpoint.get("type") == "approval_pending" and \
           checkpoint.get("approval_id") == approval_id:
            return checkpoint
    
    raise ValueError(f"Approval {approval_id} not found in task {task_id}")
```

### CLI Approval Callback Integration

```python
# Updated src/channels/cli.py

from src.security.approval_flow import (
    ApprovalRequest,
    generate_approval_for_request,
    verify_and_consume_approval
)

def run_cli():
    # ... existing code ...
    
    while True:
        try:
            # ... get user input ...
            
            try:
                for output in graph.stream(initial_state, {"recursion_limit": 20}):
                    # ... process output ...
                    pass
                    
            except ApprovalRequest as approval_req:
                # Handle approval request
                approval_data = generate_approval_for_request(approval_req)
                
                console.print(Panel(
                    f"[bold yellow]需要批准[/bold yellow]\n\n"
                    f"工具: [bold]{approval_data['tool_name']}[/bold]\n"
                    f"参数: [bold]{json.dumps(approval_data['tool_args'], ensure_ascii=False)}[/bold]\n"
                    f"审批ID: [bold]{approval_data['approval_id']}[/bold]",
                    title="[bold red]高风险操作[/bold red]",
                    border_style="yellow"
                ))
                
                approved = Confirm.ask("[bold]是否批准此操作?[/bold]", default=False)
                
                if approved:
                    try:
                        # Verify and consume approval
                        checkpoint = verify_and_consume_approval(
                            token=approval_data["token"],
                            approval_id=approval_data["approval_id"],
                            task_id=approval_req.task_id,
                            owner_user_id=approval_req.owner_user_id,
                            action_hash=approval_data["action_hash"]
                        )
                        
                        # Resume execution with approved tool
                        # Restore state and continue
                        resume_state = {
                            "messages": checkpoint.get("snapshot", {}).get("messages", []),
                            "approved_tool": {
                                "name": approval_req.tool_name,
                                "args": approval_req.tool_args,
                                "call_id": approval_req.tool_call_id
                            }
                        }
                        
                        # Continue graph execution
                        # ...
                        
                    except ValueError as e:
                        console.print(f"[bold red]批准验证失败: {e}[/bold red]")
                else:
                    console.print("[yellow]操作已拒绝[/yellow]")
                    task.status = "Rejected"
                    save_task(task)
                    
        except KeyboardInterrupt:
            # ... existing code ...
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual approval prompts | ApprovalRequest exception | Phase 3 | Declarative, composable |
| In-memory state | Database persistence | Phase 3 | Survives restarts |
| Plain SHA256 hash | Canonical JSON hash | Phase 3 | TOCTOU protection |
| Separate verify/consume | Atomic token consumption | Phase 3 | Replay prevention |

**Deprecated/outdated:**
- Simple yes/no prompts without action verification
- Storing approval state in memory only
- Using timestamp-only for expiration (needs nonce)

## Open Questions

1. **LangGraph Interrupt Pattern**
   - What we know: LangGraph supports checkpointing via SqliteSaver/MemorySaver
   - What's unclear: How to properly integrate with ApprovalRequest exception pattern
   - Recommendation: Use exception-based flow control, catch at CLI level, resume with restored state

2. **Bingbu Agent Sandbox Isolation**
   - What we know: Need sandboxed code execution
   - What's unclear: Which sandboxing library (RestrictedPython, subprocess isolation, Docker)
   - Recommendation: Start with subprocess isolation (existing pattern), add RestrictedPython for in-process execution

3. **Approval Timeout Handling**
   - What we know: Tokens have expiration
   - What's unclear: How to handle expired pending approvals (auto-reject? notify?)
   - Recommendation: Auto-reject on token expiration, notify user via logs

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | pytest (already in requirements.txt) |
| Config file | tests/conftest.py (exists) |
| Quick run command | `pytest tests/security/test_approval_flow.py -x` |
| Full suite command | `pytest tests/ -v --cov=src` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HIL-01 | High-risk tools trigger ApprovalRequest | unit | `pytest tests/security/test_approval_flow.py::test_high_risk_raises_approval -x` | Wave 0 |
| HIL-02 | Action hash is canonical and verifiable | unit | `pytest tests/security/test_approval_flow.py::test_action_hash_canonical -x` | Wave 0 |
| HIL-03 | Token consumption is atomic | unit | `pytest tests/security/test_approval_flow.py::test_token_atomic_consume -x` | Wave 0 |
| HIL-03 | Replay attack is detected | unit | `pytest tests/security/test_approval_flow.py::test_replay_attack_blocked -x` | Wave 0 |
| HIL-04 | State persists across restart | integration | `pytest tests/integration/test_approval_resume.py -x` | Wave 0 |
| AGT-01 | Bingbu Agent can write code | unit | `pytest tests/agents/test_bingbu.py::test_code_writing -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `pytest tests/security/test_approval_flow.py -x`
- **Per wave merge:** `pytest tests/ -v --cov=src --cov-report=term-missing`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/security/test_approval_flow.py` - Tests for HIL-01, HIL-02, HIL-03
- [ ] `tests/integration/test_approval_resume.py` - Tests for HIL-04 state persistence
- [ ] `tests/agents/test_bingbu.py` - Tests for AGT-01 Bingbu Agent
- [ ] `tests/fixtures/approval_fixtures.py` - Shared fixtures for approval testing

## Sources

### Primary (HIGH confidence)
- Existing codebase analysis - approval.py, taint_engine.py, db.py, zhongshu.py, hubu.py
- requirements.txt - LangGraph version and dependencies
- tests/security/test_approval_tokens.py - Existing test patterns
- tests/security/test_taint_engine.py - Existing test patterns

### Secondary (MEDIUM confidence)
- Code patterns from Shangshu/Hubu agents for Bingbu implementation
- CLI approval_callback pattern for approval integration

### Tertiary (LOW confidence)
- None - all findings based on existing codebase

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Based on existing codebase and dependencies
- Architecture: HIGH - Patterns established in Shangshu/Hubu agents
- Pitfalls: HIGH - Derived from security requirements and existing security modules
- Implementation approach: MEDIUM - LangGraph checkpointing details need verification

**Research date:** 2026-04-04
**Valid until:** 30 days (stable architecture)
