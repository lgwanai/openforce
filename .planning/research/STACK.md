# Stack Research: SwarmOS v5.1

**Domain:** Distributed AI Agent Orchestration Platform (Event-Sourced, gRPC, Multi-Tenant)
**Researched:** 2026-05-16
**Confidence:** HIGH

## Recommended Stack

### Core Framework & Runtime

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Rust (stable) | 1.86+ | Systems language | Project mandate; memory safety, zero-cost abstractions, strong type system for deterministic scheduling |
| Tokio | 1.52.3 | Async runtime | Industry standard for Rust async. Work-stealing scheduler, rich ecosystem (channels, time, signals, I/O). 672M+ downloads. Essential for gRPC server, multi-worker concurrency, and distributed task coordination. |
| Tonic | 0.14.6 | gRPC framework | De facto standard Rust gRPC implementation (274M+ downloads). Built on hyper + tower. Native async/await. Supports unary, server/client/bidirectional streaming. gRPC health checking, reflection, mTLS via rustls. Tight prost integration. |
| Prost | 0.14.3 | Protobuf codegen | Canonical protobuf for Rust (409M+ downloads). Maintained by tokio-rs team. Generates clean, idiomatic Rust structs. Tonic depends on prost. Supports proto2 and proto3. |
| Tower | 0.5.x | Middleware framework | Modular middleware composition for gRPC services. Buffer, concurrency_limit, rate_limit, timeout, retry, load_shed layers. Powers tonic interceptors and service composition. |

### Database & Event Sourcing

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| PostgreSQL | 16+ | Primary database | Project mandate. Proven for event sourcing patterns. Supports JSONB for flexible event payloads. ACID transactions for CAS writes. LISTEN/NOTIFY for real-time projection updates. |
| SQLx | 0.8.6 | Async PostgreSQL client | **Best fit for event sourcing** (97M+ downloads). Compile-time SQL verification catches schema errors at build time. Full async support with tokio. Raw SQL control needed for append-only event inserts and CAS transactions. JSONB support for event payloads. PgListener for real-time NOTIFY. Built-in migrations. Connection pooling. Transaction support with savepoints. |
| sqlx-cli | 0.8.x | Migration tool | Manages migration scripts. `sqlx migrate run`, `sqlx migrate revert`, `sqlx migrate add`. Compile-time query checking requires `cargo sqlx prepare`. |

### Serialization & Data Formats

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Prost | 0.14.3 | gRPC message serialization | Same as above. Generated types used for all internal service contracts (Worker Spec, Session events, Lease commands, etc.). |
| serde + serde_json | 1.x | JSON serialization | For REST gateway responses, configuration files, event payloads stored in PostgreSQL JSONB columns. Serde is the Rust serialization standard. |
| serde_bytes | 0.11.x | Efficient byte serialization | For binary payloads in event log. Avoids base64 overhead of default serde byte handling. |
| uuid | 1.x | ID generation | Standard for generating globally unique IDs (event_id, session_id, lease_id, worker_spec_id). UUIDv7 recommended for time-ordered primary keys. |

### Observability

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Tracing | 0.1.44 | Structured logging & spans | De facto Rust observability framework (596M+ downloads). Async-aware structured events with spans for causality tracking. `#[instrument]` macro for automatic function tracing. Essential for debugging distributed agent workflows. |
| tracing-subscriber | 0.3.23 | Log/trace subscriber | Composable layer system (415M+ downloads). EnvFilter for dynamic log levels. Multiple output formats (JSON, pretty, compact). File rotation via tracing-appender. |
| tracing-opentelemetry | 0.32.x | OpenTelemetry bridge | Connects tracing spans to OTEL collectors. Enables distributed trace visualization in Jaeger/Tempo. |
| OpenTelemetry (opentelemetry) | 0.32.0 | OTEL API + SDK | Standards-based observability (189M+ downloads). Traces, metrics, and logs via OTEL protocol. |
| opentelemetry-otlp | 0.32.0 | OTLP exporter | Exports traces/metrics/logs to OTEL collector via gRPC or HTTP (101M+ downloads). Uses tonic under the hood. |
| metrics | 0.24.x | Application metrics | Lightweight metrics facade. Gauges, counters, histograms. Pluggable exporters. |
| metrics-exporter-prometheus | 0.16.x | Prometheus exporter | Exposes `/metrics` endpoint for Prometheus scraping. For platform operators. |

### Testing

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Testcontainers | 0.27.3 | Integration testing | Spins up real PostgreSQL (and other services) in Docker for integration tests (23M+ downloads). Ensures tests run against actual database behavior, not mocks. Supports Docker Compose for multi-service setups. Postgres module comes pre-built. |
| tokio::test | 1.x | Async test runtime | Macro for async test functions using tokio runtime. Standard for Rust async projects. |
| sqlx::test | 0.8.x | Database test fixtures | SQLx provides test fixtures and in-test migration support. Can auto-create/drop test databases. |
| proptest | 1.x | Property-based testing | For testing CAS semantics, fencing token logic, and event ordering. Generates random sequences of concurrent operations to verify invariants. |
| mockall | 0.14.x | Mocking framework | For unit testing gRPC service handlers. Generate mock implementations of service traits. |
| rstest | 0.24.x | Fixture-based tests | Parametrized tests, test fixtures. Cleaner than raw `#[test]` for complex scenarios. |

### Sandboxing & Isolation

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Wasmtime | 44.0.1 | Agent Space sandbox | Lightweight WASM runtime for executing Agent ReAct loops (23M+ downloads). Millisecond startup, fine-grained memory isolation via linear memory, capability-based security via WASI. Embeddable Rust API. Best fit for Agent Space where only Python/Node agent logic runs. |
| Firecracker (via firectl/containerd) | latest | Execution/Target Space sandbox | Full microVM isolation for running arbitrary code (full-stack tests, dependencies). KVM-based security boundary. Stronger than containers for multi-tenant isolation. Slower startup (100-200ms) but appropriate for batch test execution. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tower-http | 0.6.x | HTTP middleware | CORS, tracing, auth, request-id, compression for REST gateway. Used alongside tonic for the REST↔gRPC gateway. |
| axum | 0.8.x | REST gateway | Lightweight HTTP framework for the REST↔gRPC translation layer. Built on tower + hyper, shares ecosystem with tonic. |
| rustls | 0.23.x | TLS | Modern TLS implementation in Rust. Used by tonic for mTLS between services. No OpenSSL dependency. |
| tokio-stream | 0.1.x | Stream utilities | Bridges sync/async streams. Used with tonic bidirectional streaming RPCs. |
| futures | 0.3.x | Future combinators | StreamExt, TryStreamExt, FutureExt. Complements tokio's channel system. |
| async-trait | 0.1.x | Async trait methods | Required for defining async methods in traits (tonic service definitions). Stabilized in Rust 1.75, but tonic still uses it. |
| chrono | 0.4.x | DateTime handling | Timezone-aware timestamps for event log. All events carry `occurred_at` in UTC. |
| thiserror | 2.x | Error types | Derive macro for structured error types. Each service module defines its error enum. |
| clap | 4.x | CLI argument parsing | For service binaries (scheduler, gateway, worker daemon). Derive-based API. |
| config | 0.15.x | Configuration management | Layered config (file, env, defaults). Multi-tenant configuration per tenant. |
| jsonwebtoken | 9.x | JWT handling | Capability token issuance and validation. Worker-to-control-plane auth. |
| sha2 | 0.10.x | Content hashing | SHA-256 for artifact/manifest verification, idempotency key derivation. |
| ring | 0.17.x | Cryptography | Fast crypto primitives. Used by rustls, jwt, and hash verification. |
| dashmap | 6.x | Concurrent HashMap | Lock-free concurrent map for in-memory caches (session projections, policy lookups). |
| arc-swap | 1.x | Atomic pointer swap | Zero-downtime configuration hot-reload. Atomically swap policy/bundle references. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| cargo | Build system | Standard Rust build tool. `cargo build`, `cargo test`, `cargo run`. |
| cargo-watch | Auto-reload on changes | `cargo watch -x run` for development. |
| sqlx-cli | Database migrations | `sqlx migrate add <name>`, `sqlx migrate run`. |
| protoc | Protobuf compiler | Required by prost-build. Install via system package manager. |
| grpcurl | gRPC debugging | Like curl but for gRPC. `grpcurl -plaintext localhost:50051 list`. |
| docker / docker-compose | Container runtime | For testcontainers, local PostgreSQL, OTEL collector, sandbox testing. |
| just | Command runner | Modern Make alternative. `just test`, `just migrate`, `just proto`. |
| cargo-deny | License/security audit | Check dependency licenses and security advisories. |
| cargo-audit | Security audit | Check for known vulnerabilities in dependencies. |

## Installation

```bash
# Core framework
cargo add tokio --features full
cargo add tonic --features tls,tls-roots
cargo add prost

# Database
cargo add sqlx --features runtime-tokio-rustls,postgres,json,migrate,chrono,uuid
cargo add uuid --features v7,serde

# Serialization
cargo add serde --features derive
cargo add serde_json
cargo add serde_bytes

# Observability
cargo add tracing
cargo add tracing-subscriber --features json,env-filter
cargo add tracing-opentelemetry
cargo add opentelemetry
cargo add opentelemetry-otlp --features grpc-tonic,http-proto
cargo add opentelemetry_sdk --features rt-tokio
cargo add metrics
cargo add metrics-exporter-prometheus

# HTTP / REST
cargo add axum
cargo add tower-http --features cors,trace,auth,request-id,compression

# Supporting
cargo add chrono --features serde
cargo add thiserror
cargo add clap --features derive
cargo add config
cargo add jsonwebtoken
cargo add sha2
cargo add dashmap
cargo add arc-swap
cargo add async-trait
cargo add futures
cargo add tokio-stream

# Build dependencies
cargo add --build prost-build
cargo add --build tonic-build

# Dev dependencies
cargo add --dev testcontainers --features postgres
cargo add --dev testcontainers-modules --features postgres
cargo add --dev proptest
cargo add --dev mockall
cargo add --dev rstest
cargo add --dev tokio --features test-util
```

## Detailed Rationale by Domain

### 1. gRPC Framework: Tonic (No Contest)

Tonic is the unambiguous standard for gRPC in Rust. The alternatives are essentially non-existent for production use:

**Why Tonic:**
- Built by the same team behind tokio and prost, guaranteeing ecosystem cohesion
- Tower-based middleware layer enables request-level auth, rate limiting, timeout enforcement without custom gRPC interceptors
- gRPC health checking protocol (`grpc.health.v1`) built-in for Kubernetes readiness probes
- Server reflection for tooling compatibility (`grpcurl`, `evans`)
- Bidirectional streaming for long-lived Worker heartbeat connections
- Native mTLS via rustls for inter-service authentication

**What NOT to use:**
- **grpcio**: C++ binding via FFI. Non-idiomatic Rust, memory safety concerns, no async/await. Deprecated by CNCF.
- **grpc-rust**: Abandoned. No async support. Last updated 2019.
- **Custom HTTP/2 + protobuf**: Reinventing gRPC framing, codegen, and streaming at scale is unjustifiable.

**Confidence: HIGH** — tonic is the Rust community's settled choice, maintained by the core tokio-rs organization.

### 2. Event Sourcing: Build from SQLx Primitives (No Dedicated Crate)

Rust does not have a dominant event sourcing framework comparable to Akka Persistence (Scala), EventStoreDB client, or Marten (.NET). The ecosystem pattern is to build event stores from PostgreSQL primitives.

**Why build from SQLx rather than use a dedicated crate:**

| Approach | Pros | Cons |
|----------|------|------|
| Dedicated ES crate (e.g., `cqrs-es`, `eventsourcing`) | Higher-level abstractions | Immature (few downloads), opinionated event/model coupling, limited PostgreSQL-specific optimizations, maintenance risk |
| **Build from SQLx** | Full control over event table schema, CAS semantics, projection strategy. Compile-time SQL verification. | More initial code |

**Recommended patterns:**
- **Event table:** `(event_id UUID, session_id UUID, event_type TEXT, event_version BIGINT, payload JSONB, occurred_at TIMESTAMPTZ, ...)` — append-only, no UPDATE
- **CAS via PostgreSQL transactions:** `INSERT INTO events ... WHERE NOT EXISTS (SELECT 1 FROM events WHERE session_id = $1 AND event_version = $2)` — or use `INSERT ... ON CONFLICT DO NOTHING` with version unique constraint
- **Projections:** Materialized views rebuilt from event log on demand. Debezium/CDC for streaming projection updates.
- **Snapshots:** Periodic snapshot events to avoid full replays.

**Confidence: HIGH** — this is the established pattern in Rust event sourcing. Projects like `arroyo`, `materialize`, and `feldera` build event processing on PostgreSQL primitives.

### 3. PostgreSQL Client: SQLx > tokio-postgres > Diesel (for Event Sourcing)

| Criterion | SQLx 0.8.6 | tokio-postgres 0.7.17 | Diesel 2.3.9 |
|-----------|------------|----------------------|--------------|
| **Compile-time SQL checking** | Yes (`query!` macro) | No | Yes (DSL macros) |
| **Raw SQL flexibility** | Full | Full | Limited (DSL-first) |
| **Async/await** | Yes | Yes | Via diesel_async |
| **JSONB support** | First-class | Manual (serde) | Via diesel_json |
| **Migrations** | Built-in | None | Built-in (diesel_cli) |
| **LISTEN/NOTIFY** | PgListener API | Manual | Not supported |
| **Connection pool** | Built-in (deadpool-based) | Via deadpool/bb8 | Via deadpool |
| **Type safety** | Compile-time + runtime | Runtime only | Compile-time (DSL) |
| **Downloads** | 97M | 45M | 27M |

**Verdict: SQLx for event sourcing because:**
1. Event sourcing requires raw SQL control — append-only inserts, CAS transactions, version-based conflict detection. Diesel's DSL abstraction gets in the way.
2. `PgListener` for PostgreSQL NOTIFY enables real-time projection updates without polling.
3. JSONB first-class support for flexible event payloads.
4. Compile-time query verification catches schema drift in CI before deployment.
5. Migration support integrated — no separate tool needed.

**When to use alternatives:**
- **tokio-postgres**: If you need extremely fine-grained connection control (custom pooling, pipeline mode). But SQLx wraps it with better ergonomics.
- **Diesel**: If you have a CRUD-heavy service with well-known schemas that benefit from DSL type-checking. Not for flexible event stores.

**Confidence: HIGH** — verified against sqlx official docs, Context7, and crates.io data.

### 4. Serialization: Prost + serde (Uncontested)

Prost is the defacto protobuf implementation in Rust (409M downloads vs ~5M for alternatives).

**Why Prost:**
- Maintained by the tokio-rs organization — same team as tonic and tokio
- Generates clean, idiomatic Rust structs (not C++-style builder patterns like `rust-protobuf`)
- `Message` trait for encode/decode with zero-copy where possible
- `prost-build` for build.rs codegen, `prost-types` for well-known types
- `prost-reflect` for dynamic protobuf reflection if needed

**Why serde + serde_json:**
- Event payloads in JSONB columns need JSON serialization
- REST gateway responses are JSON
- Configuration files, policy definitions
- Serde is the universal Rust serialization framework

**What NOT to use:**
- **rust-protobuf**: Generates verbose, non-idiomatic code with builder patterns. 10x fewer users.
- **quick-protobuf**: Niche, limited type support.
- **Manual bytes manipulation**: No.

**Confidence: HIGH.**

### 5. Async Runtime: Tokio (Uncontested)

Tokio is the Rust async runtime. 672M downloads. No realistic alternative for this project.

**Key patterns for SwarmOS:**

```
Graceful shutdown:
  tokio::select! {
      _ = tokio::signal::ctrl_c() => { /* drain connections */ }
      result = server.serve(addrs) => { /* server exited */ }
  }

Worker heartbeat channels:
  let (tx, rx) = tokio::sync::mpsc::channel(256);
  // tx sent to worker, rx held by scheduler for timeout monitoring

Concurrent scheduling tick:
  tokio::spawn(async move {
      let mut interval = tokio::time::interval(Duration::from_secs(1));
      loop {
          interval.tick().await;
          scheduler_tick(session_id).await;
      }
  });

CPU-bound work offloading:
  tokio::task::spawn_blocking(|| {
      // Heavy computation: plan compilation, AST merging
  });
```

**What NOT to use:**
- **async-std**: Largely dormant. Tokio has won the ecosystem.
- **smol**: Lightweight alternative but lacks ecosystem maturity for distributed systems.
- **Custom event loop**: Unjustifiable for a project this size.

**Confidence: HIGH.**

### 6. Testing: Testcontainers + SQLx Test Fixtures + Proptest

**Approach:**

```
Unit tests:
  - Service handler logic with mocked gRPC clients (mockall)
  - Scheduler state machine transitions (pure function tests)
  - Event payload validation (serde round-trip tests)
  - CAS transaction logic (with mock database trait)

Integration tests (testcontainers):
  - Full event store: write events, CAS conflict, projection rebuild
  - Lease service: lease issuance, renewal, expiry, fencing
  - gRPC endpoint: request/response, error codes, streaming
  - Database migrations: up/down cycle

E2E tests:
  - Docker Compose: PostgreSQL + scheduler + worker daemon + gateway
  - Full workflow: PlanCompiled → TaskLeased → TaskStarted → TaskSucceeded
  - Fencing token test: lease A + lease B, verify A's writes rejected
  - Idempotency: duplicate SubmitArtifact with same command_id

Property-based tests (proptest):
  - CAS conflict scenarios with concurrent lease attempts
  - Event ordering invariants under out-of-order delivery
  - Plan epoch transition correctness
```

**Test organization:**
```
crates/session-store/
  tests/
    unit/          # Pure function tests, no I/O
    integration/   # Requires testcontainers PostgreSQL
    e2e/           # Requires full docker-compose stack
```

**Confidence: MEDIUM-HIGH** — testcontainers-rs is mature (v0.27) but the e2e patterns for gRPC services require careful setup. Verified against testcontainers-rs Context7 docs.

### 7. Observability: tracing + OpenTelemetry (Standard Stack)

The Rust observability stack is mature and well-integrated.

**Architecture:**

```
Application Code
    |
    v
tracing (spans, events, #[instrument])
    |
    +---> tracing-subscriber (fmt layer: stdout JSON/pretty)
    |         +-- EnvFilter: RUST_LOG=swarmos=debug,info
    |
    +---> tracing-opentelemetry layer
              |
              v
         opentelemetry-otlp (gRPC exporter)
              |
              v
         OTEL Collector (aggregation, batching)
              |
              +---> Jaeger/Tempo (traces)
              +---> Prometheus (metrics via metrics-exporter-prometheus)
              +---> Loki/Elasticsearch (logs)
```

**Key instrumentation points for SwarmOS:**
- Every gRPC request gets a span with `session_id`, `tenant_id`, `task_id`
- Scheduler tick: span per tick cycle with task counts
- Event append: span with `event_type`, `session_version`
- Worker heartbeat: span with `lease_id`, `fencing_token`
- Effect execution: span covering approval → dispatch → commit lifecycle
- Custom metrics: `lease_renew_failure_rate`, `task_timeout_rate`, `projection_lag_seconds`

**What NOT to use:**
- **log crate alone**: Lacks structured fields, spans, and async context propagation
- **env_logger**: Legacy approach, incompatible with tracing's span model
- **slog**: Functional but ecosystem has converged on tracing

**Confidence: HIGH** — the tracing + opentelemetry stack is the Rust community standard for 2025/2026.

### 8. Sandboxing: WASM for Agent Space, Firecracker for Execution Space

This project's three-space architecture (Agent Space / Project Workspace / Execution Space) maps cleanly to two different sandbox technologies:

**Agent Space → Wasmtime (WASM)**
- The Agent runs a lightweight ReAct loop: call LLM API, parse response, invoke tools, repeat
- This is compute-light, I/O-bound work that doesn't need a full OS
- WASM provides: millisecond startup, capability-based security (WASI), fine-grained resource limits
- Wasmtime's Rust embedding API is mature and well-documented (44.0.1, 23M downloads)
- Host functions can provide controlled access to: LLM API, Project Tools RPC, effect request interface
- No filesystem access needed beyond WASI virtual FS

**Execution/Target Space → Firecracker microVM**
- Running `npm test`, `go build`, integration tests requires a full OS with real dependencies
- Firecracker provides KVM-based hardware isolation — strong multi-tenant boundary
- Slower startup (100-200ms) is acceptable for batch test execution
- Managed via containerd/firectl or direct Firecracker API
- Each execution gets a fresh microVM, destroyed after use — no cross-tenant state leakage

**What NOT to use:**
- **Docker containers alone for multi-tenant execution**: Shared kernel means weaker isolation between tenants. Acceptable for single-tenant, insufficient for multi-tenant platform.
- **WASM for execution space**: Cannot run `gcc`, `npm`, `go build`, database processes. Wrong tool.
- **Firecracker for agent space**: Overkill. 100ms+ startup per ReAct loop turn is too slow.
- **gVisor**: More complex deployment than Firecracker for equivalent isolation. Firecracker is Rust-native friendly.

**Confidence: MEDIUM-HIGH** — WASM sandboxing is proven (Cloudflare Workers, Fermyon Spin). Firecracker is AWS production-proven. The integration surface area (WASM host functions, Firecracker orchestration) is the largest implementation risk.

## Alternatives Considered

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| Tonic | grpcio | C++ binding, non-idiomatic, no async/await, deprecated |
| SQLx | Diesel | ORM DSL restricts raw SQL needed for event store patterns; diesel_async is younger/less mature |
| SQLx | tokio-postgres | Lower-level API, no compile-time SQL checking, no built-in migrations; SQLx wraps it with better ergonomics |
| Prost | rust-protobuf | Non-idiomatic Rust, verbose builder patterns, smaller ecosystem |
| Tokio | async-std | Dormant ecosystem, tokio has won community consensus |
| Tracing | log + env_logger | No structured fields, no spans, no async context propagation |
| Wasmtime | Wasmer | Wasmtime has stronger Bytecode Alliance backing, better WASI support, more Rust-native API |
| Firecracker | Docker | Shared kernel = weaker multi-tenant isolation |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| **Diesel ORM for event store** | Schema DSL restricts append-only flexibility; `diesel_async` is not as battle-tested as sqlx's async support; compile-time schema checking via DSL doesn't help with dynamic event payloads | SQLx with raw SQL |
| **grpcio** | C++ FFI dependency, memory safety risk, no idiomatic async/await, deprecated by CNCF | Tonic |
| **rust-protobuf** | Non-idiomatic generated code, verbose builder patterns, limited streaming integration | Prost |
| **log crate (alone)** | No span hierarchy, no structured fields, can't propagate context across async boundaries | tracing |
| **actix-rt / async-std** | Ecosystem fragmentation; tokio is the Rust async standard | Tokio |
| **Docker-only sandboxing** | Shared kernel between tenants is insufficient for enterprise multi-tenant isolation requirements | WASM (Agent) + Firecracker (Execution) |
| **Hand-rolled RPC protocol** | Reinventing framing, codegen, streaming, error codes, health checking, load balancing | gRPC via Tonic |
| **In-process scheduling queue** | No durability, can't survive restarts, can't scale horizontally | PostgreSQL-backed event-sourced scheduler |
| **Custom event store on raw files** | No transactions, no concurrency control, no queryability for projections | PostgreSQL with event sourcing tables |
| **Manual thread management** | Error-prone, can't benefit from tokio's work-stealing scheduler | tokio::spawn / spawn_blocking |

## Stack Patterns by Component

### Session Store (Event Log)
- SQLx with PostgreSQL
- JSONB for event payloads
- CAS via `INSERT ... ON CONFLICT` with `(session_id, event_version)` unique constraint
- Projections as materialized views refreshed on demand

### Scheduler
- Tokio task per scheduler tick cycle (interval-based)
- `tokio::select!` for heartbeat monitoring with timeouts
- `sqlx::PgListener` for real-time task readiness notifications
- `dashmap` for in-memory projection cache with TTL

### gRPC Services (Internal)
- Tonic server with tower middleware stack: timeout → auth → rate_limit → handler
- mTLS via tonic + rustls
- gRPC health checking for Kubernetes probes
- Streaming RPCs for Worker heartbeat channels

### Gateway (REST ↔ gRPC)
- Axum HTTP server with tower-http middleware
- tonic client pool for backend gRPC calls
- Request ID propagation across REST → gRPC boundary
- OpenTelemetry span context bridging

### Worker Daemon
- tokio::process or wasmtime for Agent execution
- gRPC client for control plane communication
- Heartbeat via streaming RPC or periodic unary calls
- Fencing token validation on every control plane call

### Observability Pipeline
- tracing instrumented on every service boundary
- tracing-opentelemetry layer in every process
- OTEL Collector as sidecar or daemonset
- Prometheus for metrics, Jaeger/Tempo for traces

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| tonic 0.14.x | prost 0.14.x | Same tokio-rs release train |
| tonic 0.14.x | tower 0.5.x | tonic depends on tower internally |
| sqlx 0.8.x | tokio 1.x (any recent) | sqlx uses tokio runtime |
| sqlx 0.8.x | uuid 1.x with "v7" feature | For UUID primary keys |
| tracing 0.1.x | tracing-subscriber 0.3.x | Standard pairing |
| tracing-opentelemetry 0.32.x | opentelemetry 0.32.x | Must match OTEL API version |
| opentelemetry-otlp 0.32.x | tonic 0.14.x | gRPC exporter uses tonic |
| axum 0.8.x | tower-http 0.6.x | Both on tower 0.5 |
| axum 0.8.x | tonic 0.14.x | Same tower/HTTP stack underneath |
| wasmtime 44.x | anyhow 1.x | wasmtime uses anyhow for errors |

## Cargo Workspace Structure

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "2"
members = [
    "crates/session-store",      # Event-sourced session with SQLx
    "crates/scheduler",          # Deterministic DAG scheduler
    "crates/lease-service",      # Lease issuance, renewal, fencing
    "crates/worker-spec",        # Worker Spec builder
    "crates/effect-gateway",     # Side effect isolation
    "crates/gateway",            # REST ↔ gRPC translation
    "crates/proto",              # Protobuf definitions + generated code
    "crates/common",             # Shared types, errors, utilities
    "crates/observability",      # Tracing/metrics initialization
]
```

This organization maps to the 8 work packages defined in the architecture document, with `proto` and `common` as shared foundations.

## Sources

- Context7: `/hyperium/tonic` — Tonic interceptors, streaming, middleware (HIGH confidence)
- Context7: `/launchbadge/sqlx` — SQLx compile-time queries, transactions, PgListener, JSONB (HIGH confidence)
- Context7: `/tokio-rs/prost` — Prost message encoding, code generation (HIGH confidence)
- Context7: `/tokio-rs/tracing` — Tracing spans, subscriber composition, OpenTelemetry bridge (HIGH confidence)
- Context7: `/open-telemetry/opentelemetry-rust` — OTLP gRPC exporter, metrics, logs setup (HIGH confidence)
- Context7: `/bytecodealliance/wasmtime` — WASM embedding API, WASI, host functions (HIGH confidence)
- Context7: `/tower-rs/tower` — ServiceBuilder middleware composition (HIGH confidence)
- Context7: `/diesel-rs/diesel_async` — diesel_async transactions and pooling (MEDIUM confidence)
- Context7: `/testcontainers/testcontainers-rs` — Docker Compose integration, Postgres module (HIGH confidence)
- crates.io API — Version verification for all crates (HIGH confidence)
- SwarmOS v5.1 architecture document — System requirements and constraints (project source)

---
*Stack research for: SwarmOS v5.1 - Distributed AI Agent Orchestration Platform*
*Researched: 2026-05-16*
*All version numbers verified against crates.io as of research date.*
