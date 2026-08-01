use anyhow::Result;
use http::Uri;
use hyper_util::rt::TokioIo;
use tonic::transport::{Channel, Endpoint};
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::{CreateSessionRequest, DestroySessionRequest};

pub struct Client(EngineServiceClient<Channel>);

impl Client {
    pub async fn connect(sock: &str) -> Result<Self> {
        let sock_owned = sock.to_string();
        let channel = Endpoint::try_from("http://localhost")?
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                let s = sock_owned.clone();
                async move {
                    Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
                }
            }))
            .await?;
        Ok(Self(EngineServiceClient::new(channel)))
    }

    pub async fn create_session(&mut self, name: &str) -> Result<(String, String)> {
        let r = self
            .0
            .create_session(CreateSessionRequest {
                name: name.to_string(),
            })
            .await?
            .into_inner();
        Ok((r.session_id, r.token))
    }

    pub async fn destroy_session(&mut self, id: &str) -> Result<()> {
        self.0
            .destroy_session(DestroySessionRequest {
                session_id: id.into(),
            })
            .await?;
        Ok(())
    }
}
