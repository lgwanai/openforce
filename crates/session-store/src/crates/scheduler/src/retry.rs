pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_backoff_secs: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self { Self { max_retries: 3, base_backoff_secs: 10 } }
}

impl RetryPolicy {
    pub fn should_retry(&self, attempt: u32) -> bool { attempt < self.max_retries }
    pub fn backoff_secs(&self, attempt: u32) -> u32 { self.base_backoff_secs * 2u32.pow(attempt) }
}
