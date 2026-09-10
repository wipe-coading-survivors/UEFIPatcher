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
                image_id: "mock-image-1".into(),
                name: "mock.bin".into(),
                path: "/dev/null".into(),
                ..Default::default()
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
            nodes: vec![Node {
                path: "0".into(),
                r#type: 65,
                subtype: 0,
                guid: String::new(),
                offset: 0,
                size: 256,
                name: String::new(),
                action: 0,
            }],
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
        Ok(Response::new(ArtifactsListResponse { artifacts: vec![] }))
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
            forms: vec![FormInfo {
                form_id: "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0".into(),
                formset_guid: "5C60F367-A505-419A-859E-2A4FF6CA6FE5".into(),
                form_id_ifr: 1,
                title: "Main".into(),
                visible: true,
            }],
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
            strings: vec![StringInfo {
                language: "eng".into(),
                string_id: 1,
                text: "Hello".into(),
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
            inserted_form_ids: vec![42],
            string_ids: std::collections::HashMap::from([("mock".into(), 2u32)]),
        }))
    }
    async fn hii_form_hijack(
        &self,
        _req: Request<HiiFormHijackRequest>,
    ) -> Result<Response<HiiFormHijackResponse>, Status> {
        Ok(Response::new(HiiFormHijackResponse {
            string_ids: std::collections::HashMap::from([("mock".into(), 3u32)]),
            unlock_flips: vec!["pkg+0x67a: 01 -> 02".into()],
            help_controls: vec![HiiHelpControlEdit {
                question_id: 0x3B,
                offset: 0x40,
                old_string_id: 0x2A,
                new_string_id: 3,
            }],
            help_records: vec![HiiHelpRecordEdit {
                question_id: 0x3B,
                record_offset: 0x1F4,
                old_string_id: 0x1A4,
                new_string_id: 3,
            }],
            form_ifr_start: 0x5A,
            form_ifr_end: 0x8C,
        }))
    }
    async fn hii_gates_list(
        &self,
        req: Request<HiiGatesListRequest>,
    ) -> Result<Response<HiiGatesListResponse>, Status> {
        let _ = req;
        Ok(Response::new(HiiGatesListResponse {
            gates: vec![GateInfo {
                gate_kind: "suppress".into(),
                wraps: "ref".into(),
                form_id: 10029,
                host_form_id: 10002,
                question_id: 0,
                expression: "1 == 1".into(),
                flippable: true,
                flip: "pkg+0x67a: 01 -> 02".into(),
                scope_offset: 0x66C,
            }],
        }))
    }
    async fn hii_unlock(
        &self,
        req: Request<HiiUnlockRequest>,
    ) -> Result<Response<HiiUnlockResponse>, Status> {
        let _ = req;
        Ok(Response::new(HiiUnlockResponse {
            gates: vec![GateInfo {
                gate_kind: "grayout".into(),
                wraps: "question".into(),
                form_id: 10029,
                host_form_id: 10029,
                question_id: 0x3B,
                expression: "0x009A == 0x0001".into(),
                flippable: true,
                flip: "pkg+0xdd1: 01 00 -> ff ff".into(),
                scope_offset: 0xDCB,
            }],
            applied_flips: vec!["pkg+0xdd1: 01 00 -> ff ff".into()],
        }))
    }
    async fn hii_question_info(
        &self,
        _req: Request<HiiQuestionInfoRequest>,
    ) -> Result<Response<HiiQuestionInfoResponse>, Status> {
        Ok(Response::new(HiiQuestionInfoResponse {
            question: Some(mock_question()),
        }))
    }
    async fn hii_set_value(
        &self,
        _req: Request<HiiSetValueRequest>,
    ) -> Result<Response<HiiSetValueResponse>, Status> {
        Ok(Response::new(HiiSetValueResponse {
            question: Some(mock_question()),
            applied_flips: vec!["file …raw body store+0x62: 00 -> 01".into()],
            stores: vec!["mock store".into()],
        }))
    }
    async fn hii_question_add(
        &self,
        _req: Request<HiiQuestionAddRequest>,
    ) -> Result<Response<HiiQuestionAddResponse>, Status> {
        let mut string_ids = std::collections::HashMap::new();
        string_ids.insert("Serial Console".to_string(), 2u32);
        Ok(Response::new(HiiQuestionAddResponse {
            questions: vec![HiiQuestionAddOutcome {
                question_id: 0x200,
                string_ids,
                spf_record_offset: 0x13C,
            }],
            refs: vec![],
        }))
    }
    async fn hii_page_add(
        &self,
        _req: Request<HiiPageAddRequest>,
    ) -> Result<Response<HiiPageAddResponse>, Status> {
        Ok(Response::new(HiiPageAddResponse {
            form_id: 10021,
            slot: 1,
            page_offset: 0x178,
            title_string_id: 0x1A7,
        }))
    }
}

fn mock_question() -> QuestionInfo {
    QuestionInfo {
        form_id: 10029,
        question_id: 0x3B,
        kind: "one_of".into(),
        var_store_id: 1,
        varstore: Some(VarStoreInfo {
            id: 1,
            guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
            size: 0x72,
            name: "Setup".into(),
        }),
        var_offset: 0x3A,
        width: 1,
        min: 0,
        max: 0,
        step: 0,
        options: vec![
            OptionEntry {
                string_id: 4,
                value: 0,
                flags: 0x30,
            },
            OptionEntry {
                string_id: 3,
                value: 1,
                flags: 0x00,
            },
        ],
        defaults: vec![DefaultEntry {
            default_id: 0,
            r#type: 0,
            value: 1,
        }],
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
            .session_create(SessionCreateRequest {
                name: "test".into(),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(!resp.session_id.is_empty());
        assert!(!resp.token.is_empty());
    }
}
