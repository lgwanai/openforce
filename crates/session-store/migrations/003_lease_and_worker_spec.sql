-- ============================================================
-- Lease records: one per task_attempt
-- ============================================================
CREATE TABLE lease_records (
    lease_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id      UUID NOT NULL,
    task_id         UUID NOT NULL,
    task_attempt    INTEGER NOT NULL CHECK (task_attempt >= 1),
    fencing_token   BIGINT NOT NULL CHECK (fencing_token >= 0),
    worker_spec_id  UUID NOT NULL,
    state           VARCHAR(16) NOT NULL DEFAULT 'active'
                    CHECK (state IN ('active', 'expired', 'renewed', 'revoked')),
    expire_at       TIMESTAMPTZ NOT NULL,
    renewal_deadline TIMESTAMPTZ,
    heartbeat_interval_sec INTEGER NOT NULL DEFAULT 15,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (session_id, task_id, task_attempt)
);

CREATE INDEX idx_lease_records_expire ON lease_records (state, expire_at);
CREATE INDEX idx_lease_records_fencing ON lease_records (session_id, task_id, fencing_token DESC);

-- ============================================================
-- Worker Spec records: frozen execution specifications (section 17)
-- ============================================================
CREATE TABLE worker_specs (
    worker_spec_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id       UUID NOT NULL,
    task_id          UUID NOT NULL,
    task_attempt     INTEGER NOT NULL,
    plan_version     INTEGER NOT NULL,
    plan_epoch       INTEGER NOT NULL,
    lease_id         UUID NOT NULL,
    fencing_token    BIGINT NOT NULL,
    spec_json        JSONB NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (session_id, task_id, task_attempt)
);

CREATE INDEX idx_worker_specs_lookup ON worker_specs (lease_id);

COMMENT ON TABLE worker_specs IS
'Frozen WorkerSpec per task attempt. Never mutated after creation.';
