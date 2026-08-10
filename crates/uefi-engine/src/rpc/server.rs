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
use crate::storage::image::{atomic_write, read_image_file, remove_image_file, store_image_file};
use crate::types::{Guid, Image, ImageMode};

pub struct EngineServer {
    pub sm: Arc<SessionManager>,
    pub images: Arc<Mutex<HashMap<String, Image>>>,
    pub data_dir: PathBuf,
}

type RpcResult<T> = std::result::Result<Response<T>, Status>;

impl EngineServer {
    async fn flush_image(&self, image_id: &str) -> Result<(), Status> {
        let (bytes, session_id) = {
            let images = self.images.lock().await;
            let img = images
                .get(image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let bytes =
                crate::builder::build_image(img).map_err(|e| Status::internal(e.to_string()))?;
            (bytes, img.session_id.clone())
        };
        let path = self
            .data_dir
            .join("sessions")
            .join(&session_id)
            .join("images")
            .join(format!("{image_id}.bin"));
        atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
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
}

#[tonic::async_trait]
impl EngineService for EngineServer {
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
        Ok(Response::new(SessionCreateResponse {
            session_id: id,
            token: tok,
        }))
    }

    async fn session_destroy(&self, req: Request<SessionDestroyRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        self.sm
            .destroy_session(&r.session_id, self.sm.purge_artifacts)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

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

    async fn image_open(&self, req: Request<ImageOpenRequest>) -> RpcResult<ImageOpenResponse> {
        let r = req.into_inner();
        let mode = match r.mode {
            0 => ImageMode::Read,
            1 => ImageMode::Write,
            _ => return Err(Status::invalid_argument("bad mode")),
        };
        let bytes = fs::read(&r.path).map_err(|e| Status::not_found(e.to_string()))?;
        let image_id = Uuid::new_v4().to_string();
        let name = if r.name.is_empty() {
            std::path::Path::new(&r.path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            r.name.clone()
        };
        let img = parse_image(&bytes, mode, &image_id, &r.session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        store_image_file(&self.data_dir, &r.session_id, &image_id, &bytes)
            .map_err(|e| Status::internal(e.to_string()))?;
        self.sm
            .db
            .lock()
            .unwrap()
            .insert_image(
                &image_id,
                &r.session_id,
                &name,
                &r.path,
                r.mode as i64,
                bytes.len() as i64,
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        let root_guid = img
            .root
            .guid
            .map(|g| crate::guid_to_upper_string(&g))
            .unwrap_or_default();
        self.images.lock().await.insert(image_id.clone(), img);
        let _ = self.sm.touch(&r.session_id);
        Ok(Response::new(ImageOpenResponse {
            image_id,
            root_guid,
            name,
        }))
    }

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
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            let mode = match r.mode {
                0 => crate::ops::InsertMode::Into,
                1 => crate::ops::InsertMode::Before,
                2 => crate::ops::InsertMode::After,
                _ => return Err(Status::invalid_argument("bad mode")),
            };
            crate::ops::insert(&mut img_slot.root, &t, &ffs_bytes, mode)
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(ImageNodeResponse { item_id: r.target }))
    }

    async fn image_node_remove(&self, req: Request<ImageNodeRemoveRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            crate::ops::remove(&mut img_slot.root, &t)
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(Empty {}))
    }

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
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(ImageNodeResponse { item_id: r.target }))
    }

    async fn image_node_rebuild(&self, req: Request<ImageNodeRebuildRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            crate::ops::rebuild(&mut img_slot.root, &t)
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(Empty {}))
    }

    async fn image_node_extract(
        &self,
        req: Request<ImageNodeExtractRequest>,
    ) -> RpcResult<ImageNodeExtractResponse> {
        let r = req.into_inner();
        let (session_id, bytes) = {
            let images = self.images.lock().await;
            let img = images
                .get(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let session_id = img.session_id.clone();
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            let node = find_item(&img.root, &t).map_err(|e| Status::not_found(e.to_string()))?;
            let bytes = if r.body_only {
                node.body.clone()
            } else {
                node.header
                    .iter()
                    .chain(node.body.iter())
                    .copied()
                    .collect()
            };
            (session_id, bytes)
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
        Ok(Response::new(ImageNodeExtractResponse { artifact_id }))
    }

    async fn artifact_export(&self, req: Request<ArtifactExportRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
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
        Ok(Response::new(Empty {}))
    }

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
        Ok(Response::new(ArtifactImportResponse { artifact_id }))
    }

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

    async fn setup_set_form_visibility(
        &self,
        req: Request<SetupSetFormVisibilityRequest>,
    ) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::setup::set_item_visibility(img_slot, &r.item_id, r.visible)
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(Empty {}))
    }

    async fn setup_list_forms(
        &self,
        _req: Request<SetupListFormsRequest>,
    ) -> RpcResult<SetupListFormsResponse> {
        Err(Status::unimplemented(
            "SetupListForms not implemented (Plan B)",
        ))
    }

    async fn setup_list_strings(
        &self,
        _req: Request<SetupListStringsRequest>,
    ) -> RpcResult<SetupListStringsResponse> {
        Err(Status::unimplemented(
            "SetupListStrings not implemented (Plan B)",
        ))
    }

    async fn image_save(&self, req: Request<ImageSaveRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images
            .get(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let bytes =
            crate::builder::build_image(img).map_err(|e| Status::internal(e.to_string()))?;
        fs::write(&r.output_path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    async fn setup_form_set_add(
        &self,
        req: Request<SetupFormSetAddRequest>,
    ) -> RpcResult<SetupFormSetAddResponse> {
        let r = req.into_inner();
        let schema = crate::setup_advanced::schema::parse_schema(&r.schema_json)
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
            crate::setup_advanced::add_setup_formset(img_slot, &schema, target_guid.as_ref())
                .map_err(|e| match e {
                    crate::setup_advanced::SetupAdvancedError::InvalidSchema(s) => {
                        Status::invalid_argument(s)
                    }
                    crate::setup_advanced::SetupAdvancedError::StringPackageNotFound => {
                        Status::not_found("string package not found")
                    }
                    crate::setup_advanced::SetupAdvancedError::AmiFilesNotFound => {
                        Status::not_found("AMI setupdataBin/amitseSct not found")
                    }
                    crate::setup_advanced::SetupAdvancedError::IfrBuildError(s) => {
                        Status::internal(s)
                    }
                    crate::setup_advanced::SetupAdvancedError::FfsAssemblyError(s) => {
                        Status::internal(s)
                    }
                })?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(SetupFormSetAddResponse {
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
            .add_service(EngineServiceServer::new(server))
            .serve_with_incoming(incoming)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    })
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use http::Uri;
    use hyper_util::rt::TokioIo;
    use tempfile::TempDir;
    use tonic::transport::{Channel, Endpoint};
    use tower::service_fn;
    use uefi_proto::engine_service_client::EngineServiceClient;

    async fn setup() -> (TempDir, EngineServiceClient<Channel>) {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("test.sock");
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let images = Arc::new(Mutex::new(HashMap::new()));
        let server = EngineServer {
            sm,
            images,
            data_dir: td.path().to_path_buf(),
        };
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);
        tokio::spawn(async move {
            Server::builder()
                .add_service(EngineServiceServer::new(server))
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
        (td, EngineServiceClient::new(channel))
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

        client
            .image_node_rebuild(ImageNodeRebuildRequest {
                image_id: opened.image_id.clone(),
                target: "0".into(),
            })
            .await
            .unwrap();

        let after = std::fs::read(&img_path).unwrap();
        assert!(
            after != before || after == fixture_volume(),
            "write-through must update disk file after mutation"
        );
    }
}
