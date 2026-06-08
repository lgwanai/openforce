use openforce_proto::swarmos::v1::session_store_client::SessionStoreClient;
use tonic::transport::Channel;
#[allow(dead_code)]
pub struct GrpcClients {
    pub session_store: SessionStoreClient<Channel>,
}
impl GrpcClients {
    #[allow(dead_code)]
    pub async fn connect(addr: &str) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            session_store: SessionStoreClient::connect(format!("http://{addr}")).await?,
        })
    }
}
