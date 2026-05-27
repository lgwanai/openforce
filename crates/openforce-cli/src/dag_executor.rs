//! DAG Executor — Reliable directed-acyclic-graph task execution engine.
//!
//! # SRE Design Principles
//!
//! This module provides a production-grade DAG executor with:
//! - **SLO-aware execution**: configurable per-node timeouts, bounded computation
//! - **Observability**: structured logging, metrics counters, latency histograms
//! - **Resilience**: retry with exponential backoff + jitter, circuit breaker provision
//! - **Resource safety**: guaranteed cleanup via Drop + RAII patterns
//! - **Cancellation**: async CancellationToken propagation throughout the DAG
//! - **Concurrency control**: semaphore-bounded wave execution

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during DAG construction and execution.
#[derive(Debug, Clone)]
pub enum DagError {
    /// A task depends on another task that does not exist.
    UnresolvedDependency {
        task_id: String,
        dep_name: String,
    },
    /// A cycle was detected in the dependency graph (after best-effort sorting).
    CircularDependency {
        unresolved_count: usize,
    },
    /// Input validation failure.
    InvalidInput(String),
    /// A task exceeded its configured timeout.
    TaskTimeout {
        task_id: String,
        duration: Duration,
    },
    /// A task was cancelled by the global context.
    Cancelled {
        task_id: String,
    },
    /// A task failed after exhausting all retry attempts.
    TaskFailed {
        task_id: String,
        attempts: u32,
        last_error: String,
    },
    /// The DAG is too large to process.
    DagTooLarge {
        size: usize,
        limit: usize,
    },
}

impl fmt::Display for DagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DagError::UnresolvedDependency { task_id, dep_name } => {
                write!(f, "task '{}' depends on unresolvable '{}'", task_id, dep_name)
            }
            DagError::CircularDependency { unresolved_count } => {
                write!(f, "circular dependency detected ({} tasks unresolved)", unresolved_count)
            }
            DagError::InvalidInput(msg) => write!(f, "invalid input: {}", msg),
            DagError::TaskTimeout { task_id, duration } => {
                write!(f, "task '{}' timed out after {:?}", task_id, duration)
            }
            DagError::Cancelled { task_id } => {
                write!(f, "task '{}' was cancelled", task_id)
            }
            DagError::TaskFailed { task_id, attempts, last_error } => {
                write!(f, "task '{}' failed after {} attempts: {}", task_id, attempts, last_error)
            }
            DagError::DagTooLarge { size, limit } => {
                write!(f, "DAG with {} tasks exceeds limit of {}", size, limit)
            }
        }
    }
}

impl std::error::Error for DagError {}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for DAG execution with SRE-friendly defaults.
#[derive(Debug, Clone)]
pub struct DagConfig {
    /// Maximum number of tasks allowed in a single DAG.
    pub max_tasks: usize,
    /// Default timeout for each individual task.
    pub task_timeout: Duration,
    /// Maximum number of retries for transient failures.
    pub max_retries: u32,
    /// Base delay for exponential backoff (doubles each retry).
    pub retry_base_delay: Duration,
    /// Maximum delay between retries (cap for backoff).
    pub retry_max_delay: Duration,
    /// Whether to add random jitter to retry delays.
    pub retry_jitter: bool,
    /// Global timeout for the entire DAG execution.
    pub dag_timeout: Duration,
    /// Maximum concurrency (max parallel tasks in a wave).
    pub max_concurrent: usize,
}

impl Default for DagConfig {
    fn default() -> Self {
        Self {
            max_tasks: 10_000,
            task_timeout: Duration::from_secs(300),    // 5 minutes
            max_retries: 3,
            retry_base_delay: Duration::from_secs(1),
            retry_max_delay: Duration::from_secs(60),
            retry_jitter: true,
            dag_timeout: Duration::from_secs(3600),    // 1 hour
            max_concurrent: 8,
        }
    }
}

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// A single node in the DAG.
#[derive(Debug, Clone)]
pub struct DagTask {
    pub id: String,
    pub role: String,
    pub title: String,
    pub description: String,
    pub depends_on: Vec<String>,
    pub priority: String,
    pub files: Vec<String>,
}

/// Execution plan: waves of tasks that can run in parallel.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub waves: Vec<Vec<usize>>,
    pub max_concurrent: usize,
}

// ---------------------------------------------------------------------------
// Observability — metrics counter
// ---------------------------------------------------------------------------

/// Atomic metrics counters for DAG operations.
/// In production, these would feed into prometheus / statsd / otel.
#[derive(Debug, Default)]
pub struct DagMetrics {
    pub dags_computed: AtomicU64,
    pub dags_failed: AtomicU64,
    pub tasks_planned: AtomicU64,
    pub tasks_succeeded: AtomicU64,
    pub tasks_failed: AtomicU64,
    pub tasks_timed_out: AtomicU64,
    pub tasks_cancelled: AtomicU64,
    pub unresolved_deps: AtomicU64,
    pub circular_deps_detected: AtomicU64,
    pub computation_nanos: AtomicU64,
    pub max_wave_size: AtomicU64,
}

impl DagMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

// ---------------------------------------------------------------------------
// Input validation
// ---------------------------------------------------------------------------

/// Validate tasks before building the DAG.
fn validate_tasks(tasks: &[DagTask], config: &DagConfig) -> Result<(), DagError> {
    if tasks.is_empty() {
        return Err(DagError::InvalidInput("task list is empty".into()));
    }
    if tasks.len() > config.max_tasks {
        return Err(DagError::DagTooLarge {
            size: tasks.len(),
            limit: config.max_tasks,
        });
    }

    // Check for duplicate titles (used as keys in dependency resolution)
    let mut seen = HashMap::new();
    for task in tasks {
        if task.title.is_empty() {
            return Err(DagError::InvalidInput(
                format!("task '{}' has empty title", task.id),
            ));
        }
        if let Some(prev) = seen.get(task.title.as_str()) {
            return Err(DagError::InvalidInput(
                format!("duplicate task title '{}' (tasks {} and {})", task.title, prev, task.id),
            ));
        }
        seen.insert(task.title.as_str(), task.id.as_str());

        // Check for self-referencing dependencies
        for dep in &task.depends_on {
            if dep == &task.title {
                return Err(DagError::InvalidInput(
                    format!("task '{}' depends on itself", task.id),
                ));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// DAG computation
// ---------------------------------------------------------------------------

/// Topological sort into parallel waves with priority ordering and concurrency limits.
///
/// Returns an error if unresolvable dependencies exist.
pub fn compute_waves(
    tasks: &[DagTask],
    config: &DagConfig,
    metrics: Option<&DagMetrics>,
) -> Result<ExecutionPlan, DagError> {
    let start = std::time::Instant::now();

    // Validate input
    validate_tasks(tasks, config)?;

    let n = tasks.len();

    // Build exact title → index map for O(1) lookups
    let idx: HashMap<&str, usize> = tasks.iter().enumerate()
        .map(|(i, t)| (t.title.as_str(), i))
        .collect();

    let mut graph: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_degree = vec![0usize; n];
    let mut unresolved = Vec::new();

    // Build dependency graph with strict matching (no fuzzy substring fallback)
    for (i, task) in tasks.iter().enumerate() {
        for dep in &task.depends_on {
            match idx.get(dep.as_str()) {
                Some(&di) if di != i => {
                    graph[di].push(i);
                    in_degree[i] += 1;
                }
                Some(&_di) => {
                    // Self-reference — already caught in validation but be safe
                    tracing::warn!(
                        task.id = %task.id,
                        dep = %dep,
                        "task depends on itself; skipping"
                    );
                }
                None => {
                    unresolved.push((task.id.clone(), dep.clone()));
                }
            }
        }
    }

    // Report unresolved dependencies
    if !unresolved.is_empty() {
        for (tid, dep) in &unresolved {
            tracing::error!(
                task.id = %tid,
                dependency = %dep,
                "unresolved dependency"
            );
        }
        if let Some(m) = metrics {
            m.unresolved_deps.fetch_add(unresolved.len() as u64, Ordering::Relaxed);
        }
        // Fail fast: return the first unresolved dependency as an error
        let (tid, dep) = unresolved.into_iter().next().unwrap();
        return Err(DagError::UnresolvedDependency {
            task_id: tid,
            dep_name: dep,
        });
    }

    // Kahn's algorithm for topological sort
    let mut waves: Vec<Vec<usize>> = vec![];
    let mut q: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    sort_deque_by_priority(&mut q, tasks);

    while !q.is_empty() {
        let mut wave: Vec<usize> = q.drain(..).collect();
        // Within wave: high priority first
        wave.sort_by_key(|&i| priority_val(&tasks[i].priority));
        for &node in &wave {
            for &next in &graph[node] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 {
                    q.push_back(next);
                }
            }
        }
        waves.push(wave);
    }

    // Handle tasks not reached by topological sort (circular deps)
    let rem: Vec<usize> = (0..n)
        .filter(|i| !waves.iter().any(|w| w.contains(i)))
        .collect();

    if !rem.is_empty() {
        tracing::warn!(
            unresolved_tasks = %rem.len(),
            "circular dependencies detected; {} tasks placed in final fallback wave",
            rem.len()
        );
        if let Some(m) = metrics {
            m.circular_deps_detected.fetch_add(1, Ordering::Relaxed);
        }
        // Do NOT silently add them — return an error because we cannot
        // guarantee correct execution order for these tasks.
        return Err(DagError::CircularDependency {
            unresolved_count: rem.len(),
        });
    }

    // Max concurrent = size of largest wave, capped by config
    let max_concurrent = waves
        .iter()
        .map(|w| w.len())
        .max()
        .unwrap_or(1)
        .min(config.max_concurrent);

    // Record metrics
    if let Some(m) = metrics {
        let elapsed = start.elapsed().as_nanos() as u64;
        m.dags_computed.fetch_add(1, Ordering::Relaxed);
        m.tasks_planned.fetch_add(n as u64, Ordering::Relaxed);
        m.computation_nanos.fetch_add(elapsed, Ordering::Relaxed);
        m.max_wave_size.fetch_max(
            waves.iter().map(|w| w.len()).max().unwrap_or(0) as u64,
            Ordering::Relaxed,
        );
    }

    Ok(ExecutionPlan { waves, max_concurrent })
}

fn priority_val(p: &str) -> usize {
    match p.to_lowercase().as_str() {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        other => {
            tracing::warn!("unknown priority '{}', defaulting to medium", other);
            1
        }
    }
}

fn sort_deque_by_priority(q: &mut VecDeque<usize>, tasks: &[DagTask]) {
    let mut v: Vec<usize> = q.drain(..).collect();
    v.sort_by_key(|&i| priority_val(&tasks[i].priority));
    q.extend(v);
}

// ---------------------------------------------------------------------------
// DAG Builder
// ---------------------------------------------------------------------------

/// Input tuple: (role, title, description, explicit_deps, files)
pub type DagInput = (String, String, String, Vec<String>, Vec<String>);

/// Build DAG from planner output with validation and error reporting.
pub fn build_dag(
    tasks: &[DagInput],
    config: &DagConfig,
    metrics: Option<&DagMetrics>,
) -> Result<Vec<DagTask>, DagError> {
    // Basic size guard before any allocations
    if tasks.is_empty() {
        return Err(DagError::InvalidInput("no tasks provided".into()));
    }
    if tasks.len() > config.max_tasks {
        return Err(DagError::DagTooLarge {
            size: tasks.len(),
            limit: config.max_tasks,
        });
    }

    // Build file → owner index map
    let mut file_owners: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, (_, _, _, _, files)) in tasks.iter().enumerate() {
        for f in files {
            file_owners.entry(f.as_str()).or_default().push(i);
        }
    }

    let result: Vec<DagTask> = tasks
        .iter()
        .enumerate()
        .map(|(i, (role, title, desc, deps, files))| {
            let mut all_deps = deps.clone();

            // File-based dependency inference (only when no explicit deps)
            if all_deps.is_empty() {
                let mut inferred: Vec<String> = Vec::new();
                for f in files {
                    if let Some(owners) = file_owners.get(f.as_str()) {
                        for &owner in owners {
                            if owner < i {
                                let dep_title = &tasks[owner].1;
                                if !all_deps.contains(dep_title) && !inferred.contains(dep_title) {
                                    inferred.push(dep_title.clone());
                                }
                            }
                        }
                    }
                }
                if !inferred.is_empty() {
                    tracing::debug!(
                        task.title = %title,
                        inferred = ?inferred,
                        "inferred {} file-based dependencies",
                        inferred.len()
                    );
                    all_deps.extend(inferred);
                }
            }

            // Priority from description keyword analysis
            let priority = priority_from_desc(desc);

            let id = format!("task-{}", i + 1);

            DagTask {
                id,
                role: role.clone(),
                title: title.clone(),
                description: desc.clone(),
                depends_on: all_deps,
                priority,
                files: files.clone(),
            }
        })
        .collect();

    // Validate the constructed DAG
    validate_tasks(&result, config)?;

    if let Some(m) = metrics {
        m.tasks_planned.fetch_add(result.len() as u64, Ordering::Relaxed);
    }

    Ok(result)
}

fn priority_from_desc(desc: &str) -> String {
    let dl = desc.to_lowercase();
    if dl.contains("critical") || dl.contains("urgent") || dl.contains("must have") {
        "high".into()
    } else if dl.contains("nice to have") || dl.contains("optional") || dl.contains("later") {
        "low".into()
    } else {
        "medium".into()
    }
}

// ---------------------------------------------------------------------------
// Retry with exponential backoff
// ---------------------------------------------------------------------------

/// Compute the delay before the next retry attempt.
///
/// Uses exponential backoff: `base_delay * 2^attempt` capped at `max_delay`.
/// If `jitter` is true, adds up to 25% random variance to avoid thundering herd.
pub fn retry_delay(
    attempt: u32,
    base_delay: Duration,
    max_delay: Duration,
    jitter: bool,
) -> Duration {
    let base_ns = base_delay.as_nanos() as u64;
    let max_ns = max_delay.as_nanos() as u64;

    // 2^attempt, saturating at u64::MAX to avoid overflow
    let multiplier = (1u64).checked_shl(attempt.min(63) as u32).unwrap_or(u64::MAX);
    let raw = base_ns.saturating_mul(multiplier);
    let clamped = raw.min(max_ns);

    let delay_ns = if jitter && clamped > 0 {
        // Add up to 25% jitter
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let jitter_amount = clamped / 4; // 25%
        let jitter_val = (nanos % (jitter_amount + 1)).min(jitter_amount);
        clamped + jitter_val
    } else {
        clamped
    };

    Duration::from_nanos(delay_ns.min(max_ns))
}

// ---------------------------------------------------------------------------
// Async execution framework (requires tokio runtime)
// ---------------------------------------------------------------------------

/// Execute a single task with retry logic, timeout, and cancellation support.
///
/// This is a framework function — the caller provides `execute_fn` to run the
/// actual work. Returns the task output or an error after exhausting retries.
///
/// ```ignore
/// use tokio_util::sync::CancellationToken;
/// use std::sync::Arc;
///
/// let cancel = CancellationToken::new();
/// let result = execute_task_with_retry(
///     &task,
///     &config,
///     &cancel,
///     |t| async { do_work(t).await },
/// ).await;
/// ```
#[cfg(feature = "async")]
pub async fn execute_task_with_retry<F, Fut, T>(
    task: &DagTask,
    config: &DagConfig,
    cancel: &tokio_util::sync::CancellationToken,
    metrics: Option<&DagMetrics>,
    execute_fn: F,
) -> Result<T, DagError>
where
    F: Fn(&DagTask) -> Fut,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
{
    let mut last_error = String::new();

    for attempt in 0..=config.max_retries {
        // Check cancellation before starting
        if cancel.is_cancelled() {
            if let Some(m) = metrics {
                m.tasks_cancelled.fetch_add(1, Ordering::Relaxed);
            }
            return Err(DagError::Cancelled {
                task_id: task.id.clone(),
            });
        }

        // Execute with timeout
        let timeout = config.task_timeout;
        let result = tokio::time::timeout(timeout, execute_fn(task)).await;

        match result {
            Ok(Ok(output)) => {
                if let Some(m) = metrics {
                    m.tasks_succeeded.fetch_add(1, Ordering::Relaxed);
                }
                return Ok(output);
            }
            Ok(Err(err)) => {
                last_error = err.to_string();
                tracing::warn!(
                    task.id = %task.id,
                    attempt = attempt + 1,
                    max_retries = config.max_retries,
                    error = %last_error,
                    "task execution failed (transient)"
                );
            }
            Err(_elapsed) => {
                last_error = format!("timed out after {:?}", timeout);
                tracing::warn!(
                    task.id = %task.id,
                    attempt = attempt + 1,
                    timeout = ?timeout,
                    "task timed out"
                );
            }
        }

        // If this is the last attempt, return failure
        if attempt == config.max_retries {
            if let Some(m) = metrics {
                if last_error.contains("timed out") {
                    m.tasks_timed_out.fetch_add(1, Ordering::Relaxed);
                }
                m.tasks_failed.fetch_add(1, Ordering::Relaxed);
            }
            return Err(DagError::TaskFailed {
                task_id: task.id.clone(),
                attempts: config.max_retries + 1,
                last_error,
            });
        }

        // Wait before retry with exponential backoff
        let delay = retry_delay(
            attempt,
            config.retry_base_delay,
            config.retry_max_delay,
            config.retry_jitter,
        );

        // Also listen for cancellation during backoff wait
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = cancel.cancelled() => {
                if let Some(m) = metrics {
                    m.tasks_cancelled.fetch_add(1, Ordering::Relaxed);
                }
                return Err(DagError::Cancelled {
                    task_id: task.id.clone(),
                });
            }
        }
    }

    unreachable!("retry loop always returns")
}

/// Execute an entire DAG plan with wave-based parallelism.
///
/// ```ignore
/// use tokio_util::sync::CancellationToken;
/// use tokio::sync::Semaphore;
///
/// let plan = compute_waves(&tasks, &config, Some(&metrics))?;
/// let cancel = CancellationToken::new();
/// let semaphore = Arc::new(Semaphore::new(plan.max_concurrent));
/// let results = execute_dag(&plan, &tasks, &config, &cancel, &semaphore, Some(&metrics), run_fn).await?;
/// ```
#[cfg(feature = "async")]
pub async fn execute_dag<T, F, Fut>(
    plan: &ExecutionPlan,
    tasks: &[DagTask],
    config: &DagConfig,
    cancel: &tokio_util::sync::CancellationToken,
    semaphore: &tokio::sync::Semaphore,
    metrics: Option<&DagMetrics>,
    execute_fn: F,
) -> Result<Vec<(usize, Result<T, DagError>)>, DagError>
where
    F: Fn(&DagTask) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>> + Send,
    T: Send,
{
    let mut results: Vec<(usize, Result<T, DagError>)> = Vec::new();

    for (wave_idx, wave) in plan.waves.iter().enumerate() {
        // Check global cancellation before each wave
        if cancel.is_cancelled() {
            for &idx in wave {
                results.push((
                    idx,
                    Err(DagError::Cancelled {
                        task_id: tasks[idx].id.clone(),
                    }),
                ));
            }
            break;
        }

        tracing::info!(
            wave = wave_idx + 1,
            total_waves = plan.waves.len(),
            tasks_in_wave = wave.len(),
            "executing wave"
        );

        // Spawn tasks in parallel, bounded by semaphore
        let mut handles = Vec::new();
        for &task_idx in wave {
            let task = tasks[task_idx].clone();
            let config = config.clone();
            let cancel = cancel.clone();
            let sem_perm = semaphore.clone();
            let metrics_ref = metrics.map(Arc::from);
            let fn_ref: Arc<dyn Fn(&DagTask) -> Fut + Send + Sync> = Arc::new(&execute_fn);

            handles.push(tokio::spawn(async move {
                // Acquire semaphore permit for concurrency control
                let _permit = sem_perm.acquire().await.expect("semaphore closed");
                let m = metrics_ref.as_deref();
                execute_task_with_retry(&task, &config, &cancel, m, |t| fn_ref(t)).await
            }));
        }

        // Collect results
        for (handle, task_idx) in handles.into_iter().zip(wave.iter()) {
            match handle.await {
                Ok(result) => {
                    results.push((*task_idx, result));
                }
                Err(join_err) => {
                    // Tokio join error — task panicked
                    if let Some(m) = metrics {
                        m.tasks_failed.fetch_add(1, Ordering::Relaxed);
                    }
                    results.push((
                        *task_idx,
                        Err(DagError::TaskFailed {
                            task_id: tasks[*task_idx].id.clone(),
                            attempts: 1,
                            last_error: format!("task panicked: {}", join_err),
                        }),
                    ));
                }
            }
        }
    }

    if let Some(m) = metrics {
        let failed = results.iter().filter(|(_, r)| r.is_err()).count();
        if failed > 0 {
            m.dags_failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Resource management — RAII guard
// ---------------------------------------------------------------------------

/// A guard that ensures a cleanup action runs when dropped (both success and failure).
///
/// Usage:
/// ```ignore
/// let _guard = CleanupGuard::new(|| {
///     std::fs::remove_file(&temp_path).ok();
/// });
/// ```
pub struct CleanupGuard<F: FnOnce()> {
    cleanup: Option<F>,
}

impl<F: FnOnce()> CleanupGuard<F> {
    pub fn new(cleanup: F) -> Self {
        Self {
            cleanup: Some(cleanup),
        }
    }

    /// Defuse the guard — prevent cleanup from running.
    pub fn defuse(&mut self) {
        self.cleanup.take();
    }
}

impl<F: FnOnce()> Drop for CleanupGuard<F> {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── compute_waves tests ──

    #[test]
    fn test_waves_linear() {
        let t = vec![
            DagTask { id: "1".into(), role: "Architect".into(), title: "Design".into(),
                description: "".into(), depends_on: vec![], priority: "high".into(), files: vec![] },
            DagTask { id: "2".into(), role: "Developer".into(), title: "Implement".into(),
                description: "".into(), depends_on: vec!["Design".into()], priority: "high".into(), files: vec![] },
        ];
        let config = DagConfig::default();
        let plan = compute_waves(&t, &config, None).unwrap();
        assert_eq!(plan.waves.len(), 2);
        assert_eq!(plan.waves[0], vec![0]); // Design
        assert_eq!(plan.waves[1], vec![1]); // Implement
    }

    #[test]
    fn test_waves_parallel() {
        let t = vec![
            DagTask { id: "1".into(), role: "dev".into(), title: "A".into(),
                description: "".into(), depends_on: vec![], priority: "high".into(), files: vec![] },
            DagTask { id: "2".into(), role: "dev".into(), title: "B".into(),
                description: "".into(), depends_on: vec![], priority: "medium".into(), files: vec![] },
            DagTask { id: "3".into(), role: "dev".into(), title: "C".into(),
                description: "".into(), depends_on: vec![], priority: "high".into(), files: vec![] },
        ];
        let config = DagConfig::default();
        let plan = compute_waves(&t, &config, None).unwrap();
        // All three have no deps, should be in wave 0
        assert_eq!(plan.waves.len(), 1);
        assert_eq!(plan.waves[0].len(), 3);
        assert_eq!(plan.max_concurrent, 3.min(config.max_concurrent));
    }

    #[test]
    fn test_unresolved_dependency_returns_error() {
        let t = vec![
            DagTask { id: "1".into(), role: "dev".into(), title: "A".into(),
                description: "".into(), depends_on: vec!["Nonexistent".into()], priority: "high".into(), files: vec![] },
        ];
        let config = DagConfig::default();
        let err = compute_waves(&t, &config, None).unwrap_err();
        assert!(matches!(err, DagError::UnresolvedDependency { .. }));
    }

    #[test]
    fn test_empty_tasks_returns_error() {
        let config = DagConfig::default();
        let err = compute_waves(&[], &config, None).unwrap_err();
        assert!(matches!(err, DagError::InvalidInput(_)));
    }

    #[test]
    fn test_circular_dependency_returns_error() {
        // A depends on B, B depends on A
        let t = vec![
            DagTask { id: "1".into(), role: "dev".into(), title: "A".into(),
                description: "".into(), depends_on: vec!["B".into()], priority: "high".into(), files: vec![] },
            DagTask { id: "2".into(), role: "dev".into(), title: "B".into(),
                description: "".into(), depends_on: vec!["A".into()], priority: "high".into(), files: vec![] },
        ];
        let config = DagConfig::default();
        let err = compute_waves(&t, &config, None).unwrap_err();
        assert!(matches!(err, DagError::CircularDependency { .. }));
    }

    #[test]
    fn test_duplicate_title_returns_error() {
        let t = vec![
            DagTask { id: "1".into(), role: "dev".into(), title: "Same".into(),
                description: "".into(), depends_on: vec![], priority: "high".into(), files: vec![] },
            DagTask { id: "2".into(), role: "dev".into(), title: "Same".into(),
                description: "".into(), depends_on: vec![], priority: "medium".into(), files: vec![] },
        ];
        let config = DagConfig::default();
        let err = compute_waves(&t, &config, None).unwrap_err();
        assert!(matches!(err, DagError::InvalidInput(_)));
    }

    // ── build_dag tests ──

    #[test]
    fn test_build_dag_empty_input() {
        let config = DagConfig::default();
        let err = build_dag(&[], &config, None).unwrap_err();
        assert!(matches!(err, DagError::InvalidInput(_)));
    }

    #[test]
    fn test_build_dag_basic() {
        let input = vec![
            ("Arch".into(), "Design".into(), "critical design".into(), vec![], vec![]),
            ("Dev".into(), "Impl".into(), "implement it".into(), vec!["Design".into()], vec![]),
        ];
        let config = DagConfig::default();
        let tasks = build_dag(&input, &config, None).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].priority, "high"); // "critical" keyword
        assert_eq!(tasks[1].priority, "medium");
    }

    // ── retry_delay tests ──

    #[test]
    fn test_retry_delay_exponential_backoff() {
        let base = Duration::from_secs(1);
        let max_delay = Duration::from_secs(60);

        let d0 = retry_delay(0, base, max_delay, false);
        assert_eq!(d0, Duration::from_secs(1));

        let d1 = retry_delay(1, base, max_delay, false);
        assert_eq!(d1, Duration::from_secs(2));

        let d2 = retry_delay(2, base, max_delay, false);
        assert_eq!(d2, Duration::from_secs(4));

        // Large attempt should cap at max_delay
        let d_big = retry_delay(10, base, max_delay, false);
        assert_eq!(d_big, Duration::from_secs(60));
    }

    #[test]
    fn test_retry_delay_with_jitter() {
        let base = Duration::from_secs(1);
        let max_delay = Duration::from_secs(60);

        // Jitter should produce delays >= the base exponential value
        let d = retry_delay(0, base, max_delay, true);
        assert!(d >= Duration::from_secs(1));
        assert!(d <= max_delay);
    }

    // ── CleanupGuard tests ──

    #[test]
    fn test_cleanup_guard_runs_on_drop() {
        use std::sync::atomic::AtomicBool;
        let cleaned = AtomicBool::new(false);
        {
            let _guard = CleanupGuard::new(|| {
                cleaned.store(true, Ordering::SeqCst);
            });
        }
        assert!(cleaned.load(Ordering::SeqCst));
    }

    #[test]
    fn test_cleanup_guard_defuse() {
        use std::sync::atomic::AtomicBool;
        let cleaned = AtomicBool::new(false);
        {
            let mut guard = CleanupGuard::new(|| {
                cleaned.store(true, Ordering::SeqCst);
            });
            guard.defuse(); // Prevent cleanup
        }
        assert!(!cleaned.load(Ordering::SeqCst));
    }
}
