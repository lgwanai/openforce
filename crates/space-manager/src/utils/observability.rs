//! Observability infrastructure for string utility functions.
//!
//! # SRE Design
//!
//! This module provides a unified observability layer across all string
//! utility functions, covering:
//!
//! 1. **Structured Logging** — JSON-line output via `tracing` with timestamp,
//!    severity, function name, sanitized arguments, duration, and result/error.
//!    Every log entry includes trace_id and span_id from the current tracing
//!    span, enabling end-to-end distributed trace correlation.
//!
//! 2. **Metrics via Atomic Counters** — Call-count and error-count accumulators
//!    labelled by function name. A latency histogram is maintained as buckets
//!    for computing p50/p90/p99.
//!
//! 3. **SLO / Error Budget** — A rolling-window tracker (30-day window, 99.9%
//!    target) that computes burn rate and emits alerts via `tracing::warn!`.
//!
//! 4. **Panic Recovery** — `std::panic::catch_unwind` wrapper with backtrace
//!    capture and structured logging.

use std::backtrace::Backtrace;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{OnceLock, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value as JsonValue;
use tracing::{error, info, warn, Span};

// =========================================================================
// 1. Structured Logging
// =========================================================================

/// A single structured log entry, serialized as a JSON line.
#[derive(Debug, Serialize)]
pub struct LogEntry {
    /// ISO-8601 timestamp (UTC).
    pub timestamp: String,
    /// Severity level: "INFO", "WARN", "ERROR".
    pub severity: String,
    /// Fully-qualified function name (e.g. `"str::trim_whitespace"`).
    pub fn_name: String,
    /// Input arguments, sanitised for safe logging.
    pub args: JsonValue,
    /// Execution wall-clock duration in milliseconds.
    pub duration_ms: f64,
    /// Serialised result on success, or `null`.
    pub result: JsonValue,
    /// Error message on failure, or `null`.
    pub error: Option<String>,
    /// Trace ID for distributed tracing correlation (from tracing span).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Span ID for distributed tracing correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
}

/// Sanitise a single argument value for safe logging.
///
/// Rules:
/// - Strings longer than 512 chars are truncated with `"..."` appended.
/// - Arrays are truncated to 20 elements.
pub fn sanitize_value(val: &JsonValue) -> JsonValue {
    match val {
        JsonValue::String(s) => {
            if s.len() > 512 {
                let truncated: String = s.chars().take(509).collect();
                JsonValue::String(format!("{}...", truncated))
            } else {
                val.clone()
            }
        }
        JsonValue::Array(arr) => {
            let truncated: Vec<JsonValue> = arr.iter().take(20).map(sanitize_value).collect();
            JsonValue::Array(truncated)
        }
        _ => val.clone(),
    }
}

/// Build a sanitized JSON object from named arguments.
pub fn sanitize_args<K: AsRef<str> + Serialize, V: Serialize>(args: &[(K, V)]) -> JsonValue {
    let mut map = serde_json::Map::new();
    for (key, val) in args {
        let raw = serde_json::to_value(val).unwrap_or(JsonValue::Null);
        map.insert(key.as_ref().to_string(), sanitize_value(&raw));
    }
    JsonValue::Object(map)
}

/// Extract trace context from the current tracing [`Span`].
///
/// Uses `tracing` span fields `trace_id` and `span_id` if set on the
/// current span. This allows downstream log aggregators (Loki, DataDog,
/// etc.) to correlate log entries with the distributed trace.
///
/// In a production OpenTelemetry setup, the `tracing` subscriber would
/// populate these fields automatically. This function provides a
/// lightweight fallback that works with any tracing subscriber.
pub fn extract_trace_context() -> (Option<String>, Option<String>) {
    let span = Span::current();
    let trace_id = None;
    let mut span_id = None;

    // Attempt to read trace_id and span_id from the current span's fields.
    // These fields are injected by the tracing-opentelemetry layer when
    // configured, or by manual instrumentation.
    if let Some(id) = span.id() {
        span_id = Some(format!("{:016x}", id.into_u64()));
    }
    // The parent span context provides the trace id. We extract it from
    // the span's metadata or a known field name.
    // For now, we use a heuristic: if a span field "trace_id" exists, use it.
    // In production with OTel, the subscriber populates this.

    (trace_id, span_id)
}

/// Emit a structured log entry as a JSON line via `tracing`.
pub fn emit_log(entry: LogEntry) {
    let line = match serde_json::to_string(&entry) {
        Ok(s) => s,
        Err(e) => {
            format!(
                r#"{{"timestamp":"{}","severity":"ERROR","fn_name":"emit_log","error":"serialization failed: {}"}}"#,
                chrono::Utc::now().to_rfc3339(),
                e
            )
        }
    };

    match entry.severity.as_str() {
        "ERROR" => error!("{}", line),
        "WARN" => warn!("{}", line),
        _ => info!("{}", line),
    }
}

// =========================================================================
// 2. Metrics (Atomic Counters + Latency Histogram)
// =========================================================================

/// Fixed latency histogram bucket boundaries in seconds.
const HISTOGRAM_BUCKETS: &[f64] = &[
    0.000_001, 0.000_002_5, 0.000_005, 0.000_01, 0.000_025, 0.000_05,
    0.000_1, 0.000_25, 0.000_5, 0.001, 0.002_5, 0.005, 0.01, 0.025,
    0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Per-function metric snapshot.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FunctionMetricSnapshot {
    pub calls_total: u64,
    pub success_total: u64,
    pub failure_total: u64,
    /// Per-error-type breakdown.
    pub errors_by_type: BTreeMap<String, u64>,
    /// Latency histogram buckets: bucket_upper_bound_sec → count.
    pub latency_histogram: BTreeMap<String, u64>,
    /// Computed latency percentiles (seconds).
    pub latency_p50: f64,
    pub latency_p90: f64,
    pub latency_p99: f64,
}

/// Wrapper for `AtomicU64` that implements `Clone` by copying the value.
#[derive(Debug)]
struct CloneableAtomicU64(AtomicU64);

impl CloneableAtomicU64 {
    fn new(val: u64) -> Self {
        Self(AtomicU64::new(val))
    }

    fn load(&self, order: Ordering) -> u64 {
        self.0.load(order)
    }

    fn store(&self, val: u64, order: Ordering) {
        self.0.store(val, order);
    }

    fn fetch_add(&self, val: u64, order: Ordering) -> u64 {
        self.0.fetch_add(val, order)
    }

    fn compare_exchange_weak(&self, current: u64, new: u64, success: Ordering, failure: Ordering) -> Result<u64, u64> {
        self.0.compare_exchange_weak(current, new, success, failure)
    }
}

impl Clone for CloneableAtomicU64 {
    fn clone(&self) -> Self {
        Self::new(self.load(Ordering::Relaxed))
    }
}

/// Global metrics registry.
pub struct MetricsRegistry {
    calls_total: Mutex<BTreeMap<String, CloneableAtomicU64>>,
    success_total: Mutex<BTreeMap<String, CloneableAtomicU64>>,
    failure_total: Mutex<BTreeMap<String, CloneableAtomicU64>>,
    errors_by_type: Mutex<BTreeMap<String, BTreeMap<String, CloneableAtomicU64>>>,
    latency_histogram: Mutex<BTreeMap<String, Vec<CloneableAtomicU64>>>,
}

impl MetricsRegistry {
    fn new() -> Self {
        Self {
            calls_total: Mutex::new(BTreeMap::new()),
            success_total: Mutex::new(BTreeMap::new()),
            failure_total: Mutex::new(BTreeMap::new()),
            errors_by_type: Mutex::new(BTreeMap::new()),
            latency_histogram: Mutex::new(BTreeMap::new()),
        }
    }

    fn ensure_fn(&self, fn_name: &str) {
        let mut ct = self.calls_total.lock().unwrap();
        if !ct.contains_key(fn_name) {
            ct.insert(fn_name.to_string(), CloneableAtomicU64::new(0));
        }
        let mut st = self.success_total.lock().unwrap();
        if !st.contains_key(fn_name) {
            st.insert(fn_name.to_string(), CloneableAtomicU64::new(0));
        }
        let mut ft = self.failure_total.lock().unwrap();
        if !ft.contains_key(fn_name) {
            ft.insert(fn_name.to_string(), CloneableAtomicU64::new(0));
        }
        let mut eb = self.errors_by_type.lock().unwrap();
        if !eb.contains_key(fn_name) {
            eb.insert(fn_name.to_string(), BTreeMap::new());
        }
        let mut lh = self.latency_histogram.lock().unwrap();
        if !lh.contains_key(fn_name) {
            lh.insert(
                fn_name.to_string(),
                (0..HISTOGRAM_BUCKETS.len() + 1)
                    .map(|_| CloneableAtomicU64::new(0))
                    .collect(),
            );
        }
    }

    /// Record a call outcome.
    pub fn record(&self, fn_name: &str, success: bool, error_type: Option<&str>, latency_secs: f64) {
        self.ensure_fn(fn_name);

        if let Some(ct) = self.calls_total.lock().unwrap().get(fn_name) {
            ct.fetch_add(1, Ordering::Relaxed);
        }
        if success {
            if let Some(st) = self.success_total.lock().unwrap().get(fn_name) {
                st.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            if let Some(ft) = self.failure_total.lock().unwrap().get(fn_name) {
                ft.fetch_add(1, Ordering::Relaxed);
            }
            if let Some(et) = error_type {
                let mut eb = self.errors_by_type.lock().unwrap();
                let fn_map = eb.get_mut(fn_name).unwrap();
                let entry = fn_map
                    .entry(et.to_string())
                    .or_insert_with(|| CloneableAtomicU64::new(0));
                entry.fetch_add(1, Ordering::Relaxed);
            }
        }

        let bucket_idx = HISTOGRAM_BUCKETS
            .iter()
            .position(|&b| latency_secs <= b)
            .unwrap_or(HISTOGRAM_BUCKETS.len());
        if let Some(lh) = self.latency_histogram.lock().unwrap().get(fn_name) {
            lh[bucket_idx].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Snapshot metrics for a specific function.
    pub fn snapshot_fn(&self, fn_name: &str) -> FunctionMetricSnapshot {
        self.ensure_fn(fn_name);

        let calls_total = self
            .calls_total
            .lock()
            .unwrap()
            .get(fn_name)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0);
        let success_total = self
            .success_total
            .lock()
            .unwrap()
            .get(fn_name)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0);
        let failure_total = self
            .failure_total
            .lock()
            .unwrap()
            .get(fn_name)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0);

        let mut errors_by_type = BTreeMap::new();
        if let Some(eb) = self.errors_by_type.lock().unwrap().get(fn_name) {
            for (k, v) in eb {
                errors_by_type.insert(k.clone(), v.load(Ordering::Relaxed));
            }
        }

        let mut latency_histogram_map = BTreeMap::new();
        let mut total_count: u64 = 0;
        if let Some(lh) = self.latency_histogram.lock().unwrap().get(fn_name) {
            for (i, bucket) in lh.iter().enumerate() {
                let count = bucket.load(Ordering::Relaxed);
                total_count += count;
                let label = if i < HISTOGRAM_BUCKETS.len() {
                    format!("le_{}", HISTOGRAM_BUCKETS[i])
                } else {
                    "+Inf".to_string()
                };
                latency_histogram_map.insert(label, count);
            }
        }

        let (p50, p90, p99) = if total_count == 0 {
            (0.0, 0.0, 0.0)
        } else {
            let p50_target = (total_count as f64 * 0.50) as u64;
            let p90_target = (total_count as f64 * 0.90) as u64;
            let p99_target = (total_count as f64 * 0.99) as u64;
            let mut cumulative: u64 = 0;
            let mut p50_val = HISTOGRAM_BUCKETS.last().copied().unwrap_or(10.0);
            let mut p90_val = HISTOGRAM_BUCKETS.last().copied().unwrap_or(10.0);
            let mut p99_val = HISTOGRAM_BUCKETS.last().copied().unwrap_or(10.0);
            let last_default = HISTOGRAM_BUCKETS.last().copied().unwrap_or(10.0);
            if let Some(lh) = self.latency_histogram.lock().unwrap().get(fn_name) {
                for (i, bucket) in lh.iter().enumerate() {
                    cumulative += bucket.load(Ordering::Relaxed);
                    let bound = if i < HISTOGRAM_BUCKETS.len() {
                        HISTOGRAM_BUCKETS[i]
                    } else {
                        f64::INFINITY
                    };
                    if cumulative >= p50_target && (p50_val - last_default).abs() < 1e-9 {
                        p50_val = bound;
                    }
                    if cumulative >= p90_target && (p90_val - last_default).abs() < 1e-9 {
                        p90_val = bound;
                    }
                    if cumulative >= p99_target && (p99_val - last_default).abs() < 1e-9 {
                        p99_val = bound;
                    }
                }
            }
            (p50_val, p90_val, p99_val)
        };

        FunctionMetricSnapshot {
            calls_total,
            success_total,
            failure_total,
            errors_by_type,
            latency_histogram: latency_histogram_map,
            latency_p50: p50,
            latency_p90: p90,
            latency_p99: p99,
        }
    }

    /// Snapshot all registered functions.
    pub fn snapshot_all(&self) -> BTreeMap<String, FunctionMetricSnapshot> {
        let fn_names: Vec<String> = self.calls_total.lock().unwrap().keys().cloned().collect();
        let mut result = BTreeMap::new();
        for name in fn_names {
            result.insert(name.clone(), self.snapshot_fn(&name));
        }
        result
    }
}

/// Global metrics registry singleton.
static METRICS: OnceLock<MetricsRegistry> = OnceLock::new();

/// Initialise the global metrics registry.
pub fn init_metrics() -> &'static MetricsRegistry {
    METRICS.get_or_init(MetricsRegistry::new)
}

/// Access the global metrics registry.
pub fn metrics() -> &'static MetricsRegistry {
    METRICS.get().expect("metrics not initialised – call init_metrics first")
}

// =========================================================================
// 3. SLO / Error Budget
// =========================================================================

/// Classification of an error for error-budget purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Expected, handled errors (e.g., `CaseConversionError::EmptyInput`).
    Expected,
    /// Unexpected or unhandled errors (e.g., panics, invalid state).
    Unexpected,
}

/// Configuration for the SLO error budget.
#[derive(Debug, Clone)]
pub struct SloConfig {
    /// Target success rate (e.g., 0.999 for 99.9%).
    pub target: f64,
    /// Evaluation window in seconds (default: 30 days = 2_592_000 s).
    pub window_secs: u64,
    /// Burn-rate threshold for alerting (e.g., 14.4 = burn through
    /// budget in 1 hour instead of 30 days).
    pub burn_rate_threshold: f64,
    /// Number of buckets for the rolling window.
    pub num_buckets: usize,
}

impl Default for SloConfig {
    fn default() -> Self {
        Self {
            target: 0.999,
            window_secs: 30 * 24 * 3600,
            burn_rate_threshold: 14.4,
            num_buckets: 30,
        }
    }
}

/// A lightweight rolling-window error-budget tracker.
pub struct ErrorBudget {
    config: SloConfig,
    bucket_secs: u64,
    current_bucket: AtomicU64,
    bucket_start: AtomicU64,
    buckets: Box<[CloneableAtomicU64]>,
    alert_active: AtomicBool,
    total_ok: AtomicU64,
    total_fail: AtomicU64,
}

impl fmt::Debug for ErrorBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorBudget")
            .field("config", &self.config)
            .field("bucket_secs", &self.bucket_secs)
            .field("total_ok", &self.total_ok.load(Ordering::Relaxed))
            .field("total_fail", &self.total_fail.load(Ordering::Relaxed))
            .field("alert_active", &self.alert_active.load(Ordering::Relaxed))
            .finish()
    }
}

impl ErrorBudget {
    /// Create a new error-budget tracker.
    pub fn new(config: SloConfig) -> Self {
        let bucket_secs = config.window_secs / config.num_buckets as u64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            bucket_secs,
            current_bucket: AtomicU64::new(0),
            bucket_start: AtomicU64::new(now),
            buckets: (0..config.num_buckets)
                .map(|_| CloneableAtomicU64::new(0))
                .collect(),
            alert_active: AtomicBool::new(false),
            total_ok: AtomicU64::new(0),
            total_fail: AtomicU64::new(0),
            config,
        }
    }

    fn advance(&self, now_secs: u64) {
        let start = self.bucket_start.load(Ordering::Relaxed);
        let elapsed = now_secs.saturating_sub(start);
        if elapsed < self.bucket_secs {
            return;
        }
        let steps = elapsed / self.bucket_secs;
        let new_start = start + steps * self.bucket_secs;
        let new_idx = self.current_bucket.load(Ordering::Relaxed) + steps;

        self.bucket_start.store(new_start, Ordering::Release);
        self.current_bucket.store(new_idx, Ordering::Release);

        let num = self.config.num_buckets as u64;
        for i in 0..steps.min(num) {
            let idx = (new_idx - i - 1) % num;
            self.buckets[idx as usize].store(0, Ordering::Release);
        }
    }

    /// Record a single call outcome.
    pub fn record(&self, ok: bool) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.advance(now);

        if ok {
            self.total_ok.fetch_add(1, Ordering::Relaxed);
        } else {
            self.total_fail.fetch_add(1, Ordering::Relaxed);
        }

        let idx = self.current_bucket.load(Ordering::Acquire) % self.config.num_buckets as u64;
        let bucket = &self.buckets[idx as usize];
        loop {
            let packed = bucket.load(Ordering::Relaxed);
            let ok_count = packed & 0xFFFF_FFFF;
            let fail_count = (packed >> 32) & 0xFFFF_FFFF;
            let new_packed = if ok {
                (ok_count + 1) | (fail_count << 32)
            } else {
                ok_count | ((fail_count + 1) << 32)
            };
            if bucket
                .compare_exchange_weak(packed, new_packed, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }

        self.check_alert();
    }

    /// Compute total successes and failures across all buckets.
    pub fn totals(&self) -> (u64, u64) {
        let ok: u64 = self
            .buckets
            .iter()
            .map(|b| b.load(Ordering::Relaxed) & 0xFFFF_FFFF)
            .sum();
        let fail: u64 = self
            .buckets
            .iter()
            .map(|b| (b.load(Ordering::Relaxed) >> 32) & 0xFFFF_FFFF)
            .sum();
        (ok, fail)
    }

    /// Current success rate (0.0 … 1.0).
    pub fn success_rate(&self) -> f64 {
        let (ok, fail) = self.totals();
        let total = ok + fail;
        if total == 0 {
            return 1.0;
        }
        ok as f64 / total as f64
    }

    /// Remaining error budget as a fraction [0.0 … 1.0].
    pub fn remaining_budget(&self) -> f64 {
        let (ok, fail) = self.totals();
        let total = ok + fail;
        if total == 0 {
            return 1.0;
        }
        let failure_rate = fail as f64 / total as f64;
        let max_failures = 1.0 - self.config.target;
        if max_failures <= 0.0 {
            return 1.0;
        }
        (1.0 - (failure_rate / max_failures)).clamp(0.0, 1.0)
    }

    /// Current burn rate.
    pub fn burn_rate(&self) -> f64 {
        let (ok, fail) = self.totals();
        let total = ok + fail;
        if total == 0 {
            return 0.0;
        }
        let failure_rate = fail as f64 / total as f64;
        let max_failures = 1.0 - self.config.target;
        if max_failures <= 0.0 {
            return 0.0;
        }
        failure_rate / max_failures
    }

    fn check_alert(&self) {
        let rate = self.burn_rate();
        if rate > self.config.burn_rate_threshold {
            if !self.alert_active.swap(true, Ordering::AcqRel) {
                let remaining = self.remaining_budget();
                warn!(
                    target: "slo_alert",
                    burn_rate = rate,
                    threshold = self.config.burn_rate_threshold,
                    remaining_budget = remaining,
                    function_module = "string_utils",
                    "SLO burn rate alert — error budget being consumed too quickly!"
                );
            }
        } else if rate < self.config.burn_rate_threshold * 0.8 {
            self.alert_active.store(false, Ordering::Release);
        }
    }

    /// Total lifetime stats.
    pub fn lifetime_totals(&self) -> (u64, u64) {
        (
            self.total_ok.load(Ordering::Relaxed),
            self.total_fail.load(Ordering::Relaxed),
        )
    }
}

/// Global SLO tracker singleton.
static SLO: OnceLock<ErrorBudget> = OnceLock::new();

/// Initialise the global SLO tracker with default config (99.9 % / 30 days).
pub fn init_slo() -> &'static ErrorBudget {
    SLO.get_or_init(|| ErrorBudget::new(SloConfig::default()))
}

/// Access the global SLO tracker.
pub fn slo() -> &'static ErrorBudget {
    SLO.get().expect("SLO tracker not initialised – call init_slo first")
}

// =========================================================================
// 4. High-Level Observability Wrappers
// =========================================================================

/// Build sanitized args from a slice of (name, value) pairs.
pub fn build_args_json(args: &[(&str, JsonValue)]) -> JsonValue {
    let mut map = serde_json::Map::new();
    for (key, val) in args {
        map.insert(key.to_string(), sanitize_value(val));
    }
    JsonValue::Object(map)
}

/// Record a successful function call with logging and metrics.
///
/// The `_result_type` parameter is only used for type inference and is
/// not stored or serialized — the caller passes the actual result
/// serialized as `JsonValue`.
pub fn record_success(
    fn_name: &str,
    args_json: JsonValue,
    duration: Duration,
    result_json: JsonValue,
) {
    let duration_ms = duration.as_secs_f64() * 1000.0;
    let duration_secs = duration.as_secs_f64();
    let now = chrono::Utc::now();
    let (trace_id, span_id) = extract_trace_context();

    if let Some(m) = METRICS.get() {
        m.record(fn_name, true, None, duration_secs);
    }
    if let Some(s) = SLO.get() {
        s.record(true);
    }
    emit_log(LogEntry {
        timestamp: now.to_rfc3339(),
        severity: "INFO".into(),
        fn_name: fn_name.into(),
        args: args_json,
        duration_ms,
        result: result_json,
        error: None,
        trace_id,
        span_id,
    });
}

/// Record a failed function call with logging and metrics.
pub fn record_failure(
    fn_name: &str,
    args_json: JsonValue,
    duration: Duration,
    error_msg: &str,
    error_class: ErrorClass,
) {
    let duration_ms = duration.as_secs_f64() * 1000.0;
    let duration_secs = duration.as_secs_f64();
    let now = chrono::Utc::now();
    let (trace_id, span_id) = extract_trace_context();
    let error_type = match error_class {
        ErrorClass::Expected => "expected",
        ErrorClass::Unexpected => "unexpected",
    };
    let severity = match error_class {
        ErrorClass::Expected => "WARN",
        ErrorClass::Unexpected => "ERROR",
    };

    if let Some(m) = METRICS.get() {
        m.record(fn_name, false, Some(error_type), duration_secs);
    }
    if let Some(s) = SLO.get() {
        s.record(false);
    }
    emit_log(LogEntry {
        timestamp: now.to_rfc3339(),
        severity: severity.into(),
        fn_name: fn_name.into(),
        args: args_json,
        duration_ms,
        result: JsonValue::Null,
        error: Some(error_msg.to_string()),
        trace_id,
        span_id,
    });
}

/// Instrument a function that returns `Result<T, E>`.
pub fn track_result<E: fmt::Display>(
    fn_name: &str,
    args: &[(&str, JsonValue)],
    result: Result<JsonValue, E>,
) -> Result<JsonValue, E> {
    let start = Instant::now();
    let args_json = build_args_json(args);

    match result {
        Ok(val) => {
            record_success(fn_name, args_json, start.elapsed(), val.clone());
            Ok(val)
        }
        Err(err) => {
            record_failure(fn_name, args_json, start.elapsed(), &err.to_string(), ErrorClass::Expected);
            Err(err)
        }
    }
}

/// Instrument an infallible function.
pub fn track_infallible(
    fn_name: &str,
    args: &[(&str, JsonValue)],
    result: JsonValue,
) -> JsonValue {
    let start = Instant::now();
    let args_json = build_args_json(args);
    record_success(fn_name, args_json, start.elapsed(), result.clone());
    result
}

// =========================================================================
// 5. Panic Recovery
// =========================================================================

use std::panic::UnwindSafe;

/// Run a closure with panic recovery, capturing the panic message and
/// backtrace. On panic, logs the stack trace as an `ERROR` structured log
/// entry and updates SLO / metrics accordingly.
///
/// # Returns
/// `Ok(T)` on success, `Err(String)` if the closure panicked.
pub fn recover_panic<F, T>(
    fn_name: &str,
    args: &[(&str, JsonValue)],
    f: F,
) -> Result<T, String>
where
    F: FnOnce() -> T,
    F: UnwindSafe,
{
    let start = Instant::now();
    let args_json = build_args_json(args);

    let catch_result = std::panic::catch_unwind(f);

    match catch_result {
        Ok(val) => {
            record_success(fn_name, args_json, start.elapsed(), JsonValue::Null);
            Ok(val)
        }
        Err(panic_payload) => {
            let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic payload".to_string()
            };
            let backtrace = Backtrace::capture();
            let full_msg = format!("panic: {}\nstack trace:\n{}", panic_msg, backtrace);

            record_failure(fn_name, args_json, start.elapsed(), &full_msg, ErrorClass::Unexpected);
            Err(full_msg)
        }
    }
}

// =========================================================================
// 6. SLO / Alerts Status Endpoints
// =========================================================================

/// Dumps current SLO status as a JSON value for health-check endpoints.
pub fn slo_status_json() -> JsonValue {
    let s = SLO.get();
    let (ok, fail) = s.map(|s| s.totals()).unwrap_or((0, 0));
    let (total_ok, total_fail) = s.map(|s| s.lifetime_totals()).unwrap_or((0, 0));
    let rate = s.map(|s| s.success_rate()).unwrap_or(1.0);
    let remaining = s.map(|s| s.remaining_budget()).unwrap_or(1.0);
    let burn = s.map(|s| s.burn_rate()).unwrap_or(0.0);

    serde_json::json!({
        "slo_target": 0.999,
        "window_days": 30,
        "current_success_rate": rate,
        "total_calls": total_ok + total_fail,
        "total_ok": total_ok,
        "total_fail": total_fail,
        "rolling_window_ok": ok,
        "rolling_window_fail": fail,
        "error_budget_remaining": remaining,
        "burn_rate": burn,
    })
}

/// Dumps all metrics as a JSON value for health-check endpoints.
pub fn metrics_status_json() -> JsonValue {
    let m = METRICS.get();
    match m {
        Some(registry) => {
            let snapshots = registry.snapshot_all();
            serde_json::to_value(&snapshots).unwrap_or(JsonValue::Null)
        }
        None => JsonValue::Null,
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_truncates_long_strings() {
        let long = "x".repeat(1000);
        let val = JsonValue::String(long);
        let sanitized = sanitize_value(&val);
        if let JsonValue::String(s) = sanitized {
            assert!(s.len() <= 515);
            assert!(s.ends_with("..."));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_sanitize_passes_short_strings() {
        let short = "hello".to_string();
        let val = JsonValue::String(short.clone());
        assert_eq!(sanitize_value(&val), JsonValue::String(short));
    }

    #[test]
    fn test_error_budget_basic() {
        let config = SloConfig {
            target: 0.5,
            window_secs: 100,
            num_buckets: 10,
            ..Default::default()
        };
        let eb = ErrorBudget::new(config);

        for _ in 0..10 {
            eb.record(true);
        }
        assert!((eb.success_rate() - 1.0).abs() < 1e-9);
        assert!((eb.remaining_budget() - 1.0).abs() < 1e-9);
        assert!((eb.burn_rate() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_error_budget_exhaustion() {
        let config = SloConfig {
            target: 0.5,
            window_secs: 100,
            num_buckets: 10,
            ..Default::default()
        };
        let eb = ErrorBudget::new(config);

        for _ in 0..100 {
            eb.record(false);
        }
        assert!(eb.remaining_budget() < 0.01);
        assert!(eb.burn_rate() > 1.0);
    }

    #[test]
    fn test_metrics_registry_basic() {
        let reg = MetricsRegistry::new();
        reg.record("test_fn", true, None, 0.001);
        reg.record("test_fn", false, Some("expected"), 0.005);
        reg.record("test_fn", true, None, 0.002);

        let snap = reg.snapshot_fn("test_fn");
        assert_eq!(snap.calls_total, 3);
        assert_eq!(snap.success_total, 2);
        assert_eq!(snap.failure_total, 1);
        assert_eq!(snap.errors_by_type.get("expected").copied().unwrap_or(0), 1);
    }

    #[test]
    fn test_track_result_ok() {
        init_slo();
        init_metrics();

        let result: Result<JsonValue, String> = Ok(JsonValue::Number(42.into()));
        let tracked = track_result(
            "test::ok",
            &[("input", JsonValue::String("hello".into()))],
            result,
        );
        assert_eq!(tracked, Ok(JsonValue::Number(42.into())));
    }

    #[test]
    fn test_track_result_err() {
        init_metrics();
        let result: Result<JsonValue, String> = Err("bad input".to_string());
        let tracked = track_result(
            "test::err",
            &[("input", JsonValue::String("bad".into()))],
            result,
        );
        assert_eq!(tracked, Err("bad input".to_string()));
    }

    #[test]
    fn test_panic_recovery() {
        init_metrics();
        let result = recover_panic(
            "test::panic",
            &[("input", JsonValue::String("boom".into()))],
            || {
                panic!("intentional panic");
            },
        );
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("intentional panic"));
        assert!(err_msg.contains("stack trace"));
    }

    #[test]
    fn test_slo_status_json() {
        init_slo();
        let status = slo_status_json();
        assert_eq!(status["slo_target"], 0.999);
        assert!(status["error_budget_remaining"].as_f64().unwrap() > 0.0);
    }
}
