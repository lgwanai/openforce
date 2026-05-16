# Architecture Research: SwarmOS v5.1

**Domain:** Distributed AI Agent Orchestration Platform (Enterprise Multi-Tenant)
**Researched:** 2026-05-16
**Confidence:** HIGH (gRPC/DB layers verified via Context7; Rust workspace patterns from well-established conventions)

## Architecture Overview: Three-Plane Design

SwarmOS v5.1 follows a strict three-plane architecture:

```
===========================================================================
                              CONTROL PLANE
===========================================================================
┌──────────┐  ┌───────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐
│ Gateway  │  │ Session   │  │Scheduler │  │ Project  │  │  Effect       │
│(REST/gRPC)│  │ Store     │  │(State    │  │ Tools    │  │  Gateway     │
│          │  │(Event Log)│  │ Machine) │  │(HITL)    │  │(Side Effects)│
└────┬─────┘  └─────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘
     │              │             │              │               │
     └──────────────┴─────────────┴──────────────┴───────────────┘
                          │ (gRPC + mTLS)
===========================================================================
                              DATA PLANE
===========================================================================
┌──────────────┐  ┌──────────────┐  ┌─────────────────────────┐
│ Node Daemon  │  │Node Daemon   │  │ Worker Cluster          │
│ ┌──────────┐ │  │ ┌──────────┐ │  │ ┌─────────────────────┐ │
│ │Sandbox   │ │  │ │Sandbox   │ │  │ │ Execution Sandbox   │ │
│ │(Agent)   │ │  │ │(Agent)   │ │  │ │ (Target Space)      │ │
│ └──────────┘ │  │ └──────────┘ │  │ └─────────────────────┘ │
│ ┌──────────┐ │  │ ┌──────────┐ │  └─────────────────────────┘
│ │Warm Pool │ │  │ │Warm Pool │ │
│ └──────────┘ │  │ └──────────┘ │
└──────────────┘  └──────────────┘
===========================================================================
                            EVOLUTIONARY PLANE
===========================================================================
┌──────────┐ ┌───────────┐ ┌───────────┐ ┌──────────────────────┐
│ Observer │ │ Evaluator │ │  Evolver  │ │  Release Manager     │
│(Samples) │ │(Scoring)  │ │(Prompts)  │ │(A/B, Grayscale,     │
│          │ │           │ │           │ │ Rollback)            │
└──────────┘ └───────────┘ └───────────┘ └──────────────────────┘
===========================================================================
                            SHARED INFRASTRUCTURE
===========================================================================
┌───────────────────┐  ┌───────────────────┐  ┌───────────────────────┐
│ PostgreSQL        │  │ Object Storage    │  │ Observability Stack   │
│ (Event Log,       │  │ (Artifacts,       │  │ (Metrics, Logs,       │
│  Projections,     │  │  Manifests,       │  │  Tracing)             │
│  Outbox)          │  │  Snapshots)       │  │                       │
└───────────────────┘  └───────────────────┘  └───────────────────────┘
```

## Service Boundaries: The 11-Crate Decomposition

Based on the architecture design documents and Rust gRPC best practices, SwarmOS decomposes into the following crate boundaries. Each crate is independently deployable via its own binary (or library that multiple binaries share).

### Control Plane Services

| # | Crate Name | Binary | Responsibilities | Depends On |
|---|-----------|--------|------------------|------------|
| 1 | `swarmos-proto` | (lib only) | All `.proto` definitions, shared protobuf types, gRPC codegen | Nothing |
| 2 | `swarmos-domain` | (lib only) | Shared domain types: Event, Command, Task, Lease, WorkerSpec, Session, PlanEpoch. Implements state machine transitions, validation rules, idempotency checks | `swarmos-proto` |
| 3 | `session-store` | Yes | Event-sourced Session CRUD, append-only Event Log writes with CAS semantics, Projection materialization. The sole writer to the Event Log. | `swarmos-domain`, PostgreSQL |
| 4 | `scheduler` | Yes | Deterministic DAG advancement main loop, Lease issuance with fencing tokens, heartbeat monitoring, timeout/retry logic, WorkerSpec builder, tenant-aware fair scheduling | `session-store` (gRPC client), `swarmos-domain` |
| 5 | `project-tools` | Yes | ReadProjectFile, ReadProjectTree, SubmitProjectPatch, DeleteProjectFile RPC handlers. Path capability authorization, Patch semantic classifier (safe/moderate/sensitive/reject), ApprovalRequest generation, Merge Service for concurrent patches | `session-store` (gRPC client), `swarmos-domain` |
| 6 | `approval-service` | Yes | ApprovalRequest lifecycle (create/get/approve/reject), ApprovalToken issuance and consumption with context binding (session_id, task_id, fencing_token, content_sha256), token TTL management | `session-store` (gRPC client), `swarmos-domain` |
| 7 | `effect-gateway` | Yes | Effect request validation, idempotency_key dedup, approval policy engine (separation of duties, four-eyes principle, break-glass), Outbox pattern for eventual external execution, manifest (payload_sha256 + object_version_id) immutability binding | `approval-service` (gRPC client), `swarmos-domain` |
| 8 | `gateway` | Yes | REST to gRPC proxy. Middleware chain (RequestID, Recovery, Logging, Timeout, AuthN, AuthZ, Idempotency). JSON field mapping (snake_case to lowerCamelCase), gRPC oneof to HTTP status mapping, ToolErrorCode to HTTP status mapping. Metadata forwarding to downstream gRPC services. | All control plane services (gRPC clients) |

### Data Plane Services

| # | Crate Name | Binary | Responsibilities | Depends On |
|---|-----------|--------|------------------|------------|
| 9 | `node-daemon` | Yes | Worker lifecycle management, sandbox pool warming (Agent Image + Target Image layering), WorkerSpec validation before spawn, capability token validation, heartbeat relay, input snapshot mounting. | `scheduler` (gRPC client), container runtime |

### Evolutionary Plane Services

| # | Crate Name | Binary | Responsibilities | Depends On |
|---|-----------|--------|------------------|------------|
| 10 | `evolver` | Yes | Observer data collection (structured metrics, error patterns, tool call traces), Evaluator scoring pipeline, Evolver prompt/SOP generation, Release Manager (grayscale/A/B/rollback). **Strictly offline/read-only from control plane.** | `session-store` (read-only gRPC client), `swarmos-domain` |

### Supporting Crates

| # | Crate Name | Binary | Responsibilities | Depends On |
|---|-----------|--------|------------------|------------|
| 11 | `policy-engine` | (lib) | Reusable policy evaluation engine: tenant quotas, budget enforcement, model egress policies, tool policies, network policies, kill switches. Shared by scheduler, project-tools, effect-gateway. | `swarmos-domain` |

### Why 8 Services, Not Fewer Not More

The design documents prescribe these service boundaries for clear architectural reasons:

- **Session Store is a singleton writer** to the Event Log. All CAS operations, fencing token validation, and event append atomicity must happen in ONE place. Co-locating it with other services would create circular dependencies or shared-state coupling.
- **Scheduler must be independent** from Session Store because it needs independent scaling (CPU-bound DAG traversal). It reads projections, writes commands.
- **Project Tools and Approval Service are separate** because project tool RPCs are high-frequency (every ReAct cycle), while approval interactions are human-paced. Different scaling profiles, different availability requirements.
- **Effect Gateway is the choke point for all real-world side effects.** It must never be co-located with Worker-runnable code for security isolation.
- **Gateway is the only REST entry point.** Separating it allows the gRPC-only internal services to be unburdened by HTTP concerns.

## Crate Organization: Rust Monorepo Workspace

### Recommended Structure

```
openforce/
├── Cargo.toml                  # Workspace root
├── Cargo.lock
├── rust-toolchain.toml         # Pin Rust version (1.85+)
├── .cargo/
│   └── config.toml             # Build settings, registry mirrors
│
├── crates/
│   ├── proto/                  # swarmos-proto
│   │   ├── Cargo.toml
│   │   ├── build.rs            # prost_build::compile_protos()
│   │   └── proto/
│   │       ├── common.proto
│   │       ├── session.proto
│   │       ├── scheduler.proto
│   │       ├── project_tools.proto
│   │       ├── approvals.proto
│   │       ├── effects.proto
│   │       └── node_daemon.proto
│   │
│   ├── domain/                 # swarmos-domain (lib only)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── event.rs        # Event envelope, event type enum, payload types
│   │       ├── command.rs      # Command types, command dedup
│   │       ├── task.rs         # Task state machine, transitions, fencing
│   │       ├── lease.rs        # Lease model, renewal rules
│   │       ├── worker_spec.rs  # WorkerSpec builder, validation
│   │       ├── plan.rs         # Plan/PlanEpoch, DAG, inheritance mapping
│   │       ├── session.rs      # Session aggregate root, CAS version
│   │       ├── patch.rs        # Patch semantics, classification
│   │       ├── approval.rs     # ApprovalRequest, ApprovalBinding, Token
│   │       ├── effect.rs       # Effect types, idempotency, manifest
│   │       └── tenant.rs       # Tenant policies, quotas, kill switches
│   │
│   ├── session-store/          # session-store (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs         # gRPC server bootstrap
│   │       ├── lib.rs
│   │       ├── server.rs       # gRPC service implementation
│   │       ├── store.rs        # Event log append with CAS
│   │       ├── projection.rs   # Projection builder (materialized views)
│   │       ├── repo.rs         # Repository pattern over Event Log
│   │       └── migrations/     # SQLx migrations
│   │
│   ├── scheduler/              # scheduler (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── lib.rs
│   │       ├── server.rs       # gRPC service: spawn, heartbeat, control
│   │       ├── loop.rs         # Main scheduler tick loop
│   │       ├── dag.rs          # DAG dependency resolution
│   │       ├── lease_service.rs
│   │       ├── worker_spec_builder.rs
│   │       ├── retry.rs        # Retry policy, escalation
│   │       ├── quota.rs        # Tenant quota management
│   │       └── kill_switch.rs  # Emergency circuit breakers
│   │
│   ├── project-tools/          # project-tools (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs       # gRPC service implementation
│   │       ├── read_file.rs
│   │       ├── read_tree.rs
│   │       ├── submit_patch.rs # Incl. semantic classifier
│   │       ├── delete_file.rs
│   │       ├── merge.rs        # Patch merge service
│   │       └── authz.rs        # Path capability authorization
│   │
│   ├── approval-service/      # approval-service (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs
│   │       ├── request.rs      # Create/get approval requests
│   │       ├── approve.rs      # Approve with token binding
│   │       ├── reject.rs
│   │       └── token.rs        # Token issuance, validation, consumption
│   │
│   ├── effect-gateway/         # effect-gateway (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs
│   │       ├── request.rs      # EffectRequest handling
│   │       ├── idempotency.rs  # Idempotency key dedup
│   │       ├── outbox.rs       # Outbox pattern implementation
│   │       ├── manifest.rs     # Manifest immutability binding
│   │       ├── dispatcher.rs   # Async dispatch to external executors
│   │       └── reconciler.rs   # Repair job for outbox anomalies
│   │
│   ├── gateway/                # gateway (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── lib.rs
│   │       ├── server.rs       # Axum HTTP server bootstrap
│   │       ├── router.rs       # Route registration
│   │       ├── middleware/
│   │       │   ├── request_id.rs
│   │       │   ├── recovery.rs
│   │       │   ├── logging.rs
│   │       │   ├── timeout.rs
│   │       │   ├── authn.rs
│   │       │   ├── authz.rs
│   │       │   └── idempotency.rs
│   │       ├── handler/
│   │       │   ├── project_tools_handler.rs
│   │       │   ├── approvals_handler.rs
│   │       │   └── health_handler.rs
│   │       ├── mapper/
│   │       │   ├── grpc_to_http.rs
│   │       │   ├── http_to_grpc.rs
│   │       │   └── error_mapper.rs
│   │       └── client/
│   │           ├── grpc_conn.rs
│   │           ├── project_tools_client.rs
│   │           ├── approvals_client.rs
│   │           └── session_client.rs
│   │
│   ├── node-daemon/            # node-daemon (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs       # gRPC service for Scheduler
│   │       ├── sandbox.rs      # Firecracker/container manager
│   │       ├── pool.rs         # Warm pool management
│   │       └── worker_agent.rs # Worker lifecycle inside sandbox
│   │
│   ├── evolver/                # evolver (binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── observer.rs     # Data collection
│   │       ├── evaluator.rs    # Quality scoring
│   │       ├── evolver.rs      # Prompt/SOP generation
│   │       ├── release_mgr.rs  # Grayscale/A/B/Rollback
│   │       └── red_team.rs     # Red-team scenario runner
│   │
│   └── policy-engine/          # policy-engine (lib only)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── tenant_policy.rs
│           ├── budget.rs
│           ├── model_egress.rs
│           ├── tool_policy.rs
│           ├── network_policy.rs
│           └── kill_switch.rs
│
├── tests/                      # Integration tests (see Testing section)
│   ├── integration/
│   │   ├── session_flow.rs
│   │   ├── scheduler_cycle.rs
│   │   ├── fencing_token.rs
│   │   ├── effect_gateway.rs
│   │   └── multi_tenant.rs
│   └── common/
│       ├── mod.rs
│       ├── test_db.rs          # Test postgres helpers
│       └── test_grpc.rs        # Test gRPC client setup
│
└── deployments/                # Dockerfiles, k8s manifests, configs
    ├── docker/
    │   ├── session-store.Dockerfile
    │   ├── scheduler.Dockerfile
    │   └── ...
    └── kubernetes/
        └── ...
```

### Cargo.toml Workspace Root

```toml
[workspace]
members = [
    "crates/proto",
    "crates/domain",
    "crates/session-store",
    "crates/scheduler",
    "crates/project-tools",
    "crates/approval-service",
    "crates/effect-gateway",
    "crates/gateway",
    "crates/node-daemon",
    "crates/evolver",
    "crates/policy-engine",
]
resolver = "2"

[workspace.dependencies]
# Async runtime
tokio = { version = "1.42", features = ["full"] }

# gRPC
tonic = "0.12"
tonic-health = "0.12"
tonic-reflection = "0.12"
prost = "0.13"

# HTTP (Gateway only)
axum = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace", "request-id"] }

# Database
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "json"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
opentelemetry = "0.27"

# Utilities
uuid = { version = "1.11", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2.0"
anyhow = "1.0"

# Crypto
ring = "0.17"
sha2 = "0.10"

# Testing
rstest = "0.23"
testcontainers = "0.23"
tower-test = "0.4"
```

### Why Monorepo

1. **Shared proto definitions** define the contract surface. Changes to proto files must be reviewed alongside all service implementations. In a multi-repo setup, proto changes become multi-repo coordination hell.
2. **Atomic cross-cutting changes** -- upgrading prost, tonic, or sqlx versions affects all services simultaneously. Monorepo makes `cargo update` a single operation.
3. **Consistent toolchain** -- one `rust-toolchain.toml`, one `Cargo.lock`, unified CI pipeline.
4. **Domain types reuse** -- the `swarmos-domain` crate is consumed by 7+ other crates. Keeping them in one workspace avoids publishing internal-only crates.
5. **Integration tests** can import any crate directly for white-box testing.

### Import Graph (Dependency Direction)

```
swarmos-proto (no deps)
    |
    v
swarmos-domain (depends on proto only)
    |
    +--------+--------+--------+--------+
    |        |        |        |        |
    v        v        v        v        v
session  scheduler project  approval effect   policy-engine
-store           -tools   -service  -gateway
    |        |        |        |        |
    +--------+--------+--------+--------+
                      |
                      v
                   gateway (HTTP facade, depends on all above)
                      |
                      v
                   External Clients / UI
```

## gRPC Service Mesh: Tonic + Tower Middleware

### Technology Stack

| Component | Crate | Role |
|-----------|-------|------|
| gRPC Server | `tonic` 0.12 | All service binaries |
| gRPC Client | `tonic::transport::Channel` | Inter-service calls |
| Protocol Codegen | `prost` 0.13 + `tonic-build` | Proto -> Rust structs + service traits |
| Health Checking | `tonic-health` | Standard gRPC health protocol for K8s probes |
| Service Reflection | `tonic-reflection` | Debug tooling via grpcurl |
| TLS | `tonic::transport::ClientTlsConfig` / `ServerTlsConfig` | mTLS between services |
| Middleware | `tonic::Interceptor` (gRPC layer) + `tower::ServiceBuilder` (HTTP layer) | AuthN/AuthZ, logging, timeouts, rate limiting |
| HTTP Server | `axum` 0.8 | Gateway HTTP layer only |

### Server Bootstrap Pattern

Every service binary follows the same bootstrap pattern:

```rust
// crates/session-store/src/main.rs
use tonic::transport::Server;
use tonic_health::server::HealthReporter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load configuration
    let config = swarmos_config::load()?;

    // 2. Initialize tracing/observability
    swarmos_observability::init(&config.observability)?;

    // 3. Establish database pool
    let db_pool = sqlx::PgPool::connect(&config.database_url).await?;

    // 4. Run migrations
    sqlx::migrate!("./migrations").run(&db_pool).await?;

    // 5. Construct service implementation
    let svc = SessionStoreService::new(db_pool);

    // 6. Set up health reporter
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<SessionStoreServer<SessionStoreService>>()
        .await;

    // 7. Build server with interceptors
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(swarmos_proto::session::FILE_DESCRIPTOR_SET)
        .build_v1alpha()?;

    let auth_interceptor = auth::server_interceptor();

    Server::builder()
        .add_service(health_service)
        .add_service(reflection_service)
        .add_service(
            SessionStoreServer::with_interceptor(svc, auth_interceptor)
        )
        .serve(config.listen_addr.parse()?)
        .await?;

    Ok(())
}
```

### Client Connection Pattern

Services that need to call other services create typed gRPC clients:

```rust
// crates/scheduler/src/client.rs
use tonic::transport::{Channel, ClientTlsConfig, Certificate};
use std::time::Duration;

pub struct ServiceClients {
    pub session_store: swarmos_proto::session::SessionStoreClient<Channel>,
    pub project_tools: swarmos_proto::project_tools::ProjectToolServiceClient<Channel>,
}

impl ServiceClients {
    pub async fn connect(config: &ServiceConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let tls = if let Some(ca_path) = &config.tls_ca_path {
            let pem = std::fs::read_to_string(ca_path)?;
            let ca = Certificate::from_pem(pem);
            ClientTlsConfig::new()
                .ca_certificate(ca)
                .domain_name(&config.domain_name)
        } else {
            ClientTlsConfig::new().with_native_roots()
        };

        let channel = Channel::from_shared(config.session_store_addr.clone())?
            .tls_config(tls.clone())?
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .connect()
            .await?;

        Ok(Self {
            session_store: SessionStoreClient::new(channel),
            // ... other clients
        })
    }
}
```

### Inter-Service Auth: mTLS + Capability Token

From the architecture doc section 22 (implementation appendix B):

1. **Connection-level identity**: mTLS via `tonic::transport::ServerTlsConfig` with client certificate verification
2. **Lease-level capability token**: Short-lived JWT (1-5 minute TTL) issued by Scheduler at lease time, scoped to a single tenant/session/task/lease
3. **Business-level validation**: All services verify `tenant_id + session_id + task_id + lease_id + fencing_token` on every write

The Tonic interceptor for capability token validation:

```rust
// crates/session-store/src/auth.rs
use tonic::{Request, Status};

// Extracted from the mTLS connection + bearer token
#[derive(Clone)]
pub struct CallerIdentity {
    pub component: String,
    pub instance_id: String,
    pub tenant_id: String,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub lease_id: Option<String>,
    pub fencing_token: Option<u64>,
    pub scopes: Vec<String>,
}

pub fn server_interceptor() -> impl tonic::Interceptor<CallerIdentity> {
    move |mut req: Request<()>| {
        // 1. Extract mTLS identity from peer certificates
        let component = extract_mtls_identity(req.peer_certs())?;

        // 2. Extract and validate capability token from metadata
        let capability = extract_capability_token(req.metadata())?;
        capability.validate_not_revoked()?;
        capability.validate_not_expired()?;

        // 3. Construct identity, inject into request extensions
        let identity = CallerIdentity {
            component,
            instance_id: capability.instance_id,
            tenant_id: capability.tenant_id,
            session_id: capability.session_id,
            task_id: capability.task_id,
            lease_id: capability.lease_id,
            fencing_token: capability.fencing_token,
            scopes: capability.scopes,
        };

        req.extensions_mut().insert(identity);
        Ok(req)
    }
}
```

## Event Sourcing in Rust with PostgreSQL

### Design

SwarmOS uses **Event Sourcing with PostgreSQL** as the single source of truth. The Session Store owns all writes to the Event Log. All state transitions are recorded as append-only events with monotonically increasing `session_version`.

### Event Log Schema

```sql
-- crates/session-store/migrations/001_event_log.sql
CREATE TABLE event_log (
    -- Primary ordering key: monotonically increasing, DB-assigned
    event_id      BIGSERIAL PRIMARY KEY,

    -- Business identifiers
    event_type    VARCHAR(64) NOT NULL,     -- e.g., 'TaskLeased', 'ArtifactSubmitted'
    session_id    UUID NOT NULL,
    tenant_id     UUID NOT NULL,

    -- CAS (Compare-And-Set) versioning
    session_version BIGINT NOT NULL,        -- The CAS token

    -- Task context (nullable for session-level events)
    task_id       UUID,
    task_attempt  INTEGER,

    -- Causal chain
    causation_id  UUID NOT NULL,            -- Which command caused this event
    correlation_id UUID NOT NULL,           -- Which user request/flow

    -- Producer identity
    producer_component VARCHAR(64) NOT NULL,
    producer_instance  VARCHAR(128) NOT NULL,

    -- Event data
    payload       JSONB NOT NULL,           -- Structured event-specific data

    -- Ordering
    occurred_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Index for projection reads
    CONSTRAINT idx_session_version UNIQUE (session_id, session_version)
);

-- Command deduplication table
CREATE TABLE command_dedup (
    command_id    UUID PRIMARY KEY,
    result        JSONB,                    -- Cached result for idempotent replay
    processed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Projection table (example: current task state)
CREATE TABLE task_projection (
    session_id    UUID NOT NULL,
    task_id       UUID NOT NULL,
    state         VARCHAR(16) NOT NULL,     -- Pending/Ready/Leased/Running/...
    task_attempt  INTEGER NOT NULL,
    current_lease_id UUID,
    current_fencing_token BIGINT,
    current_worker_spec_id UUID,
    updated_at    TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (session_id, task_id)
);
```

### CAS Append Pattern

The critical pattern for atomic event appending with version checking:

```rust
// crates/session-store/src/store.rs
use sqlx::{PgPool, Postgres, Transaction};
use swarmos_domain::event::Event;

impl EventStore {
    /// Append events with CAS (Compare-And-Set) on session_version.
    /// Returns Err(VersionConflict) if another writer beat us.
    pub async fn append_events(
        pool: &PgPool,
        session_id: Uuid,
        expected_version: i64,
        events: &[Event],
    ) -> Result<i64, AppendError> {
        let mut tx = pool.begin().await?;

        // 1. Lock the session row to serialize concurrent appends.
        //    pg_advisory_xact_lock auto-releases on transaction end.
        sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
            .bind(session_id.as_u64_pair().0)
            .bind(session_id.as_u64_pair().1)
            .execute(&mut *tx)
            .await?;

        // 2. Read current max session_version
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(session_version) FROM event_log WHERE session_id = $1"
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;

        let expected = current.unwrap_or(0);
        if expected != expected_version {
            tx.rollback().await?;
            return Err(AppendError::VersionConflict {
                expected: expected_version,
                actual: expected,
            });
        }

        // 3. Insert events with monotonically increasing version
        let mut next_version = expected + 1;
        for event in events {
            sqlx::query(
                "INSERT INTO event_log (event_type, session_id, tenant_id, session_version,
                 task_id, task_attempt, causation_id, correlation_id,
                 producer_component, producer_instance, payload)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
            )
            .bind(&event.event_type)
            .bind(session_id)
            .bind(&event.tenant_id)
            .bind(next_version)
            .bind(event.task_id)
            .bind(event.task_attempt)
            .bind(event.causation_id)
            .bind(event.correlation_id)
            .bind(&event.producer_component)
            .bind(&event.producer_instance)
            .bind(serde_json::to_value(&event.payload)?)
            .execute(&mut *tx)
            .await?;

            next_version += 1;
        }

        // 4. Commit -- advisory lock auto-releases
        tx.commit().await?;

        Ok(next_version - 1) // Return final session_version
    }
}
```

Key decisions:
- **PostgreSQL advisory locks** (`pg_advisory_xact_lock`) serialize concurrent writes per session without table-level locking. The lock auto-releases on transaction commit/rollback.
- **`session_version` is the CAS token.** The caller always passes `expected_version` from their last read. If another writer incremented it, the append fails with a conflict.
- **No UPDATE of historical events.** The Event Log is truly append-only.
- **Command dedup via `command_id`.** The `command_dedup` table enables idempotent command handling -- replaying the same command_id returns the cached result.

### Projection Building

Projections are materialized views built from the Event Log for efficient reads:

```rust
// crates/session-store/src/projection.rs
impl ProjectionBuilder {
    /// Rebuild the task_projection from the Event Log.
    pub async fn rebuild_task_projection(
        pool: &PgPool,
        session_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        // Fold over all events, building current state
        sqlx::query(
            r#"
            WITH ordered_events AS (
                SELECT
                    session_version,
                    event_type,
                    (payload->>'task_id')::UUID as task_id,
                    (payload->>'task_attempt')::INTEGER as task_attempt,
                    payload
                FROM event_log
                WHERE session_id = $1
                ORDER BY session_version ASC
            )
            -- Use INSERT ... ON CONFLICT to upsert into projection
            ...
            "#
        )
        .bind(session_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Incremental projection update: process only events since last_snapshot_version
    pub async fn update_incremental(
        pool: &PgPool,
        session_id: Uuid,
        from_version: i64,
    ) -> Result<i64, sqlx::Error> {
        // Process new events, return new high-water mark
        todo!()
    }
}
```

Projections are rebuilt from scratch at startup or on-demand. In steady state, an incremental updater runs on a timer or via PostgreSQL `LISTEN/NOTIFY` (using `sqlx::postgres::PgListener`).

### Why Rust + SQLx + PostgreSQL for Event Sourcing

| Concern | Solution | Confidence |
|---------|----------|------------|
| CAS atomicity | PostgreSQL serializable transaction + advisory lock | HIGH (verified via Context7 SQLx docs) |
| Append-only guarantee | No UPDATE on event_log table; only INSERT | HIGH |
| Projection rebuild | SQLx `query_as` with folding over ordered events | MEDIUM (pattern is standard, projection complexity varies) |
| Command dedup | Separate `command_dedup` table, upsert on command_id | HIGH |
| LISTEN/NOTIFY | `sqlx::postgres::PgListener` for real-time projection updates | HIGH (verified via Context7) |
| Transactions | `pool.begin().await?` with `tx.commit()/rollback()` | HIGH (verified via Context7) |

## Concurrency Model: Tokio Tasks + Message Channels

### Decision: NOT a Full Actor Framework

After analysis, SwarmOS uses **tokio tasks with message-passing channels** rather than a formal actor framework (Actix, Ractor, or custom actor library). Rationale:

1. **Scheduler is a state machine, not an actor system.** The Scheduler main loop is a single `tokio::spawn` task that runs `scheduler_tick()` in a loop. It already has well-defined concurrency boundaries (one writer to session state at a time via CAS).
2. **Each gRPC service is its own process.** Intra-process actor patterns solve a different problem (concurrency within a single process). SwarmOS already solves this via separate gRPC services.
3. **Tokio's primitives are sufficient.** `tokio::sync::mpsc` for notification channels, `Arc<RwLock<T>>` for shared read-mostly state, `tokio::spawn` for concurrent task execution.
4. **Actor frameworks add ceremony without benefit here.** The scheduler's concurrency model is: one primary loop + N task-spawning operations. Adding actor mailboxes would add indirection without solving any new problem.

### Scheduler Concurrency Model

```rust
// crates/scheduler/src/loop.rs
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;

pub struct SchedulerRuntime {
    // Shared read-mostly state
    config: Arc<RwLock<SchedulerConfig>>,

    // Alert channels for external events
    heartbeat_rx: mpsc::Receiver<HeartbeatEvent>,
    submission_rx: mpsc::Receiver<SubmissionEvent>,
    control_rx: mpsc::Receiver<ControlCommand>,

    // gRPC clients to other services
    clients: ServiceClients,
}

impl SchedulerRuntime {
    pub async fn run(mut self) {
        let mut tick_interval = tokio::time::interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                // 1. Scheduled tick: evaluate DAG, expire leases, queue ready tasks
                _ = tick_interval.tick() => {
                    self.process_tick().await;
                }

                // 2. External events: heartbeats, submissions, control commands
                Some(event) = self.heartbeat_rx.recv() => {
                    self.process_heartbeat(event).await;
                }
                Some(event) = self.submission_rx.recv() => {
                    self.process_submission(event).await;
                }
                Some(cmd) = self.control_rx.recv() => {
                    self.process_control(cmd).await;
                }
            }
        }
    }

    async fn process_tick(&mut self) {
        // Load active sessions from projection (read-only)
        let sessions = self.load_active_sessions().await;

        for session in sessions {
            // 1. Check for expired leases -> emit TaskTimedOut
            self.expire_stale_leases(&session).await;

            // 2. Check for satisfied dependencies -> emit TaskReadied
            self.advance_dag(&session).await;

            // 3. Queue Ready tasks for leasing via task_spawner
            self.lease_ready_tasks(&session).await;
        }
    }
}
```

### Shared State Pattern for Service Handlers

Each gRPC service handler uses `Arc<T>` for shared state:

```rust
#[derive(Clone)]
pub struct SessionStoreService {
    db: PgPool,                    // Clone is cheap (Arc internally)
    event_store: Arc<EventStore>,
    projection_builder: Arc<ProjectionBuilder>,
}

#[tonic::async_trait]
impl SessionStore for SessionStoreService {
    async fn append_events(
        &self,
        request: Request<AppendEventsRequest>,
    ) -> Result<Response<AppendEventsResponse>, Status> {
        let identity = request.extensions().get::<CallerIdentity>()
            .ok_or(Status::unauthenticated("missing identity"))?;

        let req = request.into_inner();

        let events: Vec<Event> = req.events
            .into_iter()
            .map(Event::from_proto)
            .collect::<Result<_, _>>()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let new_version = self.event_store
            .append_events(&self.db, req.session_id, req.expected_version, &events)
            .await
            .map_err(|e| match e {
                AppendError::VersionConflict { expected, actual } =>
                    Status::failed_precondition(format!(
                        "version conflict: expected {}, actual {}", expected, actual
                    )),
                _ => Status::internal(e.to_string()),
            })?;

        Ok(Response::new(AppendEventsResponse {
            new_session_version: new_version,
        }))
    }
}
```

### Why Not Actors

| Actor Feature | Equivalent in Tokio | Notes |
|---------------|---------------------|-------|
| Mailbox | `tokio::sync::mpsc::channel` | Bounded channels give backpressure |
| Supervision | Custom -- use `tokio::task::JoinSet` + monitoring | Restart via task respawn |
| Location transparency | Not needed -- all services communicate via gRPC | Service mesh handles routing |
| State encapsulation | `Arc<RwLock<T>>` | Type-checked at compile time |
| Message serialization | Not needed -- channels carry typed Rust structs | Actors serialize for remote use; we don't have that need |

## Data Flow: Lease, Fencing, and Submission

### The Critical Path: Task Lease + Fencing

This is the system's most sensitive data flow (architecture doc sections 5, 6, 18, 21):

```
                    SCHEDULER                              SESSION STORE
                        │                                        │
  [scheduler_tick()]    │                                        │
  1. Task becomes Ready │                                        │
                        │                                        │
  2. build WorkerSpec   │                                        │
     + tenant policy    │                                        │
                        │                                        │
  3. ─── LeaseTask ─────────────────────────────────────────────>
     { session_id,      │                                        │
       task_id,         │   4. BEGIN TRANSACTION                 │
       expected_version}│   5. pg_advisory_xact_lock(session)    │
                        │   6. READ current task state            │
                        │   7. IF state != "Ready" → REJECT      │
                        │   8. IF version != expected → REJECT   │
                        │   9. new attempt, lease, fencing token  │
                        │  10. INSERT TaskLeased event            │
                        │  11. UPSERT command_dedup               │
                        │  12. COMMIT                             │
                        │                                        │
  13. ◄─── LeaseTaskResult ──────────────────────────────────── │
     { lease_id,         │                                        │
       fencing_token,    │                                        │
       worker_spec_id }  │                                        │
                        │                                        │
  14. Issue capability  │                                        │
      token (JWT)        │                                        │
                        │                                        │
  15. ─── SpawnWorker ─────────────────────────────> NODE DAEMON│
      { worker_spec,     │                           │           │
        capability_token }│                          │           │
                        │                           16. Validate  │
                        │                              worker_spec│
                        │                           17. Pull image│
                        │                           18. Spawn     │
                        │                              sandbox   │
                        │                              │         │
                        │                           19. Inject   │
                        │                              cap token │
                        │                              │         │
                        │                           ┌──────────┐ │
                        │                           │  WORKER  │ │
                        │                           │ ReAct    │ │
                        │                           │ loop     │ │
                        │                           └──────────┘ │
```

### Submission Flow (with fencing check)

```
    WORKER                  PROJECT TOOLS              SESSION STORE
      │                         │                          │
 20. SubmitPatch {              │                          │
       command_id,              │                          │
       session_id,              │                          │
       task_id,                 │                          │
       task_attempt,            │                          │
       lease_id,                │                          │
       fencing_token: 9,        │                          │
       patch_ref }              │                          │
      │                         │                          │
      │  21. Validate cap token │                          │
      │     (scope, expiry,     │                          │
      │      binding)           │                          │
      │                         │                          │
      │  22. ──── ReadTask ───>│                          │
      │        { session_id,   │                          │
      │          task_id }      │                          │
      │                         │                          │
      │  23. ◄── task_state ── │                          │
      │        { state: Running│                          │
      │          current_fencing│                         │
      │          _token: 10 }   │                          │
      │                         │                          │
      │  24. IF token 9 < 10:  │                          │
      │      REJECT with        │                          │
      │      FENCING_TOKEN_STALE│                          │
      │      ← Worker detects  │                          │
      │        eviction,        │                          │
      │        self-terminates  │                          │
      │                         │                          │
      │  (If token matches:     │                          │
      │   continue to patch     │                          │
      │   classification,       │                          │
      │   merge, event append)  │                          │
```

### Effect Gateway Flow

```
    WORKER                   EFFECT GATEWAY          EXTERNAL EXECUTOR
      │                           │                        │
  25. RequestEffect {             │                        │
        idempotency_key,          │                        │
        effect_type: deploy,      │                        │
        manifest_id }             │                        │
      │                           │                        │
      │  26. Check idempotency    │                        │
      │      key dedup            │                        │
      │                           │                        │
      │  27. Validate lease +     │                        │
      │      fencing token        │                        │
      │                           │                        │
      │  28. Resolve manifest →   │                        │
      │      verify payload_sha256│                        │
      │      + object_version_id  │                        │
      │                           │                        │
      │  29. Evaluate approval     │                        │
      │      policy - if needed,  │                        │
      │      push to approval     │                        │
      │      service              │                        │
      │                           │                        │
      │  30. Write EffectRequested│                        │
      │      + effect_outbox      │                        │
      │                           │                        │
      │  31. ◄── effect_id ────  │                        │
      │                           │                        │
      │                    [async dispatcher]              │
      │                           │  32. Read outbox       │
      │                           │  33. Call external     │
      │                           │      executor ────────>│
      │                           │                        │  34. Execute
      │                           │  35. ◄── result ───── │
      │                           │  36. Write             │
      │                           │      EffectCommitted   │
```

## Build Order: Dependency-Driven Phase Sequence

The build must follow the import graph. Here is the recommended order:

### Phase 0: Foundation (Weeks 1-2)
**Crates:** `swarmos-proto`, `swarmos-domain`

1. **swarmos-proto** -- Define all `.proto` files and generate Rust code via `prost-build` in `build.rs`. This crate has zero internal dependencies and establishes the contract surface for everything else.
2. **swarmos-domain** -- Build the shared domain types, state machine transitions, validation logic, and all data structures. Depends only on proto.

**Verification:** `cargo build --workspace` passes. Proto generation works. Domain types compile.

### Phase 1: Event Log Foundation (Weeks 2-4)
**Crates:** `session-store`

3. **session-store** -- The most critical service. Build Event Log with CAS semantics, command dedup, projection builder, and migrations. This is the first service that needs a real database.

**Verification:** Integration tests with testcontainers Postgres: append events with version check, reject stale versions, rebuild projection from event log, command dedup works.

### Phase 2: Scheduling Core (Weeks 3-5, partial overlap with Phase 1)
**Crates:** `scheduler`, `policy-engine`

4. **policy-engine** -- The reusable policy evaluation library. Can be built in parallel with session-store since it has no DB dependency.
5. **scheduler** -- DAG traversal, lease service, heartbeat monitoring, WorkerSpec builder. Depends on session-store gRPC client.

**Verification:** Scheduler main loop can load sessions, advance DAG, issue leases, detect expired leases, handle fencing.

### Phase 3: Toolchain & Side Effects (Weeks 5-7)
**Crates:** `project-tools`, `approval-service`, `effect-gateway`

6. **project-tools** -- File read/tree/patch/delete with path capability authz and semantic classifier. Depends on session-store.
7. **approval-service** -- Approval request lifecycle, token binding. Depends on session-store.
8. **effect-gateway** -- Effect validation, idempotency, outbox, manifest. Depends on approval-service.

**Verification:** End-to-end flow: Worker submits patch -> semantic classifier triggers HITL -> approval granted -> patch merged -> effect requested -> effect committed.

### Phase 4: External Interface (Weeks 6-8)
**Crate:** `gateway`

9. **gateway** -- REST to gRPC proxy. Middleware chain, JSON mapping, error code mapping. Depends on all control plane services.

**Verification:** HTTP requests to gateway correctly proxy to gRPC backends. Error codes map correctly. Oneof responses map to correct HTTP status codes.

### Phase 5: Data Plane (Weeks 7-10)
**Crate:** `node-daemon`

10. **node-daemon** -- Sandbox management, warm pool, Worker lifecycle. Depends on scheduler and capability token validation.

### Phase 6: Evolution (Weeks 10+, post-MVP)
**Crate:** `evolver`

11. **evolver** -- Observer, evaluator, evolver, release manager. Depends on read-only session-store access.

### Dependency Graph for Build Order

```
Phase 0:  proto ──> domain
                      │
Phase 1:              ├──> session-store (PostgreSQL)
                      │         │
Phase 2:    policy-engine       ├──> scheduler
                      │         │
Phase 3:              │         ├──> project-tools
                      │         ├──> approval-service
                      │         └──> effect-gateway
                      │                    │
Phase 4:              └────────────────────┴──> gateway
                                               │
Phase 5:    scheduler ──> node-daemon
                                               │
Phase 6:    session-store (read-only) ──> evolver
```

## Integration Testing Architecture

### Strategy

SwarmOS uses two levels of integration testing:

1. **Crate-level integration tests** -- Test each service in isolation with real Postgres (testcontainers), mock downstream gRPC services
2. **End-to-end flow tests** -- Spin up all services, run complete workflow scenarios

### Test Infrastructure

```rust
// tests/common/test_db.rs
use testcontainers::{runners::AsyncRunner, ContainerAsync};
use testcontainers_modules::postgres::Postgres;
use sqlx::PgPool;

pub struct TestDb {
    container: ContainerAsync<Postgres>,
    pub pool: PgPool,
}

impl TestDb {
    pub async fn new() -> Self {
        let container = Postgres::default()
            .with_tag("16-alpine")
            .start()
            .await
            .expect("Failed to start Postgres container");

        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

        let pool = PgPool::connect(&url).await.unwrap();

        // Run migrations
        sqlx::migrate!("../crates/session-store/migrations")
            .run(&pool)
            .await
            .unwrap();

        Self { container, pool }
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Container auto-stops; no explicit cleanup needed
    }
}
```

### Session Store Integration Test

```rust
// tests/integration/session_flow.rs
use swarmos_session_store::{EventStore, SessionStoreService};
use swarmos_domain::event::{Event, EventType};

#[tokio::test]
async fn test_append_events_cas_success() {
    let db = TestDb::new().await;
    let store = EventStore::new();

    let session_id = Uuid::now_v7();
    let events = vec![Event::session_created(session_id, tenant_id)];

    // First append: expected_version=0
    let v1 = store.append_events(&db.pool, session_id, 0, &events).await.unwrap();
    assert_eq!(v1, 1);

    // Second append: expected_version=1
    let events2 = vec![Event::plan_compiled(session_id, 1, Some(task_id))];
    let v2 = store.append_events(&db.pool, session_id, 1, &events2).await.unwrap();
    assert_eq!(v2, 2);
}

#[tokio::test]
async fn test_append_events_cas_conflict() {
    let db = TestDb::new().await;
    let store = EventStore::new();

    let session_id = Uuid::now_v7();

    // Writer A appends successfully
    let events_a = vec![Event::session_created(session_id, tenant_id)];
    store.append_events(&db.pool, session_id, 0, &events_a).await.unwrap();

    // Writer B tries with stale expected_version=0
    let events_b = vec![Event::plan_compiled(session_id, 1, None)];
    let result = store.append_events(&db.pool, session_id, 0, &events_b).await;

    assert!(matches!(result, Err(AppendError::VersionConflict { .. })));
}

#[tokio::test]
async fn test_command_dedup() {
    let db = TestDb::new().await;
    let store = EventStore::new();

    let cmd_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    // First attempt
    let result1 = store.handle_command(&db.pool, cmd_id, || {
        // ... command logic that appends events
        Ok("first_result")
    }).await.unwrap();
    assert_eq!(result1, "first_result");

    // Replay (idempotent)
    let result2 = store.handle_command(&db.pool, cmd_id, || {
        panic!("Should not execute -- dedup table returns cached result");
    }).await.unwrap();
    assert_eq!(result2, "first_result");
}
```

### Fencing Token E2E Test

```rust
// tests/integration/fencing_token.rs

#[tokio::test]
async fn test_fencing_token_rejects_stale_submission() {
    let ctx = TestContext::new().await;

    // 1. Create session with a task
    let session_id = ctx.create_session().await;
    let task_id = ctx.create_task(session_id, "test_task").await;

    // 2. Scheduler leases task attempt=1, fencing_token=1
    let lease1 = ctx.scheduler.lease_task(session_id, task_id, 0).await.unwrap();
    assert_eq!(lease1.fencing_token, 1);

    // 3. Task times out, scheduler issues attempt=2, fencing_token=2
    let lease2 = ctx.scheduler.lease_task(session_id, task_id, 1).await.unwrap();
    assert_eq!(lease2.fencing_token, 2);

    // 4. Old Worker (from attempt 1) tries to submit with token=1
    let result = ctx.project_tools.submit_patch(
        session_id, task_id,
        lease1.lease_id, lease1.fencing_token, // STALE!
        "patch content"
    ).await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "fencing_token_stale");
}
```

## Deployment Architecture

### Per-Crate Dockerfiles

Each binary crate has its own Dockerfile using a multi-stage build:

```dockerfile
# deployments/docker/session-store.Dockerfile
FROM rust:1.85-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin session-store

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/session-store /usr/local/bin/session-store
COPY crates/session-store/migrations /migrations
USER 1000:1000
EXPOSE 50051
ENTRYPOINT ["session-store"]
```

### Kubernetes Deployment Model

Each service deploys as a Kubernetes Deployment with:
- **gRPC health checking** via `tonic-health` (K8s gRPC health probe)
- **Horizontal Pod Autoscaling** based on CPU and custom metrics
- **Service mesh** (optional Linkerd/Istio) for mTLS if not using native Tonic TLS
- **Config via environment variables** or Kubernetes Secrets for DB URLs, TLS certs

## Anti-Patterns to Avoid

### Anti-Pattern 1: Direct DB Writes from Multiple Services

**What goes wrong:** Scheduler writes directly to event_log, bypassing Session Store's CAS logic.
**Why bad:** Breaks the single-writer invariant. Two services can append with conflicting versions, corrupting the event stream.
**Do instead:** All Event Log writes go through Session Store's gRPC `AppendEvents` endpoint.

### Anti-Pattern 2: In-Process Shared State Between Services

**What goes wrong:** Co-locating Session Store and Scheduler in the same process with a shared `Arc<RwLock<HashMap>>`.
**Why bad:** (1) No process isolation -- a Scheduler panic takes down the Event Log writer. (2) No independent scaling. (3) Testing becomes impossible to isolate.
**Do instead:** Each service is a separate binary communicating via gRPC. Use protobuf contracts.

### Anti-Pattern 3: Putting Domain Logic in the Gateway

**What goes wrong:** Gateway handler validates fencing tokens, classifies patch semantics, or enforces approval policies.
**Why bad:** (1) Each backend service becomes a "dumb" CRUD wrapper with no invariants. (2) Frontend and Worker calls bypass Gateway's gRPC entry points anyway. (3) Business logic scatters across layers.
**Do instead:** Gateway is a pure protocol translator (REST to gRPC). All business logic lives in the backend gRPC services.

### Anti-Pattern 4: Synchronous Cross-Service Calls for Write Operations

**What goes wrong:** Scheduler calls Session Store, then Project Tools, then Effect Gateway in a synchronous chain, all within one request's lifetime.
**Why bad:** A failure in Effect Gateway leaves Session Store in an inconsistent state with committed events that have no effect. No atomicity across service boundaries.
**Do instead:** Use command/event pattern. Scheduler emits events to Session Store (the system of record). Downstream services react to events via their own polling or notification channels.

### Anti-Pattern 5: Using `tokio::spawn` for Fire-and-Forget Critical Paths

**What goes wrong:** `tokio::spawn(async { submit_effect().await });` -- the join handle is dropped, errors are silently lost.
**Why bad:** Effects might silently fail, leaving the system believing a side effect was committed when it was not. The outbox pattern exists precisely to avoid this.
**Do instead:** All critical side effects go through the Effect Gateway's outbox pattern. The dispatcher is a persistent background task with retry and reconciliation, not a fire-and-forget spawn.

## Sources

- **Tonic gRPC framework**: Context7 documentation for `/hyperium/tonic` -- server setup, interceptors, TLS, health checks, client channels. HIGH confidence.
- **Tower middleware**: Context7 documentation for `/websites/rs_tower` -- ServiceBuilder, Layer trait, concurrency/rate limiting, buffer, retry. HIGH confidence.
- **SQLx PostgreSQL**: Context7 documentation for `/launchbadge/sqlx` -- transactions, advisory locks, LISTEN/NOTIFY, pool management, migrations. HIGH confidence.
- **Prost protobuf codegen**: Context7 documentation for `/tokio-rs/prost` -- build.rs compilation, configuration. HIGH confidence.
- **Tokio async runtime**: Context7 documentation for `/tokio-rs/tokio` -- spawn, shared state, channels. HIGH confidence.
- **SwarmOS Architecture Design**: `architecture_v5.0_dynamic_orchestration.md` -- 31 sections covering three-plane design, Event-Sourced Session, lease/fencing, Effect Gateway approval policies, tenant governance. Primary reference for all domain decisions.
- **SwarmOS Gateway Implementation**: `swarmos_v5.1_gateway_implementation.md` -- middleware chain, REST to gRPC mapping, Go reference implementation adapted to Rust/Axum.
- **SwarmOS Gateway Mapping**: `swarmos_v5.1_gateway_mapping.md` -- gRPC method to REST path mapping, oneof resolution, error code tables.

---
*Architecture research for: SwarmOS v5.1 distributed AI agent orchestration platform*
*Researched: 2026-05-16*
