use crate::observer::Observation;

/// Evaluator scores execution quality from observations.
/// Architecture doc section 11.1: offline evaluation before promotion.
pub struct Evaluator {
    pub latency_p50_threshold_ms: f64,
    pub latency_p95_threshold_ms: f64,
    pub success_rate_threshold: f64,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self { latency_p50_threshold_ms: 5000.0, latency_p95_threshold_ms: 30000.0, success_rate_threshold: 0.95 }
    }
}

#[derive(Debug, Clone)]
pub struct EvaluationResult {
    pub passed: bool,
    pub score: f64,
    pub issues: Vec<String>,
}

impl Evaluator {
    pub fn evaluate(&self, observations: &[Observation]) -> EvaluationResult {
        let mut issues = vec![];
        let total = observations.len() as f64;
        if total == 0.0 {
            return EvaluationResult { passed: true, score: 1.0, issues };
        }

        let successes: f64 = observations.iter().filter(|o| o.value > 0.5 && o.metric_type == "success").count() as f64;
        let success_rate = if total > 0.0 { successes / total } else { 0.0 };
        if success_rate < self.success_rate_threshold {
            issues.push(format!("success rate {:.2} below threshold {:.2}", success_rate, self.success_rate_threshold));
        }

        let passed = issues.is_empty();
        let score = if total > 0.0 { successes / total } else { 1.0 };
        EvaluationResult { passed, score, issues }
    }
}
