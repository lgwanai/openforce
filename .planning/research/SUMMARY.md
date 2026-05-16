# Project Research Summary

**Project:** SwarmOS v5.1
**Domain:** Enterprise Multi-Tenant AI Agent Orchestration Platform (Event-Sourced, gRPC, Rust)
**Researched:** 2026-05-16
**Confidence:** HIGH

## Executive Summary

SwarmOS v5.1 is an enterprise platform that orchestrates AI agents with production-grade reliability. It sits between two existing categories: **workflow engines** (Temporal, Prefect) that provide deterministic execution but treat AI as just another activity, and **agent frameworks** (LangGraph, CrewAI, AutoGPT) that provide AI-native orchestration but lack enterprise governance, multi-tenancy, and production controls. SwarmOS bridges this gap by combining a deterministic Rust scheduler with an LLM planner, event-sourced session storage, lease-based concurrency control with fencing tokens, and a formal Effect Gateway that gates all real-world side effects.

The recommended approach is a **Rust monorepo with 11 crates** organized around a three-plane architecture (Control, Data, Evolutionary). Internal services communicate via gRPC with mTLS and capability tokens. The single source of truth is a PostgreSQL-backed append-only Event Log using CAS (Compare-And-Set) semantics -- every state change is auditable, replayable, and recoverable. The stack is conservative and community-standard: **Tonic** for gRPC (uncontested standard), **SQLx** for database (compile-time SQL verification, JSONB support, LISTEN/NOTIFY), **Tokio** for async runtime, **tracing + OpenTelemetry** for observability, and **Wasmtime/Firecracker** for sandbox isolation. All version numbers were verified against crates.io as of the research date.

The key risks are: (1) **correctness regressions in concurrent CAS operations** -- single-threaded tests won't catch these, requiring property-based testing with `proptest` and deterministic simulation; (2) **prompt injection via tool outputs** -- the Worker's ReAct loop feeds untrusted file content into LLM context, demanding mandatory injection-defense guardrails from day one; (3) **projection rebuild time exceeding recovery SLAs** -- snapshotting and incremental rebuild must be built into Phase 1, not retrofitted later; and (4) **tenant data leakage through shared caches and observability** -- every data access layer must enforce tenant isolation at the type system level from Phase 1. Mitigations for each are detailed in PITFALLS.md.

## Key Findings

### Recommended Stack

The full stack is documented in [STACK.md](./STACK.md). All version numbers verified against crates.io as of research date.

**Core technologies:**

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| **Language** | Rust 1.86+ (stable) | Project mandate. Memory safety, zero-cost abstractions, strong type system for deterministic scheduler state machines. |
| **Async Runtime** | Tokio 1.52 | Uncontested Rust async standard (672M downloads). Work-stealing scheduler, channels, time, signals, I/O. |
| **gRPC Framework** | Tonic 0.14 + Prost 0.14 | De facto Rust gRPC stack (maintained by tokio-rs). Bidirectional streaming, mTLS via rustls, health checking, reflection. No viable alternative exists. |
| **Database Client** | SQLx 0.8 + PostgreSQL 16 | Compile-time SQL verification catches schema drift in CI. Raw SQL control for CAS transactions and append-only inserts. JSONB first-class support. `PgListener` for real-time projection updates. |
| **Serialization** | Prost (gRPC) + serde_json (JSONB/events/REST) | Prost generates idiomatic Rust structs. Serde handles event payloads in PostgreSQL JSONB columns. |
| **Observability** | tracing 0.1 + OpenTelemetry 0.32 | De facto Rust observability stack. Async-aware spans, `#[instrument]` macro, OTLP gRPC export to Jaeger/Tempo. |
| **Sandboxing** | Wasmtime 44 (Agent Space) + Firecracker (Execution Space) | WASM for lightweight ReAct loop isolation (millisecond startup). Firecracker microVMs for full-stack test execution with KVM-based isolation. |
| **Testing** | Testcontainers 0.27 + Proptest + Mockall | Real PostgreSQL in Docker for integration tests. Property-based testing for CAS and fencing edge cases. |
| **HTTP Gateway** | Axum 0.8 + tower-http 0.6 | Lightweight REST-to-gRPC proxy. Shares tower/hyper ecosystem with Tonic. |

**Key stack decisions:**
- **Build event sourcing from SQLx primitives, NOT a dedicated ES crate.** Rust has no mature event sourcing framework comparable to Akka Persistence or Marten. The established pattern is to build from PostgreSQL primitives (append-only tables, CAS transactions, materialized views).
- **Diesel ORM explicitly rejected for event store.** Its DSL restricts the raw SQL flexibility needed for append-only patterns and CAS versioning.
- **Not a full actor framework.** Tokio tasks with message-passing channels are sufficient. The concurrency model is one primary scheduler loop + N task-spawning operations.

### Expected Features

The full feature landscape is documented in [FEATURES.md](./FEATURES.md). Researched against Temporal, LangGraph, CrewAI, and AutoGPT.

**Must have for v1 (table stakes + core differentiators):**

| # | Feature | Category |
|---|---------|----------|
| TS-01 | Task State Machine (Pending/Ready/Leased/Running/Succeeded/Failed/TimedOut/Cancelled) | Table Stake |
| TS-02 | DAG Dependency Resolution | Table Stake |
| TS-03 | Retry with Backoff | Table Stake |
| TS-04 | Timeout Management (wall clock, execution, heartbeat) | Table Stake |
| TS-05 | Observability (metrics, logging, tracing via OpenTelemetry) | Table Stake |
| TS-06 | API Gateway (REST/gRPC with middleware chain) | Table Stake |
| TS-07 | Authentication & Authorization (mTLS + capability tokens) | Table Stake |
| TS-08 | Multi-Model LLM Support (pluggable provider abstraction) | Table Stake |
| TS-09 | Tool Calling Framework | Table Stake |
| TS-10 | Session/State Persistence (durable execution) | Table Stake |
| D-01 | Deterministic Scheduler + LLM Planner Separation | Differentiator |
| D-02 | Event-Sourced Session with Append-Only Event Log | Differentiator |
| D-03 | Lease + Fencing Token for Concurrent AI Workers | Differentiator |
| D-04 | Effect Gateway (basic: idempotency + approval) | Differentiator |
| D-05 | Frozen Worker Spec (immutable per-attempt execution spec) | Differentiator |
| D-06 | Project Tools (basic: ReadProjectFile, SubmitProjectPatch, path auth) | Differentiator |
| D-07 | Approval Token with Context Binding | Differentiator |

**Should have for v1.1 (enterprise governance):**
- D-08: Three-Space Isolation (full MicroVM isolation)
- D-06 Extended: Patch Classifier Semantic Grading (safe/moderate/sensitive/reject)
- D-09: Plan Epoch Semantics (Inherited/Frozen/Invalidated task classification)
- D-10: Multi-Tenant Data Isolation with Classification
- D-11: Kill Switch & Emergency Circuit Breaking
- D-04 Extended: Effect Gateway (Outbox/Inbox, compensation, reconciler)

**Defer to v2+ (self-optimization):**
- D-12: Evolutionary Plane (Observer/Evaluator/Evolver with canary releases)
- D-12 Extended: A/B Testing & Automated Rollback
- Advanced Multi-Region Scheduling

**Anti-features explicitly rejected:**
- Open-ended agent autonomy (unbounded cost, unreproducible results) -- every Worker operates within a frozen Spec with budget limits
- LLM as state machine (non-deterministic, can't replay or audit) -- deterministic Scheduler owns all state transitions
- Hot-reloading prompts in production (unauditable, impossible rollback) -- all prompts are versioned bundles with canary releases
- Worker-managed side effects (duplicate deployments, unbounded blast radius) -- all side effects go through Effect Gateway
- Platform-root credential sharing (single compromise = full platform compromise) -- per-Worker short-lived capability tokens

### Architecture Approach

The full architecture is documented in [ARCHITECTURE.md](./ARCHITECTURE.md). SwarmOS follows a strict **three-plane design:**

1. **Control Plane (8 services):** The deterministic brain. Gateway (REST/gRPC proxy), Session Store (sole writer to append-only Event Log with CAS), Scheduler (DAG advancement, lease issuance, heartbeat monitoring), Project Tools (path-authorized file operations with HITL), Approval Service (token issuance/validation/consumption), Effect Gateway (idempotent side effect gating), plus supporting crates.
2. **Data Plane (1 service):** Node Daemon manages Worker lifecycles, sandbox pool warming, WorkerSpec validation, capability token validation, heartbeat relay.
3. **Evolutionary Plane (1 service):** Observer/Evaluator/Evolver/Release Manager. Strictly read-only from control plane. Phase 3 only.

**Service decomposition (11 crates):** `swarmos-proto` (lib) -> `swarmos-domain` (lib) -> `session-store`, `scheduler`, `project-tools`, `approval-service`, `effect-gateway`, `policy-engine` -> `gateway` -> external. Plus `node-daemon` (data plane) and `evolver` (evolutionary plane).

**Key architectural decisions:**
- **Monorepo with 11 crates**: Shared proto definitions, atomic cross-cutting changes, consistent toolchain, reusable domain types.
- **Session Store is a singleton writer** to the Event Log. All CAS operations, fencing token validation, and event append atomicity happen in one place.
- **Each gRPC service is a separate binary/process** -- no in-process shared state between services. Communication via protobuf contracts.
- **Gateway is a pure REST-to-gRPC protocol translator** -- no business logic in the gateway layer.
- **Concurrency model: tokio tasks + message channels** (not a formal actor framework). The scheduler is a single main loop with `tokio::select!` patterns.
- **CAS via PostgreSQL advisory locks** (`pg_advisory_xact_lock`) with `session_version` as the CAS token. No UPDATE of historical events.

### Critical Pitfalls

The full pitfall catalog (16 critical/moderate, 3 agent-specific, 5 testing gaps) is documented in [PITFALLS.md](./PITFALLS.md). Top 5 that demand Phase 1 attention:

1. **Async Mutex Deadlock in gRPC Handler Chains** -- Holding a `tokio::sync::Mutex` across a gRPC call that transitively re-acquires the same lock freezes the entire tokio worker pool. **Prevention:** Never hold a tokio Mutex across gRPC calls. Use `std::sync::Mutex` for short-held non-async critical sections. Prefer message channels over shared mutable state.

2. **CAS Without Serialized Database Row Lock** -- Implementing CAS as read-then-write in application code creates a TOCTOU race. Between SELECT and UPDATE, another transaction commits. **Prevention:** Always use `SELECT ... FOR UPDATE` inside a transaction, or use atomic `UPDATE ... RETURNING` where the WHERE clause IS the CAS check.

3. **Scheduler Split-Brain from Missing Session Ownership Layer** -- CAS prevents double-writes but doesn't prevent two Scheduler instances from both believing they own a Session, wasting resources and spawning Workers whose outputs get rejected. **Prevention:** Implement a Session-level ownership lease (separate from task leases). Use PostgreSQL advisory lock or dedicated `session_ownership` table with TTL.

4. **Prompt Injection via Tool Outputs** -- Worker tool outputs (file content, command output) containing injection payloads are fed back into LLM context, causing model deviation. **Prevention:** Structured context separation with message roles, output sanitization before re-injection, tool output wrapping with trust markers, context budget for tool outputs.

5. **Incomplete Tenant Isolation in Shared Caches/Observability** -- Missing `tenant_id` on cache keys or log statements causes cross-tenant data leakage. **Prevention:** Mandatory tenant prefix on every data access layer. Use Rust type system (`TenantScoped<T>` wrapper). Tenant-aware logging facade that strips business data. PostgreSQL RLS as safety net.

## Implications for Roadmap

Based on combined research from all four files, here is the recommended phase structure. The dependency analysis from FEATURES.md and the build order from ARCHITECTURE.md are in strong agreement.

### Phase 1: Foundation -- Correctness First
**Rationale:** The Event Log + Session Store is the single source of truth. Nothing else works without correct, auditable state. Beginning here ensures every subsequent phase builds on a trustworthy foundation. Dependencies from the feature graph confirm: Session Store (TS-10, D-02) enables Lease Service (D-03), which enables Scheduler (D-01), which enables everything else.

**Delivers:** Append-only Event Log with CAS semantics, command dedup, projection builder with snapshots and incremental rebuild, Task State Machine, multi-tenant schema design from day one.

**Addresses features:** TS-01, TS-10, D-02
**Architecture components:** `swarmos-proto` (lib), `swarmos-domain` (lib), `session-store` (binary), `policy-engine` (lib)
**Stack elements:** SQLx, PostgreSQL, prost, serde, uuid, chrono, thiserror

**Must avoid pitfalls:** PF1 (async mutex deadlock), PF2 (CAS without row lock), PF5 (incomplete tenant isolation), PF6 (projection rebuild timeout), PF8 (unversioned events), PF10 (missing command idempotency)
**Mandatory tests:** CAS success, CAS conflict, command dedup replay, projection rebuild from event zero, snapshot + tail replay, versioned event upcast

### Phase 2: Scheduling Core & Worker Execution
**Rationale:** With trustworthy state, the Scheduler can reliably advance DAGs, issue leases, and manage Worker lifecycles. The Feature Dependencies graph shows: Session Store -> Lease Service -> Worker Spec Builder -> Scheduler -> DAG Resolution. This is the chain that makes the system actually run tasks.

**Delivers:** Deterministic Scheduler main loop, DAG dependency resolution, lease issuance/fencing/renewal/timeout, Worker Spec builder with frozen specs, retry with backoff, heartbeat monitoring, Node Daemon with sandbox management (ephemeral, not warm pool), multi-model LLM support, tool calling framework, API Gateway (REST/gRPC), auth (mTLS + capability tokens), full observability pipeline.

**Addresses features:** D-01, D-03, D-05, TS-02, TS-03, TS-04, TS-06, TS-07, TS-08, TS-09, TS-05
**Architecture components:** `scheduler`, `gateway`, `node-daemon` (basic)
**Stack elements:** Tonic server/client, mTLS, tower middleware, axum, tower-http, opentelemetry, wasmtime (Agent Space only, ephemeral)

**Must avoid pitfalls:** PF3 (scheduler split-brain), PF7 (lease clock skew), PF11 (compile time explosion), PF12 (blocking in async), PF13 (streaming without backpressure), Agent PF1 (unbounded ReAct loop), Agent PF2 (hallucinated tools), Agent PF3 (context window amnesia)
**Mandatory tests:** Fencing token rejects stale submissions, session ownership lease prevents split-brain, clock skew monitoring, streaming backpressure under load, Worker terminates on progress stall

### Phase 3: Safety Boundaries -- Side Effects & HITL
**Rationale:** With Workers executing tasks, the next priority is preventing harm. Project Tools gate file access with path authorization. Effect Gateway gates all real-world side effects with idempotency and approval. Approval Token binds human decisions to execution context. The Feature Dependencies graph shows these are independent of each other but all depend on lease service and session store from Phases 1-2.

**Delivers:** Project Tools (ReadProjectFile, SubmitProjectPatch, path authorization), Patch Semantic Classifier (safe/moderate/sensitive/reject), Approval Service (request lifecycle, token issuance/validation/consumption with context binding), Effect Gateway (idempotency, outbox pattern, manifest binding, reconciler).

**Addresses features:** D-06 (full), D-07, D-04 (basic + outbox/reconciler)
**Architecture components:** `project-tools`, `approval-service`, `effect-gateway`
**Stack elements:** sha2 (content binding), jsonwebtoken (approval tokens)

**Must avoid pitfalls:** PF4 (prompt injection via tools), PF14 (loose approval binding without content hash), PF16 (orphaned outbox messages), PF15 (dirty sandbox reuse -- mitigated by ephemeral sandboxes, not warm pool yet)
**Mandatory tests:** Path traversal rejection, patch semantic classification, approval token content binding (modify file after approval -> reject), outbox recovery after Dispatcher crash, idempotency_key dedup

### Phase 4: Enterprise Governance
**Rationale:** Multi-tenancy is designed from Phase 1 (tenant_id in every schema), but full enterprise governance requires tenant-level policies, data classification, fair scheduling, and circuit breakers. This phase is only safe after the core control plane (Phases 1-3) is stable -- you cannot enforce governance on a broken foundation.

**Delivers:** Multi-tenant data isolation with classification, tenant-level quotas and fair scheduling, data classification + model egress policies, Kill Switch with break-glass, BYOK key lifecycle, token revocation, tenancy offboarding flow, full Three-Space Isolation (Firecracker microVMs, warm pool with overlay filesystems).

**Addresses features:** D-08 (full), D-09, D-10 (full), D-11 (full)
**Architecture components:** Full `node-daemon` (warm pool), `policy-engine` extensions
**Stack elements:** Firecracker (Execution Space), dashmap, arc-swap

**Must avoid pitfalls:** PF9 (observer data pollution -- Observer collection starts but must be sanitized), PF15 (dirty sandbox reuse -- overlay filesystems + checksum verification)
**Mandatory tests:** Cross-tenant artifact access denied, RLS policy verification, kill switch activation mid-task, offboarding data purge verification, warm pool sandbox state reset

### Phase 5: Self-Optimization -- Evolutionary Plane
**Rationale:** The Evolutionary Plane (Observer/Evaluator/Evolver) can only operate on reliable metrics from a stable control plane. "First make it correct, then make it smart." All research files agree this is a v2+ feature. Observer data collection infrastructure should be laid in Phase 2 (passive) but the active Evolver pipeline must wait.

**Delivers:** Observer data collection (structured metrics only, no raw content), Evaluator quality scoring, Evolver prompt/SOP generation, Release Manager (grayscale, A/B testing, automated rollback), Red-Team scenario runner.

**Addresses features:** D-12 (full)
**Architecture components:** `evolver` (binary)
**Stack elements:** Read-only session-store gRPC client

**Must avoid pitfalls:** PF9 (observer data pollution -- sanitization pipeline mandatory before Observer collects any raw data)
**Mandatory tests:** Tenant data not present in Evolver-generated prompts, A/B rollback on quality regression, Red-Team scenario pass/fail automation

### Phase Ordering Rationale

1. **Correctness before scale:** Session Store + Scheduler foundation (Phases 1-2) must be correct before adding concurrent Workers or multi-tenancy complexity. You cannot retrofit CAS correctness.
2. **Safety before enterprise:** Effect Gateway + HITL (Phase 3) prevent harm before adding multi-tenant data isolation (Phase 4). Single-tenant safety is prerequisite to multi-tenant safety.
3. **Enterprise before evolution:** Tenant data policies must be enforced before any shared learning pipeline (Phase 5). The Observer cannot sample cross-tenant data without policy enforcement.
4. **Evolution last:** Self-optimization is impossible without reliable metrics and a stable control plane. This is by far the highest-complexity, lowest-urgency feature.

### Research Flags

**Phases needing deeper research during planning (`/gsd-research-phase`):**
- **Phase 3 (Safety Boundaries):** Patch semantic classification (patterns for grading edits as safe/moderate/sensitive), approval policy engine design (separation of duties, four-eyes principle), outbox pattern implementation details with PostgreSQL.
- **Phase 4 (Enterprise Governance):** Firecracker microVM orchestration in Rust (API surface, lifecycle management), BYOK key lifecycle with HSM integration, data classification taxonomy for AI agent workloads.
- **Phase 5 (Evolutionary Plane):** Prompt optimization algorithms that respect tenant data boundaries, A/B testing statistical framework for agent quality metrics, red-team scenario generation.

**Phases with well-documented patterns (skip research-phase):**
- **Phase 1 (Foundation):** Event sourcing on PostgreSQL, CAS transactions, SQLx patterns, and projection building are well-documented. The implementation is custom but the patterns are established.
- **Phase 2 (Scheduling Core):** gRPC service mesh with Tonic + Tower, lease/fencing patterns, DAG traversal, retry/timeout -- all standard distributed systems patterns with Rust-native implementations. No novel research needed.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | **HIGH** | All version numbers verified against crates.io. Tonic, SQLx, Tokio, and tracing are Rust community standards with official Context7 documentation. No speculative tech. |
| Features | **HIGH** | Validated against Context7 docs for Temporal (7323 snippets), LangGraph (1121), CrewAI (4460), and AutoGPT (354). Direct comparison of production feature sets. Gap analysis is evidence-based. |
| Architecture | **HIGH** | Based on project's own architecture design documents (31-section specification, gateway implementation spec, test case registry). gRPC patterns and DB patterns verified against Context7 docs for Tonic and SQLx. |
| Pitfalls | **MEDIUM** | Web search was unavailable during research. Claims about distributed systems patterns (fencing, clock skew, actor models) are from training data, not verified live. Context7-verified claims (Tonic streaming, SQLx transactions, Tokio Mutex) are HIGH confidence. Project architecture documents are authoritative PRIMARY sources. |

**Overall confidence:** **HIGH** -- stack and architecture decisions are well-founded on community standards and project specifications. The MEDIUM confidence on pitfalls is mitigated by the project's own architecture documents specifying exactly these concerns (CAS, fencing, isolation, event versioning).

### Gaps to Address

1. **Firecracker Rust integration surface area.** The STACK.md research confirms Firecracker as the right choice for Execution Space, but the actual Rust API for managing microVM lifecycles (create, boot, snapshot, destroy) needs validation during Phase 4 planning. Handled by: dedicated research-phase task before Phase 4 implementation.

2. **Patch semantic classification algorithm.** The FEATURES.md describes safe/moderate/sensitive/reject grading for code patches, but the actual classification approach (heuristic? embedding-based? rule-based?) is not specified. Handled by: prototype during Phase 3 planning with `gsd-research-phase`.

3. **Evolver prompt optimization with tenant data isolation.** The EVOLVE-03 requirement says Evolver generates prompts from Observer data, but cross-tenant data must never leak into prompts. The mechanism for tenant-scoped prompt optimization needs design. Handled by: research-phase before Phase 5.

4. **PostgreSQL RLS as multi-tenant safety net.** PITFALLS.md recommends RLS as a defensive layer, but the architecture documents don't specify RLS policies. Handled by: add RLS policy creation to the Phase 1 migration scripts as a "defense in depth" measure.

## Sources

### Primary (HIGH confidence)
- Context7 `/hyperium/tonic` — Tonic server setup, interceptors, TLS, health checks, client channels
- Context7 `/launchbadge/sqlx` — Compile-time queries, transactions, advisory locks, LISTEN/NOTIFY, pool management, migrations, JSONB
- Context7 `/tokio-rs/prost` — Prost message encoding, build.rs code generation
- Context7 `/tokio-rs/tracing` — Tracing spans, subscriber composition, OpenTelemetry bridge
- Context7 `/open-telemetry/opentelemetry-rust` — OTLP gRPC exporter, metrics, logs setup
- Context7 `/bytecodealliance/wasmtime` — WASM embedding API, WASI, host functions
- Context7 `/tower-rs/tower` — ServiceBuilder middleware composition
- Context7 `/testcontainers/testcontainers-rs` — Docker Compose integration, Postgres module
- Context7 `/websites/temporal_io` (7323 snippets) — Enterprise features, multi-tenancy, security, determinism, retry policies
- Context7 `/websites/langchain_oss_python_langgraph` (1121 snippets) — Durable execution, HITL (interrupt), checkpointer, subgraphs
- Context7 `/websites/crewai_en` (4460 snippets) — Guardrails, memory, observability, task delegation
- crates.io API — Version verification for all crates listed in STACK.md

### Secondary (MEDIUM confidence)
- SwarmOS Architecture Design (`architecture_v5.0_dynamic_orchestration.md` — 31 sections) — Three-plane design, Event-Sourced Session, lease/fencing, Effect Gateway, tenant governance. Primary domain reference.
- SwarmOS Gateway Implementation (`swarmos_v5.1_gateway_implementation.md`) — Middleware chain, REST-to-gRPC mapping
- SwarmOS Gateway Mapping (`swarmos_v5.1_gateway_mapping.md`) — gRPC method to REST path mapping, error code tables
- SwarmOS Project Tools HITL (`swarmos_v5.1_project_tools_hitl_implementation.md`) — Approval Token, Patch Classifier, HITL state machine
- SwarmOS Test Case Registry (`swarmos_v5.1_test_case_registry.md`) — 24 standardized test cases
- SwarmOS Red Team Plan (`swarmos_v5.1_launch_validation_red_team_plan.md`) — 17 security/test scenarios
- Rust async book — Async patterns, deadlock prevention, Mutex usage (training data, cross-referenced)
- Distributed systems literature — Fencing token design (Kleppmann), lease-based leadership, clock skew (training data)

### Tertiary (LOW confidence -- needs validation)
- Evolver prompt optimization algorithms — training data only; needs dedicated research-phase before Phase 5
- Patch semantic classification approaches — training data only; needs prototype validation in Phase 3
- Firecracker Rust orchestration patterns — project choice justified but implementation surface needs validation

---
*Research completed: 2026-05-16*
*Ready for roadmap: yes*
