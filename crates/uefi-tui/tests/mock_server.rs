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
    async fn session_create(
        &self,
        _req: Request<SessionCreateRequest>,
    ) -> Result<Response<SessionCreateResponse>, Status> {
        let id = uuid::Uuid::new_v4().to_string();
        let tok = uuid::Uuid::new_v4().to_string();
        self.sessions.lock().await.insert(id.clone(), tok.clone());
        Ok(Response::new(SessionCreateResponse {
            session_id: id,
            token: tok,
        }))
    }
    async fn session_destroy(
        &self,
        req: Request<SessionDestroyRequest>,
    ) -> Result<Response<Empty>, Status> {
        let r = req.into_inner();
        self.sessions.lock().await.remove(&r.session_id);
        Ok(Response::new(Empty {}))
    }
    async fn sessions_list(
        &self,
        _req: Request<SessionsListRequest>,
    ) -> Result<Response<SessionsListResponse>, Status> {
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
        Ok(Response::new(SessionsListResponse { sessions }))
    }
    async fn image_open(
        &self,
        _req: Request<ImageOpenRequest>,
    ) -> Result<Response<ImageOpenResponse>, Status> {
        Ok(Response::new(ImageOpenResponse {
            image_id: uuid::Uuid::new_v4().to_string(),
            root_guid: String::new(),
            name: "mock.bin".into(),
        }))
    }
    async fn image_close(
        &self,
        _req: Request<ImageCloseRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn images_list(
        &self,
        _req: Request<ImagesListRequest>,
    ) -> Result<Response<ImagesListResponse>, Status> {
        Ok(Response::new(ImagesListResponse {
            images: vec![ImageInfo {
                image_id: "mock-img-1".into(),
                name: "mock.bin".into(),
                path: "/tmp/mock.bin".into(),
                mode: 0,
                size: 16777216,
                created_at: 0,
                last_activity: 0,
            }],
        }))
    }
    async fn image_save(&self, _req: Request<ImageSaveRequest>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn image_status(
        &self,
        req: Request<ImageStatusRequest>,
    ) -> Result<Response<ImageStatusResponse>, Status> {
        let r = req.into_inner();
        Ok(Response::new(ImageStatusResponse {
            info: Some(ImageInfo {
                image_id: r.image_id,
                ..Default::default()
            }),
        }))
    }
    async fn image_nodes_list(
        &self,
        _req: Request<ImageNodesListRequest>,
    ) -> Result<Response<ImageNodesResponse>, Status> {
        Ok(Response::new(ImageNodesResponse {
            nodes: vec![
                Node { path: "".into(),       r#type: 62, subtype: 0,    guid: String::new(), offset: 0,    size: 16777216, name: "Image".into() },
                Node { path: "0".into(),      r#type: 65, subtype: 0,    guid: String::new(), offset: 0,    size: 8388608,  name: "ME".into() },
                Node { path: "1".into(),      r#type: 65, subtype: 0,    guid: String::new(), offset: 8388608, size: 4194304, name: "DXE".into() },
                Node { path: "1/0".into(),    r#type: 66, subtype: 0x07, guid: "ABC".into(),   offset: 8388608, size: 4096,   name: "Setup".into() },
                Node { path: "1/0/0".into(),  r#type: 67, subtype: 0x15, guid: String::new(), offset: 8388608, size: 24,     name: String::new() },
            ],
        }))
    }
    async fn image_nodes_search(
        &self,
        _req: Request<ImageNodesSearchRequest>,
    ) -> Result<Response<ImageNodesResponse>, Status> {
        Ok(Response::new(ImageNodesResponse { nodes: vec![] }))
    }
    async fn image_node_insert(
        &self,
        _req: Request<ImageNodeInsertRequest>,
    ) -> Result<Response<ImageNodeResponse>, Status> {
        Ok(Response::new(ImageNodeResponse {
            item_id: "mock".into(),
        }))
    }
    async fn image_node_remove(
        &self,
        _req: Request<ImageNodeRemoveRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn image_node_replace(
        &self,
        _req: Request<ImageNodeReplaceRequest>,
    ) -> Result<Response<ImageNodeResponse>, Status> {
        Ok(Response::new(ImageNodeResponse {
            item_id: "mock".into(),
        }))
    }
    async fn image_node_rebuild(
        &self,
        _req: Request<ImageNodeRebuildRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn image_node_extract(
        &self,
        _req: Request<ImageNodeExtractRequest>,
    ) -> Result<Response<ImageNodeExtractResponse>, Status> {
        Ok(Response::new(ImageNodeExtractResponse {
            artifact_id: uuid::Uuid::new_v4().to_string(),
        }))
    }
    async fn artifacts_list(
        &self,
        _req: Request<ArtifactsListRequest>,
    ) -> Result<Response<ArtifactsListResponse>, Status> {
        Ok(Response::new(ArtifactsListResponse {
            artifacts: vec![ArtifactInfo {
                artifact_id: "mock-art-1".into(),
                kind: "section".into(),
                size: 4096,
                created_at: 0,
                source: "extracted".into(),
            }],
        }))
    }
    async fn artifact_import(
        &self,
        _req: Request<ArtifactImportRequest>,
    ) -> Result<Response<ArtifactImportResponse>, Status> {
        Ok(Response::new(ArtifactImportResponse {
            artifact_id: uuid::Uuid::new_v4().to_string(),
        }))
    }
    async fn artifact_export(
        &self,
        _req: Request<ArtifactExportRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn setup_list_forms(
        &self,
        _req: Request<SetupListFormsRequest>,
    ) -> Result<Response<SetupListFormsResponse>, Status> {
        Ok(Response::new(SetupListFormsResponse { forms: vec![] }))
    }
    async fn setup_set_form_visibility(
        &self,
        _req: Request<SetupSetFormVisibilityRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn setup_list_strings(
        &self,
        _req: Request<SetupListStringsRequest>,
    ) -> Result<Response<SetupListStringsResponse>, Status> {
        Ok(Response::new(SetupListStringsResponse { strings: vec![] }))
    }
    async fn setup_form_set_add(
        &self,
        _req: Request<SetupFormSetAddRequest>,
    ) -> Result<Response<SetupFormSetAddResponse>, Status> {
        Ok(Response::new(SetupFormSetAddResponse {
            new_ffs_id: "mock".into(),
            inserted_form_ids: vec![],
            string_ids: std::collections::HashMap::new(),
        }))
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
    use uefi_tui::app::App;
    use uefi_tui::commands;

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
            .session_create(SessionCreateRequest {
                name: "test".into(),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(!resp.session_id.is_empty());
        assert!(!resp.token.is_empty());
    }

    #[tokio::test]
    async fn open_sets_active_image_and_builds_tree() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state).await.unwrap();
        let mut app = App::new();
        let res = commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client).await;
        assert!(res.is_ok(), "open failed: {:?}", res.err());
        assert_eq!(app.active_image_id, client.state.active_image_id);
        assert!(app.active_image_id.is_some());
        assert!(app.tree.len() >= 5);
        assert!(app.tree[0].has_children);
        assert!(app.registry.images.len() == 1);
        assert!(app.registry.artifacts.len() == 1);
    }
}
