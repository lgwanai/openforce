use serde::{Deserialize, Serialize};

/// Execution status of a single worker run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkerStatus {
    /// 已完成 — Worker 成功执行完毕，所有任务均已完成。
    ///
    /// 当 Worker 的 run 函数正常返回且无错误时设置此状态。
    /// 这是唯一的成功终止状态，表示 Worker 交付了预期输出。
    Completed,

    /// 失败 — Worker 执行过程中遇到错误，未能成功完成。
    ///
    /// 当 Worker 的 run 函数因代码异常、逻辑错误或外部服务不可用
    /// 而返回错误时设置此状态。区别于 `Stalled`（停滞）和 `Blocked`（阻塞），
    /// 此状态表示 Worker 确实执行了但未成功。
    Failed,

    /// 停滞 — Worker 执行超时或卡住，未能在预期时间内产生结果。
    ///
    /// 当 Worker 启动后因长时间无响应、死锁或操作超时（timeout）
    /// 而被系统判定为停滞时设置此状态。Worker 可能仍在运行，
    /// 但已超出允许的时间窗口，系统不再等待其结果。
    Stalled,

    /// 阻塞 — Worker 因前置依赖未满足或被外部条件阻止而无法执行。
    ///
    /// 当 Worker 的前置任务未完成、所需资源不可用或调度器判定
    /// 执行条件不满足时设置此状态。Worker 从未实际启动执行，
    /// 而是被调度层直接标记为阻塞状态。
    Blocked,
}

impl WorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stalled => "stalled",
            Self::Blocked => "blocked",
        }
    }

    pub fn from_status_and_action(success: bool, action: &str) -> Self {
        if success {
            Self::Completed
        } else {
            match action {
                "blocked" => Self::Blocked,
                "stalled" | "timeout" => Self::Stalled,
                _ => Self::Failed,
            }
        }
    }
}

/// Reference to full worker output storage location.
///
/// Used for progressive disclosure of worker output. The `File` variant stores a
/// local filesystem path and is used in local development scenarios. The `Redis`
/// variant stores a session and worker ID for retrieval from a Redis-backed
/// session store, used in production deployments where worker output is
/// persisted to a shared Redis instance.
///
/// # Examples
///
/// ```ignore
/// // Local development: store output as a file
/// let local = OutputRef::File("/tmp/worker_output.json".into());
///
/// // Production: store output reference in Redis
/// let prod = OutputRef::Redis {
///     session_id: "session-abc".into(),
///     worker_id: "worker-42".into(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputRef {
    /// Stores output to a local filesystem path.
    ///
    /// Used during **local development** when no shared storage (Redis) is
    /// available. The contained `String` is the absolute or relative path to
    /// the output file on disk.
    ///
    /// # Example
    ///
    /// ```ignore
    /// OutputRef::File("/tmp/worker_out.json".into())
    /// ```
    File(String),

    /// Stores output reference in a Redis-backed session store.
    ///
    /// Used in **production deployments** where worker output is persisted to
    /// a shared Redis instance. Contains:
    /// - `session_id` — the session under which the output is stored
    /// - `worker_id` — the specific worker whose output is being referenced
    ///
    /// # Example
    ///
    /// ```ignore
    /// OutputRef::Redis {
    ///     session_id: "sess-001".into(),
    ///     worker_id: "worker-007".into(),
    /// }
    /// ```
    Redis {
        session_id: String,
        worker_id: String,
    },
}

impl OutputRef {
    pub fn file(path: &str) -> Self {
        Self::File(path.to_string())
    }

    pub fn redis(sid: &str, wid: &str) -> Self {
        Self::Redis {
            session_id: sid.to_string(),
            worker_id: wid.to_string(),
        }
    }
}

/// Compact summary of Planner context — a single worker's output folder with progressive disclosure.
///
/// **Level 1** (always in Planner context): `one_liner`, `status`, `key_notes`
/// **Level 2** (expand on demand): full output text via `expand_folder()`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerOutputFolder {
    pub worker_id: String,
    pub role: String,
    pub title: String,
    pub status: WorkerStatus,
    /// One-line summary extracted from output or key findings.
    pub one_liner: String,
    /// Key findings from worker memory (max 5 items).
    pub key_notes: Vec<String>,
    /// Location of full output for Level-2 expansion.
    pub output_ref: OutputRef,
    pub subtasks_done: usize,
    pub subtasks_total: usize,
    pub cycles: usize,
}

impl WorkerOutputFolder {
    /// Single-line summary for prompt injection.
    pub fn summary_line(&self) -> String {
        let icon = match self.status {
            WorkerStatus::Completed => "OK",
            WorkerStatus::Failed => "FAIL",
            WorkerStatus::Stalled => "STALL",
            WorkerStatus::Blocked => "BLOCKED",
        };
        format!(
            "[{}] {} ({}) [{}/{}]: {}",
            icon,
            self.worker_id,
            self.role,
            self.subtasks_done,
            self.subtasks_total,
            self.one_liner
        )
    }

    /// Format all folder summaries as a compact prompt section.
    /// Non-completed workers get their key_notes auto-expanded.
    pub fn format_for_prompt(folders: &[Self]) -> String {
        if folders.is_empty() {
            return String::new();
        }
        let mut out = String::from("## Previous Worker Results\n");
        for f in folders {
            out.push_str(&format!("- {}\n", f.summary_line()));
            if f.status != WorkerStatus::Completed && !f.key_notes.is_empty() {
                out.push_str("  Findings:\n");
                for note in &f.key_notes {
                    out.push_str(&format!("    - {note}\n"));
                }
            }
        }
        if out.len() > 6000 {
            out.truncate(6000);
            out.push_str("\n... (truncated)\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_status_from_action() {
        assert_eq!(
            WorkerStatus::from_status_and_action(true, "completed"),
            WorkerStatus::Completed
        );
        assert_eq!(
            WorkerStatus::from_status_and_action(false, "blocked"),
            WorkerStatus::Blocked
        );
        assert_eq!(
            WorkerStatus::from_status_and_action(false, "stalled"),
            WorkerStatus::Stalled
        );
        assert_eq!(
            WorkerStatus::from_status_and_action(false, "timeout"),
            WorkerStatus::Stalled
        );
        assert_eq!(
            WorkerStatus::from_status_and_action(false, "error"),
            WorkerStatus::Failed
        );
    }

    #[test]
    fn test_format_for_prompt_empty() {
        assert_eq!(WorkerOutputFolder::format_for_prompt(&[]), "");
    }

    #[test]
    fn test_format_for_prompt_mixed() {
        let folders = vec![
            WorkerOutputFolder {
                worker_id: "w0".into(),
                role: "dev".into(),
                title: "task1".into(),
                status: WorkerStatus::Completed,
                one_liner: "All tests pass".into(),
                key_notes: vec!["Coverage at 85%".into()],
                output_ref: OutputRef::file("/tmp/x.json"),
                subtasks_done: 5,
                subtasks_total: 5,
                cycles: 3,
            },
            WorkerOutputFolder {
                worker_id: "w1".into(),
                role: "tester".into(),
                title: "task2".into(),
                status: WorkerStatus::Failed,
                one_liner: "Tests failed".into(),
                key_notes: vec!["3/5 tests fail".into(), "Null pointer in handler".into()],
                output_ref: OutputRef::file("/tmp/y.json"),
                subtasks_done: 2,
                subtasks_total: 5,
                cycles: 2,
            },
        ];
        let result = WorkerOutputFolder::format_for_prompt(&folders);
        assert!(result.contains("[OK] w0"));
        assert!(result.contains("[FAIL] w1"));
        assert!(result.contains("Null pointer in handler"));
    }

    #[test]
    fn test_format_for_prompt_completed_no_findings() {
        let folders = vec![WorkerOutputFolder {
            worker_id: "w0".into(),
            role: "dev".into(),
            title: "t".into(),
            status: WorkerStatus::Completed,
            one_liner: "Done".into(),
            key_notes: vec![],
            output_ref: OutputRef::file("/tmp/x.json"),
            subtasks_done: 1,
            subtasks_total: 1,
            cycles: 1,
        }];
        let result = WorkerOutputFolder::format_for_prompt(&folders);
        assert!(!result.contains("Findings:"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let f = WorkerOutputFolder {
            worker_id: "w1".into(),
            role: "architect".into(),
            title: "Design API".into(),
            status: WorkerStatus::Completed,
            one_liner: "API designed with 3 endpoints".into(),
            key_notes: vec!["RESTful".into(), "OpenAPI 3.0".into()],
            output_ref: OutputRef::redis("sid", "w1"),
            subtasks_done: 3,
            subtasks_total: 3,
            cycles: 5,
        };
        let json = serde_json::to_string(&f).unwrap();
        let f2: WorkerOutputFolder = serde_json::from_str(&json).unwrap();
        assert_eq!(f.worker_id, f2.worker_id);
        assert_eq!(f.status, f2.status);
        assert_eq!(f.one_liner, f2.one_liner);
        assert_eq!(f.key_notes, f2.key_notes);
    }
}
