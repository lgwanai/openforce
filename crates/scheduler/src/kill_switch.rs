pub struct SchedulerKillSwitch;
impl SchedulerKillSwitch {
    pub fn new() -> Self { Self }
    pub fn can_schedule(&self, _tid: &str) -> bool { true }
}
