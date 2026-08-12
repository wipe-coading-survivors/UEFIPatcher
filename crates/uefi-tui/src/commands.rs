use http::Uri;
use hyper_util::rt::TokioIo;
use tonic::Request;
use tonic::transport::{Channel, Endpoint};
use uefi_common::state::{State, resolve_sock};
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::*;

use crate::app::App;

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

pub async fn connect(cli_sock: Option<&str>, state: State) -> Result<Client, String> {
    let sock = resolve_sock(cli_sock, &state);
    let sock_str = sock.display().to_string();
    let channel = Endpoint::try_from("http://localhost")
        .map_err(|e| e.to_string())?
        .connect_with_connector(tower::service_fn(move |_: Uri| {
            let s = sock_str.clone();
            async move {
                Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
            }
        }))
        .await
        .map_err(|e| e.to_string())?;
    Ok(Client {
        inner: EngineServiceClient::new(channel),
        state,
    })
}

pub async fn execute_command(
    app: &mut App,
    cmdline: &str,
    client: &mut Client,
) -> Result<String, String> {
    let parts: Vec<&str> = cmdline.split_whitespace().collect();
    if parts.is_empty() {
        return Err("empty command".into());
    }
    let cmd = parts[0];
    match cmd {
        "open" | "o" => {
            let path = parts
                .get(1)
                .ok_or("usage: :open PATH [--mode read|write]")?;
            let mode = if parts.contains(&"write") { 1 } else { 0 };
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let req = ImageOpenRequest {
                session_id: sid,
                path: path.to_string(),
                mode,
                name: String::new(),
            };
            let r = client
                .inner
                .image_open(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.image_loaded = true;
            app.status_msg = format!("opened image {}", r.image_id);
            let dump = client
                .inner
                .image_nodes_list(auth_req(
                    &client.state,
                    ImageNodesListRequest {
                        image_id: r.image_id.clone(),
                        filter: String::new(),
                    },
                ))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.tree = crate::tree::build_tree(&dump.nodes);
            app.cursor = 0;
            app.active_image_id = Some(r.image_id.clone());
            client.state.active_image_id = Some(r.image_id.clone());
            let _ = refresh_registry(app, client).await;
            Ok(r.image_id)
        }
        "save" | "s" => {
            let path = parts.get(1).ok_or("usage: :save OUTPUT")?;
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let req = ImageSaveRequest {
                image_id: iid,
                output_path: path.to_string(),
            };
            client
                .inner
                .image_save(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("saved to {path}");
            Ok(path.to_string())
        }
        "extract" => {
            let target = parts.get(1).ok_or("usage: :extract TARGET [--body-only]")?;
            let body_only = parts.contains(&"--body-only");
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let req = ImageNodeExtractRequest {
                image_id: iid,
                target: target.to_string(),
                body_only,
            };
            let r = client
                .inner
                .image_node_extract(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("extracted artifact {}", r.artifact_id);
            Ok(r.artifact_id)
        }
        "export" => {
            let artifact_id = parts.get(1).ok_or("usage: :export ARTIFACT_ID [PATH]")?;
            let path = parts.get(2).map(|s| s.to_string()).unwrap_or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default()
            });
            let req = ArtifactExportRequest {
                artifact_id: artifact_id.to_string(),
                output_path: path.clone(),
            };
            client
                .inner
                .artifact_export(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("exported {artifact_id} to {path}");
            Ok(artifact_id.to_string())
        }
        "import" => {
            let file = parts.get(1).ok_or("usage: :import FILE")?;
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let req = ArtifactImportRequest {
                session_id: sid,
                path: file.to_string(),
            };
            let r = client
                .inner
                .artifact_import(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("imported artifact {}", r.artifact_id);
            Ok(r.artifact_id)
        }
        "artifacts" => {
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let req = ArtifactsListRequest { session_id: sid };
            let r = client
                .inner
                .artifacts_list(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("{} artifacts", r.artifacts.len());
            Ok(format!("{} artifacts", r.artifacts.len()))
        }
        "quit" | "q" => {
            app.quit = true;
            Ok("quitting".into())
        }
        "help" | "h" => {
            app.show_help = !app.show_help;
            Ok("help toggled".into())
        }
        _ => Err(format!("unknown command: :{cmd}, try :help")),
    }
}

pub async fn refresh_registry(app: &mut App, client: &mut Client) -> Result<(), String> {
    let sid = client.state.session_id.clone().ok_or("no session")?;
    let imgs = client
        .inner
        .images_list(auth_req(&client.state, ImagesListRequest { session_id: sid.clone() }))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    let arts = client
        .inner
        .artifacts_list(auth_req(&client.state, ArtifactsListRequest { session_id: sid }))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.registry.images = imgs.images;
    app.registry.artifacts = arts.artifacts;
    if app.registry.cursor >= app.registry_selectable().len() {
        app.registry.cursor = 0;
    }
    Ok(())
}
