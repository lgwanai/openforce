use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Observer collects runtime metrics without affecting the main execution path.
/// Architecture doc section 11.1: bypaass observation only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: Uuid,
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub task_type: String,
    pub metric_type: String,
    pub value: f64,
    pub labels: Vec<(String, String)>,
    pub observed_at: DateTime<Utc>,
}

pub struct Observer;

impl Observer {
    pub fn record_latency(session_id: Uuid, task_id: Uuid, task_type: &str, latency_ms: f64) -> Observation {
        Observation {
            id: Uuid::now_v7(), session_id, task_id, task_type: task_type.into(),
            metric_type: "latency_ms".into(), value: latency_ms,
            labels: vec![], observed_at: Utc::now(),
        }
    }

    pub fn record_success_rate(session_id: Uuid, success: bool) -> Observation {
        Observation {
            id: Uuid::now_v7(), session_id, task_id: Uuid::nil(),
            task_type: "".into(), metric_type: "success".into(),
            value: if success { 1.0 } else { 0.0 },
            labels: vec![], observed_at: Utc::now(),
        }
    }

    pub fn record_tool_call(session_id: Uuid, task_id: Uuid, tool_name: &str, success: bool) -> Observation {
        Observation {
            id: Uuid::now_v7(), session_id, task_id, task_type: "".into(),
            metric_type: "tool_call".into(), value: if success { 1.0 } else { 0.0 },
            labels: vec![("tool".into(), tool_name.into())], observed_at: Utc::now(),
        }
    }
}
