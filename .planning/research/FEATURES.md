# Feature Landscape

**Domain:** Enterprise Multi-Tenant AI Agent Orchestration Platform
**Researched:** 2026-05-16
**Confidence:** HIGH
**Sources:** Context7 (Temporal, LangGraph, CrewAI, AutoGPT official docs), SwarmOS architecture v5.0-v5.1 design docs, industry pattern analysis

---

## Executive Summary

The AI agent orchestration market is bifurcated: **workflow engines** (Temporal, Prefect) provide deterministic execution but treat AI as just another activity; **agent frameworks** (LangGraph, CrewAI, AutoGPT) provide AI-native orchestration but lack enterprise governance, multi-tenancy, and production controls. No existing platform delivers the combination SwarmOS targets: deterministic scheduling + event-sourced audit + AI-specific isolation + enterprise governance in a single system.

The feature landscape below identifies what must exist for v1, what differentiates SwarmOS, and what to explicitly avoid. Features are validated against Temporal, LangGraph, CrewAI, AutoGPT, and production deployment patterns.

---

## Table Stakes (Users Expect These)

Features all enterprise orchestration platforms must have. Missing any = product feels broken.

| # | Feature | Why Expected | Complexity | Gap in Existing Solutions |
|---|---------|--------------|------------|---------------------------|
| TS-01 | **Task State Machine** with Pending/Ready/Running/Succeeded/Failed/TimedOut/Cancelled | Every orchestration platform (Temporal, Prefect, Airflow) has this. Users demand visibility into task lifecycle. | LOW | Temporal, LangGraph, CrewAI all have this. Table stakes. |
| TS-02 | **DAG Dependency Resolution** — tasks advance only when upstream dependencies satisfied | Core scheduling primitive. Both Temporal (workflow-as-code) and LangGraph (graph edges) enforce this. | MEDIUM | Temporal via code structure; LangGraph via edges. Standard. |
| TS-03 | **Retry with Backoff** — automatic retry on transient failures with configurable backoff | Production systems fail. Users expect retry without custom code. Temporal's retry policies set the bar. | LOW | Temporal: excellent; LangGraph: via checkpointer resume; CrewAI: guardrail retries. Standard. |
| TS-04 | **Timeout Management** — per-task wall clock, execution, heartbeat timeouts | Without timeouts, hung tasks consume resources indefinitely. Temporal has 4 timeout types. | LOW | Temporal: comprehensive; LangGraph: no native timeout system; CrewAI: none. Standard. |
| TS-05 | **Observability** — metrics, structured logging, distributed tracing | Enterprises require observability for SOC2/ISO compliance. OpenTelemetry is standard. | MEDIUM | Temporal: built-in metrics + Cloud UI; LangGraph: LangSmith integration; CrewAI: Opik/Portkey. Standard. |
| TS-06 | **API Gateway** — REST/gRPC API with auth, rate limiting, middleware chain | External integration requires a consistent API surface. Every production platform has this. | MEDIUM | Standard pattern across all platforms. Table stakes. |
| TS-07 | **Authentication & Authorization** — service identity (mTLS), RBAC, token-based access | Multi-service systems require identity. Temporal has mTLS + claims; LangGraph Cloud has auth. | MEDIUM | Temporal: mTLS + JWT claims; LangGraph Cloud: API key auth. Standard. |
| TS-08 | **Multi-Model LLM Support** — pluggable provider abstraction for OpenAI, Anthropic, etc. | Agents use different models for different tasks. Users expect provider flexibility. | LOW | LangGraph/CrewAI/AutoGPT all support multiple providers via LangChain/own abstractions. Table stakes. |
| TS-09 | **Tool Calling Framework** — standardized tool definition, invocation, result handling | AI agents need tools to interact with the world. Function calling is table stakes. | MEDIUM | LangGraph: tool binding; CrewAI: tool decorators; AutoGPT: command registry. Standard. |
| TS-10 | **Session/State Persistence** — durable state across agent runs, process restarts, deployments | Long-running AI tasks span hours/days. Temporal's durability and LangGraph's checkpointer set the expectation. | HIGH | Temporal: event history replay; LangGraph: checkpoint persistence. Standard. |

---

## Differentiators (Competitive Advantage)

Features that set SwarmOS apart. These are where we compete against Temporal, LangGraph, CrewAI, and AutoGPT.

| # | Feature | Value Proposition | Complexity | Why Competitors Don't Have This |
|---|---------|-------------------|------------|--------------------------------|
| D-01 | **Deterministic Scheduler + LLM Planner Separation** | Planner proposes, Scheduler disposes. LLM creativity is governed by a deterministic state machine that is reproducible, auditable, and bounded. No other platform enforces this architectural split. | HIGH | Temporal: workflows are deterministic code, not LLM-driven. LangGraph: LLM can be any node -- no scheduler gate. CrewAI: LLM-driven delegation only. |
| D-02 | **Event-Sourced Session with Append-Only Event Log** | Complete causal chain for every state change: who/what/why/when. Enables audit, replay, debt recovery, and compliance. Temporal has event history but it's opaque binary replay, not a queryable audit log. | HIGH | Temporal: event history exists but is opaque, used only for replay. No AI-specific events. LangGraph: checkpoints, not events. CrewAI: no event log. AutoGPT: no persistence layer. |
| D-03 | **Lease + Fencing Token for Concurrent AI Workers** | Prevents dual writes from expired Workers. Every write validated against current lease and monotonic fencing token. No other AI orchestration platform has this. Temporal has task queues but no explicit fencing for concurrent AI output. | HIGH | Temporal: task reassignment exists but no application-level fencing token exposed. LangGraph: no lease concept. CrewAI: sequential processes only. Gap clearly exists. |
| D-04 | **Effect Gateway** — all real-world side effects go through a single, auditable gateway | Deployments, DB migrations, emails, API calls -- all gated, idempotent, approval-controlled, and compensating. No other platform separates "thinking" from "acting" with a formal gateway. | HIGH | Temporal: Activities are side effects but no separate gateway. LangGraph: no side effect isolation. CrewAI: tools are unconstrained. This is a significant gap. |
| D-05 | **Frozen Worker Spec** — immutable execution specification per task attempt | Every Worker launched with a signed, versioned, immutable spec binding: prompt_bundle, tool_policy, sandbox_image, inputs, budget. Enables exact replay, audit, and A/B comparison. Temporal has workflow definitions but they aren't frozen per-attempt with content-addressed integrity. | HIGH | Temporal: workflow code versioned, but not content-addressed per execution. LangGraph: no spec concept. CrewAI: no frozen spec. No platform freezes all execution parameters per task attempt. |
| D-06 | **Project Tools with Path-Based Authorization + HITL** | Workers can only read/write specific paths (allowed_read_paths, allowed_write_paths, forbidden_paths). Sensitive operations (deletes, core config changes) auto-escalate to human approval with Patch semantic classification. | HIGH | LangGraph: interrupt() for HITL but no path-level authorization or semantic patch classification. CrewAI: guardrails but no workspace tools. Temporal: no file operations. Clear differentiator. |
| D-07 | **Approval Token with Strong Context Binding** | Approval tokens bound to session_id + task_id + task_attempt + lease_id + fencing_token + snapshot_id + content_hash. Any context change invalidates token. Single-use by default. No other platform binds approval to this full context chain. | MEDIUM | LangGraph: interrupt/resume but no approval token model. CrewAI: human checkpoints but no binding. Gap exists across all platforms. |
| D-08 | **Three-Space Isolation** (Agent Space / Project Workspace / Execution Space) | Agent sandbox (lightweight, LLM-only) decoupled from workspace (shared VFS) decoupled from test sandbox (full runtime with DB). Prevents cross-contamination and enforces least privilege. | HIGH | No platform separates these spaces. LangGraph runs in single process. CrewAI runs in single process. Temporal: Workers run anywhere but no explicit space model. Unique architecture. |
| D-09 | **Plan Epoch with Inherited/Frozen/Invalidated Task Semantics** | When re-planning occurs, old tasks are explicitly classified and old results cannot silently merge into new plans. Prevents plan version contamination. | MEDIUM | Temporal: workflow code can change but no explicit epoch semantics. LangGraph: no formal re-planning. CrewAI: no plan versioning. Unique feature. |
| D-10 | **Multi-Tenant Data Isolation with Classification** | Tenant Secret, Business, Operational, and Derived Learning data classified with policy enforcement at every boundary (model egress, observer sampling, cross-region transfer). Temporal has Namespace isolation but no data classification. | HIGH | Temporal: Namespace isolation (logical) but no content-aware data policies. LangGraph/CrewAI/AutoGPT: no multi-tenant isolation at all. Major gap. |
| D-11 | **Kill Switch & Emergency Circuit Breaking** | Tenant-level isolation, model endpoint blocking, effect freezing, bundle revocation -- all with TTL, audit, and recovery. No other platform has this as first-class feature. | MEDIUM | Temporal: no kill switch (operational). LangGraph: none. CrewAI: none. Unique. |
| D-12 | **Evolutionary Plane with Versioned, Canary-Released Prompts** | Observer/Evaluator/Evolver pipeline operates out-of-band. New prompt/SOP/policy versions go through canary release, A/B testing, and rollback. Never silently replaces production. | HIGH | No platform has formal evolutionary optimization pipeline. Temporal has no AI optimization. LangGraph has no versioned prompt management. Unique. |

---

## Anti-Features (Commonly Requested, Often Problematic)

Features that seem good but create problems at enterprise scale.

| # | Feature | Why Requested | Why Problematic | Alternative in SwarmOS |
|---|---------|---------------|-----------------|----------------------|
| AF-01 | **"Open-ended Agent Autonomy"** — agents that freely decide what to do next without bounded task scope | Users want AI to "figure it out" | Leads to unbounded cost, unreproducible results, and security failures. AutoGPT's original flaw was unbounded autonomy leading to infinite loops. | Each Worker operates within a frozen Worker Spec with budget limits (max_tokens, max_tool_calls, max_wall_clock). Micro-planning allowed within spec boundary only. |
| AF-02 | **"Single Global Shared File System"** — all Workers read/write the same directory | Simplifies Worker setup | Causes lost updates, conflicts, cross-tenant leakage. Classic distributed system anti-pattern. | Project Workspace as controlled VFS with SubmitPatch protocol, Merge Service, and path-based authorization. No direct filesystem access. |
| AF-03 | **"LLM as State Machine"** — using LLM output to directly control task state transitions | Reduces code complexity | Non-deterministic. Cannot replay, audit, or guarantee correctness. LLM hallucinations become state corruption. | Deterministic Scheduler owns all state transitions. LLM only participates in planning and content generation, never state management. |
| AF-04 | **"Hot-Reloading Prompts in Production"** — updating prompts without version control | Fast iteration on prompt quality | Unauditable changes, impossible rollback, A/B contamination. "Which prompt caused that error?" becomes unanswerable. | All prompts/Policies/SOPs are versioned bundles. Worker Spec freezes versions at launch. New versions go through canary release pipeline. |
| AF-05 | **"Worker-Managed Side Effects"** — letting Workers directly call production APIs, databases, deployment tools | Reduces architectural complexity | Duplicate deployments, unbounded blast radius, no audit trail for external actions. Classic "agent calling APIs directly" failure mode. | All side effects go through Effect Gateway with idempotency_key, approval policies, outbox pattern, and compensating transactions. |
| AF-06 | **"Platform-Root Credential Sharing"** — all Workers share a single set of credentials | Simplifies credential management | Single compromised Worker = full platform compromise. No audit trail per Worker. Violates least privilege. | Each Worker gets short-lived capability tokens scoped to specific task/lease/tools. Node Daemon, Worker, Scheduler each have distinct identities. |
| AF-07 | **"Cross-Tenant Shared Training Data"** — mixing data from all tenants for model improvement | Better model performance through more data | GDPR/CCPA violation, data leakage, tenant trust destruction. Enterprise tenants will not tolerate their code/logs training competitors' models. | Data classified by sensitivity. Observer/Evolver only consume de-identified, aggregated, opt-in data. Per-tenant training_opt_in policy enforced. |
| AF-08 | **"Real-Time Streaming Everywhere"** — streaming all intermediate outputs to users | Feels responsive | Creates false sense of progress. Users make decisions on incomplete agent output. Adds complexity without value for most workflows. | Streaming reserved for specific HITL checkpoints and status updates. Batch delivery for final artifacts. "Show progress, not noise." |

---

## Feature Dependencies

```
Session Store (TS-10) + Event Log (D-02)
  └──requires──> PostgreSQL with CAS transactions
  └──enables──> Lease Service (D-03)
  └──enables──> Plan Epoch (D-09)
  └──enables──> Projection Builder (TS-05)

Lease Service (D-03)
  └──requires──> Session Store (TS-10)
  └──required_by──> Worker Spec Builder (D-05)
  └──required_by──> Fencing Token Validation (D-03)

Worker Spec Builder (D-05)
  └──requires──> Lease Service (D-03)
  └──requires──> Prompt Bundle Registry (D-12)
  └──required_by──> Scheduler (D-01)

Deterministic Scheduler (D-01)
  └──requires──> Session Store (TS-10)
  └──requires──> Lease Service (D-03)
  └──requires──> Worker Spec Builder (D-05)
  └──required_by──> Task State Machine (TS-01)
  └──required_by──> DAG Resolution (TS-02)

Project Tools (D-06)
  └──requires──> Lease Service (D-03)
  └──requires──> Worker Spec Builder (D-05) for path policies
  └──required_by──> HITL State Machine
  └──required_by──> Patch Classifier

Effect Gateway (D-04)
  └──requires──> Lease Service (D-03)
  └──requires──> Approval Token (D-07)
  └──requires──> Outbox/Inbox infrastructure
  └──enables──> Idempotency (TS-04)

Approval Token (D-07)
  └──requires──> Lease Service (D-03)
  └──required_by──> Effect Gateway (D-04)
  └──required_by──> Project Tools HITL (D-06)

Three-Space Isolation (D-08)
  └──requires──> Node Daemon / Sandbox infrastructure
  └──required_by──> Secure Worker Execution (D-03)

Multi-Tenant Isolation (D-10)
  └──requires──> Session Store with tenant_id primary key
  └──required_by──> Kill Switch (D-11)
  └──required_by──> Data Classification (D-10)

Evolutionary Plane (D-12)
  └──requires──> Event Log (D-02) for sample collection
  └──requires──> Multi-Tenant Data Policies (D-10)
  └──requires──> Bundle Versioning/Canary infrastructure
  └──NOT required for MVP (Phase 3 feature)

Kill Switch (D-11)
  └──requires──> Multi-Tenant Isolation (D-10)
  └──requires──> Policy Engine
  └──required_by──> Production readiness
```

### Dependency Notes

- **D-01 (Deterministic Scheduler) requires TS-10 + D-03:** The Scheduler's state machine operates on Session state and needs Lease Service for task assignment. Must be built after foundation.
- **D-06 (Project Tools) requires D-03 + D-05:** Path authorization comes from Worker Spec; lease/fencing validation adds safety. Can be built partially without full fencing for v0 but not for v1.
- **D-04 (Effect Gateway) requires D-07 + Outbox:** Cannot implement Effect Gateway without Approval Token model and a reliable message delivery pattern (outbox).
- **D-12 (Evolutionary Plane) is Phase 3 only:** Observer/Evolver depend on Event Log and tenant data policies. Must not be built before core control plane is stable. "First make it correct, then make it smart."
- **D-08 (Three-Space Isolation) is architecture-level:** Must be designed into Worker/Sandbox from day one even if Phase 1 uses simplified isolation (single-container with namespace separation).

---

## MVP Definition

### Launch With (v1) — "Correctness First"

Minimum viable product that validates the core value proposition: making AI agents reliable in production.

- [ ] **TS-01: Task State Machine** — Foundation for all scheduling. Every other feature depends on task states.
- [ ] **TS-10: Session/State Persistence** (D-02: Event-Sourced Session) — Append-only event log is the single source of truth. Cannot build anything reliable without it.
- [ ] **D-01: Deterministic Scheduler** — The core differentiator. Separates LLM planning from deterministic execution.
- [ ] **D-03: Lease + Fencing Token** — Prevents dual-write disasters. Essential for concurrent Worker safety.
- [ ] **TS-02: DAG Dependency Resolution** — Required for multi-step AI workflows.
- [ ] **TS-03/TS-04: Retry + Timeout Management** — Production reliability baseline.
- [ ] **D-05: Frozen Worker Spec** — Enables audit, replay, and A/B comparison from day one.
- [ ] **D-04: Effect Gateway** (core: idempotency + basic approval) — Side effect safety cannot be retrofitted.
- [ ] **D-06: Project Tools** (core: ReadProjectFile, SubmitProjectPatch, path authorization) — Worker interaction with workspace.
- [ ] **D-07: Approval Token** (core binding) — Required for any sensitive operation HITL.
- [ ] **TS-05: Observability** (metrics, logging, tracing) — Production systems need visibility.
- [ ] **TS-06: API Gateway** (REST/gRPC with middleware) — External interface.
- [ ] **TS-07: Authentication & Authorization** (mTLS + capability tokens) — Security baseline.
- [ ] **TS-08: Multi-Model LLM Support** — Required for different task types.
- [ ] **TS-09: Tool Calling Framework** — Required for agent interaction with tools.

### Add in v1.1 — "Enterprise Governance"

- [ ] **D-10: Multi-Tenant Data Isolation** (full classification and policies) — Required for enterprise customers but single-tenant can validate product first.
- [ ] **D-06 Extended: Patch Classifier Semantic Grading** — SAFE/MODERATE/SENSITIVE/REJECT with auto-escalation.
- [ ] **D-08: Three-Space Isolation** (full MicroVM isolation) — Can start with container-level isolation and upgrade.
- [ ] **D-09: Plan Epoch Semantics** — Important for complex re-planning scenarios.
- [ ] **D-04 Extended: Effect Gateway** (Outbox/Inbox, compensation, reconciler) — Full side effect reliability.
- [ ] **TS-07 Extended: Full RBAC + ABAC** — Tenant-level role management.
- [ ] **D-11: Kill Switch** (basic tenant isolation) — Emergency controls.

### v2+ — "Self-Optimization"

- [ ] **D-12: Evolutionary Plane** (Observer/Evaluator/Evolver with canary releases) — Only after control plane is battle-tested. "First make it correct, then make it smart."
- [ ] **D-12 Extended: A/B Testing & Automated Rollback** — Requires stable metrics baseline.
- [ ] **D-11 Extended: Platform-Level Circuit Breaking** — After tenant-level kill switches are proven.
- [ ] **Advanced Multi-Region Scheduling** — After single-region reliability established.

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Risk if Missing | Priority |
|---------|-----------|---------------------|-----------------|----------|
| TS-01: Task State Machine | HIGH | LOW | HIGH (no scheduling possible) | **P1** |
| D-02: Event-Sourced Session | HIGH | HIGH | HIGH (no audit, no replay, no recovery) | **P1** |
| D-01: Deterministic Scheduler | HIGH | HIGH | HIGH (no reliable execution) | **P1** |
| D-03: Lease + Fencing Token | HIGH | MEDIUM | HIGH (dual-write corruption) | **P1** |
| D-05: Frozen Worker Spec | HIGH | MEDIUM | MEDIUM (no audit/replay) | **P1** |
| D-04: Effect Gateway (basic) | HIGH | HIGH | HIGH (uncontrolled side effects) | **P1** |
| D-06: Project Tools (basic) | HIGH | MEDIUM | MEDIUM (no worker interaction) | **P1** |
| TS-10: State Persistence | HIGH | HIGH | HIGH (no durability) | **P1** |
| D-07: Approval Token | MEDIUM | MEDIUM | MEDIUM (no HITL safety) | **P1** |
| TS-05: Observability | HIGH | MEDIUM | MEDIUM (no visibility) | **P1** |
| D-10: Multi-Tenant Isolation | MEDIUM | HIGH | LOW (single-tenant first) | **P2** |
| D-08: Three-Space Isolation | HIGH | HIGH | MEDIUM (container isolation sufficient initially) | **P2** |
| D-06 Ext: Patch Classifier | MEDIUM | MEDIUM | LOW (manual categorization works initially) | **P2** |
| D-11: Kill Switch | MEDIUM | LOW | LOW (manual intervention works initially) | **P2** |
| D-12: Evolutionary Plane | LOW | HIGH | LOW (manual optimization works) | **P3** |

---

## Competitor Feature Analysis

| Feature | Temporal | LangGraph | CrewAI | AutoGPT | SwarmOS (Our Plan) |
|---------|----------|-----------|--------|---------|-------------------|
| **Execution Model** | Deterministic code workflows | LLM-driven graph nodes | Role-based sequential/hierarchical | Continuous autonomous agent | **Deterministic scheduler + LLM planner (hybrid)** |
| **Durable Execution** | Event history replay | Checkpointer (state persistence) | No durability layer | No persistence | **Event-sourced append-only Event Log + Projection** |
| **Multi-Tenancy** | Namespace isolation (logical) | Thread isolation only | None | None | **Full tenant isolation with data classification** |
| **Concurrency Safety** | Task queues (no explicit fencing) | Single-thread per thread_id | Sequential processes only | Single agent | **Lease + Fencing Token + CAS transactions** |
| **HITL / Interrupts** | Signals (generic) | interrupt() function | Human checkpoints | None | **Project Tools HITL + Approval Token binding + Patch classifier** |
| **Side Effect Management** | Activities (no gateway) | Tool calls (direct) | Tool calls (direct) | Command registry (direct) | **Effect Gateway with idempotency, approval, compensation** |
| **Worker Isolation** | Worker processes (same env) | Single Python process | Single Python process | Single Python process | **Three-space isolation (Agent/Workspace/Target)** |
| **Prompt/SOP Versioning** | N/A (code versioning) | No versioning | No versioning | No versioning | **Frozen Worker Spec + Bundle canary releases** |
| **Audit Trail** | Event history (queryable via tctl) | Checkpoint history | None | None | **Full event log with causal chain per action** |
| **Emergency Controls** | None (operational) | None | None | None | **Kill switch, break-glass, bundle revocation** |
| **Self-Optimization** | None | None | None | None | **Evolutionary Plane (Phase 3, versioned, canary)** |
| **Language** | Go, Java, Python, TypeScript, .NET, Rust | Python, JavaScript | Python | Python | **Rust (full-stack)** |

### Competitor Gap Analysis

**Temporal Strengths:** Best-in-class deterministic execution, retry policies, multi-language SDKs, production-proven at scale. Excellent event history for replay.

**Temporal Weaknesses for AI:** Treats AI as just another activity. No AI-specific primitives (prompt versioning, model selection, tool policies). Event history exists but is opaque binary -- not designed as queryable audit trail. No data classification for LLM egress. No Effect Gateway concept. Namespace isolation but no content-aware tenant data policies.

**LangGraph Strengths:** Best-in-class AI-native graph orchestration. Excellent HITL with interrupt(). Subgraph model enables multi-agent. Durable execution via checkpointer. Streaming support.

**LangGraph Weaknesses for Enterprise:** No multi-tenancy. No deterministic scheduling layer (LLM can be any node). No lease/fencing for concurrent safety. No side effect isolation gateway. No frozen execution specs. Checkpoints are state snapshots, not auditable event logs. No kill switch. No data classification for LLM calls.

**CrewAI Strengths:** Intuitive role-based agent model. Built-in guardrails. Good for quick multi-agent prototypes. Observability integration.

**CrewAI Weaknesses for Enterprise:** Python-only, single-process execution. No persistence beyond in-memory. No concurrency safety. No enterprise governance (multi-tenancy, data isolation, audit). Guardrails are output-level only, not workspace/path-level. No deterministic scheduler.

**AutoGPT Strengths:** Autonomous agent concept. Plugin architecture. Good for experimentation.

**AutoGPT Weaknesses for Enterprise:** Unbounded autonomy leads to cost/behavior explosions. No production readiness. No multi-tenancy. No audit. No concurrency. Research tool, not production platform.

---

## Gap Analysis: What SwarmOS Fills

### Gap 1: No platform has AI-specific deterministic scheduling
Temporal has deterministic workflows but they're code, not AI decisions. LangGraph lets LLMs directly control execution flow. SwarmOS's Planner/Scheduler split is unique: Planner proposes, deterministic Scheduler commits and advances.

### Gap 2: No platform has AI-specific audit trail
Temporal's event history is for replay, not audit. LangGraph's checkpoints are state snapshots, not causal event chains. SwarmOS's append-only Event Log with event type enumeration, causation_id, and correlation_id provides an AI-specific audit trail that answers "who did what, with which prompt, under which lease, with what result."

### Gap 3: No platform has proper concurrency control for AI Workers
Temporal has task queues but no application-level fencing token for AI output. LangGraph has no concurrent execution model. SwarmOS's Lease/Fencing/Idempotent Command pattern prevents the dual-write problems that plague multi-Agent systems.

### Gap 4: No platform separates "thinking" from "acting" with a formal gateway
Effect Gateway is unique. Every other platform allows agents to directly execute side effects. SwarmOS enforces that all real-world mutations go through a single auditable gateway with idempotency, approval, and compensation.

### Gap 5: No platform has enterprise multi-tenancy for AI
Temporal has Namespace isolation for workflow orchestration, but no data classification, model egress policies, or tenant-level kill switches. LangGraph/CrewAI/AutoGPT have zero multi-tenancy. SwarmOS is designed multi-tenant from day one with data classification, tenant policies, and isolation controls.

---

## Phase Structure Implications

Based on dependency analysis and competitor gaps, the recommended phase structure:

### Phase 1: Core Control Plane (Foundation)
Focus: Make one task run correctly, once. Prove correctness.
- Session Store + Event Log (TS-10, D-02)
- Task State Machine (TS-01)
- Command Handler + CAS transactions
- Projection Builder
- Single-tenant, single-region, single Scheduler

### Phase 2: Worker Execution (Correctness at Scale)
Focus: Make many tasks run correctly, concurrently.
- Lease Service + Fencing (D-03)
- Worker Spec Builder (D-05)
- Deterministic Scheduler (D-01)
- DAG Resolution (TS-02)
- Retry + Timeout (TS-03, TS-04)
- Node Daemon + Sandbox (D-08 simplified)
- Multi-Model LLM Support (TS-08)
- Tool Calling Framework (TS-09)
- API Gateway (TS-06)
- AuthN/AuthZ (TS-07)
- Observability (TS-05)

### Phase 3: Safety Boundaries
Focus: Prevent bad things from happening.
- Project Tools + Path Authorization (D-06 basic)
- Effect Gateway + Idempotency (D-04 basic)
- Approval Token (D-07)
- HITL State Machine (D-06 extended)
- Patch Classifier (D-06 extended)
- Basic Kill Switch (D-11)

### Phase 4: Enterprise Governance
Focus: Make it safe for multiple organizations.
- Multi-Tenant Data Isolation (D-10)
- Data Classification + Policies
- Tenant Quotas + Fair Scheduling
- Effect Gateway Extended (Outbox/Inbox, Compensation)
- Full Kill Switch + Break-Glass (D-11)

### Phase 5: Self-Optimization (Evolutionary Plane)
Focus: Get smarter over time, safely.
- Observer + Evaluator (D-12)
- Bundle Versioning + Canary Release
- A/B Testing + Automated Rollback
- Prompt/SOP Evolution Pipeline

### Why This Order?
1. **Correctness before scale:** Event-sourced Session + Scheduler is the foundation. Everything else builds on trustworthy state.
2. **Safety before enterprise:** Effect Gateway + HITL prevent harm before adding multi-tenancy complexity.
3. **Enterprise before evolution:** Multi-tenant data isolation must exist before any shared learning pipeline.
4. **Evolution last:** Self-optimization is impossible without reliable metrics and stable control plane.

---

## Sources

- **Temporal**: Context7 `/websites/temporal_io` (7323 snippets), `/temporalio/documentation` (4552 snippets) — enterprise features, multi-tenancy, security, determinism, retry policies, Namespace isolation
- **LangGraph**: Context7 `/websites/langchain_oss_python_langgraph` (1121 snippets) — durable execution, HITL (interrupt), checkpointer, subgraphs, streaming, multi-agent, state management
- **CrewAI**: Context7 `/websites/crewai_en` (4460 snippets), `/crewaiinc/crewai` (2843 snippets) — guardrails, memory, observability, enterprise features, task delegation
- **AutoGPT**: Context7 `/significant-gravitas/autogpt` (354 snippets) — component architecture, agent forge, command registry
- **SwarmOS Architecture**: `architecture_v5.0_dynamic_orchestration.md` (31 sections), `swarmos_v5.1_project_tools_hitl_implementation.md`, `swarmos_v5.1_test_case_registry.md`

---

*Feature research for: Enterprise Multi-Tenant AI Agent Orchestration Platform*
*Researched: 2026-05-16*
*Next: Use this research to inform requirements definition and roadmap creation.*
