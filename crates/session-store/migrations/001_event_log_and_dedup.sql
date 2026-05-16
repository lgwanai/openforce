-- ============================================================
-- Event Log: append-only, no UPDATE, no DELETE
-- Architecture doc section 16, 21
-- ============================================================
CREATE TABLE event_log (
    event_id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type       VARCHAR(64) NOT NULL,
    session_id       UUID NOT NULL,
    tenant_id        UUID NOT NULL,
    plan_version     INTEGER NOT NULL DEFAULT 0,
    plan_epoch       INTEGER NOT NULL DEFAULT 0,
    task_id          UUID,
    task_attempt     INTEGER,
    causation_id     UUID NOT NULL,
    correlation_id   UUID NOT NULL,
    producer_component VARCHAR(128) NOT NULL,
    producer_instance  VARCHAR(128) NOT NULL,
    producer_region    VARCHAR(64) NOT NULL DEFAULT '',
    session_version  BIGINT NOT NULL,
    payload          JSONB NOT NULL,
    occurred_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_event_log_session_version UNIQUE (session_id, session_version)
);

CREATE INDEX idx_event_log_session ON event_log (session_id, session_version);
CREATE INDEX idx_event_log_tenant ON event_log (tenant_id, session_id);
CREATE INDEX idx_event_log_type ON event_log (event_type, occurred_at);
CREATE INDEX idx_event_log_task ON event_log (session_id, task_id, task_attempt);

COMMENT ON TABLE event_log IS
'Append-only event log for Event-Sourced Sessions. No UPDATE, no DELETE.';

-- ============================================================
-- Command dedup: idempotency for command_id replay (section 21.5)
-- ============================================================
CREATE TABLE command_dedup (
    command_id    UUID PRIMARY KEY,
    command_type  VARCHAR(64) NOT NULL,
    session_id    UUID NOT NULL,
    result        JSONB,
    error         TEXT,
    processed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_command_dedup_session ON command_dedup (session_id, processed_at DESC);

COMMENT ON TABLE command_dedup IS
'Ensures command-level idempotency. Same command_id always returns first result.';
