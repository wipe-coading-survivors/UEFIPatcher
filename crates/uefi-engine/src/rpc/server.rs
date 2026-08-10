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

use crate::parser::image::{dump_tree, list_items, parse_image};
use crate::parser::target::{find_item, parse_target};
use crate::session::SessionManager;
use crate::storage::Db;
use crate::types::{Guid, Image, ImageMode};

pub struct EngineServer {
    pub sm: Arc<SessionManager>,
    pub images: Arc<Mutex<HashMap<String, Image>>>,
    pub data_dir: PathBuf,
}

type RpcResult<T> = std::result::Result<Response<T>, Status>;

#[tonic::async_trait]
impl EngineService for EngineServer {
    async fn create_session(
        &self,
        req: Request<CreateSessionRequest>,
    ) -> RpcResult<CreateSessionResponse> {
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
        Ok(Response::new(CreateSessionResponse {
            session_id: id,
            token: tok,
        }))
    }

    async fn destroy_session(&self, req: Request<DestroySessionRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        self.sm
            .destroy_session(&r.session_id, self.sm.purge_artifacts)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    async fn list_sessions(
        &self,
        _req: Request<ListSessionsRequest>,
    ) -> RpcResult<ListSessionsResponse> {
        let rows = self
            .sm
            .list_sessions()
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListSessionsResponse {
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

    async fn open_image(&self, req: Request<OpenImageRequest>) -> RpcResult<OpenImageResponse> {
        let r = req.into_inner();
        let mode = match r.mode {
            0 => ImageMode::Read,
            1 => ImageMode::Write,
            _ => return Err(Status::invalid_argument("bad mode")),
        };
        let bytes = fs::read(&r.image_path).map_err(|e| Status::not_found(e.to_string()))?;
        let image_id = Uuid::new_v4().to_string();
        let img = parse_image(&bytes, mode, &image_id, &r.session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        let root_guid = img
            .root
            .guid
            .map(|g| crate::guid_to_upper_string(&g))
            .unwrap_or_default();
        self.images.lock().await.insert(image_id.clone(), img);
        let _ = self.sm.touch(&r.session_id);
        Ok(Response::new(OpenImageResponse {
            image_id,
            root_guid,
        }))
    }

    async fn dump_tree(&self, req: Request<DumpTreeRequest>) -> RpcResult<DumpTreeResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images
            .get(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let format = match r.format {
            0 => uefi_proto::DumpFormat::Text,
            1 => uefi_proto::DumpFormat::Tsv,
            _ => return Err(Status::invalid_argument("bad format")),
        };
        let text = dump_tree(&img.root, format);
        Ok(Response::new(DumpTreeResponse { text }))
    }

    async fn list_items(&self, req: Request<ListItemsRequest>) -> RpcResult<ListItemsResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images
            .get(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let items = list_items(
            &img.root,
            if r.filter.is_empty() {
                None
            } else {
                Some(&r.filter)
            },
        );
        Ok(Response::new(ListItemsResponse { items }))
    }

    async fn search_items(
        &self,
        req: Request<SearchItemsRequest>,
    ) -> RpcResult<SearchItemsResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images
            .get(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
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
        let items = crate::parser::image::search(&img.root, &r.query, &modes, limit);
        Ok(Response::new(SearchItemsResponse { items }))
    }

    async fn find_item(&self, req: Request<FindItemRequest>) -> RpcResult<FindItemResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images
            .get(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let _ = find_item(&img.root, &t).map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(FindItemResponse { item_id: r.target }))
    }

    async fn insert(&self, req: Request<InsertRequest>) -> RpcResult<InsertResponse> {
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
        let mut images = self.images.lock().await;
        let img = images
            .get_mut(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let mode = match r.mode {
            0 => crate::ops::InsertMode::Into,
            1 => crate::ops::InsertMode::Before,
            2 => crate::ops::InsertMode::After,
            _ => return Err(Status::invalid_argument("bad mode")),
        };
        crate::ops::insert(&mut img.root, &t, &ffs_bytes, mode)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InsertResponse { item_id: r.target }))
    }

    async fn remove(&self, req: Request<RemoveRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let mut images = self.images.lock().await;
        let img = images
            .get_mut(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        crate::ops::remove(&mut img.root, &t).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    async fn replace(&self, req: Request<ReplaceRequest>) -> RpcResult<ReplaceResponse> {
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
        let mut images = self.images.lock().await;
        let img = images
            .get_mut(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        crate::ops::replace(&mut img.root, &t, &data, r.body_only)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ReplaceResponse { item_id: r.target }))
    }

    async fn rebuild(&self, req: Request<RebuildRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let mut images = self.images.lock().await;
        let img = images
            .get_mut(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        crate::ops::rebuild(&mut img.root, &t).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    async fn extract_artifact(
        &self,
        req: Request<ExtractArtifactRequest>,
    ) -> RpcResult<ExtractArtifactResponse> {
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
        Ok(Response::new(ExtractArtifactResponse { artifact_id }))
    }

    async fn export_artifact(&self, req: Request<ExportArtifactRequest>) -> RpcResult<Empty> {
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

    async fn import_artifact(
        &self,
        req: Request<ImportArtifactRequest>,
    ) -> RpcResult<ImportArtifactResponse> {
        let r = req.into_inner();
        let bytes = fs::read(&r.file_path).map_err(|e| Status::not_found(e.to_string()))?;
        let artifact_id = Uuid::new_v4().to_string();
        let path = crate::storage::artifact::store_artifact_file(
            &self.data_dir,
            &r.session_id,
            &artifact_id,
            &bytes,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        let source = Path::new(&r.file_path)
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
        Ok(Response::new(ImportArtifactResponse { artifact_id }))
    }

    async fn list_artifacts(
        &self,
        req: Request<ListArtifactsRequest>,
    ) -> RpcResult<ListArtifactsResponse> {
        let r = req.into_inner();
        let arts = self
            .sm
            .db
            .lock()
            .unwrap()
            .list_artifacts(&r.session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListArtifactsResponse {
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

    async fn set_setup_item_visibility(
        &self,
        req: Request<SetSetupItemVisibilityRequest>,
    ) -> RpcResult<Empty> {
        let r = req.into_inner();
        let mut images = self.images.lock().await;
        let img = images
            .get_mut(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        crate::setup::set_item_visibility(img, &r.item_id, r.visible)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    async fn save_image(&self, req: Request<SaveImageRequest>) -> RpcResult<Empty> {
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

    async fn add_setup_form_set(
        &self,
        req: Request<AddSetupFormSetRequest>,
    ) -> RpcResult<AddSetupFormSetResponse> {
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
        let mut images = self.images.lock().await;
        let img = images
            .get_mut(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let result = crate::setup_advanced::add_setup_formset(img, &schema, target_guid.as_ref())
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
            crate::setup_advanced::SetupAdvancedError::IfrBuildError(s) => Status::internal(s),
            crate::setup_advanced::SetupAdvancedError::FfsAssemblyError(s) => Status::internal(s),
        })?;
        Ok(Response::new(AddSetupFormSetResponse {
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
            .create_session(CreateSessionRequest::default())
            .await
            .unwrap()
            .into_inner();
        assert!(!resp.session_id.is_empty());
        assert!(!resp.token.is_empty());
        client
            .destroy_session(DestroySessionRequest {
                session_id: resp.session_id,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn create_and_list_sessions() {
        let (_td, mut client) = setup().await;
        let resp = client
            .create_session(CreateSessionRequest::default())
            .await
            .unwrap()
            .into_inner();
        let list = client
            .list_sessions(ListSessionsRequest {})
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
    async fn rpc_open_save_round_trip() {
        let (td, mut client) = setup().await;
        let orig = td.path().join("orig.bin");
        let out = td.path().join("out.bin");
        std::fs::write(&orig, fixture_volume()).unwrap();
        let session = client
            .create_session(CreateSessionRequest::default())
            .await
            .unwrap()
            .into_inner();
        let opened = client
            .open_image(OpenImageRequest {
                session_id: session.session_id.clone(),
                image_path: orig.to_string_lossy().to_string(),
                mode: ImageMode::Read as i32,
            })
            .await
            .unwrap()
            .into_inner();
        let dumped = client
            .dump_tree(DumpTreeRequest {
                image_id: opened.image_id.clone(),
                format: 0,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(!dumped.text.is_empty());
        client
            .save_image(SaveImageRequest {
                image_id: opened.image_id,
                output_path: out.to_string_lossy().to_string(),
            })
            .await
            .unwrap();
        let saved = std::fs::read(&out).unwrap();
        assert_eq!(saved, fixture_volume());
    }
}
