use std::collections::HashMap;
use tokio::time::{interval, Duration};
use uuid::Uuid;
use tracing::{info, warn};
use openforce_proto::swarmos::v1::{session_store_client::SessionStoreClient, ExecuteCommandRequest, ListTasksRequest, Command as ProtoCommand};
pub struct SchedulerRuntime { client: SessionStoreClient<tonic::transport::Channel>, instance_id: String }
impl SchedulerRuntime {
    pub fn new(client: SessionStoreClient<tonic::transport::Channel>) -> Self { Self { client, instance_id: Uuid::now_v7().to_string() } }
    pub async fn run(&mut self) {
        let mut tick = interval(Duration::from_millis(500));
        info!("Scheduler loop started");
        loop { tick.tick().await; }
    }
}
