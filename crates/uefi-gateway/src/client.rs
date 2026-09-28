use http::Uri;
use hyper_util::rt::TokioIo;
use serde_json::Value;
use std::path::Path;
use tonic::Request;
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;
use uefi_proto::descriptor::{DynamicMessage, MethodDescriptor, SerializeOptions};
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::*;

use crate::session::SessionMap;

pub struct EngineClient {
    typed: EngineServiceClient<Channel>,
    grpc: tonic::client::Grpc<Channel>,
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
            typed: EngineServiceClient::new(channel.clone()),
            grpc: tonic::client::Grpc::new(channel),
        })
    }

    /// Generic unary-вызов любого метода engine.EngineService: JSON → DynamicMessage →
    /// gRPC → DynamicMessage → JSON. Auth-метадата — как у типизированных обёрток.
    pub async fn call(
        &mut self,
        method: &MethodDescriptor,
        body: Value,
        sessions: &SessionMap,
        session_id: &str,
    ) -> Result<Value, tonic::Status> {
        let token = sessions
            .get_token(session_id)
            .await
            .ok_or_else(|| tonic::Status::unauthenticated("no token for session"))?;
        let msg = DynamicMessage::deserialize(method.input(), body)
            .map_err(|e| tonic::Status::invalid_argument(format!("request decode: {e}")))?;
        let mut request = Request::new(msg);
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        request
            .metadata_mut()
            .insert("x-session-id", session_id.parse().unwrap());
        let path = http::uri::PathAndQuery::try_from(format!(
            "/{}/{}",
            uefi_proto::descriptor::SERVICE_NAME,
            method.name()
        ))
        .map_err(|e| tonic::Status::internal(format!("path: {e}")))?;
        self.grpc
            .ready()
            .await
            .map_err(|e| tonic::Status::unknown(format!("service not ready: {e}")))?;
        let resp = self
            .grpc
            .unary(request, path, crate::bridge::DynCodec::new(method.output()))
            .await?;
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::new(&mut buf);
        resp.into_inner()
            .serialize_with_options(
                &mut ser,
                &SerializeOptions::new().skip_default_fields(false),
            )
            .map_err(|e| tonic::Status::internal(format!("response encode: {e}")))?;
        serde_json::from_slice(&buf)
            .map_err(|e| tonic::Status::internal(format!("response encode: {e}")))
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
            .typed
            .session_create(SessionCreateRequest { name: name.into() })
            .await?
            .into_inner();
        Ok((r.session_id, r.token))
    }
    pub async fn session_destroy(&mut self, id: &str) -> Result<(), tonic::Status> {
        self.typed
            .session_destroy(SessionDestroyRequest {
                session_id: id.into(),
            })
            .await?;
        Ok(())
    }
    pub async fn sessions_list(&mut self) -> Result<Vec<SessionInfo>, tonic::Status> {
        Ok(self
            .typed
            .sessions_list(SessionsListRequest {})
            .await?
            .into_inner()
            .sessions)
    }
    pub async fn image_upload(
        &mut self,
        sessions: &SessionMap,
        session_id: &str,
        data: Vec<u8>,
        name: &str,
        mode: i32,
    ) -> Result<ImageOpenResponse, tonic::Status> {
        let req = ImageUploadRequest {
            session_id: session_id.into(),
            data,
            mode,
            name: name.into(),
        };
        Ok(self
            .typed
            .image_upload(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner())
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
        self.typed
            .image_save(Self::auth_req(sessions, session_id, req).await?)
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
            .typed
            .artifact_import(Self::auth_req(sessions, session_id, req).await?)
            .await?
            .into_inner()
            .artifact_id)
    }
}
