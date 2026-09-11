use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use uefi_proto::engine_service_server::{EngineService, EngineServiceServer};
use uefi_proto::*;

use crate::parser::image::{list_items, parse_image};
use crate::parser::target::{find_item, parse_target};
use crate::session::SessionManager;
use crate::storage::Db;
use crate::storage::image::{
    atomic_write, read_image_file, read_snapshot_file, remove_image_file, store_image_file,
    store_snapshot_file,
};
use crate::types::{Guid, Image, ImageMode};

pub struct EngineServer {
    pub sm: Arc<SessionManager>,
    pub images: Arc<Mutex<HashMap<String, Image>>>,
    pub data_dir: PathBuf,
}

type RpcResult<T> = std::result::Result<Response<T>, Status>;

fn builder_error_status(e: crate::builder::BuilderError) -> Status {
    match e {
        crate::builder::BuilderError::RecompressionUnsupported => {
            Status::failed_precondition(e.to_string())
        }
        crate::builder::BuilderError::Compression(crate::compress::CompressError::EmptyInput) => {
            Status::failed_precondition(e.to_string())
        }
        _ => Status::internal(e.to_string()),
    }
}

fn ops_error_status(e: crate::ops::OpsError) -> Status {
    match e {
        crate::ops::OpsError::MutationBehindCompression | crate::ops::OpsError::ImmutableRegion => {
            Status::failed_precondition(e.to_string())
        }
        _ => Status::internal(e.to_string()),
    }
}

fn hii_error_status(e: crate::hii::HiiError) -> Status {
    match e {
        crate::hii::HiiError::NotFound
        | crate::hii::HiiError::StringPackageNotFound
        | crate::hii::HiiError::NoSuppressScope => Status::not_found(e.to_string()),
        crate::hii::HiiError::NotASetupItem
        | crate::hii::HiiError::InvalidSchema(_)
        | crate::hii::HiiError::HidingUnsupported => Status::invalid_argument(e.to_string()),
        crate::hii::HiiError::NotWritable
        | crate::hii::HiiError::GateExpressionUnsupported(_)
        | crate::hii::HiiError::MutationBehindCompression
        | crate::hii::HiiError::PeGrowthUnsupported
        | crate::hii::HiiError::ValueOpUnsupported(_)
        | crate::hii::HiiError::IdOccupied(_) => Status::failed_precondition(e.to_string()),
        _ => Status::internal(e.to_string()),
    }
}

fn hii_error_status_ctx(e: crate::hii::HiiError, ctx: &str) -> Status {
    match e {
        crate::hii::HiiError::NotFound => Status::not_found(format!("{ctx} not found")),
        _ => hii_error_status(e),
    }
}

fn artifact_output_path_is_relative(p: &str) -> bool {
    std::path::Path::new(p).is_relative()
}

impl EngineServer {
    async fn flush_image(&self, image_id: &str) -> Result<(), Status> {
        let (bytes, session_id) = {
            let images = self.images.lock().await;
            let img = images
                .get(image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let bytes = crate::builder::build_image(img).map_err(builder_error_status)?;
            (bytes, img.session_id.clone())
        };
        let path = self
            .data_dir
            .join("sessions")
            .join(&session_id)
            .join("images")
            .join(format!("{image_id}.bin"));
        if let Ok(metadata) = fs::metadata(&path) {
            let existing_size = metadata.len() as usize;
            if bytes.len() < existing_size {
                return Err(Status::failed_precondition(format!(
                    "build_image output ({}) is smaller than stored file ({}); \
                     refusing write to prevent data loss",
                    bytes.len(),
                    existing_size
                )));
            }
        }
        atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        let mut images = self.images.lock().await;
        if let Some(img) = images.get_mut(image_id) {
            let mode = img.mode;
            let session_id = img.session_id.clone();
            match parse_image(&bytes, mode, image_id, &session_id) {
                Ok(refreshed) => *img = refreshed,
                Err(e) => {
                    images.remove(image_id);
                    drop(images);
                    return Err(Status::internal(e.to_string()));
                }
            }
        }
        drop(images);
        self.sm
            .db
            .lock()
            .unwrap()
            .touch_image(image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(())
    }

    async fn get_or_load_image(&self, image_id: &str) -> Result<Image, Status> {
        {
            let images = self.images.lock().await;
            if let Some(img) = images.get(image_id) {
                return Ok(img.clone());
            }
        }
        let row = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_image(image_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("image not found"))?;
        let bytes = read_image_file(&self.data_dir, &row.session_id, image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        let mode = if row.mode == 1 {
            ImageMode::Write
        } else {
            ImageMode::Read
        };
        let img = crate::parser::image::parse_image(&bytes, mode, image_id, &row.session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        self.images
            .lock()
            .await
            .insert(image_id.into(), img.clone());
        Ok(img)
    }

    async fn open_from_bytes(
        &self,
        session_id: &str,
        bytes: Vec<u8>,
        mode: ImageMode,
        name: &str,
        path: &str,
    ) -> RpcResult<ImageOpenResponse> {
        let image_id = Uuid::new_v4().to_string();
        let img = parse_image(&bytes, mode, &image_id, session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        store_image_file(&self.data_dir, session_id, &image_id, &bytes)
            .map_err(|e| Status::internal(e.to_string()))?;
        self.sm
            .db
            .lock()
            .unwrap()
            .insert_image(
                &image_id,
                session_id,
                name,
                path,
                mode as i64,
                bytes.len() as i64,
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        let root_guid = img
            .root
            .guid
            .map(|g| crate::guid_to_upper_string(&g))
            .unwrap_or_default();
        self.images.lock().await.insert(image_id.clone(), img);
        let _ = self.sm.touch(session_id);
        tracing::info!(name = %name, size = bytes.len(), image_id = %image_id, "image uploaded");
        Ok(Response::new(ImageOpenResponse {
            image_id,
            root_guid,
            name: name.to_string(),
        }))
    }
}

#[tonic::async_trait]
impl EngineService for EngineServer {
    #[tracing::instrument(skip(self, req), err)]
    async fn session_create(
        &self,
        req: Request<SessionCreateRequest>,
    ) -> RpcResult<SessionCreateResponse> {
        let r = req.into_inner();
        let name = if r.name.is_empty() {
            std::env::var("PWD").unwrap_or_default()
        } else {
            r.name
        };
        let (id, tok) = self
            .sm
            .create_session(&name)
            .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(name = %name, session_id = %id, "session created");
        Ok(Response::new(SessionCreateResponse {
            session_id: id,
            token: tok,
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn session_destroy(&self, req: Request<SessionDestroyRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        self.sm
            .destroy_session(&r.session_id, self.sm.purge_artifacts)
            .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(session_id = %r.session_id, purged = self.sm.purge_artifacts, "session destroyed");
        Ok(Response::new(Empty {}))
    }

    #[tracing::instrument(skip(self, _req), err)]
    async fn sessions_list(
        &self,
        _req: Request<SessionsListRequest>,
    ) -> RpcResult<SessionsListResponse> {
        let rows = self
            .sm
            .list_sessions()
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(SessionsListResponse {
            sessions: rows
                .into_iter()
                .map(|r| SessionInfo {
                    session_id: r.id,
                    name: r.name,
                    created_at: r.created_at,
                    last_activity: r.last_activity,
                })
                .collect(),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_open(&self, req: Request<ImageOpenRequest>) -> RpcResult<ImageOpenResponse> {
        let r = req.into_inner();
        let mode = match r.mode {
            0 => ImageMode::Read,
            1 => ImageMode::Write,
            _ => return Err(Status::invalid_argument("bad mode")),
        };
        let bytes = fs::read(&r.path).map_err(|e| Status::not_found(e.to_string()))?;
        let name = if r.name.is_empty() {
            std::path::Path::new(&r.path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            r.name.clone()
        };
        self.open_from_bytes(&r.session_id, bytes, mode, &name, &r.path)
            .await
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_upload(&self, req: Request<ImageUploadRequest>) -> RpcResult<ImageOpenResponse> {
        let r = req.into_inner();
        let mode = match r.mode {
            0 => ImageMode::Read,
            1 => ImageMode::Write,
            _ => return Err(Status::invalid_argument("bad mode")),
        };
        let name = if r.name.is_empty() {
            "upload".to_string()
        } else {
            r.name.clone()
        };
        self.open_from_bytes(&r.session_id, r.data, mode, &name, &name)
            .await
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_close(&self, req: Request<ImageCloseRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let row = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_image(&r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("image not found"))?;
        self.sm
            .db
            .lock()
            .unwrap()
            .delete_image(&r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        remove_image_file(&self.data_dir, &row.session_id, &r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        self.images.lock().await.remove(&r.image_id);
        Ok(Response::new(Empty {}))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn images_list(&self, req: Request<ImagesListRequest>) -> RpcResult<ImagesListResponse> {
        let r = req.into_inner();
        let rows = self
            .sm
            .db
            .lock()
            .unwrap()
            .list_images(&r.session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ImagesListResponse {
            images: rows
                .into_iter()
                .map(|r| ImageInfo {
                    image_id: r.id,
                    name: r.name,
                    path: r.path,
                    mode: r.mode as i32,
                    size: r.size as u64,
                    created_at: r.created_at,
                    last_activity: r.last_activity,
                })
                .collect(),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_status(
        &self,
        req: Request<ImageStatusRequest>,
    ) -> RpcResult<ImageStatusResponse> {
        let r = req.into_inner();
        let row = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_image(&r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("image not found"))?;
        Ok(Response::new(ImageStatusResponse {
            info: Some(ImageInfo {
                image_id: row.id,
                name: row.name,
                path: row.path,
                mode: row.mode as i32,
                size: row.size as u64,
                created_at: row.created_at,
                last_activity: row.last_activity,
            }),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_nodes_list(
        &self,
        req: Request<ImageNodesListRequest>,
    ) -> RpcResult<ImageNodesResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let nodes = list_items(
            &img.root,
            if r.filter.is_empty() {
                None
            } else {
                Some(&r.filter)
            },
        );
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(ImageNodesResponse { nodes }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_nodes_search(
        &self,
        req: Request<ImageNodesSearchRequest>,
    ) -> RpcResult<ImageNodesResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let modes: Vec<uefi_common::search::SearchMode> = r
            .modes
            .iter()
            .map(|m| match m {
                0 => uefi_common::search::SearchMode::Name,
                1 => uefi_common::search::SearchMode::Utf8,
                2 => uefi_common::search::SearchMode::Utf16Le,
                _ => uefi_common::search::SearchMode::Bytes,
            })
            .collect();
        let limit = r.limit as usize;
        let nodes = crate::parser::image::search(&img.root, &r.query, &modes, limit);
        Ok(Response::new(ImageNodesResponse { nodes }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_node_insert(
        &self,
        req: Request<ImageNodeInsertRequest>,
    ) -> RpcResult<ImageNodeResponse> {
        let r = req.into_inner();
        let ffs_bytes = if !r.artifact_id.is_empty() {
            let art = self
                .sm
                .db
                .lock()
                .unwrap()
                .get_artifact(&r.artifact_id)
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("artifact not found"))?;
            crate::storage::artifact::read_artifact_file(&self.data_dir, &art.session_id, &art.id)
                .map_err(|e| Status::internal(e.to_string()))?
        } else {
            fs::read(&r.ffs_path).map_err(|e| Status::not_found(e.to_string()))?
        };
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let mode = match r.mode {
            0 => crate::ops::InsertMode::Into,
            1 => crate::ops::InsertMode::Before,
            2 => crate::ops::InsertMode::After,
            _ => return Err(Status::invalid_argument("bad mode")),
        };
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::ops::insert(&mut img_slot.root, &t, &ffs_bytes, mode)
                .map_err(ops_error_status)?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        let source = if !r.artifact_id.is_empty() {
            r.artifact_id.as_str()
        } else {
            r.ffs_path.as_str()
        };
        tracing::info!(
            image_id = %r.image_id,
            target = %r.target,
            mode = ?mode,
            source = %source,
            size = ffs_bytes.len(),
            "artifact inserted"
        );
        Ok(Response::new(ImageNodeResponse { item_id: r.target }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_node_remove(&self, req: Request<ImageNodeRemoveRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            crate::ops::remove(&mut img_slot.root, &t).map_err(ops_error_status)?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, target = %r.target, "node removed");
        Ok(Response::new(Empty {}))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_node_replace(
        &self,
        req: Request<ImageNodeReplaceRequest>,
    ) -> RpcResult<ImageNodeResponse> {
        let r = req.into_inner();
        let data = if !r.artifact_id.is_empty() {
            let art = self
                .sm
                .db
                .lock()
                .unwrap()
                .get_artifact(&r.artifact_id)
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("artifact not found"))?;
            crate::storage::artifact::read_artifact_file(&self.data_dir, &art.session_id, &art.id)
                .map_err(|e| Status::internal(e.to_string()))?
        } else {
            fs::read(&r.ffs_path).map_err(|e| Status::not_found(e.to_string()))?
        };
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            crate::ops::replace(&mut img_slot.root, &t, &data, r.body_only)
                .map_err(ops_error_status)?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        let source = if !r.artifact_id.is_empty() {
            r.artifact_id.as_str()
        } else {
            r.ffs_path.as_str()
        };
        tracing::info!(
            image_id = %r.image_id,
            target = %r.target,
            body_only = r.body_only,
            source = %source,
            size = data.len(),
            "node replaced"
        );
        Ok(Response::new(ImageNodeResponse { item_id: r.target }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_node_rebuild(&self, req: Request<ImageNodeRebuildRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            crate::ops::rebuild(&mut img_slot.root, &t).map_err(ops_error_status)?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(Empty {}))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_node_extract(
        &self,
        req: Request<ImageNodeExtractRequest>,
    ) -> RpcResult<ImageNodeExtractResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let (session_id, bytes, node_ty, node_subtype, node_guid) = {
            let session_id = img.session_id.clone();
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            let node = find_item(&img.root, &t).map_err(|e| Status::not_found(e.to_string()))?;
            let ty = node.node_type;
            let sub = node.subtype;
            let guid = node.guid;
            let bytes = if r.body_only {
                node.body.clone()
            } else {
                node.header
                    .iter()
                    .chain(node.body.iter())
                    .copied()
                    .collect()
            };
            (session_id, bytes, ty, sub, guid)
        };
        let artifact_id = Uuid::new_v4().to_string();
        let path = crate::storage::artifact::store_artifact_file(
            &self.data_dir,
            &session_id,
            &artifact_id,
            &bytes,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        let kind = if r.body_only { "body" } else { "whole" };
        let source = format!("{}{}", r.target, if r.body_only { ":body" } else { "" });
        self.sm
            .db
            .lock()
            .unwrap()
            .insert_artifact(
                &artifact_id,
                &session_id,
                kind,
                &path.to_string_lossy(),
                bytes.len() as i64,
                &source,
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(
            image_id = %r.image_id,
            target = %r.target,
            body_only = r.body_only,
            ty = ?node_ty,
            subtype = node_subtype,
            guid = ?node_guid,
            size = bytes.len(),
            artifact_id = %artifact_id,
            "artifact extracted"
        );
        Ok(Response::new(ImageNodeExtractResponse { artifact_id }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn artifact_export(&self, req: Request<ArtifactExportRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        if artifact_output_path_is_relative(&r.output_path) {
            return Err(Status::invalid_argument("output_path must be absolute"));
        }
        let art = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_artifact(&r.artifact_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("artifact not found"))?;
        crate::storage::artifact::write_artifact_to_output(
            &self.data_dir,
            &art.session_id,
            &art.id,
            &r.output_path,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(
            artifact_id = %r.artifact_id,
            output_path = %r.output_path,
            size = art.size,
            "artifact exported"
        );
        Ok(Response::new(Empty {}))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn artifact_import(
        &self,
        req: Request<ArtifactImportRequest>,
    ) -> RpcResult<ArtifactImportResponse> {
        let r = req.into_inner();
        let bytes = fs::read(&r.path).map_err(|e| Status::not_found(e.to_string()))?;
        let artifact_id = Uuid::new_v4().to_string();
        let path = crate::storage::artifact::store_artifact_file(
            &self.data_dir,
            &r.session_id,
            &artifact_id,
            &bytes,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        let source = Path::new(&r.path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        self.sm
            .db
            .lock()
            .unwrap()
            .insert_artifact(
                &artifact_id,
                &r.session_id,
                "imported",
                &path.to_string_lossy(),
                bytes.len() as i64,
                &source,
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(
            name = %source,
            size = bytes.len(),
            kind = "imported",
            artifact_id = %artifact_id,
            "artifact imported"
        );
        Ok(Response::new(ArtifactImportResponse { artifact_id }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn artifacts_list(
        &self,
        req: Request<ArtifactsListRequest>,
    ) -> RpcResult<ArtifactsListResponse> {
        let r = req.into_inner();
        let arts = self
            .sm
            .db
            .lock()
            .unwrap()
            .list_artifacts(&r.session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ArtifactsListResponse {
            artifacts: arts
                .into_iter()
                .map(|a| ArtifactInfo {
                    artifact_id: a.id,
                    kind: a.kind,
                    size: a.size as u64,
                    created_at: a.created_at,
                    source: a.source,
                })
                .collect(),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_set_form_visibility(
        &self,
        req: Request<HiiSetFormVisibilityRequest>,
    ) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::set_item_visibility(img_slot, &r.item_id, r.visible)
                .map_err(|e| hii_error_status_ctx(e, &r.item_id))?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, visible = r.visible, "form visibility set");
        Ok(Response::new(Empty {}))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_list_forms(
        &self,
        req: Request<HiiListFormsRequest>,
    ) -> RpcResult<HiiListFormsResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let forms = crate::hii::forms::collect_forms(&img);
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(HiiListFormsResponse { forms }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_list_strings(
        &self,
        req: Request<HiiListStringsRequest>,
    ) -> RpcResult<HiiListStringsResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let strings = crate::hii::strings::collect_strings(&img);
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(HiiListStringsResponse { strings }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_save(&self, req: Request<ImageSaveRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let bytes = crate::builder::build_image(&img).map_err(builder_error_status)?;
        fs::write(&r.output_path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_form_set_add(
        &self,
        req: Request<HiiFormSetAddRequest>,
    ) -> RpcResult<HiiFormSetAddResponse> {
        let r = req.into_inner();
        let schema = crate::hii::schema::parse_schema(&r.schema_json)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let target_guid: Option<Guid> = if r.target_ffs_guid.is_empty() {
            None
        } else {
            Some(
                Guid::try_parse(&r.target_ffs_guid)
                    .map_err(|e| Status::invalid_argument(e.to_string()))?,
            )
        };
        let img = self.get_or_load_image(&r.image_id).await?;
        let result = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::formset_add::add_setup_formset(img_slot, &schema, target_guid.as_ref())
                .map_err(|e| match e {
                    crate::hii::HiiError::InvalidSchema(s) => Status::invalid_argument(s),
                    crate::hii::HiiError::StringPackageNotFound => {
                        Status::not_found("string package not found")
                    }
                    crate::hii::HiiError::AmiFilesNotFound => {
                        Status::not_found("AMI setupdataBin/amitseSct not found")
                    }
                    crate::hii::HiiError::MutationBehindCompression
                    | crate::hii::HiiError::PeGrowthUnsupported => {
                        Status::failed_precondition(e.to_string())
                    }
                    _ => Status::internal(e.to_string()),
                })?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(HiiFormSetAddResponse {
            new_ffs_id: result.new_ffs_guid.to_string(),
            inserted_form_ids: result
                .inserted_form_ids
                .into_iter()
                .map(|f| f as u32)
                .collect(),
            string_ids: result
                .string_ids
                .into_iter()
                .map(|(k, v)| (k, v as u32))
                .collect(),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_form_add(&self, req: Request<HiiFormAddRequest>) -> RpcResult<HiiFormAddResponse> {
        let r = req.into_inner();
        let schema = crate::hii::schema::parse_schema(&r.schema_json)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let img = self.get_or_load_image(&r.image_id).await?;
        let result = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::form_add::add_form(img_slot, &r.target, &schema)
                .map_err(|e| hii_error_status_ctx(e, &r.target))?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, target = %r.target, "form added to existing formset");
        Ok(Response::new(HiiFormAddResponse {
            inserted_form_ids: result
                .inserted_form_ids
                .into_iter()
                .map(|f| f as u32)
                .collect(),
            string_ids: result
                .string_ids
                .into_iter()
                .map(|(k, v)| (k, v as u32))
                .collect(),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_form_hijack(
        &self,
        req: Request<HiiFormHijackRequest>,
    ) -> RpcResult<HiiFormHijackResponse> {
        let r = req.into_inner();
        let schema = crate::hii::schema::parse_hijack_schema(&r.schema_json)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let setupdata_guid: Option<Guid> = if r.setupdata_guid.is_empty() {
            None
        } else {
            Some(
                Guid::try_parse(&r.setupdata_guid)
                    .map_err(|e| Status::invalid_argument(e.to_string()))?,
            )
        };
        let img = self.get_or_load_image(&r.image_id).await?;
        let result = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::form_hijack::hijack_form(
                img_slot,
                &r.target,
                &schema,
                setupdata_guid.as_ref(),
            )
            .map_err(|e| match e {
                crate::hii::HiiError::AmiFilesNotFound => Status::not_found(e.to_string()),
                _ => hii_error_status_ctx(e, &r.target),
            })?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, target = %r.target, "form hijacked");
        Ok(Response::new(HiiFormHijackResponse {
            string_ids: result
                .string_ids
                .iter()
                .map(|(k, v)| (k.clone(), u32::from(*v)))
                .collect(),
            form_ifr_start: result.form_ifr_start,
            form_ifr_end: result.form_ifr_end,
            unlock_flips: result.unlock_flips.clone(),
            help_controls: result
                .help_controls
                .iter()
                .map(|c| HiiHelpControlEdit {
                    question_id: u32::from(c.question_id),
                    offset: c.offset as u32,
                    old_string_id: u32::from(c.old_string_id),
                    new_string_id: u32::from(c.new_string_id),
                })
                .collect(),
            help_records: result
                .help_records
                .iter()
                .map(|r| HiiHelpRecordEdit {
                    question_id: u32::from(r.question_id),
                    record_offset: r.record_offset as u32,
                    old_string_id: u32::from(r.old_string_id),
                    new_string_id: u32::from(r.new_string_id),
                })
                .collect(),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_gates_list(
        &self,
        req: Request<HiiGatesListRequest>,
    ) -> RpcResult<HiiGatesListResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let gates = crate::hii::gates_list(&img, &r.item_id)
            .map_err(|e| hii_error_status_ctx(e, &r.item_id))?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, count = gates.len(), "hii gates listed");
        Ok(Response::new(HiiGatesListResponse { gates }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_unlock(&self, req: Request<HiiUnlockRequest>) -> RpcResult<HiiUnlockResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let outcome = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::unlock(img_slot, &r.item_id)
                .map_err(|e| hii_error_status_ctx(e, &r.item_id))?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, flips = outcome.applied.len(), "hii unlock");
        Ok(Response::new(HiiUnlockResponse {
            gates: outcome.gates,
            applied_flips: outcome.applied,
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_question_info(
        &self,
        req: Request<HiiQuestionInfoRequest>,
    ) -> RpcResult<HiiQuestionInfoResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let question = crate::hii::question_info(&img, &r.item_id)
            .map_err(|e| hii_error_status_ctx(e, &r.item_id))?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, "hii question info");
        Ok(Response::new(HiiQuestionInfoResponse {
            question: Some(question),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_set_value(
        &self,
        req: Request<HiiSetValueRequest>,
    ) -> RpcResult<HiiSetValueResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let outcome = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::set_value(img_slot, &r.item_id, r.value)
                .map_err(|e| hii_error_status_ctx(e, &r.item_id))?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, value = r.value, flips = outcome.applied.len(), "hii set value");
        Ok(Response::new(HiiSetValueResponse {
            question: Some(outcome.question),
            applied_flips: outcome.applied,
            stores: outcome.stores,
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_question_add(
        &self,
        req: Request<HiiQuestionAddRequest>,
    ) -> RpcResult<HiiQuestionAddResponse> {
        let r = req.into_inner();
        let schema = crate::hii::schema::parse_question_add_schema(&r.schema_json)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let img = self.get_or_load_image(&r.image_id).await?;
        let mut outcomes = Vec::with_capacity(schema.questions.len());
        let mut ref_outcomes = Vec::with_capacity(schema.refs.len());
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::check_question_add(img_slot, &r.target, &schema.questions)
                .map_err(|e| hii_error_status_ctx(e, &r.target))?;
            let question_qids: Vec<u16> = schema.questions.iter().map(|q| q.question_id).collect();
            crate::hii::check_ref_add(img_slot, &r.target, &schema.refs, &question_qids)
                .map_err(|e| hii_error_status_ctx(e, &r.target))?;
            for q in &schema.questions {
                let result = crate::hii::add_question(img_slot, &r.target, q)
                    .map_err(|e| hii_error_status_ctx(e, &r.target))?;
                outcomes.push(HiiQuestionAddOutcome {
                    question_id: u32::from(result.question_id),
                    string_ids: result
                        .string_ids
                        .into_iter()
                        .map(|(k, v)| (k, u32::from(v)))
                        .collect(),
                    spf_record_offset: result.spf_record_offset as u32,
                });
            }
            for rf in &schema.refs {
                let result = crate::hii::add_ref(img_slot, &r.target, rf)
                    .map_err(|e| hii_error_status_ctx(e, &r.target))?;
                ref_outcomes.push(HiiQuestionAddOutcome {
                    question_id: u32::from(result.question_id),
                    string_ids: result
                        .string_ids
                        .into_iter()
                        .map(|(k, v)| (k, u32::from(v)))
                        .collect(),
                    spf_record_offset: 0,
                });
            }
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, target = %r.target, count = outcomes.len(), refs = ref_outcomes.len(), "hii questions added");
        Ok(Response::new(HiiQuestionAddResponse {
            questions: outcomes,
            refs: ref_outcomes,
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_page_add(&self, req: Request<HiiPageAddRequest>) -> RpcResult<HiiPageAddResponse> {
        let r = req.into_inner();
        let schema = crate::hii::schema::parse_page_add_schema(&r.schema_json)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let img = self.get_or_load_image(&r.image_id).await?;
        let result = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::add_page(img_slot, &r.target, &schema)
                .map_err(|e| hii_error_status_ctx(e, &r.target))?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, target = %r.target, form_id = result.form_id, slot = result.slot, "hii page added");
        Ok(Response::new(HiiPageAddResponse {
            form_id: u32::from(result.form_id),
            slot: result.slot as u32,
            page_offset: result.page_offset as u32,
            title_string_id: u32::from(result.title_string_id),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_snapshot_create(
        &self,
        req: Request<ImageSnapshotCreateRequest>,
    ) -> RpcResult<ImageSnapshotCreateResponse> {
        let r = req.into_inner();
        let row = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_image(&r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("image not found"))?;
        let bytes = read_image_file(&self.data_dir, &row.session_id, &r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        let snapshot_id = Uuid::new_v4().to_string();
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let name = if r.name.is_empty() {
            format!("snap-{now_secs}")
        } else {
            r.name
        };
        store_snapshot_file(
            &self.data_dir,
            &row.session_id,
            &r.image_id,
            &snapshot_id,
            &bytes,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        self.sm
            .db
            .lock()
            .unwrap()
            .insert_image_snapshot(
                &snapshot_id,
                &r.image_id,
                &name,
                bytes.len() as i64,
                now_secs as i64,
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        let _ = self.sm.touch(&row.session_id);
        Ok(Response::new(ImageSnapshotCreateResponse {
            snapshot_id,
            created_at: now_secs as i64,
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_snapshots_list(
        &self,
        req: Request<ImageSnapshotsListRequest>,
    ) -> RpcResult<ImageSnapshotsListResponse> {
        let r = req.into_inner();
        let rows = self
            .sm
            .db
            .lock()
            .unwrap()
            .list_image_snapshots(&r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ImageSnapshotsListResponse {
            snapshots: rows
                .into_iter()
                .map(|s| ImageSnapshotInfo {
                    snapshot_id: s.id,
                    name: s.name,
                    created_at: s.created_at,
                    size: s.size as u64,
                })
                .collect(),
        }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn image_snapshot_restore(
        &self,
        req: Request<ImageSnapshotRestoreRequest>,
    ) -> RpcResult<Empty> {
        let r = req.into_inner();
        let snap = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_image_snapshot(&r.snapshot_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("snapshot not found"))?;
        if snap.image_id != r.image_id {
            return Err(Status::invalid_argument(
                "snapshot belongs to another image",
            ));
        }
        let row = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_image(&r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("image not found"))?;
        let bytes =
            read_snapshot_file(&self.data_dir, &row.session_id, &r.image_id, &r.snapshot_id)
                .map_err(|e| Status::internal(e.to_string()))?;
        let path = self
            .data_dir
            .join("sessions")
            .join(&row.session_id)
            .join("images")
            .join(format!("{}.bin", r.image_id));
        atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        self.images.lock().await.remove(&r.image_id);
        self.sm
            .db
            .lock()
            .unwrap()
            .touch_image(&r.image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(image_id = %r.image_id, snapshot_id = %r.snapshot_id, "snapshot restored");
        Ok(Response::new(Empty {}))
    }
}

#[cfg(unix)]
pub fn serve(
    socket_path: &Path,
    db: Db,
    data_dir: PathBuf,
    ttl: Duration,
    gc_interval: Duration,
    purge_artifacts: bool,
) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let _ = std::fs::remove_file(socket_path);
        let listener = tokio::net::UnixListener::bind(socket_path)
            .map_err(|e| anyhow::anyhow!("bind unix socket {}: {e}", socket_path.display()))?;
        let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);
        let sm = Arc::new(SessionManager::new(
            db,
            data_dir.clone(),
            ttl,
            gc_interval,
            purge_artifacts,
        ));
        sm.clone().spawn_gc();
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::new())),
            data_dir,
        };
        Server::builder()
            .add_service(
                EngineServiceServer::new(server).max_decoding_message_size(64 * 1024 * 1024),
            )
            .serve_with_incoming(incoming)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    })
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::types::{Action, FfsNode, FfsType, GuidedSectionParsingData, ParsingData};
    use http::Uri;
    use hyper_util::rt::TokioIo;
    use tempfile::TempDir;
    use tonic::transport::{Channel, Endpoint};
    use tower::service_fn;
    use uefi_proto::engine_service_client::EngineServiceClient;

    async fn spawn_engine_on(
        dir: &Path,
    ) -> (
        EngineServiceClient<Channel>,
        Vec<tokio::task::JoinHandle<()>>,
    ) {
        let sock = dir.join(format!("{}.sock", uuid::Uuid::new_v4().simple()));
        let db = crate::storage::open_db(&dir.join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            dir.to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let images = Arc::new(Mutex::new(HashMap::new()));
        let server = EngineServer {
            sm,
            images,
            data_dir: dir.to_path_buf(),
        };
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);
        let handle = tokio::spawn(async move {
            Server::builder()
                .add_service(
                    EngineServiceServer::new(server).max_decoding_message_size(64 * 1024 * 1024),
                )
                .serve_with_incoming(incoming)
                .await
                .unwrap();
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let sock_str = sock.to_string_lossy().to_string();
        let channel = Endpoint::try_from("http://localhost")
            .unwrap()
            .connect_with_connector(service_fn(move |_: Uri| {
                let s = sock_str.clone();
                async move {
                    Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
                }
            }))
            .await
            .unwrap();
        (EngineServiceClient::new(channel), vec![handle])
    }

    async fn setup() -> (TempDir, EngineServiceClient<Channel>) {
        let td = TempDir::new().unwrap();
        let (client, _keep) = spawn_engine_on(td.path()).await;
        (td, client)
    }

    #[test]
    fn builder_error_status_maps_recompression_to_failed_precondition() {
        let st = builder_error_status(crate::builder::BuilderError::RecompressionUnsupported);
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert!(st.message().contains("cannot be rebuilt"));
        let st = builder_error_status(crate::builder::BuilderError::SizeMismatch);
        assert_eq!(st.code(), tonic::Code::Internal);
    }

    #[test]
    fn builder_error_status_maps_empty_input_to_failed_precondition() {
        let st = builder_error_status(crate::builder::BuilderError::Compression(
            crate::compress::CompressError::EmptyInput,
        ));
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert!(st.message().contains("empty"));
        let st = builder_error_status(crate::builder::BuilderError::Compression(
            crate::compress::CompressError::RoundTripFailed,
        ));
        assert_eq!(st.code(), tonic::Code::Internal);
    }

    #[test]
    fn hii_error_status_maps_preconditions_not_found_and_bad_target() {
        let st = hii_error_status(crate::hii::HiiError::NotWritable);
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        let st = hii_error_status(crate::hii::HiiError::MutationBehindCompression);
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert!(st.message().contains("recompressed"));
        let st = hii_error_status(crate::hii::HiiError::NotASetupItem);
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
        let st = hii_error_status(crate::hii::HiiError::NotFound);
        assert_eq!(st.code(), tonic::Code::NotFound);
        let st = hii_error_status(crate::hii::HiiError::StringPackageNotFound);
        assert_eq!(st.code(), tonic::Code::NotFound);
        let st = hii_error_status(crate::hii::HiiError::InvalidIfr);
        assert_eq!(st.code(), tonic::Code::Internal);
        let st = hii_error_status(crate::hii::HiiError::InvalidSchema("bad".into()));
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
        let st = hii_error_status(crate::hii::HiiError::PeGrowthUnsupported);
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert!(st.message().contains("grow"));
    }

    #[test]
    fn hii_error_status_maps_id_occupied() {
        let st = hii_error_status(crate::hii::HiiError::IdOccupied(7));
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn hii_error_status_maps_gate_expression_unsupported() {
        let st = hii_error_status(crate::hii::HiiError::GateExpressionUnsupported(
            "suppress gate at pkg+0x674".into(),
        ));
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn hii_error_status_maps_no_suppress_scope_and_hiding() {
        let st = hii_error_status(crate::hii::HiiError::NoSuppressScope);
        assert_eq!(st.code(), tonic::Code::NotFound);
        assert!(st.message().contains("REF-parent"));
        let st = hii_error_status(crate::hii::HiiError::HidingUnsupported);
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
        assert!(st.message().contains("not implemented"));
    }

    #[test]
    fn hii_error_status_ctx_enriches_only_not_found() {
        let st = hii_error_status_ctx(crate::hii::HiiError::NotFound, "0#99");
        assert_eq!(st.code(), tonic::Code::NotFound);
        assert_eq!(st.message(), "0#99 not found");

        let st = hii_error_status_ctx(crate::hii::HiiError::NotWritable, "0#99");
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert_eq!(st.message(), crate::hii::HiiError::NotWritable.to_string());
    }

    #[test]
    fn artifact_export_rejects_relative_output_path() {
        assert!(artifact_output_path_is_relative("out.bin"));
        assert!(artifact_output_path_is_relative("./out.bin"));
        assert!(!artifact_output_path_is_relative("/tmp/out.bin"));
    }

    const FORMSET_ADD_STR_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";
    const FORMSET_ADD_LIST_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
    const FORMSET_ADD_SETUPDATA_GUID: &str = "12345678-90AB-CDEF-1234-567890ABCDEF";
    const FORMSET_ADD_AMITSE_GUID: &str = "87654321-FEDC-BA09-8765-432109FEDCBA";
    const FORMSET_ADD_SCHEMA_JSON: &str = r#"{
        "formset_guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
        "title": "T",
        "help": "H",
        "class_guids": [],
        "varstores": [],
        "default_stores": [],
        "forms": [
            {
                "id": 1,
                "title": "M",
                "items": [
                    {"type": "text", "prompt": "P", "help": "H", "text_two": "X"}
                ]
            }
        ],
        "setupdata_guid": "12345678-90AB-CDEF-1234-567890ABCDEF",
        "amitse_guid": "87654321-FEDC-BA09-8765-432109FEDCBA"
    }"#;

    fn formset_add_node(node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type,
            subtype: 0,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn formset_add_string_package() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x00, 0x00, 0x00, crate::hii::string_pack::PACKAGE_STRINGS]);
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.push(0x10);
        buf.extend_from_slice(b"first");
        buf.push(0x00);
        buf.push(0x00);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn formset_add_hii_pkg(kind: u8, payload: &[u8]) -> Vec<u8> {
        let len = 4 + payload.len();
        let mut b = vec![
            (len & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            ((len >> 16) & 0xFF) as u8,
            kind,
        ];
        b.extend_from_slice(payload);
        b
    }

    fn formset_add_hii_list(guid: &Guid, pkgs: &[&[u8]]) -> Vec<u8> {
        let total = 20 + 4 + pkgs.iter().map(|p| p.len()).sum::<usize>();
        let mut b = guid.to_bytes().to_vec();
        b.extend_from_slice(&(total as u32).to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        b
    }

    fn formset_add_hii_pe() -> Vec<u8> {
        let guid = Guid::try_parse(FORMSET_ADD_LIST_GUID).unwrap();
        let form = formset_add_hii_pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string_pkg = formset_add_string_package();
        let blob = formset_add_hii_list(&guid, &[&form, &string_pkg]);
        crate::hii::pe_resource::synth_hii_pe("HII", &blob)
    }

    fn formset_add_cert_blocked_pe() -> Vec<u8> {
        let mut pe = formset_add_hii_pe();
        assert!(crate::hii::pe_resource::try_grow_rsrc_tail(&mut pe, 32));
        pe[0xe8..0xec].copy_from_slice(&0x1000u32.to_le_bytes());
        pe[0xec..0xf0].copy_from_slice(&8u32.to_le_bytes());
        pe
    }

    fn formset_add_pfs_body() -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(b"$SPF");
        body.extend_from_slice(&[0u8; 8]);
        body
    }

    fn formset_add_image(pe: Vec<u8>, wrapper_guid: Option<Guid>) -> Image {
        let mut pe_section = formset_add_node(FfsType::Section, pe, vec![]);
        pe_section.subtype = crate::ffs::EFI_SECTION_PE32;
        let section = if let Some(guid) = wrapper_guid {
            let mut wrapper = formset_add_node(FfsType::Section, vec![], vec![pe_section]);
            wrapper.subtype = crate::ffs::EFI_SECTION_GUID_DEFINED;
            wrapper.parsing_data = ParsingData::GuidedSection(GuidedSectionParsingData {
                guid,
                dictionary_size: 0x0080_0000,
            });
            wrapper
        } else {
            pe_section
        };
        let mut file = formset_add_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::try_parse(FORMSET_ADD_STR_GUID).unwrap());
        let mut pfs = formset_add_node(FfsType::Section, formset_add_pfs_body(), vec![]);
        pfs.subtype = crate::ffs::EFI_SECTION_FREEFORM_SUBTYPE_GUID;
        let mut pe32 = formset_add_node(FfsType::Section, vec![0x4Du8, 0x5A, 0x00, 0x00], vec![]);
        pe32.subtype = crate::ffs::EFI_SECTION_PE32;
        let mut setupdata = formset_add_node(FfsType::File, vec![], vec![pfs]);
        setupdata.guid = Some(Guid::try_parse(FORMSET_ADD_SETUPDATA_GUID).unwrap());
        let mut amitse = formset_add_node(FfsType::File, vec![], vec![pe32]);
        amitse.guid = Some(Guid::try_parse(FORMSET_ADD_AMITSE_GUID).unwrap());
        let volume = formset_add_node(FfsType::Volume, vec![], vec![file, setupdata, amitse]);
        let root = formset_add_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "i".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    async fn formset_add_status(img: Image) -> Status {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        server
            .hii_form_set_add(Request::new(HiiFormSetAddRequest {
                image_id: "i".into(),
                schema_json: FORMSET_ADD_SCHEMA_JSON.to_string(),
                target_ffs_guid: String::new(),
            }))
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn hii_form_set_add_maps_mutation_behind_compression_to_failed_precondition() {
        let st = formset_add_status(formset_add_image(
            formset_add_hii_pe(),
            Some(crate::ffs::tiano_guid()),
        ))
        .await;
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert!(st.message().contains("recompressed"));
    }

    #[tokio::test]
    async fn hii_form_set_add_maps_pe_growth_unsupported_to_failed_precondition() {
        let st = formset_add_status(formset_add_image(formset_add_cert_blocked_pe(), None)).await;
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert!(st.message().contains("grow"));
    }

    const FORM_ADD_STR_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";
    const FORM_ADD_SCHEMA_JSON: &str = r#"{
        "formset_guid": "11111111-2222-3333-4444-555555555555",
        "title": "T",
        "help": "H",
        "class_guids": [],
        "varstores": [],
        "default_stores": [],
        "forms": [
            {
                "id": 42,
                "title": "NewForm",
                "items": [
                    {"type": "text", "prompt": "P", "help": "H", "text_two": "X"}
                ]
            }
        ]
    }"#;

    async fn form_add_status(img: Image, target: &str, schema_json: &str) -> Status {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        server
            .hii_form_add(Request::new(HiiFormAddRequest {
                image_id: "i".into(),
                target: target.into(),
                schema_json: schema_json.into(),
            }))
            .await
            .unwrap_err()
    }

    fn form_add_string_package() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x00, 0x00, 0x00, crate::hii::string_pack::PACKAGE_STRINGS]);
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.push(0x10);
        buf.extend_from_slice(b"first");
        buf.push(0x00);
        buf.push(0x00);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn form_add_bare_form_package() -> Vec<u8> {
        use crate::hii::ifr_builder::IfrBuilder;
        let mut b = IfrBuilder::new();
        b.emit_form_set(
            &Guid::try_parse("A1B2C3D4-E5F6-7890-ABCD-EF1234567890").unwrap(),
            1,
            1,
            &[],
        );
        b.emit_form(1, 1);
        b.emit_end();
        b.emit_end();
        let ifr = b.build();
        formset_add_hii_pkg(r_efi::hii::PACKAGE_FORMS, &ifr)
    }

    fn form_add_section_bytes(stype: u8, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + body.len());
        v.extend_from_slice(&crate::ffs::size_to_uint24((4 + body.len()) as u32));
        v.push(stype);
        v.extend_from_slice(body);
        v
    }

    fn form_add_bare_flash() -> Vec<u8> {
        let mut content =
            form_add_section_bytes(crate::ffs::EFI_SECTION_RAW, &form_add_string_package());
        content.extend(form_add_section_bytes(
            crate::ffs::EFI_SECTION_RAW,
            &form_add_bare_form_package(),
        ));
        let mut file = vec![0u8; 24];
        let g = Guid::try_parse(FORM_ADD_STR_GUID).unwrap();
        file[0..16].copy_from_slice(&g.to_bytes());
        file[18] = crate::ffs::EFI_FV_FILETYPE_RAW;
        file[20..23].copy_from_slice(&crate::ffs::size_to_uint24((24 + content.len()) as u32));
        file.extend_from_slice(&content);

        let mut body = Vec::new();
        let aligned = (body.len() + 7) & !7;
        body.resize(aligned, 0xFF);
        body.extend_from_slice(&file);
        body.extend_from_slice(&[0xFF; 1024]);
        let total = 56 + body.len();
        let mut buf = vec![0xFFu8; 32 + total];
        let fv = &mut buf[32..];
        fv[32..40].copy_from_slice(&(total as u64).to_le_bytes());
        fv[40..44].copy_from_slice(&crate::ffs::EFI_FVH_SIGNATURE.to_le_bytes());
        fv[44..48].copy_from_slice(&crate::ffs::EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        fv[48..50].copy_from_slice(&56u16.to_le_bytes());
        fv[55] = 2;
        fv[56..].copy_from_slice(&body);
        buf
    }

    async fn form_add_ok(img: Image, target: &str) -> HiiFormAddResponse {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        server
            .hii_form_add(Request::new(HiiFormAddRequest {
                image_id: "i".into(),
                target: target.into(),
                schema_json: FORM_ADD_SCHEMA_JSON.into(),
            }))
            .await
            .unwrap()
            .into_inner()
    }

    #[tokio::test]
    async fn hii_form_add_round_trips_on_bare_channel() {
        let data = form_add_bare_flash();
        let img = crate::parser::image::parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let resp = form_add_ok(img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1").await;
        assert_eq!(resp.inserted_form_ids, vec![42]);
        assert!(resp.string_ids.contains_key("NewForm"));
    }

    fn form_add_resource_hii_pe() -> Vec<u8> {
        let guid = Guid::try_parse(FORMSET_ADD_LIST_GUID).unwrap();
        let form = form_add_bare_form_package();
        let string_pkg = formset_add_string_package();
        let blob = formset_add_hii_list(&guid, &[&form, &string_pkg]);
        crate::hii::pe_resource::synth_hii_pe("HII", &blob)
    }

    fn form_add_cert_blocked_resource_pe() -> Vec<u8> {
        let mut pe = form_add_resource_hii_pe();
        pe[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        pe
    }

    #[tokio::test]
    async fn hii_form_add_maps_pe_growth_unsupported_to_failed_precondition() {
        let mut pe_sec = formset_add_node(
            FfsType::Section,
            form_add_cert_blocked_resource_pe(),
            vec![],
        );
        pe_sec.subtype = crate::ffs::EFI_SECTION_PE32;
        let mut file = formset_add_node(FfsType::File, vec![], vec![pe_sec]);
        file.guid = Some(Guid::try_parse(FORM_ADD_STR_GUID).unwrap());
        let volume = formset_add_node(FfsType::Volume, vec![], vec![file]);
        let root = formset_add_node(FfsType::Image, vec![], vec![volume]);
        let img = Image {
            image_id: "i".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let st = form_add_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            FORM_ADD_SCHEMA_JSON,
        )
        .await;
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert!(st.message().contains("grow"));
    }

    #[tokio::test]
    async fn hii_form_add_maps_unknown_target_to_not_found() {
        let data = form_add_bare_flash();
        let img = crate::parser::image::parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let st = form_add_status(
            img,
            "00000000-0000-0000-0000-000000000001:0x19:0",
            FORM_ADD_SCHEMA_JSON,
        )
        .await;
        assert_eq!(st.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn hii_form_add_maps_invalid_schema_to_invalid_argument() {
        let data = form_add_bare_flash();
        let img = crate::parser::image::parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let st = form_add_status(img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1", "{bad").await;
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
    }

    async fn form_hijack_status(
        img: Image,
        target: &str,
        schema_json: &str,
        setupdata_guid: &str,
    ) -> Status {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        server
            .hii_form_hijack(Request::new(HiiFormHijackRequest {
                image_id: "i".into(),
                target: target.into(),
                schema_json: schema_json.into(),
                setupdata_guid: setupdata_guid.into(),
            }))
            .await
            .unwrap_err()
    }

    const FORM_HIJACK_SCHEMA_JSON: &str = r#"{"questions": [
        {"question_id": 17, "prompt": "PQ", "help": "PH"}]}"#;

    #[tokio::test]
    async fn hii_form_hijack_maps_bad_schema_to_invalid_argument() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let st = form_hijack_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7",
            "{",
            "",
        )
        .await;
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn hii_form_hijack_maps_unknown_form_to_not_found() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let st = form_hijack_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#99",
            FORM_HIJACK_SCHEMA_JSON,
            "",
        )
        .await;
        assert_eq!(st.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn hii_form_hijack_maps_missing_spf_to_not_found() {
        let mut img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let sd_guid = Guid::try_parse("12345678-90AB-CDEF-1234-567890ABCDEF").unwrap();
        let vol = img
            .root
            .children
            .iter_mut()
            .find(|v| v.children.iter().any(|f| f.guid == Some(sd_guid)))
            .unwrap();
        vol.children.retain(|f| f.guid != Some(sd_guid));
        let st = form_hijack_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7",
            FORM_HIJACK_SCHEMA_JSON,
            "00000000-0000-0000-0000-00000000DEAD",
        )
        .await;
        assert_eq!(st.code(), tonic::Code::NotFound);
    }

    async fn question_add_status(img: Image, target: &str, schema_json: &str) -> Status {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        server
            .hii_question_add(Request::new(HiiQuestionAddRequest {
                image_id: "i".into(),
                target: target.into(),
                schema_json: schema_json.into(),
            }))
            .await
            .unwrap_err()
    }

    const QUESTION_ADD_SCHEMA_JSON: &str = r#"{"questions": [{
        "form_id": 10019, "prompt": "Serial Console", "help": "H",
        "question_id": 512, "var_store_id": 1, "var_offset": 128, "size": 1,
        "options": [{"text": "Disabled", "value": 0}, {"text": "Enabled", "value": 1, "default": "optimized"}]}
    ]}"#;

    #[tokio::test]
    async fn hii_question_add_maps_bad_schema_to_invalid_argument() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let st =
            question_add_status(img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7", "{").await;
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn hii_question_add_maps_unknown_form_to_not_found() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let mut schema =
            crate::hii::schema::parse_question_add_schema(QUESTION_ADD_SCHEMA_JSON).unwrap();
        schema.questions[0].form_id = 99;
        let st = question_add_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#99",
            &serde_json::to_string(&schema).unwrap(),
        )
        .await;
        assert_eq!(st.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn hii_question_add_maps_form_mismatch_to_invalid_argument() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let schema =
            crate::hii::schema::parse_question_add_schema(QUESTION_ADD_SCHEMA_JSON).unwrap();
        let st = question_add_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7",
            &serde_json::to_string(&schema).unwrap(),
        )
        .await;
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn hii_question_add_list_failure_applies_nothing() {
        let (flash, _, _) = crate::hii::question_add_fixtures::question_add_bare_flash_image();
        let img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let images = Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)])));
        let server = EngineServer {
            sm,
            images: images.clone(),
            data_dir: td.path().to_path_buf(),
        };
        const TARGET: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#10019";
        const OVERLAP_LIST: &str = r#"{"questions": [
            {"form_id": 10019, "prompt": "A", "help": "AH", "question_id": 512,
             "var_store_id": 1, "var_offset": 128, "size": 1,
             "options": [{"text": "Off", "value": 0}, {"text": "On", "value": 1, "default": "optimized"}]},
            {"form_id": 10019, "prompt": "B", "help": "BH", "question_id": 513,
             "var_store_id": 1, "var_offset": 128, "size": 1,
             "options": [{"text": "Off", "value": 0}, {"text": "On", "value": 1, "default": "optimized"}]}
        ]}"#;
        const DUP_QID_LIST: &str = r#"{"questions": [
            {"form_id": 10019, "prompt": "A", "help": "AH", "question_id": 512,
             "var_store_id": 1, "var_offset": 128, "size": 1,
             "options": [{"text": "Off", "value": 0}, {"text": "On", "value": 1, "default": "optimized"}]},
            {"form_id": 10019, "prompt": "C", "help": "CH", "question_id": 512,
             "var_store_id": 1, "var_offset": 129, "size": 1,
             "options": [{"text": "Off", "value": 0}, {"text": "On", "value": 1, "default": "optimized"}]}
        ]}"#;
        for schema_json in [OVERLAP_LIST, DUP_QID_LIST] {
            let st = server
                .hii_question_add(Request::new(HiiQuestionAddRequest {
                    image_id: "i".into(),
                    target: TARGET.into(),
                    schema_json: schema_json.into(),
                }))
                .await
                .unwrap_err();
            assert_eq!(st.code(), tonic::Code::InvalidArgument);
        }
        let untouched = images.lock().await;
        let img_ref = untouched.get("i").unwrap();
        assert_eq!(
            crate::builder::build_image(img_ref).unwrap(),
            flash,
            "a failing question list must leave the image exactly as it was"
        );
    }

    #[tokio::test]
    async fn hii_question_add_processes_refs_only_schema() {
        let (flash, _, _) = crate::hii::question_add_fixtures::question_add_bare_flash_image();
        let img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        const TARGET: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#10019";
        const REFS_ONLY: &str = r#"{"refs": [
            {"form_id": 10020, "prompt": "Goto Page", "help": "Goto Page help", "question_id": 768}
        ]}"#;
        let resp = server
            .hii_question_add(Request::new(HiiQuestionAddRequest {
                image_id: "i".into(),
                target: TARGET.into(),
                schema_json: REFS_ONLY.into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(
            resp.questions.is_empty(),
            "refs-only schema must not touch questions"
        );
        assert_eq!(resp.refs.len(), 1);
        assert_eq!(resp.refs[0].question_id, 768);
        assert!(resp.refs[0].string_ids.contains_key("Goto Page"));
        assert_eq!(
            resp.refs[0].spf_record_offset, 0,
            "a goto ref has no $SPF record"
        );
    }

    const PAGE_ADD_SCHEMA_JSON: &str = r#"{"form_id": 10021, "title": "New Page"}"#;

    async fn page_add_status(img: Image, target: &str, schema_json: &str) -> Status {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        server
            .hii_page_add(Request::new(HiiPageAddRequest {
                image_id: "i".into(),
                target: target.into(),
                schema_json: schema_json.into(),
            }))
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn hii_page_add_maps_bad_schema_to_invalid_argument() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let st = page_add_status(img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7", "{").await;
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn hii_page_add_maps_unknown_parent_form_to_not_found() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let st = page_add_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#99",
            PAGE_ADD_SCHEMA_JSON,
        )
        .await;
        assert_eq!(st.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn hii_page_add_maps_duplicate_page_to_invalid_argument() {
        let (flash, _, _) = crate::hii::question_add_fixtures::question_add_bare_flash_image();
        let img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        const TARGET: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#10019";
        let ok = server
            .hii_page_add(Request::new(HiiPageAddRequest {
                image_id: "i".into(),
                target: TARGET.into(),
                schema_json: PAGE_ADD_SCHEMA_JSON.into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(ok.form_id, 10021);
        assert_eq!(ok.slot, 1);
        assert!(ok.page_offset > 0);
        assert!(
            ok.title_string_id >= 1,
            "a title string id must be allocated"
        );
        let st = server
            .hii_page_add(Request::new(HiiPageAddRequest {
                image_id: "i".into(),
                target: TARGET.into(),
                schema_json: PAGE_ADD_SCHEMA_JSON.into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn create_and_destroy_session() {
        let (_td, mut client) = setup().await;
        let resp = client
            .session_create(SessionCreateRequest::default())
            .await
            .unwrap()
            .into_inner();
        assert!(!resp.session_id.is_empty());
        assert!(!resp.token.is_empty());
        client
            .session_destroy(SessionDestroyRequest {
                session_id: resp.session_id,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn create_and_list_sessions() {
        let (_td, mut client) = setup().await;
        let resp = client
            .session_create(SessionCreateRequest::default())
            .await
            .unwrap()
            .into_inner();
        let list = client
            .sessions_list(SessionsListRequest {})
            .await
            .unwrap()
            .into_inner();
        assert!(
            list.sessions
                .iter()
                .any(|s| s.session_id == resp.session_id)
        );
    }

    fn fixture_volume() -> Vec<u8> {
        use crate::ffs::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE};
        let mut buf = vec![0xFFu8; 256];
        buf[32..40].copy_from_slice(&256u64.to_le_bytes());
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        buf[48..50].copy_from_slice(&56u16.to_le_bytes());
        buf[55] = 2;
        buf
    }

    #[tokio::test]
    async fn flush_image_writes_bytes_to_data_dir() {
        let (td, mut client) = setup().await;
        let orig = td.path().join("orig.bin");
        std::fs::write(&orig, fixture_volume()).unwrap();
        let session = client
            .session_create(SessionCreateRequest::default())
            .await
            .unwrap()
            .into_inner();
        let opened = client
            .image_open(ImageOpenRequest {
                session_id: session.session_id.clone(),
                path: orig.to_string_lossy().to_string(),
                mode: ImageMode::Read as i32,
                name: "test.bin".into(),
            })
            .await
            .unwrap()
            .into_inner();
        let img_path = td
            .path()
            .join("sessions")
            .join(&session.session_id)
            .join("images")
            .join(format!("{}.bin", opened.image_id));
        assert!(img_path.exists(), "image bytes must be persisted on open");
        let saved = std::fs::read(&img_path).unwrap();
        assert_eq!(saved, fixture_volume());
    }

    async fn create_session(client: &mut EngineServiceClient<Channel>) -> String {
        client
            .session_create(SessionCreateRequest::default())
            .await
            .unwrap()
            .into_inner()
            .session_id
    }

    async fn list_nodes(client: &mut EngineServiceClient<Channel>, image_id: &str) -> Vec<Node> {
        client
            .image_nodes_list(tonic::Request::new(ImageNodesListRequest {
                image_id: image_id.into(),
                filter: String::new(),
            }))
            .await
            .unwrap()
            .into_inner()
            .nodes
    }

    fn fv_image_with_two_files() -> Vec<u8> {
        let mut buf = vec![0xFFu8; 256];
        buf[32..40].copy_from_slice(&256u64.to_le_bytes());
        buf[40..44].copy_from_slice(&crate::ffs::EFI_FVH_SIGNATURE.to_le_bytes());
        buf[44..48].copy_from_slice(&crate::ffs::EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        buf[48..50].copy_from_slice(&56u16.to_le_bytes());
        buf[55] = 2;
        let guid = uuid::Uuid::new_v4();
        for (k, at) in [56usize, 88].iter().enumerate() {
            let mut f = vec![0u8; 32];
            f[0..16].copy_from_slice(guid.as_bytes());
            f[18] = 0x01;
            f[20..23].copy_from_slice(&crate::ffs::size_to_uint24(32));
            f[24] = [0xAA, 0xBB][k];
            buf[*at..*at + 32].copy_from_slice(&f);
        }
        buf
    }

    #[tokio::test]
    async fn image_upload_roundtrip() {
        let (_td, mut client) = setup().await;
        let sid = create_session(&mut client).await;
        let resp = client
            .image_upload(tonic::Request::new(ImageUploadRequest {
                session_id: sid,
                data: fv_image_with_two_files(),
                mode: 0,
                name: "up.bin".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        let nodes = client
            .image_nodes_list(tonic::Request::new(ImageNodesListRequest {
                image_id: resp.image_id.clone(),
                filter: String::new(),
            }))
            .await
            .unwrap()
            .into_inner()
            .nodes;
        assert!(!nodes.is_empty());
    }

    #[tokio::test]
    async fn image_upload_rejected_over_small_limit() {
        let td = tempfile::TempDir::new().unwrap();
        let sock = td.path().join("small.sock");
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::new())),
            data_dir: td.path().to_path_buf(),
        };
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(
                    EngineServiceServer::new(server).max_decoding_message_size(1024 * 1024),
                )
                .serve_with_incoming(incoming)
                .await
                .unwrap();
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let channel = Endpoint::try_from("http://localhost")
            .unwrap()
            .connect_with_connector(tower::service_fn({
                let s = sock.to_string_lossy().to_string();
                move |_: http::Uri| {
                    let s = s.clone();
                    async move {
                        Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(
                            tokio::net::UnixStream::connect(s).await?,
                        ))
                    }
                }
            }))
            .await
            .unwrap();
        let mut client =
            EngineServiceClient::new(channel).max_encoding_message_size(64 * 1024 * 1024);
        let result = client
            .image_upload(tonic::Request::new(ImageUploadRequest {
                session_id: "s".into(),
                data: vec![0u8; 2 * 1024 * 1024],
                mode: 0,
                name: "big".into(),
            }))
            .await;
        assert!(
            result.is_err(),
            "server with 1MB decode limit must reject 2MB upload"
        );
    }

    #[tokio::test]
    async fn remove_rpc_leaves_clean_tree() {
        let (_td, mut client) = setup().await;
        let sid = create_session(&mut client).await;
        let dir = _td.path().join("src.bin");
        std::fs::write(&dir, fv_image_with_two_files()).unwrap();
        let open = client
            .image_open(tonic::Request::new(ImageOpenRequest {
                session_id: sid.clone(),
                path: dir.display().to_string(),
                mode: 1,
                name: "t".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        client
            .image_node_remove(tonic::Request::new(ImageNodeRemoveRequest {
                image_id: open.image_id.clone(),
                target: "0/0".into(),
            }))
            .await
            .unwrap();
        let nodes = client
            .image_nodes_list(tonic::Request::new(ImageNodesListRequest {
                image_id: open.image_id.clone(),
                filter: String::new(),
            }))
            .await
            .unwrap()
            .into_inner()
            .nodes;
        assert!(
            nodes.iter().all(|n| n.action == 50),
            "no pending markers after write-through flush"
        );
        let files: Vec<_> = nodes
            .iter()
            .filter(|n| n.r#type == FfsType::File as u32)
            .collect();
        assert_eq!(files.len(), 1, "removed file must disappear from the tree");
        assert_eq!(
            files[0].offset, 56,
            "survivor is re-materialized first in the rebuilt FV"
        );
    }

    fn two_fv_image_with_markers() -> Vec<u8> {
        let mut buf = vec![0xFFu8; 512];
        let guid = Guid::try_parse("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        let markers = [[0xAAu8, 0xBB], [0xCC, 0xDD]];
        for (v, base) in [0usize, 256].iter().enumerate() {
            let vol = &mut buf[*base..*base + 256];
            vol[32..40].copy_from_slice(&256u64.to_le_bytes());
            vol[40..44].copy_from_slice(&crate::ffs::EFI_FVH_SIGNATURE.to_le_bytes());
            vol[44..48].copy_from_slice(&crate::ffs::EFI_FVB2_ERASE_POLARITY.to_le_bytes());
            vol[48..50].copy_from_slice(&56u16.to_le_bytes());
            vol[55] = 2;
            for (k, &marker) in markers[v].iter().enumerate() {
                let at = 56 + 32 * k;
                let mut f = vec![0u8; 32];
                f[0..16].copy_from_slice(&guid.to_bytes());
                f[18] = 0x01;
                f[20..23].copy_from_slice(&crate::ffs::size_to_uint24(32));
                f[24] = marker;
                vol[at..at + 32].copy_from_slice(&f);
            }
        }
        buf
    }

    #[tokio::test]
    async fn second_flush_does_not_resurrect_removed_file() {
        let (_td, mut client) = setup().await;
        let sid = create_session(&mut client).await;
        let dir = _td.path().join("two.bin");
        std::fs::write(&dir, two_fv_image_with_markers()).unwrap();
        let open = client
            .image_open(tonic::Request::new(ImageOpenRequest {
                session_id: sid.clone(),
                path: dir.display().to_string(),
                mode: 1,
                name: "t".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        for target in ["0/0", "1/0"] {
            client
                .image_node_remove(tonic::Request::new(ImageNodeRemoveRequest {
                    image_id: open.image_id.clone(),
                    target: target.into(),
                }))
                .await
                .unwrap();
        }
        let img_path = _td
            .path()
            .join("sessions")
            .join(&sid)
            .join("images")
            .join(format!("{}.bin", open.image_id));
        let stored = std::fs::read(&img_path).unwrap();
        assert_eq!(stored.len(), 512, "flash length must be preserved");
        assert!(
            !stored.contains(&0xAA),
            "removed FV0 file marker must not resurrect via a flush that bypasses its FV"
        );
        assert!(stored.contains(&0xBB), "FV0 survivor marker must remain");
        assert!(
            !stored.contains(&0xCC),
            "removed FV1 file marker must be gone"
        );
        assert!(stored.contains(&0xDD), "FV1 survivor marker must remain");
    }

    #[tokio::test]
    async fn image_snapshot_create_and_list_roundtrip() {
        let (td, mut client) = setup().await;
        let sid = create_session(&mut client).await;
        let src = td.path().join("snap.bin");
        std::fs::write(&src, fv_image_with_two_files()).unwrap();
        let open = client
            .image_open(tonic::Request::new(ImageOpenRequest {
                session_id: sid,
                path: src.display().to_string(),
                mode: 1,
                name: "t".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        let created = client
            .image_snapshot_create(tonic::Request::new(ImageSnapshotCreateRequest {
                image_id: open.image_id.clone(),
                name: "before-patch".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(!created.snapshot_id.is_empty());
        let list = client
            .image_snapshots_list(tonic::Request::new(ImageSnapshotsListRequest {
                image_id: open.image_id.clone(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(list.snapshots.len(), 1);
        assert_eq!(list.snapshots[0].name, "before-patch");
        assert_eq!(list.snapshots[0].snapshot_id, created.snapshot_id);
        assert_eq!(list.snapshots[0].size, 256);
    }

    #[tokio::test]
    async fn image_snapshot_create_on_missing_image_is_not_found() {
        let (_td, mut client) = setup().await;
        let st = client
            .image_snapshot_create(tonic::Request::new(ImageSnapshotCreateRequest {
                image_id: "nope".into(),
                name: String::new(),
            }))
            .await
            .unwrap_err();
        assert_eq!(st.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn snapshot_restore_rolls_tree_back() {
        let td = tempfile::TempDir::new().unwrap();
        let (mut client, _keep) = spawn_engine_on(td.path()).await;
        let sid = create_session(&mut client).await;
        let src = td.path().join("img.bin");
        std::fs::write(&src, fv_image_with_two_files()).unwrap();
        let open = client
            .image_open(tonic::Request::new(ImageOpenRequest {
                session_id: sid,
                path: src.display().to_string(),
                mode: 1,
                name: "t".into(),
            }))
            .await
            .unwrap()
            .into_inner();

        client
            .image_node_remove(tonic::Request::new(ImageNodeRemoveRequest {
                image_id: open.image_id.clone(),
                target: "0/0".into(),
            }))
            .await
            .unwrap();
        let nodes_at_snapshot = list_nodes(&mut client, &open.image_id).await;
        let snap = client
            .image_snapshot_create(tonic::Request::new(ImageSnapshotCreateRequest {
                image_id: open.image_id.clone(),
                name: "before-second-cut".into(),
            }))
            .await
            .unwrap()
            .into_inner();

        client
            .image_node_remove(tonic::Request::new(ImageNodeRemoveRequest {
                image_id: open.image_id.clone(),
                target: "0/0".into(),
            }))
            .await
            .unwrap();

        client
            .image_snapshot_restore(tonic::Request::new(ImageSnapshotRestoreRequest {
                image_id: open.image_id.clone(),
                snapshot_id: snap.snapshot_id.clone(),
            }))
            .await
            .unwrap();
        let nodes_after = list_nodes(&mut client, &open.image_id).await;
        assert_eq!(
            nodes_at_snapshot, nodes_after,
            "restore must roll the tree back to snapshot state"
        );

        drop(client);
        let (mut client2, _keep2) = spawn_engine_on(td.path()).await;
        let listed = client2
            .image_snapshots_list(tonic::Request::new(ImageSnapshotsListRequest {
                image_id: open.image_id.clone(),
            }))
            .await
            .unwrap()
            .into_inner()
            .snapshots;
        assert_eq!(listed.len(), 1, "snapshots survive engine restart");
        assert_eq!(listed[0].name, "before-second-cut");

        let bad = client2
            .image_snapshot_restore(tonic::Request::new(ImageSnapshotRestoreRequest {
                image_id: open.image_id.clone(),
                snapshot_id: "nope".into(),
            }))
            .await;
        assert!(bad.is_err());
    }

    #[tokio::test]
    async fn write_through_persists_mutation_to_disk() {
        let (td, mut client) = setup().await;
        let orig = td.path().join("v.bin");
        std::fs::write(&orig, fixture_volume()).unwrap();
        let session = client
            .session_create(SessionCreateRequest::default())
            .await
            .unwrap()
            .into_inner();
        let opened = client
            .image_open(ImageOpenRequest {
                session_id: session.session_id.clone(),
                path: orig.to_string_lossy().to_string(),
                mode: ImageMode::Write as i32,
                name: "v.bin".into(),
            })
            .await
            .unwrap()
            .into_inner();

        let img_path = td
            .path()
            .join("sessions")
            .join(&session.session_id)
            .join("images")
            .join(format!("{}.bin", opened.image_id));
        let before = std::fs::read(&img_path).unwrap();
        let before_mtime = std::fs::metadata(&img_path).unwrap().modified().unwrap();

        client
            .image_node_rebuild(ImageNodeRebuildRequest {
                image_id: opened.image_id.clone(),
                target: "0".into(),
            })
            .await
            .unwrap();

        let after = std::fs::read(&img_path).unwrap();
        let after_mtime = std::fs::metadata(&img_path).unwrap().modified().unwrap();
        assert!(
            after != before || after_mtime != before_mtime,
            "write-through must rewrite disk file after mutation \
             (bytes or mtime must change)"
        );
    }

    #[tokio::test]
    async fn flush_image_rejects_when_build_output_smaller_than_stored() {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let (session_id, _tok) = sm.create_session("test").unwrap();
        let image_id = "img-test";

        let stored_buf = {
            use crate::ffs::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE};
            let mut buf = vec![0xFFu8; 512];
            buf[32..40].copy_from_slice(&512u64.to_le_bytes());
            buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
            buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
            buf[48..50].copy_from_slice(&56u16.to_le_bytes());
            buf[55] = 2;
            buf
        };

        let img_path = td
            .path()
            .join("sessions")
            .join(&session_id)
            .join("images")
            .join(format!("{image_id}.bin"));
        std::fs::create_dir_all(img_path.parent().unwrap()).unwrap();
        std::fs::write(&img_path, &stored_buf).unwrap();

        let small_buf = vec![0xFFu8; 256];
        let img = parse_image(&small_buf, ImageMode::Write, image_id, &session_id).unwrap();

        let images = Arc::new(Mutex::new(HashMap::from([(image_id.to_string(), img)])));
        let server = EngineServer {
            sm,
            images,
            data_dir: td.path().to_path_buf(),
        };

        let result = server.flush_image(image_id).await;

        assert!(
            result.is_err(),
            "flush_image must reject when build output (256) < stored file (512)"
        );
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);

        let preserved = std::fs::read(&img_path).unwrap();
        assert_eq!(preserved, stored_buf, "stored file must be unchanged");
    }

    #[tokio::test]
    async fn flush_image_rejects_zero_byte_build_output() {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let (session_id, _tok) = sm.create_session("test").unwrap();
        let image_id = "img-test";

        let stored_buf = vec![0xFFu8; 64];
        let img_path = td
            .path()
            .join("sessions")
            .join(&session_id)
            .join("images")
            .join(format!("{image_id}.bin"));
        std::fs::create_dir_all(img_path.parent().unwrap()).unwrap();
        std::fs::write(&img_path, &stored_buf).unwrap();

        let img = parse_image(&[], ImageMode::Write, image_id, &session_id).unwrap();

        let images = Arc::new(Mutex::new(HashMap::from([(image_id.to_string(), img)])));
        let server = EngineServer {
            sm,
            images,
            data_dir: td.path().to_path_buf(),
        };

        let result = server.flush_image(image_id).await;

        assert!(
            result.is_err(),
            "flush_image must reject when build output is empty (0 < stored 64)"
        );
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);

        let preserved = std::fs::read(&img_path).unwrap();
        assert_eq!(preserved, stored_buf, "stored file must be unchanged");
    }
}
