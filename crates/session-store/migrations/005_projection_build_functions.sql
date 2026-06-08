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
    FOR rec IN
        WITH compiled_tasks AS (
            SELECT
                (task_def->>'task_id')::UUID AS task_id,
                COALESCE(task_def->>'task_type', '') AS task_type,
                (payload->>'plan_epoch')::INTEGER AS plan_epoch
            FROM event_log,
                 jsonb_array_elements(COALESCE(payload->'tasks', '[]'::JSONB)) AS task_def
            WHERE session_id = session_uuid
              AND event_type = 'PlanCompiled'
              AND task_def ? 'task_id'
        ),
        event_tasks AS (
            SELECT DISTINCT task_id, ''::TEXT AS task_type, 0::INTEGER AS plan_epoch
            FROM event_log
            WHERE session_id = session_uuid AND task_id IS NOT NULL
        )
        SELECT DISTINCT ON (task_id) task_id, task_type, plan_epoch
        FROM (
            SELECT * FROM compiled_tasks
            UNION ALL
            SELECT * FROM event_tasks
        ) all_tasks
        ORDER BY task_id, plan_epoch DESC
    LOOP
        INSERT INTO task_projection (
            session_id, task_id, task_type, state, task_attempt,
            current_lease_id, current_fencing_token, current_worker_spec_id,
            plan_epoch, replan_disposition, dependency_ids, downstream_ids
        ) VALUES (
            session_uuid, rec.task_id, rec.task_type,
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
            COALESCE((SELECT MAX(plan_epoch) FROM event_log WHERE session_id = session_uuid AND task_id = rec.task_id), rec.plan_epoch, 0),
            'active',
            COALESCE((
                SELECT jsonb_agg(edge->>'from_task_id')
                FROM event_log,
                     jsonb_array_elements(COALESCE(payload->'dag_edges', '[]'::JSONB)) AS edge
                WHERE session_id = session_uuid
                  AND event_type = 'PlanCompiled'
                  AND edge->>'to_task_id' = rec.task_id::TEXT
            ), '[]'::JSONB),
            COALESCE((
                SELECT jsonb_agg(edge->>'to_task_id')
                FROM event_log,
                     jsonb_array_elements(COALESCE(payload->'dag_edges', '[]'::JSONB)) AS edge
                WHERE session_id = session_uuid
                  AND event_type = 'PlanCompiled'
                  AND edge->>'from_task_id' = rec.task_id::TEXT
            ), '[]'::JSONB)
        ) ON CONFLICT (session_id, task_id) DO NOTHING;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
