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

    pub async fn create_session(&mut self, name: &str) -> anyhow::Result<(String, String)> {
        let r = self
            .inner
            .create_session(CreateSessionRequest { name: name.into() })
            .await?
            .into_inner();
        Ok((r.session_id, r.token))
    }
    pub async fn destroy_session(&mut self, id: &str) -> anyhow::Result<()> {
        self.inner
            .destroy_session(DestroySessionRequest {
                session_id: id.into(),
            })
            .await?;
        Ok(())
    }
    pub async fn list_sessions(&mut self) -> anyhow::Result<Vec<SessionInfo>> {
        Ok(self
            .inner
            .list_sessions(ListSessionsRequest {})
            .await?
            .into_inner()
            .sessions)
    }
    pub async fn open_image(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        path: &str,
        mode: i32,
    ) -> anyhow::Result<OpenImageResponse> {
        let req = OpenImageRequest {
            session_id: session_id.into(),
            image_path: path.into(),
            mode,
        };
        Ok(self
            .inner
            .open_image(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner())
    }
    pub async fn dump_tree(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        format: i32,
    ) -> anyhow::Result<String> {
        let req = DumpTreeRequest {
            image_id: image_id.into(),
            format,
        };
        Ok(self
            .inner
            .dump_tree(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .text)
    }
    pub async fn list_items(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        filter: &str,
    ) -> anyhow::Result<Vec<Item>> {
        let req = ListItemsRequest {
            image_id: image_id.into(),
            filter: filter.into(),
        };
        Ok(self
            .inner
            .list_items(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .items)
    }
    pub async fn find_item(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
    ) -> anyhow::Result<String> {
        let req = FindItemRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        Ok(self
            .inner
            .find_item(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .item_id)
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn insert(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
        ffs_path: &str,
        artifact_id: &str,
        mode: i32,
    ) -> anyhow::Result<String> {
        let req = InsertRequest {
            image_id: image_id.into(),
            target: target.into(),
            ffs_path: ffs_path.into(),
            artifact_id: artifact_id.into(),
            mode,
        };
        Ok(self
            .inner
            .insert(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .item_id)
    }
    pub async fn remove(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        let req = RemoveRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        self.inner
            .remove(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn replace(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
        data_path: &str,
        artifact_id: &str,
        body_only: bool,
    ) -> anyhow::Result<String> {
        let req = ReplaceRequest {
            image_id: image_id.into(),
            target: target.into(),
            ffs_path: data_path.into(),
            artifact_id: artifact_id.into(),
            body_only,
        };
        Ok(self
            .inner
            .replace(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .item_id)
    }
    pub async fn rebuild(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        let req = RebuildRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        self.inner
            .rebuild(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    pub async fn set_setup_visibility(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        item_id: &str,
        visible: bool,
    ) -> anyhow::Result<()> {
        let req = SetSetupItemVisibilityRequest {
            image_id: image_id.into(),
            item_id: item_id.into(),
            visible,
        };
        self.inner
            .set_setup_item_visibility(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    pub async fn save_image(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        output_path: &str,
    ) -> anyhow::Result<()> {
        let req = SaveImageRequest {
            image_id: image_id.into(),
            output_path: output_path.into(),
        };
        self.inner
            .save_image(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    pub async fn extract_artifact(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        image_id: &str,
        target: &str,
        body_only: bool,
    ) -> anyhow::Result<String> {
        let req = ExtractArtifactRequest {
            image_id: image_id.into(),
            target: target.into(),
            body_only,
        };
        Ok(self
            .inner
            .extract_artifact(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .artifact_id)
    }
    pub async fn export_artifact(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        artifact_id: &str,
        output_path: &str,
    ) -> anyhow::Result<()> {
        let req = ExportArtifactRequest {
            artifact_id: artifact_id.into(),
            output_path: output_path.into(),
        };
        self.inner
            .export_artifact(Self::auth_req(sessions, session_id, req).await?)
            .await?;
        Ok(())
    }
    pub async fn import_artifact(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        file_path: &str,
    ) -> anyhow::Result<String> {
        let req = ImportArtifactRequest {
            session_id: session_id.into(),
            file_path: file_path.into(),
        };
        Ok(self
            .inner
            .import_artifact(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .artifact_id)
    }
    pub async fn list_artifacts(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
    ) -> anyhow::Result<Vec<ArtifactInfo>> {
        let req = ListArtifactsRequest {
            session_id: session_id.into(),
        };
        Ok(self
            .inner
            .list_artifacts(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .artifacts)
    }
}
