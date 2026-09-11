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
    async fn image_upload(
        &self,
        _req: Request<ImageUploadRequest>,
    ) -> Result<Response<ImageOpenResponse>, Status> {
        Ok(Response::new(ImageOpenResponse {
            image_id: "mock-upload".into(),
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
                Node {
                    path: "".into(),
                    r#type: 62,
                    subtype: 0,
                    guid: String::new(),
                    offset: 0,
                    size: 16777216,
                    name: "Image".into(),
                    action: 0,
                    region: String::new(),
                },
                Node {
                    path: "0".into(),
                    r#type: 65,
                    subtype: 0,
                    guid: String::new(),
                    offset: 0,
                    size: 8388608,
                    name: "ME".into(),
                    action: 0,
                    region: String::new(),
                },
                Node {
                    path: "1".into(),
                    r#type: 65,
                    subtype: 0,
                    guid: String::new(),
                    offset: 8388608,
                    size: 4194304,
                    name: "DXE".into(),
                    action: 0,
                    region: String::new(),
                },
                Node {
                    path: "1/0".into(),
                    r#type: 66,
                    subtype: 0x07,
                    guid: "ABC".into(),
                    offset: 8388608,
                    size: 4096,
                    name: "Setup".into(),
                    action: 0,
                    region: String::new(),
                },
                Node {
                    path: "1/0/0".into(),
                    r#type: 67,
                    subtype: 0x15,
                    guid: String::new(),
                    offset: 8388608,
                    size: 24,
                    name: String::new(),
                    action: 0,
                    region: String::new(),
                },
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
    async fn hii_list_forms(
        &self,
        _req: Request<HiiListFormsRequest>,
    ) -> Result<Response<HiiListFormsResponse>, Status> {
        Ok(Response::new(HiiListFormsResponse {
            forms: vec![
                FormInfo {
                    form_id: "11111111-2222-3333-4444-555555555555:0x19:0".into(),
                    formset_guid: "11111111-2222-3333-4444-555555555555".into(),
                    form_id_ifr: 10001,
                    title: "Main".into(),
                    visible: true,
                },
                FormInfo {
                    form_id: "11111111-2222-3333-4444-555555555555:0x19:0".into(),
                    formset_guid: "11111111-2222-3333-4444-555555555555".into(),
                    form_id_ifr: 10019,
                    title: "Serial Port 1 Configuration".into(),
                    visible: false,
                },
                FormInfo {
                    form_id: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE:0x19:0".into(),
                    formset_guid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".into(),
                    form_id_ifr: 902,
                    title: "Platform".into(),
                    visible: true,
                },
            ],
        }))
    }
    async fn hii_set_form_visibility(
        &self,
        _req: Request<HiiSetFormVisibilityRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn hii_list_strings(
        &self,
        _req: Request<HiiListStringsRequest>,
    ) -> Result<Response<HiiListStringsResponse>, Status> {
        Ok(Response::new(HiiListStringsResponse {
            strings: vec![
                StringInfo {
                    language: "en-US".into(),
                    string_id: 1,
                    text: "Setup".into(),
                },
                StringInfo {
                    language: "en-US".into(),
                    string_id: 2,
                    text: "Advanced".into(),
                },
                StringInfo {
                    language: "en-US".into(),
                    string_id: 3,
                    text: "Serial Port".into(),
                },
            ],
        }))
    }
    async fn hii_list_questions(
        &self,
        _req: Request<HiiListQuestionsRequest>,
    ) -> Result<Response<HiiListQuestionsResponse>, Status> {
        Ok(Response::new(HiiListQuestionsResponse {
            questions: vec![
                QuestionSummary {
                    question_id: 0x210,
                    kind: "one_of".into(),
                    prompt: "Serial Port".into(),
                    var_store_id: 1,
                    var_offset: 95,
                    width: 1,
                },
                QuestionSummary {
                    question_id: 0x211,
                    kind: "numeric".into(),
                    prompt: "Baud Rate".into(),
                    var_store_id: 1,
                    var_offset: 96,
                    width: 1,
                },
            ],
        }))
    }
    async fn hii_form_tree(
        &self,
        _req: Request<HiiFormTreeRequest>,
    ) -> Result<Response<HiiFormTreeResponse>, Status> {
        Ok(Response::new(HiiFormTreeResponse {
            edges: vec![FormEdge {
                formset_guid: "11111111-2222-3333-4444-555555555555".into(),
                parent_form_id: 10001,
                form_id: 10019,
            }],
        }))
    }
    async fn hii_form_set_add(
        &self,
        _req: Request<HiiFormSetAddRequest>,
    ) -> Result<Response<HiiFormSetAddResponse>, Status> {
        Ok(Response::new(HiiFormSetAddResponse {
            new_ffs_id: "mock".into(),
            inserted_form_ids: vec![],
            string_ids: std::collections::HashMap::new(),
        }))
    }
    async fn hii_form_add(
        &self,
        _req: Request<HiiFormAddRequest>,
    ) -> Result<Response<HiiFormAddResponse>, Status> {
        Ok(Response::new(HiiFormAddResponse {
            inserted_form_ids: vec![],
            string_ids: std::collections::HashMap::new(),
        }))
    }
    async fn hii_form_hijack(
        &self,
        _req: Request<HiiFormHijackRequest>,
    ) -> Result<Response<HiiFormHijackResponse>, Status> {
        Ok(Response::new(HiiFormHijackResponse::default()))
    }
    async fn hii_gates_list(
        &self,
        _req: Request<HiiGatesListRequest>,
    ) -> Result<Response<HiiGatesListResponse>, Status> {
        Ok(Response::new(HiiGatesListResponse {
            gates: vec![GateInfo {
                gate_kind: "suppress".into(),
                wraps: "form".into(),
                form_id: 10001,
                expression: "eq(1, 1)".into(),
                flippable: true,
                ..Default::default()
            }],
        }))
    }
    async fn hii_unlock(
        &self,
        _req: Request<HiiUnlockRequest>,
    ) -> Result<Response<HiiUnlockResponse>, Status> {
        Ok(Response::new(HiiUnlockResponse {
            gates: vec![],
            applied_flips: vec!["pkg+0x1c: 01 00 -> ff ff".into()],
        }))
    }
    async fn hii_question_info(
        &self,
        req: Request<HiiQuestionInfoRequest>,
    ) -> Result<Response<HiiQuestionInfoResponse>, Status> {
        let r = req.into_inner();
        let qid = r
            .item_id
            .rsplit(':')
            .next()
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0x210);
        Ok(Response::new(HiiQuestionInfoResponse {
            question: Some(QuestionInfo {
                question_id: qid,
                kind: "one_of".into(),
                var_store_id: 1,
                var_offset: 0x5F,
                width: 1,
                options: vec![
                    OptionEntry {
                        string_id: 18,
                        value: 0,
                        flags: 0,
                        text: "Disabled".into(),
                    },
                    OptionEntry {
                        string_id: 17,
                        value: 1,
                        flags: 0,
                        text: "Enabled".into(),
                    },
                ],
                ..Default::default()
            }),
        }))
    }
    async fn hii_set_value(
        &self,
        _req: Request<HiiSetValueRequest>,
    ) -> Result<Response<HiiSetValueResponse>, Status> {
        Ok(Response::new(HiiSetValueResponse {
            question: Some(QuestionInfo {
                question_id: 0x210,
                kind: "one_of".into(),
                ..Default::default()
            }),
            applied_flips: vec!["pkg+0x3e: 01 -> 00".into()],
            stores: vec!["Setup".into()],
        }))
    }
    async fn hii_question_add(
        &self,
        _req: Request<HiiQuestionAddRequest>,
    ) -> Result<Response<HiiQuestionAddResponse>, Status> {
        Ok(Response::new(HiiQuestionAddResponse::default()))
    }
    async fn hii_page_add(
        &self,
        _req: Request<HiiPageAddRequest>,
    ) -> Result<Response<HiiPageAddResponse>, Status> {
        Ok(Response::new(HiiPageAddResponse::default()))
    }
    async fn image_snapshot_create(
        &self,
        _req: Request<ImageSnapshotCreateRequest>,
    ) -> Result<Response<ImageSnapshotCreateResponse>, Status> {
        Ok(Response::new(ImageSnapshotCreateResponse {
            snapshot_id: "snap-1".into(),
            created_at: 0,
        }))
    }
    async fn image_snapshots_list(
        &self,
        _req: Request<ImageSnapshotsListRequest>,
    ) -> Result<Response<ImageSnapshotsListResponse>, Status> {
        Ok(Response::new(ImageSnapshotsListResponse {
            snapshots: vec![ImageSnapshotInfo {
                snapshot_id: "snap-1".into(),
                name: "before".into(),
                created_at: 1,
                size: 16,
            }],
        }))
    }
    async fn image_snapshot_restore(
        &self,
        _req: Request<ImageSnapshotRestoreRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
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
    use uefi_tui::app::{App, Focus, RegistryRow};
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
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
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

    #[tokio::test]
    async fn upload_sets_active_image_and_tree() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        let out = std::env::temp_dir().join("tui-upload-test.bin");
        std::fs::write(&out, b"mock").unwrap();
        let r =
            commands::execute_command(&mut app, &format!("upload {}", out.display()), &mut client)
                .await
                .unwrap();
        assert_eq!(app.active_image_id.as_deref(), Some(r.as_str()));
        assert_eq!(client.state.active_image_id.as_deref(), Some(r.as_str()));
        assert!(app.image_loaded);
        assert!(!app.tree.is_empty());
        let _ = std::fs::remove_file(&out);
    }

    #[tokio::test]
    async fn snapshot_returns_id() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
            .await
            .unwrap();
        let r = commands::execute_command(&mut app, "snapshot before", &mut client)
            .await
            .unwrap();
        assert_eq!(r, "snap-1");
        assert_eq!(app.status_msg, "snapshot snap-1 (before)");
    }

    #[tokio::test]
    async fn snapshots_puts_list_into_status_msg() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
            .await
            .unwrap();
        let r = commands::execute_command(&mut app, "snapshots", &mut client)
            .await
            .unwrap();
        assert_eq!(app.status_msg, "snap-1  before  16B");
        assert_eq!(r, "snap-1");
    }

    #[tokio::test]
    async fn restore_refreshes_tree_and_registry() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
            .await
            .unwrap();
        app.tree.clear();
        app.registry.images.clear();
        let r = commands::execute_command(&mut app, "restore snap-1", &mut client)
            .await
            .unwrap();
        assert_eq!(r, "snap-1");
        assert!(!app.tree.is_empty(), ":restore should refresh tree");
        assert_eq!(
            app.registry.images.len(),
            1,
            ":restore should refresh registry"
        );
        assert_eq!(app.status_msg, "restored snap-1");
    }

    #[tokio::test]
    async fn refresh_populates_registry() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "refresh", &mut client)
            .await
            .unwrap();
        assert_eq!(app.registry.images.len(), 1);
        assert_eq!(app.registry.artifacts.len(), 1);
    }

    #[tokio::test]
    async fn smoke_open_collapse_switch_registry_pick() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
            .await
            .unwrap();
        // default: only root expanded, volumes collapsed
        assert!(app.tree[0].expanded);
        assert!(!app.tree[1].expanded);
        app.toggle_expand_selected();
        assert!(!app.tree[app.selected_tree_idx().unwrap()].expanded);
        commands::execute_command(&mut app, "image switch mock-img-1", &mut client)
            .await
            .unwrap();
        assert_eq!(app.active_image_id.as_deref(), Some("mock-img-1"));
        app.focus_next();
        app.focus_next();
        assert_eq!(app.focus, Focus::Registry);
        assert!(matches!(
            app.current_registry_row(),
            Some(RegistryRow::Artifact(0)) | Some(RegistryRow::Image(_))
        ));
    }

    #[tokio::test]
    async fn extract_refreshes_registry() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
            .await
            .unwrap();
        app.registry.artifacts.clear();
        commands::execute_command(&mut app, "extract 1/0", &mut client)
            .await
            .unwrap();
        assert_eq!(
            app.registry.artifacts.len(),
            1,
            ":extract should refresh registry"
        );
    }

    #[tokio::test]
    async fn import_refreshes_registry() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        app.registry.images.clear();
        app.registry.artifacts.clear();
        let f = std::fs::File::create("/tmp/uefi_tui_import_dummy.bin").unwrap();
        drop(f);
        commands::execute_command(
            &mut app,
            "import /tmp/uefi_tui_import_dummy.bin",
            &mut client,
        )
        .await
        .unwrap();
        assert!(
            !app.registry.artifacts.is_empty(),
            ":import should refresh registry"
        );
        let _ = std::fs::remove_file("/tmp/uefi_tui_import_dummy.bin");
    }

    #[tokio::test]
    async fn image_close_clears_state() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
            .await
            .unwrap();
        assert!(app.image_loaded);
        assert!(app.active_image_id.is_some());
        let active = app.active_image_id.clone().unwrap();
        commands::execute_command(&mut app, &format!("image close {active}"), &mut client)
            .await
            .unwrap();
        assert!(app.tree.is_empty(), "tree should be cleared after close");
        assert!(
            app.active_image_id.is_none(),
            "active_image_id should be None after close"
        );
        assert!(
            !app.image_loaded,
            "image_loaded should be false after close"
        );
        assert_eq!(app.cursor, 0);
    }

    #[tokio::test]
    async fn restore_session_populates_tree_and_registry() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            active_image_id: Some("mock-img-1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::restore_session(&mut app, &mut client)
            .await
            .unwrap();
        assert_eq!(app.active_image_id.as_deref(), Some("mock-img-1"));
        assert!(app.image_loaded, "image_loaded should be set on restore");
        assert!(
            app.tree.len() >= 5,
            "tree should be rebuilt from nodes list"
        );
        assert_eq!(app.registry.images.len(), 1);
        assert_eq!(app.registry.artifacts.len(), 1);
    }

    #[tokio::test]
    async fn restore_session_without_active_image_only_registry() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
            .await
            .unwrap();
        let mut app = App::new();
        commands::restore_session(&mut app, &mut client)
            .await
            .unwrap();
        assert!(app.active_image_id.is_none());
        assert!(app.tree.is_empty());
        assert_eq!(app.registry.images.len(), 1);
        assert_eq!(app.registry.artifacts.len(), 1);
    }
}
