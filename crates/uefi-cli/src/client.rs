use http::Uri;
use hyper_util::rt::TokioIo;
use tonic::Request;
use tonic::transport::{Channel, Endpoint};
use uefi_common::error::{AppError, ErrKind};
use uefi_common::state::{State, resolve_sock};
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::*;

pub struct Client {
    inner: EngineServiceClient<Channel>,
    pub state: State,
}

fn auth_req<T>(state: &State, body: T) -> Request<T> {
    let mut req = Request::new(body);
    if let (Some(sid), Some(tok)) = (&state.session_id, &state.token) {
        req.metadata_mut()
            .insert("authorization", format!("Bearer {tok}").parse().unwrap());
        req.metadata_mut()
            .insert("x-session-id", sid.parse().unwrap());
    }
    req
}

impl Client {
    pub async fn connect(cli_sock: Option<&str>, state: State) -> Result<Self, AppError> {
        let sock = resolve_sock(cli_sock, &state);
        let sock_str = sock.display().to_string();
        let channel = Endpoint::try_from("http://localhost")
            .map_err(|e| AppError::new(ErrKind::IoError, e.to_string()))?
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                let s = sock_str.clone();
                async move {
                    Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
                }
            }))
            .await
            .map_err(|e| AppError::new(ErrKind::RpcInternal, e.to_string()))?;
        Ok(Self {
            inner: EngineServiceClient::new(channel),
            state,
        })
    }

    pub fn active_image(&self) -> Result<String, AppError> {
        self.state.active_image_id.clone().ok_or_else(|| {
            AppError::new(
                ErrKind::NoActiveImage,
                "no active image; run `uefi-cli image open`",
            )
        })
    }

    pub async fn create_session(&mut self, name: &str) -> Result<(String, String), AppError> {
        let r = self
            .inner
            .create_session(CreateSessionRequest { name: name.into() })
            .await?
            .into_inner();
        Ok((r.session_id, r.token))
    }

    pub async fn destroy_session(&mut self, id: &str) -> Result<(), AppError> {
        self.inner
            .destroy_session(DestroySessionRequest {
                session_id: id.into(),
            })
            .await?;
        Ok(())
    }

    pub async fn list_sessions(&mut self) -> Result<Vec<SessionInfo>, AppError> {
        Ok(self
            .inner
            .list_sessions(ListSessionsRequest {})
            .await?
            .into_inner()
            .sessions)
    }

    pub async fn open_image(
        &mut self,
        path: &str,
        mode: i32,
    ) -> Result<(String, String), AppError> {
        let sid = self
            .state
            .session_id
            .clone()
            .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session"))?;
        let req = OpenImageRequest {
            session_id: sid,
            image_path: path.into(),
            mode,
        };
        let r = self
            .inner
            .open_image(auth_req(&self.state, req))
            .await?
            .into_inner();
        Ok((r.image_id, r.root_guid))
    }

    pub async fn list_items(
        &mut self,
        image_id: &str,
        filter: Option<&str>,
    ) -> Result<Vec<Item>, AppError> {
        let req = ListItemsRequest {
            image_id: image_id.into(),
            filter: filter.unwrap_or("").into(),
        };
        Ok(self
            .inner
            .list_items(auth_req(&self.state, req))
            .await?
            .into_inner()
            .items)
    }

    pub async fn search_items(
        &mut self,
        image_id: &str,
        query: &str,
        modes: &[i32],
        limit: u32,
    ) -> Result<Vec<Item>, AppError> {
        let req = SearchItemsRequest {
            image_id: image_id.into(),
            query: query.into(),
            modes: modes.to_vec(),
            limit,
        };
        Ok(self
            .inner
            .search_items(auth_req(&self.state, req))
            .await?
            .into_inner()
            .items)
    }

    pub async fn find_item(&mut self, image_id: &str, target: &str) -> Result<String, AppError> {
        let req = FindItemRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        Ok(self
            .inner
            .find_item(auth_req(&self.state, req))
            .await?
            .into_inner()
            .item_id)
    }

    pub async fn insert(
        &mut self,
        image_id: &str,
        target: &str,
        ffs_path: &str,
        artifact_id: &str,
        mode: i32,
    ) -> Result<String, AppError> {
        let req = InsertRequest {
            image_id: image_id.into(),
            target: target.into(),
            ffs_path: ffs_path.into(),
            artifact_id: artifact_id.into(),
            mode,
        };
        Ok(self
            .inner
            .insert(auth_req(&self.state, req))
            .await?
            .into_inner()
            .item_id)
    }

    pub async fn remove(&mut self, image_id: &str, target: &str) -> Result<(), AppError> {
        let req = RemoveRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        self.inner.remove(auth_req(&self.state, req)).await?;
        Ok(())
    }

    pub async fn replace(
        &mut self,
        image_id: &str,
        target: &str,
        data_path: &str,
        artifact_id: &str,
        body_only: bool,
    ) -> Result<String, AppError> {
        let req = ReplaceRequest {
            image_id: image_id.into(),
            target: target.into(),
            ffs_path: data_path.into(),
            artifact_id: artifact_id.into(),
            body_only,
        };
        Ok(self
            .inner
            .replace(auth_req(&self.state, req))
            .await?
            .into_inner()
            .item_id)
    }

    pub async fn rebuild(&mut self, image_id: &str, target: &str) -> Result<(), AppError> {
        let req = RebuildRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        self.inner.rebuild(auth_req(&self.state, req)).await?;
        Ok(())
    }

    pub async fn set_setup_visibility(
        &mut self,
        image_id: &str,
        item_id: &str,
        visible: bool,
    ) -> Result<(), AppError> {
        let req = SetSetupItemVisibilityRequest {
            image_id: image_id.into(),
            item_id: item_id.into(),
            visible,
        };
        self.inner
            .set_setup_item_visibility(auth_req(&self.state, req))
            .await?;
        Ok(())
    }

    pub async fn save_image(&mut self, image_id: &str, output_path: &str) -> Result<(), AppError> {
        let req = SaveImageRequest {
            image_id: image_id.into(),
            output_path: output_path.into(),
        };
        self.inner.save_image(auth_req(&self.state, req)).await?;
        Ok(())
    }

    pub async fn extract_artifact(
        &mut self,
        image_id: &str,
        target: &str,
        body_only: bool,
    ) -> Result<String, AppError> {
        let req = ExtractArtifactRequest {
            image_id: image_id.into(),
            target: target.into(),
            body_only,
        };
        Ok(self
            .inner
            .extract_artifact(auth_req(&self.state, req))
            .await?
            .into_inner()
            .artifact_id)
    }

    pub async fn export_artifact(
        &mut self,
        artifact_id: &str,
        output_path: &str,
    ) -> Result<(), AppError> {
        let req = ExportArtifactRequest {
            artifact_id: artifact_id.into(),
            output_path: output_path.into(),
        };
        self.inner
            .export_artifact(auth_req(&self.state, req))
            .await?;
        Ok(())
    }

    pub async fn import_artifact(&mut self, file_path: &str) -> Result<String, AppError> {
        let sid = self
            .state
            .session_id
            .clone()
            .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session"))?;
        let req = ImportArtifactRequest {
            session_id: sid,
            file_path: file_path.into(),
        };
        Ok(self
            .inner
            .import_artifact(auth_req(&self.state, req))
            .await?
            .into_inner()
            .artifact_id)
    }

    pub async fn list_artifacts(&mut self) -> Result<Vec<ArtifactInfo>, AppError> {
        let sid = self
            .state
            .session_id
            .clone()
            .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session"))?;
        let req = ListArtifactsRequest { session_id: sid };
        Ok(self
            .inner
            .list_artifacts(auth_req(&self.state, req))
            .await?
            .into_inner()
            .artifacts)
    }
}
