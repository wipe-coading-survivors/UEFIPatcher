use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uefi_proto::engine_service_server::{EngineService, EngineServiceServer};
use uefi_proto::*;

#[derive(Default)]
pub struct MockEngine {
    pub sessions: Arc<Mutex<HashMap<String, String>>>,
}

#[tonic::async_trait]
impl EngineService for MockEngine {
    async fn create_session(
        &self,
        _req: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let id = uuid::Uuid::new_v4().to_string();
        let tok = uuid::Uuid::new_v4().to_string();
        self.sessions.lock().await.insert(id.clone(), tok.clone());
        Ok(Response::new(CreateSessionResponse {
            session_id: id,
            token: tok,
        }))
    }
    async fn destroy_session(
        &self,
        req: Request<DestroySessionRequest>,
    ) -> Result<Response<Empty>, Status> {
        let r = req.into_inner();
        self.sessions.lock().await.remove(&r.session_id);
        Ok(Response::new(Empty {}))
    }
    async fn list_sessions(
        &self,
        _req: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let s = self.sessions.lock().await;
        let sessions = s
            .keys()
            .map(|k| SessionInfo {
                session_id: k.clone(),
                name: String::new(),
                created_at: 0,
                last_activity: 0,
            })
            .collect();
        Ok(Response::new(ListSessionsResponse { sessions }))
    }
    async fn open_image(
        &self,
        _req: Request<OpenImageRequest>,
    ) -> Result<Response<OpenImageResponse>, Status> {
        Ok(Response::new(OpenImageResponse {
            image_id: uuid::Uuid::new_v4().to_string(),
            root_guid: String::new(),
        }))
    }
    async fn dump_tree(
        &self,
        _req: Request<DumpTreeRequest>,
    ) -> Result<Response<DumpTreeResponse>, Status> {
        Ok(Response::new(DumpTreeResponse {
            text: "0 62 Image\n0/0 65 Volume\n".into(),
        }))
    }
    async fn list_items(
        &self,
        _req: Request<ListItemsRequest>,
    ) -> Result<Response<ListItemsResponse>, Status> {
        Ok(Response::new(ListItemsResponse {
            items: vec![Item {
                path: "0".into(),
                r#type: 65,
                subtype: 0,
                guid: String::new(),
                offset: 0,
                size: 256,
                name: String::new(),
            }],
        }))
    }
    async fn find_item(
        &self,
        req: Request<FindItemRequest>,
    ) -> Result<Response<FindItemResponse>, Status> {
        Ok(Response::new(FindItemResponse {
            item_id: req.into_inner().target,
        }))
    }
    async fn insert(
        &self,
        _req: Request<InsertRequest>,
    ) -> Result<Response<InsertResponse>, Status> {
        Ok(Response::new(InsertResponse {
            item_id: "mock".into(),
        }))
    }
    async fn remove(&self, _req: Request<RemoveRequest>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn replace(
        &self,
        _req: Request<ReplaceRequest>,
    ) -> Result<Response<ReplaceResponse>, Status> {
        Ok(Response::new(ReplaceResponse {
            item_id: "mock".into(),
        }))
    }
    async fn rebuild(&self, _req: Request<RebuildRequest>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn set_setup_item_visibility(
        &self,
        _req: Request<SetSetupItemVisibilityRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn save_image(&self, _req: Request<SaveImageRequest>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn extract_artifact(
        &self,
        _req: Request<ExtractArtifactRequest>,
    ) -> Result<Response<ExtractArtifactResponse>, Status> {
        Ok(Response::new(ExtractArtifactResponse {
            artifact_id: uuid::Uuid::new_v4().to_string(),
        }))
    }
    async fn export_artifact(
        &self,
        _req: Request<ExportArtifactRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn import_artifact(
        &self,
        _req: Request<ImportArtifactRequest>,
    ) -> Result<Response<ImportArtifactResponse>, Status> {
        Ok(Response::new(ImportArtifactResponse {
            artifact_id: uuid::Uuid::new_v4().to_string(),
        }))
    }
    async fn list_artifacts(
        &self,
        _req: Request<ListArtifactsRequest>,
    ) -> Result<Response<ListArtifactsResponse>, Status> {
        Ok(Response::new(ListArtifactsResponse { artifacts: vec![] }))
    }
}

pub async fn start_mock(sock: &Path) -> JoinHandle<()> {
    let _ = std::fs::remove_file(sock);
    let listener = tokio::net::UnixListener::bind(sock).unwrap();
    let incoming = UnixListenerStream::new(listener);
    let mock = MockEngine::default();
    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(EngineServiceServer::new(mock))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    handle
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper_util::rt::TokioIo;
    use tempfile::TempDir;
    use tonic::transport::Endpoint;
    use tower::service_fn;
    use uefi_proto::engine_service_client::EngineServiceClient;

    #[tokio::test]
    async fn mock_roundtrip() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let sock_str = sock.to_string_lossy().to_string();
        let channel = Endpoint::try_from("http://localhost")
            .unwrap()
            .connect_with_connector(service_fn(move |_: http::Uri| {
                let s = sock_str.clone();
                async move {
                    Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
                }
            }))
            .await
            .unwrap();
        let mut client = EngineServiceClient::new(channel);
        let resp = client
            .create_session(CreateSessionRequest {
                name: "test".into(),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(!resp.session_id.is_empty());
        assert!(!resp.token.is_empty());
    }
}
