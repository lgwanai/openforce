-- ============================================================
-- Session projection: current state snapshot from event log
-- ============================================================
CREATE TABLE session_projection (
    session_id          UUID PRIMARY KEY,
    tenant_id           UUID NOT NULL,
    goal                TEXT NOT NULL DEFAULT '',
    state               VARCHAR(16) NOT NULL DEFAULT 'active'
                        CHECK (state IN ('active', 'completed', 'aborted')),
    current_plan_version INTEGER NOT NULL DEFAULT 0,
    current_plan_epoch   INTEGER NOT NULL DEFAULT 0,
    session_version     BIGINT NOT NULL DEFAULT 0,
    policy_profile      JSONB NOT NULL DEFAULT '{}',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_session_projection_tenant ON session_projection (tenant_id, state);

-- ============================================================
-- Task projection: current state snapshot
-- ============================================================
CREATE TABLE task_projection (
    session_id           UUID NOT NULL,
    task_id              UUID NOT NULL,
    task_type            VARCHAR(128) NOT NULL DEFAULT '',
    state                VARCHAR(16) NOT NULL DEFAULT 'Pending'
                         CHECK (state IN (
                             'Pending', 'Ready', 'Leased', 'Running',
                             'Succeeded', 'Failed', 'TimedOut', 'Cancelled'
                         )),
    task_attempt         INTEGER NOT NULL DEFAULT 0,
    current_lease_id     UUID,
    current_fencing_token BIGINT NOT NULL DEFAULT 0,
    current_worker_spec_id UUID,
    plan_epoch           INTEGER NOT NULL DEFAULT 0,
    replan_disposition   VARCHAR(16) NOT NULL DEFAULT 'active'
                         CHECK (replan_disposition IN (
                             'active', 'inherited', 'frozen', 'invalidated'
                         )),
    dependency_ids       JSONB NOT NULL DEFAULT '[]',
    downstream_ids       JSONB NOT NULL DEFAULT '[]',
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, task_id)
);

CREATE INDEX idx_task_projection_state ON task_projection (session_id, state);
CREATE INDEX idx_task_projection_lease ON task_projection (current_lease_id);
CREATE INDEX idx_task_projection_epoch ON task_projection (plan_epoch, replan_disposition);

COMMENT ON TABLE task_projection IS
'Materialized view of current task state, derived from event_log for read paths.';
