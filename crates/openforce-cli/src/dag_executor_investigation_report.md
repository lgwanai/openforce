# Incident Investigation Report: dag_executor.rs Observability & Debuggability

**Severity**: SEV3 (Moderate)
**Incident Commander**: AI Incident Response Commander
**Date**: Investigation complete

---

## Executive Summary

The file `dag_executor.rs` implements DAG (Directed Acyclic Graph) wave computation and DAG construction for task scheduling — the core execution planner. **All four observability/debuggability acceptance criteria FAIL across the board.** The module is effectively a black box with zero instrumentation, zero metrics, zero error handling, and zero health visibility. This is a systemic reliability risk for any production system depending on it.

---

## Detailed Findings

### 1. STRUCTURED LOGGING — FAIL ❌

**Severity**: SEV3  
**Acceptance Criterion**: All critical operations log structured events with operation name, input size, and duration.

| Issue | Location | Impact |
|-------|----------|--------|
| Only 1 log statement in entire file | Line 83 | Complete silence during all normal execution paths |
| Uses format-string interpolation, not structured fields | Line 83: `tracing::warn!("circular deps: {} tasks in final wave", rem.len())` | Cannot query/search logs by field. Should be `tracing::warn!(count = rem.len(), "circular dependencies detected")` |
| `compute_waves()` — zero logs | Lines 44-97 | No record of: how many tasks were input, how many waves were produced, max concurrency computed |
| `build_dag()` — zero logs | Lines 108-138 | No record of: input task count, dependency inference actions, priority assignments |
| No `tracing::span!` usage | Entire file | No trace ID propagation or parent-child span relationships |
| No `info!` / `debug!` / `error!` usage | Entire file | Missing success path logging (info), diagnostic detail (debug), and failure logging (error) |
| No function-level timing instrumentation | Entire file | Impossible to identify slow DAG computations in production |

**Remediation Required**:
```rust
// Each public function needs:
pub fn compute_waves(tasks: &[DagTask]) -> ExecutionPlan {
    let span = tracing::info_span!("compute_waves", task_count = tasks.len());
    let _guard = span.enter();
    let start = std::time::Instant::now();
    // ... existing logic ...
    tracing::info!(
        task_count = tasks.len(),
        wave_count = waves.len(),
        max_concurrent = max_concurrent,
        duration_ms = start.elapsed().as_millis() as u64,
        "computed execution waves"
    );
    ExecutionPlan { waves, max_concurrent }
}
```

### 2. METRICS SYSTEM — FAIL ❌

**Severity**: SEV3  
**Acceptance Criterion**: Execution metrics (tasks started, succeeded, failed, latency histograms) are exported to a monitoring system (e.g., Prometheus).

| Issue | Location | Impact |
|-------|----------|--------|
| Zero metrics infrastructure | Entire file | No way to track execution performance over time |
| No execution counters | Entire file | Cannot monitor task throughput, success rates, or failure rates |
| No latency histograms | Entire file | Cannot detect DAG computation slowdowns or regressions |
| No `/metrics` endpoint | Not in file | No Prometheus scrape target |
| No metric export path anywhere | Entire file | Complete monitoring blind spot |

**Remediation Required**:
```rust
// Add using prometheus crate:
use lazy_static::lazy_static;
use prometheus::{register_int_counter_vec, IntCounterVec, register_histogram_vec, HistogramVec};

lazy_static! {
    static ref TASK_COUNTER: IntCounterVec = register_int_counter_vec!(
        "dag_tasks_total",
        "Total tasks processed",
        &["operation", "status"]  // e.g., compute_waves/success, build_dag/success
    ).unwrap();
    static ref DAG_COMPUTE_DURATION: HistogramVec = register_histogram_vec!(
        "dag_compute_duration_seconds",
        "Duration of DAG computation operations",
        &["operation"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
    ).unwrap();
}
```

### 3. ERROR HANDLING — FAIL ❌

**Severity**: SEV3  
**Acceptance Criterion**: Failure paths include enough context (task ID, step, error chain) to reconstruct the issue without accessing the task state.

| Issue | Location | Impact |
|-------|----------|--------|
| No function returns `Result` | Entire file | No error propagation at all — failures are silently swallowed |
| No `anyhow::Context` usage | Entire file | No error chain wrapping with task-identifying context |
| Circular dependencies handled silently | Lines 79-84 | Tasks with circular deps are dumped into final wave with only a `warn!` — no error returned, no task IDs listed, no escalation |
| `build_dag()` uses `.clone()` freely | Lines 124, 132, 135 | Excessive cloning with zero error checking; potential OOM on large inputs |
| No validation of dependency references | Lines 51-62 | If a dependency name doesn't match any task title, it's silently dropped — no warning, no error |
| Missing priority string validation | Lines 99-102 | Unknown priority strings silently mapped to "medium" — no warning about unrecognized values |
| No input validation | `compute_waves` line 44 | Empty task list would produce empty waves with max_concurrent=1 — no guard |
| Task IDs are opaque indices | Line 126: `format!("task-{}", i + 1)` | Task IDs lose connection to original planning IDs — makes cross-referencing impossible |

**Remediation Required**:
```rust
use anyhow::{Context, Result};

pub fn compute_waves(tasks: &[DagTask]) -> Result<ExecutionPlan> {
    anyhow::ensure!(!tasks.is_empty(), "compute_waves: empty task list");
    
    // ... existing logic ...
    
    if !rem.is_empty() {
        let circular_ids: Vec<&str> = rem.iter().map(|&i| tasks[i].title.as_str()).collect();
        anyhow::bail!(
            "circular dependencies detected for tasks: {:?}. \
             These tasks could not be topologically sorted.",
            circular_ids
        );
    }
    // ...
}

pub fn build_dag(tasks: &[...]) -> Result<Vec<DagTask>> {
    anyhow::ensure!(!tasks.is_empty(), "build_dag: empty task list");
    // ...
}
```

### 4. HEALTH / LIVENESS ENDPOINT — FAIL ❌

**Severity**: SEV3  
**Acceptance Criterion**: A health/liveness endpoint is available that reflects the executor's current status and dependency health.

| Issue | Location | Impact |
|-------|----------|--------|
| No health check endpoint | Entire file | Orchestration (K8s, Nomad) cannot probe executor health |
| No mechanism to detect stuck tasks | Entire file | If DAG computation hangs, no liveness probe will detect it |
| No deadlock detection | Entire file | The topological sort could infinite-loop on certain malformed inputs with no watchdog |
| No dependency health checking | Entire file | No way to verify upstream dependencies (database, planner service) are healthy |
| No `/health`, `/livez`, `/readyz` routes | Entire file | Incompatible with service mesh health checks |

**Remediation Required**:
```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use axum::{Router, routing::get, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub uptime_seconds: u64,
    pub last_successful_compute: Option<u64>,
    pub task_count: usize,
    pub circular_dependency_count: usize,
}

pub struct DagExecutorHealth {
    pub start_time: std::time::Instant,
    pub last_successful_compute: Arc<AtomicU64>,
    pub task_count: Arc<AtomicU64>,
    pub circular_dep_count: Arc<AtomicU64>,
}

impl DagExecutorHealth {
    pub fn router() -> Router {
        Router::new()
            .route("/health", get(|| async { Json(serde_json::json!({"status": "ok"})) }))
            .route("/livez", get(|| async { Json(serde_json::json!({"status": "alive"})) }))
            .route("/readyz", get(|| async { Json(serde_json::json!({"status": "ready"})) }))
    }
}
```

---

## Action Items

| ID | Action | Owner | Priority | Due | Status |
|----|--------|-------|----------|-----|--------|
| 1 | Add structured `tracing` spans + fields to all public functions (compute_waves, build_dag) | @eng | P1 | Next sprint | Not Started |
| 2 | Integrate Prometheus counters + histograms for DAG operations | @platform | P1 | Next sprint | Not Started |
| 3 | Convert functions to return `Result` types with `anyhow::Context` wrapping | @eng | P1 | Next sprint | Not Started |
| 4 | Add input validation (empty lists, circular deps as errors, missing deps warnings) | @eng | P1 | Next sprint | Not Started |
| 5 | Add `/health`, `/livez`, `/readyz` endpoints with stuck-task timeout detection | @platform | P2 | Next sprint+1 | Not Started |
| 6 | Replace format-string `tracing::warn!` with structured fields | @eng | P1 | Next sprint | Not Started |
| 7 | Add timing instrumentation to both public functions (compute waves, build dag) | @eng | P2 | Next sprint+1 | Not Started |
| 8 | Preserve original planning IDs in task identifiers instead of `task-N` | @eng | P2 | Next sprint+1 | Not Started |

---

## Lessons Learned

This module handles the **core scheduling logic** for task execution yet has:
- Zero observability — operations are invisible to operators
- Zero metrics — cannot monitor performance trends or detect regressions
- Zero error handling — failures are silently swallowed or ignored
- Zero health visibility — orchestration systems cannot verify it is working

**Root systemic cause**: The code was written as a proof-of-concept or prototype without production hardening. The gap is the lack of an observability-first development standard. Every public function should ship with logging, metrics, error handling, and health instrumentation as a baseline requirement — not as a post-hoc addition.

**Recommendation**: This module requires a reliability-focused refactor before it can be considered production-ready. File an engineering ticket categorizing this as technical debt with P1 priority.
