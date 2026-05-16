pub struct BudgetPolicy;
#[derive(Debug, Clone)] pub struct BudgetLimits {
    pub max_wall_clock_sec: i32, pub max_tool_calls: i32,
    pub max_patch_attempts: i32, pub max_cost_usd: f64,
}
impl Default for BudgetLimits {
    fn default() -> Self { Self { max_wall_clock_sec: 900, max_tool_calls: 40, max_patch_attempts: 6, max_cost_usd: 1.8 } }
}
impl BudgetPolicy {
    pub fn within(limits: &BudgetLimits, elapsed: i32, calls: i32, cost: f64) -> bool {
        elapsed <= limits.max_wall_clock_sec && calls <= limits.max_tool_calls && cost <= limits.max_cost_usd
    }
}
