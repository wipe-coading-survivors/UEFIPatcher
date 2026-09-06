use http::Uri;
use hyper_util::rt::TokioIo;
use std::path::Path;
use tonic::Request;
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::*;

use crate::session::SessionMap;

pub struct EngineClient {
    inner: EngineServiceClient<Channel>,
}

impl EngineClient {
    pub async fn connect(sock_path: &Path) -> anyhow::Result<Self> {
        let sock_str = sock_path.display().to_string();
        let channel = Endpoint::try_from("http://localhost")?
            .connect_with_connector(service_fn(move |_: Uri| {
                let s = sock_str.clone();
                async move {
                    Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
                }
            }))
            .await?;
        Ok(Self {
            inner: EngineServiceClient::new(channel),
        })
    }

    async fn auth_req<T>(
        sessions: &SessionMap,
        session_id: &str,
        body: T,
    ) -> Result<Request<T>, tonic::Status> {
        let token = sessions
            .get_token(session_id)
            .await
            .ok_or_else(|| tonic::Status::unauthenticated("no token for session"))?;
        let mut req = Request::new(body);
        req.metadata_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        req.metadata_mut()
            .insert("x-session-id", session_id.parse().unwrap());
        Ok(req)
    }

    pub async fn session_create(&mut self, name: &str) -> Result<(String, String), tonic::Status> {
        let r = self
            .inner
            .session_create(SessionCreateRequest { name: name.into() })
            .await?
            .into_inner();
        Ok((r.session_id, r.token))
    }
    pub async fn session_destroy(&mut self, id: &str) -> Result<(), tonic::Status> {
        self.inner
            .session_destroy(SessionDestroyRequest {
                session_id: id.into(),
            })
            .await?;
        Ok(())
    }
    pub async fn sessions_list(&mut self) -> Result<Vec<SessionInfo>, tonic::Status> {
        Ok(self
            .inner
            .sessions_list(SessionsListRequest {})
            .await?
            .into_inner()
            .sessions)
    }
    pub async fn image_open(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        path: &str,
        name: &str,
        mode: i32,
    ) -> Result<ImageOpenResponse, tonic::Status> {
        let req = ImageOpenRequest {
            session_id: session_id.into(),
            path: path.into(),
            name: name.into(),
            mode,
        };
        Ok(self
            .inner
            .image_open(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner())
    }
    pub async fn image_nodes_list(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        filter: &str,
    ) -> Result<Vec<Node>, tonic::Status> {
        let req = ImageNodesListRequest {
            image_id: image_id.into(),
            filter: filter.into(),
        };
        Ok(self
            .inner
            .image_nodes_list(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .nodes)
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn image_node_insert(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
        ffs_path: &str,
        artifact_id: &str,
        mode: i32,
    ) -> Result<String, tonic::Status> {
        let req = ImageNodeInsertRequest {
            image_id: image_id.into(),
            target: target.into(),
            ffs_path: ffs_path.into(),
            artifact_id: artifact_id.into(),
            mode,
        };
        Ok(self
            .inner
            .image_node_insert(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .item_id)
    }
    pub async fn image_node_remove(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
    ) -> Result<(), tonic::Status> {
        let req = ImageNodeRemoveRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        self.inner
            .image_node_remove(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn image_node_replace(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
        data_path: &str,
        artifact_id: &str,
        body_only: bool,
    ) -> Result<String, tonic::Status> {
        let req = ImageNodeReplaceRequest {
            image_id: image_id.into(),
            target: target.into(),
            ffs_path: data_path.into(),
            artifact_id: artifact_id.into(),
            body_only,
        };
        Ok(self
            .inner
            .image_node_replace(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .item_id)
    }
    pub async fn image_node_rebuild(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
    ) -> Result<(), tonic::Status> {
        let req = ImageNodeRebuildRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        self.inner
            .image_node_rebuild(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    pub async fn hii_set_form_visibility(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        item_id: &str,
        visible: bool,
    ) -> Result<(), tonic::Status> {
        let req = HiiSetFormVisibilityRequest {
            image_id: image_id.into(),
            item_id: item_id.into(),
            visible,
        };
        self.inner
            .hii_set_form_visibility(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    pub async fn image_save(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        output_path: &str,
    ) -> Result<(), tonic::Status> {
        let req = ImageSaveRequest {
            image_id: image_id.into(),
            output_path: output_path.into(),
        };
        self.inner
            .image_save(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    pub async fn image_node_extract(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
        body_only: bool,
    ) -> Result<String, tonic::Status> {
        let req = ImageNodeExtractRequest {
            image_id: image_id.into(),
            target: target.into(),
            body_only,
        };
        Ok(self
            .inner
            .image_node_extract(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .artifact_id)
    }
    pub async fn artifact_export(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        artifact_id: &str,
        output_path: &str,
    ) -> Result<(), tonic::Status> {
        let req = ArtifactExportRequest {
            artifact_id: artifact_id.into(),
            output_path: output_path.into(),
        };
        self.inner
            .artifact_export(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    pub async fn artifact_import(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        path: &str,
    ) -> Result<String, tonic::Status> {
        let req = ArtifactImportRequest {
            session_id: session_id.into(),
            path: path.into(),
        };
        Ok(self
            .inner
            .artifact_import(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .artifact_id)
    }
    pub async fn artifacts_list(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
    ) -> Result<Vec<ArtifactInfo>, tonic::Status> {
        let req = ArtifactsListRequest {
            session_id: session_id.into(),
        };
        Ok(self
            .inner
            .artifacts_list(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .artifacts)
    }
}
