CREATE OR REPLACE FUNCTION rebuild_session_projection(session_uuid UUID)
RETURNS void AS $$
BEGIN
    DELETE FROM session_projection WHERE session_id = session_uuid;
    INSERT INTO session_projection (session_id, tenant_id, goal, state,
        current_plan_version, current_plan_epoch, session_version,
        policy_profile, created_at, updated_at)
    SELECT
        session_id, tenant_id,
        COALESCE((SELECT payload->>'goal' FROM event_log
             WHERE session_id = session_uuid AND event_type = 'SessionCreated'
             ORDER BY session_version LIMIT 1), '') AS goal,
        CASE WHEN EXISTS (SELECT 1 FROM event_log
                WHERE session_id = session_uuid AND event_type = 'SessionCompleted' LIMIT 1)
             THEN 'completed'
             WHEN EXISTS (SELECT 1 FROM event_log
                WHERE session_id = session_uuid AND event_type = 'SessionAborted' LIMIT 1)
             THEN 'aborted' ELSE 'active' END AS state,
        COALESCE((SELECT (payload->>'plan_version')::INTEGER FROM event_log
             WHERE session_id = session_uuid AND event_type = 'PlanCompiled'
             ORDER BY session_version DESC LIMIT 1), 0) AS current_plan_version,
        COALESCE((SELECT (payload->>'plan_epoch')::INTEGER FROM event_log
             WHERE session_id = session_uuid AND event_type = 'PlanEpochStarted'
             ORDER BY session_version DESC LIMIT 1), 0) AS current_plan_epoch,
        COALESCE((SELECT MAX(session_version) FROM event_log WHERE session_id = session_uuid), 0) AS session_version,
        COALESCE((SELECT payload->'policy_profile' FROM event_log WHERE session_id = session_uuid AND event_type = 'SessionCreated' ORDER BY session_version LIMIT 1), '{}'::JSONB) AS policy_profile,
        (SELECT MIN(occurred_at) FROM event_log WHERE session_id = session_uuid) AS created_at,
        now() AS updated_at
    FROM (SELECT e.session_id, e.tenant_id FROM event_log e WHERE e.session_id = session_uuid LIMIT 1) AS init
    ON CONFLICT (session_id) DO NOTHING;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION rebuild_task_projection(session_uuid UUID)
RETURNS void AS $$
DECLARE
    rec RECORD;
BEGIN
    DELETE FROM task_projection WHERE session_id = session_uuid;
    FOR rec IN SELECT DISTINCT task_id FROM event_log
        WHERE session_id = session_uuid AND task_id IS NOT NULL
    LOOP
        INSERT INTO task_projection (
            session_id, task_id, task_type, state, task_attempt,
            current_lease_id, current_fencing_token, current_worker_spec_id,
            plan_epoch, replan_disposition
        ) VALUES (
            session_uuid, rec.task_id, '',
            COALESCE((SELECT CASE
                WHEN event_type = 'TaskSucceeded' THEN 'Succeeded'
                WHEN event_type = 'TaskFailed' THEN 'Failed'
                WHEN event_type = 'TaskTimedOut' THEN 'TimedOut'
                WHEN event_type = 'TaskCancelled' THEN 'Cancelled'
                WHEN event_type = 'TaskStarted' THEN 'Running'
                WHEN event_type = 'TaskLeased' THEN 'Leased'
                WHEN event_type = 'TaskReadied' THEN 'Ready'
                ELSE 'Pending' END
                FROM event_log WHERE session_id = session_uuid AND task_id = rec.task_id
                ORDER BY session_version DESC LIMIT 1), 'Pending'),
            COALESCE((SELECT MAX(task_attempt) FROM event_log WHERE session_id = session_uuid AND task_id = rec.task_id), 0),
            (SELECT (payload->>'lease_id')::UUID FROM event_log WHERE session_id = session_uuid AND task_id = rec.task_id AND event_type = 'TaskLeased' ORDER BY session_version DESC LIMIT 1),
            COALESCE((SELECT (payload->>'fencing_token')::BIGINT FROM event_log WHERE session_id = session_uuid AND task_id = rec.task_id AND event_type = 'TaskLeased' ORDER BY session_version DESC LIMIT 1), 0),
            (SELECT (payload->>'worker_spec_id')::UUID FROM event_log WHERE session_id = session_uuid AND task_id = rec.task_id AND event_type = 'TaskLeased' ORDER BY session_version DESC LIMIT 1),
            0, 'active'
        ) ON CONFLICT (session_id, task_id) DO NOTHING;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
