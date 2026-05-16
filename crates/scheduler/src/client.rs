use tonic::transport::Channel;
use openforce_proto::swarmos::v1::{session_store_client::SessionStoreClient, scheduler_client::SchedulerClient};
pub struct GrpcClients { pub session_store: SessionStoreClient<Channel> }
impl GrpcClients {
    pub async fn connect(addr: &str) -> Result<Self, tonic::transport::Error> {
        Ok(Self { session_store: SessionStoreClient::connect(format!("http://{addr}")).await? })
    }
}
