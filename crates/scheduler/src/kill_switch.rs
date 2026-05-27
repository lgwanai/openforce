#[allow(dead_code)]
pub struct SchedulerKillSwitch;
#[allow(dead_code)]
impl SchedulerKillSwitch {
    pub fn new() -> Self { Self }
    pub fn can_schedule(&self, _tid: &str) -> bool { true }
}
