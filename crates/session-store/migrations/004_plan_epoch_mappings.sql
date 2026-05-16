-- ============================================================
-- Plan Epoch: version boundaries for re-plan isolation (section 23)
-- ============================================================
CREATE TABLE plan_epochs (
    session_id      UUID NOT NULL,
    plan_epoch      INTEGER NOT NULL,
    plan_version    INTEGER NOT NULL,
    state           VARCHAR(16) NOT NULL DEFAULT 'active'
                    CHECK (state IN ('active', 'closed')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at       TIMESTAMPTZ,
    PRIMARY KEY (session_id, plan_epoch)
);

-- ============================================================
-- Epoch task mappings: Inherited/Frozen/Invalidated
-- ============================================================
CREATE TABLE epoch_task_mappings (
    session_id      UUID NOT NULL,
    from_plan_epoch INTEGER NOT NULL,
    to_plan_epoch   INTEGER NOT NULL,
    from_task_id    UUID NOT NULL,
    to_task_id      UUID,
    mode            VARCHAR(16) NOT NULL
                    CHECK (mode IN ('inherited', 'frozen', 'invalidated')),
    mapped_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, from_plan_epoch, from_task_id)
);

CREATE INDEX idx_epoch_task_mappings_to ON epoch_task_mappings (session_id, to_plan_epoch, mode);

COMMENT ON TABLE epoch_task_mappings IS
'Explicit mapping of old-plan tasks to new-plan tasks when plan epoch advances.';
