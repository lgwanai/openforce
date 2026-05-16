use tonic::transport::Channel;
use uuid::Uuid;
use openforce_proto::swarmos::v1::{
    session_store_client::SessionStoreClient,
    scheduler_client::SchedulerClient,
};

pub struct GrpcClients {
    pub session_store: SessionStoreClient<Channel>,
}

impl GrpcClients {
    pub async fn connect(session_store_addr: &str) -> Result<Self, tonic::transport::Error> {
        let ss = SessionStoreClient::connect(format!("http://{session_store_addr}")).await?;
        Ok(Self { session_store: ss })
    }
}
