# Phase 2: Budget System Implementation - Research

**Researched:** 2026-04-04
**Domain:** LLM Budget Control, Async Task Management, Circuit Breaker Pattern
**Confidence:** MEDIUM (based on project code analysis and library documentation patterns)

## Summary

This phase implements a comprehensive budget control system to prevent resource runaway in the multi-agent orchestration system. The system needs to track token consumption, enforce time limits, control API costs, provide global circuit breaking, and isolate budgets for concurrent agents.

**Primary recommendation:** Implement a `BudgetManager` class with LangChain callbacks for token tracking, `asyncio.wait_for` for timeout handling, and a hierarchical budget allocation pattern for concurrent agent isolation. Use existing `TaskRecord.budget` field for persistence.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| BUD-01 | Token budget tracking - implement token_budget consumption tracking | LangChain `get_openai_callback` pattern, custom callback handlers, response metadata extraction |
| BUD-02 | Time budget limits - implement time_budget timeout circuit breaking | `asyncio.wait_for`, `asyncio.CancelledError`, task cancellation patterns |
| BUD-03 | Cost budget limits - implement cost_budget cost control | Token-to-cost conversion, pricing tables, provider-specific rate calculations |
| BUD-04 | Global circuit breaker - force terminate when budget exhausted | Circuit breaker state machine, budget exhaustion detection, graceful termination |
| BUD-05 | Concurrent agent budget isolation - independent budgets for child agents, prevent starvation | Hierarchical budget allocation, budget inheritance patterns, isolation via deepcopy |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| langchain-community | (installed) | `get_openai_callback` for token tracking | Standard LangChain pattern for OpenAI-compatible APIs |
| asyncio | stdlib | Timeout and cancellation | Native Python async support, no external dependencies |
| dataclasses | stdlib | Budget data structures | Immutable patterns, type safety |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tiktoken | optional | Accurate token counting | When exact token counts needed before API calls |
| pybreaker | optional | Circuit breaker pattern | If implementing complex failure state machine |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Custom callback | LangChain tracers | Tracers are heavier, callbacks are simpler for budget needs |
| asyncio.wait_for | signal.alarm | wait_for works with async, alarm is process-level only |
| Manual cost calculation | LiteLLM cost tracking | LiteLLM adds dependency, manual is sufficient for 3 providers |

**Installation:**
```bash
# Already in requirements.txt
pip install langchain langchain-community langchain-core

# Optional for accurate token counting
pip install tiktoken
```

## Architecture Patterns

### Recommended Project Structure
```
src/
├── budget/
│   ├── __init__.py
│   ├── manager.py      # BudgetManager main class
│   ├── trackers.py     # Token, Time, Cost trackers
│   ├── circuit_breaker.py  # Global fuse logic
│   └── callbacks.py    # LangChain callback handlers
└── core/
    └── db.py           # TaskRecord with budget field (existing)
```

### Pattern 1: Budget Manager with Context Manager

**What:** Central budget management with context-aware tracking
**When to use:** All LLM invocations should be wrapped with budget context

```python
# Source: LangChain pattern + project requirements
from dataclasses import dataclass, field
from typing import Dict, Any, Optional
import asyncio
import time

@dataclass
class BudgetLimits:
    """Budget limits for a task."""
    max_tokens: Optional[int] = None
    max_time_seconds: Optional[int] = None
    max_cost_usd: Optional[float] = None

@dataclass
class BudgetUsage:
    """Current budget consumption."""
    tokens_used: int = 0
    time_elapsed_seconds: float = 0.0
    cost_usd: float = 0.0
    start_time: float = field(default_factory=time.time)

    def check_exceeded(self, limits: BudgetLimits) -> Optional[str]:
        """Check if any budget limit is exceeded."""
        if limits.max_tokens and self.tokens_used > limits.max_tokens:
            return f"Token budget exceeded: {self.tokens_used}/{limits.max_tokens}"
        if limits.max_time_seconds:
            elapsed = time.time() - self.start_time
            if elapsed > limits.max_time_seconds:
                return f"Time budget exceeded: {elapsed:.1f}s/{limits.max_time_seconds}s"
        if limits.max_cost_usd and self.cost_usd > limits.max_cost_usd:
            return f"Cost budget exceeded: ${self.cost_usd:.4f}/${limits.max_cost_usd}"
        return None


class BudgetExhaustedError(Exception):
    """Raised when budget is exhausted."""
    def __init__(self, message: str, usage: BudgetUsage):
        self.message = message
        self.usage = usage
        super().__init__(message)


class BudgetManager:
    """
    Central budget management for agent tasks.

    Supports hierarchical budget allocation for concurrent agents.
    Thread-safe via asyncio locks for async operations.
    """

    def __init__(self, limits: BudgetLimits, parent: Optional['BudgetManager'] = None):
        self.limits = limits
        self.usage = BudgetUsage()
        self.parent = parent
        self._children: Dict[str, 'BudgetManager'] = {}
        self._exhausted = False
        self._lock = asyncio.Lock()

    def allocate_child(self, agent_id: str, limits: BudgetLimits) -> 'BudgetManager':
        """Allocate a child budget for a concurrent agent."""
        child = BudgetManager(limits=limits, parent=self)
        self._children[agent_id] = child
        return child

    async def consume_tokens(self, tokens: int) -> None:
        """Record token consumption, checking limits."""
        async with self._lock:
            self.usage.tokens_used += tokens
            if self.parent:
                await self.parent.consume_tokens(tokens)
            if exceeded := self.usage.check_exceeded(self.limits):
                self._exhausted = True
                raise BudgetExhaustedError(exceeded, self.usage)

    def is_exhausted(self) -> bool:
        """Check if budget is exhausted (for circuit breaker)."""
        if self._exhausted:
            return True
        if self.usage.check_exceeded(self.limits):
            self._exhausted = True
        return self._exhausted
```

### Pattern 2: LangChain Callback for Token Tracking

**What:** Custom callback handler to intercept LLM responses and extract token usage
**When to use:** All LLM invocations in the system

```python
# Source: LangChain callback pattern + project utils.py integration
from langchain_core.callbacks import BaseCallbackHandler
from langchain_core.outputs import LLMResult

class BudgetCallbackHandler(BaseCallbackHandler):
    """
    LangChain callback to track token usage for budget management.

    Works with OpenAI-compatible APIs that return token counts in response_metadata.
    """

    def __init__(self, budget_manager: BudgetManager):
        self.budget_manager = budget_manager

    def on_llm_end(self, response: LLMResult, **kwargs) -> None:
        """Extract token usage from LLM response."""
        if response.llm_output:
            # Standard OpenAI format
            token_usage = response.llm_output.get('token_usage', {})
            total_tokens = token_usage.get('total_tokens', 0)
            if total_tokens:
                # Fire and forget - don't block LLM response
                asyncio.create_task(self.budget_manager.consume_tokens(total_tokens))

        # Also check individual generations
        for generation_list in response.generations:
            for generation in generation_list:
                if hasattr(generation, 'generation_info') and generation.generation_info:
                    usage = generation.generation_info.get('usage', {})
                    total = usage.get('total_tokens', 0)
                    if total:
                        asyncio.create_task(self.budget_manager.consume_tokens(total))
```

### Pattern 3: Timeout with Forced Termination

**What:** asyncio.wait_for with proper cancellation handling
**When to use:** Every agent graph invocation

```python
# Source: Python asyncio patterns
import asyncio
from typing import TypeVar, Callable

T = TypeVar('T')

async def run_with_timeout(
    coro: Callable[..., T],
    timeout_seconds: int,
    budget_manager: BudgetManager
) -> T:
    """
    Run a coroutine with timeout and budget check.

    Raises:
        asyncio.TimeoutError: If timeout exceeded
        BudgetExhaustedError: If budget exhausted during execution
    """
    try:
        return await asyncio.wait_for(coro, timeout=timeout_seconds)
    except asyncio.TimeoutError:
        budget_manager._exhausted = True
        raise
    except asyncio.CancelledError:
        # Proper cleanup on cancellation
        budget_manager._exhausted = True
        raise BudgetExhaustedError("Task cancelled due to budget exhaustion", budget_manager.usage)


# Usage in agent invocation
async def invoke_agent_with_budget(graph, state, budget_manager: BudgetManager):
    """Invoke agent graph with budget protection."""
    if budget_manager.is_exhausted():
        raise BudgetExhaustedError("Budget already exhausted", budget_manager.usage)

    timeout = budget_manager.limits.max_time_seconds or 300

    try:
        return await run_with_timeout(
            graph.ainvoke(state),
            timeout,
            budget_manager
        )
    except (asyncio.TimeoutError, BudgetExhaustedError):
        # Graceful termination - return partial state
        return {"error": "Budget exhausted", "partial_state": state}
```

### Pattern 4: Hierarchical Budget Isolation (BUD-05)

**What:** Child agent budget allocation preventing starvation
**When to use:** When spawning concurrent child agents

```python
# Source: Multi-agent orchestration patterns
from dataclasses import replace
from typing import List

def allocate_child_budgets(
    parent_limits: BudgetLimits,
    child_count: int,
    strategy: str = "equal"
) -> List[BudgetLimits]:
    """
    Allocate budgets for child agents.

    Strategies:
    - "equal": Divide parent budget equally
    - "reserve": Keep 20% for parent, divide rest equally
    """
    if strategy == "equal":
        per_child = BudgetLimits(
            max_tokens=parent_limits.max_tokens // child_count if parent_limits.max_tokens else None,
            max_time_seconds=parent_limits.max_time_seconds,  # Time not divided
            max_cost_usd=parent_limits.max_cost_usd / child_count if parent_limits.max_cost_usd else None
        )
        return [per_child] * child_count

    elif strategy == "reserve":
        reserve_ratio = 0.2
        available_tokens = int(parent_limits.max_tokens * (1 - reserve_ratio)) if parent_limits.max_tokens else None
        available_cost = parent_limits.max_cost_usd * (1 - reserve_ratio) if parent_limits.max_cost_usd else None

        per_child = BudgetLimits(
            max_tokens=available_tokens // child_count if available_tokens else None,
            max_time_seconds=parent_limits.max_time_seconds,
            max_cost_usd=available_cost / child_count if available_cost else None
        )
        return [per_child] * child_count

    return [BudgetLimits()] * child_count
```

### Anti-Patterns to Avoid

- **Blocking budget checks in LLM call path:** Budget checking must be async/non-blocking to not add latency to LLM calls
- **Shared mutable budget state without locks:** Race conditions in concurrent agent scenarios
- **Ignoring cancellation cleanup:** CancelledError must be caught and budget marked exhausted
- **Budget inheritance without isolation:** Child agents must not consume from parent budget directly

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|------|
| Token counting | Custom tokenizer | tiktoken or API response metadata | Provider-specific tokenization varies |
| Circuit breaker state machine | Custom class | Simple boolean flag + exception | Complexity not needed for this use case |
| Time tracking | Manual timestamps | BudgetUsage.start_time + time.time() | Simpler to manage as single data structure |

**Key insight:** The budget system is primarily a monitoring and enforcement layer, not a complex state machine. Simplicity enables correctness.

## Common Pitfalls

### Pitfall 1: Race Condition in Budget Consumption
**What goes wrong:** Multiple concurrent agents consume budget simultaneously, causing over-consumption
**Why it happens:** No synchronization on budget state updates
**How to avoid:** Use `asyncio.Lock` for all budget state mutations
**Warning signs:** Budget usage exceeds limits, negative remaining budget

### Pitfall 2: Missing Token Data from Non-OpenAI APIs
**What goes wrong:** Token counts not available from Minimax, Kimi, or custom providers
**Why it happens:** These providers don't return token counts in standard format
**How to avoid:** Fallback to tiktoken estimation or response length approximation
**Warning signs:** `tokens_used` stays at 0 despite LLM calls

### Pitfall 3: Timeout Not Triggering Due to Non-Cancellable Operations
**What goes wrong:** Task hangs past timeout because internal operation is blocking
**Why it happens:** asyncio.wait_for only cancels the outer coroutine, not blocking I/O
**How to avoid:** Ensure all I/O uses async operations (httpx, aiofiles, etc.)
**Warning signs:** Tasks hang indefinitely despite timeout set

### Pitfall 4: Child Agent Starvation
**What goes wrong:** One child agent consumes entire budget, others get nothing
**Why it happens:** Budget shared without isolation
**How to avoid:** Allocate independent budget limits per child agent
**Warning signs:** Some agents complete, others fail with budget exhausted immediately

### Pitfall 5: Budget State Not Persisted
**What goes wrong:** Task restarted with fresh budget, allowing unlimited consumption
**Why it happens:** Budget only tracked in memory, not saved to TaskRecord
**How to avoid:** Update `TaskRecord.budget` field after each consumption event
**Warning signs:** Budget resets on task retry

## Code Examples

### Budget Integration with Existing invoke_llm_with_tools

```python
# Source: Project src/core/utils.py integration
from src.budget.manager import BudgetManager, BudgetExhaustedError
from src.budget.callbacks import BudgetCallbackHandler

def invoke_llm_with_tools_budget(
    llm,
    tools,
    messages,
    budget_manager: BudgetManager
):
    """
    Budget-aware wrapper for LLM invocation.

    Integrates with existing invoke_llm_with_tools utility.
    """
    if budget_manager.is_exhausted():
        raise BudgetExhaustedError("Budget exhausted before LLM call", budget_manager.usage)

    # Create callback handler for this invocation
    callback = BudgetCallbackHandler(budget_manager)

    # Use existing utility with callback
    try:
        # For standard providers
        model_name = getattr(llm, "model_name", "")
        if "minimax" not in model_name.lower() and "kimi" not in model_name.lower():
            # Standard flow with callback
            llm_with_tools = llm.bind_tools(tools)
            response = llm_with_tools.invoke(messages, config={"callbacks": [callback]})
            return ensure_tool_calls_parsed(response)
        else:
            # Non-standard providers - estimate tokens from response
            response = invoke_llm_with_tools(llm, tools, messages)
            # Estimate tokens: ~4 chars per token for Chinese/English mix
            estimated_tokens = len(str(response.content)) // 4
            asyncio.create_task(budget_manager.consume_tokens(estimated_tokens))
            return response

    except Exception as e:
        if isinstance(e, BudgetExhaustedError):
            raise
        # Log but don't consume budget on failure
        raise
```

### Budget Persistence to TaskRecord

```python
# Source: Integration with src/core/db.py
from src.core.db import TaskRecord, save_task

async def persist_budget_usage(task_id: str, budget_manager: BudgetManager):
    """Persist budget usage to TaskRecord."""
    from src.core.db import get_task
    task = get_task(task_id)
    if task:
        task.budget.update({
            "tokens_used": budget_manager.usage.tokens_used,
            "time_elapsed": time.time() - budget_manager.usage.start_time,
            "cost_usd": budget_manager.usage.cost_usd,
            "exhausted": budget_manager.is_exhausted(),
            "limits": {
                "max_tokens": budget_manager.limits.max_tokens,
                "max_time_seconds": budget_manager.limits.max_time_seconds,
                "max_cost_usd": budget_manager.limits.max_cost_usd,
            }
        })
        save_task(task)
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual token counting in prompts | LangChain callbacks for automatic tracking | LangChain 0.1+ | More accurate, less code |
| signal.alarm for timeouts | asyncio.wait_for | Python 3.7+ | Works with async, more reliable |
| Per-call budget checks | Central BudgetManager | This phase | Unified enforcement, easier testing |

**Deprecated/outdated:**
- `langchain.callbacks.openai_info.get_openai_callback`: Use custom callbacks instead for multi-provider support

## Open Questions

1. **Token counting for non-OpenAI providers (Minimax, Kimi)**
   - What we know: These providers don't return token counts in standard format
   - What's unclear: Exact tokenization rules for Chinese text
   - Recommendation: Use tiktoken with cl100k_base encoding (GPT-4 compatible) as approximation, or estimate from character count

2. **Cost calculation for Tencent Coding API**
   - What we know: Using Tencent Coding API with Minimax/GLM models
   - What's unclear: Pricing structure (per-token vs per-call)
   - Recommendation: Add cost_per_1k_tokens to config/models.yaml for each provider, default to conservative estimate

3. **Budget allocation for nested agent delegation**
   - What we know: Zhongshu -> Shangshu/Hubu delegation exists
   - What's unclear: Should budget be allocated once or on each delegation call
   - Recommendation: Allocate at task creation time, pass allocated budget to delegated agent

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | pytest (existing from Phase 1) |
| Config file | tests/conftest.py (existing) |
| Quick run command | `pytest tests/budget/ -x -v` |
| Full suite command | `pytest tests/ --cov=src/budget --cov-report=term-missing` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| BUD-01 | Token consumption tracking | unit | `pytest tests/budget/test_token_tracking.py -x` | Wave 0 |
| BUD-02 | Time budget enforcement | unit | `pytest tests/budget/test_time_budget.py -x` | Wave 0 |
| BUD-03 | Cost calculation and tracking | unit | `pytest tests/budget/test_cost_tracking.py -x` | Wave 0 |
| BUD-04 | Global circuit breaker | integration | `pytest tests/budget/test_circuit_breaker.py -x` | Wave 0 |
| BUD-05 | Concurrent agent isolation | integration | `pytest tests/budget/test_concurrent_isolation.py -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `pytest tests/budget/ -x`
- **Per wave merge:** `pytest tests/ --cov=src/budget`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/budget/__init__.py` - test package initialization
- [ ] `tests/budget/conftest.py` - shared fixtures (BudgetManager, mock LLM responses)
- [ ] `tests/budget/test_token_tracking.py` - covers BUD-01
- [ ] `tests/budget/test_time_budget.py` - covers BUD-02
- [ ] `tests/budget/test_cost_tracking.py` - covers BUD-03
- [ ] `tests/budget/test_circuit_breaker.py` - covers BUD-04
- [ ] `tests/budget/test_concurrent_isolation.py` - covers BUD-05

## Sources

### Primary (HIGH confidence)
- Project code analysis: `src/core/db.py`, `src/agents/zhongshu.py`, `src/core/utils.py`
- LangChain callback patterns: Standard library documentation

### Secondary (MEDIUM confidence)
- Python asyncio documentation: Timeout and cancellation patterns
- LangChain community patterns: Token tracking approaches

### Tertiary (LOW confidence)
- Provider-specific token counting (Minimax, Kimi): Needs verification with actual API responses

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Using existing installed libraries and stdlib
- Architecture: MEDIUM - Patterns are well-established but integration with existing code needs care
- Pitfalls: HIGH - Common async and budget management issues well-documented

**Research date:** 2026-04-04
**Valid until:** 30 days - stable patterns, long validity
