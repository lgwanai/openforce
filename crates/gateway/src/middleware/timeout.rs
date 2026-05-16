use std::time::Duration;
use tower::timeout::TimeoutLayer;

pub fn timeout_layer(secs: u64) -> TimeoutLayer {
    TimeoutLayer::new(Duration::from_secs(secs))
}
