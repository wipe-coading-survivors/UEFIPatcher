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

    pub async fn session_create(&mut self, name: &str) -> Result<(String, String), AppError> {
        let r = self
            .inner
            .session_create(SessionCreateRequest { name: name.into() })
            .await?
            .into_inner();
        Ok((r.session_id, r.token))
    }

    pub async fn session_destroy(&mut self, id: &str) -> Result<(), AppError> {
        self.inner
            .session_destroy(SessionDestroyRequest {
                session_id: id.into(),
            })
            .await?;
        Ok(())
    }

    pub async fn sessions_list(&mut self) -> Result<Vec<SessionInfo>, AppError> {
        Ok(self
            .inner
            .sessions_list(SessionsListRequest {})
            .await?
            .into_inner()
            .sessions)
    }

    pub async fn image_open(
        &mut self,
        path: &str,
        name: &str,
        mode: i32,
    ) -> Result<(String, String, String), AppError> {
        let sid = self
            .state
            .session_id
            .clone()
            .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session"))?;
        let req = ImageOpenRequest {
            session_id: sid,
            path: path.into(),
            mode,
            name: name.into(),
        };
        let r = self
            .inner
            .image_open(auth_req(&self.state, req))
            .await?
            .into_inner();
        Ok((r.image_id, r.root_guid, r.name))
    }

    pub async fn image_close(&mut self, image_id: &str) -> Result<(), AppError> {
        let req = ImageCloseRequest {
            image_id: image_id.into(),
        };
        self.inner.image_close(auth_req(&self.state, req)).await?;
        Ok(())
    }

    pub async fn images_list(&mut self) -> Result<Vec<ImageInfo>, AppError> {
        let sid = self
            .state
            .session_id
            .clone()
            .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session"))?;
        let req = ImagesListRequest { session_id: sid };
        Ok(self
            .inner
            .images_list(auth_req(&self.state, req))
            .await?
            .into_inner()
            .images)
    }

    pub async fn image_status(&mut self, image_id: &str) -> Result<ImageInfo, AppError> {
        let req = ImageStatusRequest {
            image_id: image_id.into(),
        };
        let resp = self
            .inner
            .image_status(auth_req(&self.state, req))
            .await?
            .into_inner();
        resp.info
            .ok_or_else(|| AppError::new(ErrKind::RpcNotFound, "image status returned no info"))
    }

    pub async fn image_nodes_list(
        &mut self,
        image_id: &str,
        filter: Option<&str>,
    ) -> Result<Vec<Node>, AppError> {
        let req = ImageNodesListRequest {
            image_id: image_id.into(),
            filter: filter.unwrap_or("").into(),
        };
        Ok(self
            .inner
            .image_nodes_list(auth_req(&self.state, req))
            .await?
            .into_inner()
            .nodes)
    }

    pub async fn image_nodes_search(
        &mut self,
        image_id: &str,
        query: &str,
        modes: &[i32],
        limit: u32,
    ) -> Result<Vec<Node>, AppError> {
        let req = ImageNodesSearchRequest {
            image_id: image_id.into(),
            query: query.into(),
            modes: modes.to_vec(),
            limit,
        };
        Ok(self
            .inner
            .image_nodes_search(auth_req(&self.state, req))
            .await?
            .into_inner()
            .nodes)
    }

    pub async fn image_node_insert(
        &mut self,
        image_id: &str,
        target: &str,
        ffs_path: &str,
        artifact_id: &str,
        mode: i32,
    ) -> Result<String, AppError> {
        let req = ImageNodeInsertRequest {
            image_id: image_id.into(),
            target: target.into(),
            ffs_path: ffs_path.into(),
            artifact_id: artifact_id.into(),
            mode,
        };
        Ok(self
            .inner
            .image_node_insert(auth_req(&self.state, req))
            .await?
            .into_inner()
            .item_id)
    }

    pub async fn image_node_remove(
        &mut self,
        image_id: &str,
        target: &str,
    ) -> Result<(), AppError> {
        let req = ImageNodeRemoveRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        self.inner
            .image_node_remove(auth_req(&self.state, req))
            .await?;
        Ok(())
    }

    pub async fn image_node_replace(
        &mut self,
        image_id: &str,
        target: &str,
        ffs_path: &str,
        artifact_id: &str,
        body_only: bool,
    ) -> Result<String, AppError> {
        let req = ImageNodeReplaceRequest {
            image_id: image_id.into(),
            target: target.into(),
            ffs_path: ffs_path.into(),
            artifact_id: artifact_id.into(),
            body_only,
        };
        Ok(self
            .inner
            .image_node_replace(auth_req(&self.state, req))
            .await?
            .into_inner()
            .item_id)
    }

    pub async fn image_node_rebuild(
        &mut self,
        image_id: &str,
        target: &str,
    ) -> Result<(), AppError> {
        let req = ImageNodeRebuildRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        self.inner
            .image_node_rebuild(auth_req(&self.state, req))
            .await?;
        Ok(())
    }

    pub async fn image_node_extract(
        &mut self,
        image_id: &str,
        target: &str,
        body_only: bool,
    ) -> Result<String, AppError> {
        let req = ImageNodeExtractRequest {
            image_id: image_id.into(),
            target: target.into(),
            body_only,
        };
        Ok(self
            .inner
            .image_node_extract(auth_req(&self.state, req))
            .await?
            .into_inner()
            .artifact_id)
    }

    pub async fn hii_list_forms(&mut self, image_id: &str) -> Result<Vec<FormInfo>, AppError> {
        let req = HiiListFormsRequest {
            image_id: image_id.into(),
        };
        Ok(self
            .inner
            .hii_list_forms(auth_req(&self.state, req))
            .await?
            .into_inner()
            .forms)
    }

    pub async fn hii_list_strings(&mut self, image_id: &str) -> Result<Vec<StringInfo>, AppError> {
        let req = HiiListStringsRequest {
            image_id: image_id.into(),
        };
        Ok(self
            .inner
            .hii_list_strings(auth_req(&self.state, req))
            .await?
            .into_inner()
            .strings)
    }

    pub async fn hii_set_form_visibility(
        &mut self,
        image_id: &str,
        item_id: &str,
        visible: bool,
    ) -> Result<(), AppError> {
        let req = HiiSetFormVisibilityRequest {
            image_id: image_id.into(),
            item_id: item_id.into(),
            visible,
        };
        self.inner
            .hii_set_form_visibility(auth_req(&self.state, req))
            .await?;
        Ok(())
    }

    pub async fn hii_form_set_add(
        &mut self,
        image_id: &str,
        schema_json: &str,
        target_ffs_guid: Option<&str>,
    ) -> Result<(String, Vec<u32>), AppError> {
        let req = HiiFormSetAddRequest {
            image_id: image_id.into(),
            schema_json: schema_json.into(),
            target_ffs_guid: target_ffs_guid.unwrap_or("").into(),
        };
        let resp = self
            .inner
            .hii_form_set_add(auth_req(&self.state, req))
            .await?
            .into_inner();
        Ok((resp.new_ffs_id, resp.inserted_form_ids))
    }

    pub async fn image_save(&mut self, image_id: &str, output_path: &str) -> Result<(), AppError> {
        let req = ImageSaveRequest {
            image_id: image_id.into(),
            output_path: output_path.into(),
        };
        self.inner.image_save(auth_req(&self.state, req)).await?;
        Ok(())
    }

    pub async fn artifacts_list(&mut self) -> Result<Vec<ArtifactInfo>, AppError> {
        let sid = self
            .state
            .session_id
            .clone()
            .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session"))?;
        let req = ArtifactsListRequest { session_id: sid };
        Ok(self
            .inner
            .artifacts_list(auth_req(&self.state, req))
            .await?
            .into_inner()
            .artifacts)
    }

    pub async fn artifact_import(&mut self, path: &str) -> Result<String, AppError> {
        let sid = self
            .state
            .session_id
            .clone()
            .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session"))?;
        let req = ArtifactImportRequest {
            session_id: sid,
            path: path.into(),
        };
        Ok(self
            .inner
            .artifact_import(auth_req(&self.state, req))
            .await?
            .into_inner()
            .artifact_id)
    }

    pub async fn artifact_export(
        &mut self,
        artifact_id: &str,
        output_path: &str,
    ) -> Result<(), AppError> {
        let req = ArtifactExportRequest {
            artifact_id: artifact_id.into(),
            output_path: output_path.into(),
        };
        self.inner
            .artifact_export(auth_req(&self.state, req))
            .await?;
        Ok(())
    }
}
