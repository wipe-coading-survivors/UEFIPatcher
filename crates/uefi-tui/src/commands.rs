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

struct NodeCmdArgs {
    target: Option<String>,
    file: Option<String>,
    artifact_id: Option<String>,
    mode: Option<String>,
    body_only: bool,
}

fn parse_node_cmd_args(parts: &[&str]) -> NodeCmdArgs {
    let mut a = NodeCmdArgs {
        target: None,
        file: None,
        artifact_id: None,
        mode: None,
        body_only: false,
    };
    let mut i = 1;
    while i < parts.len() {
        match parts[i] {
            "--file" => {
                a.file = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--artifact-id" => {
                a.artifact_id = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--mode" => {
                a.mode = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--body-only" => {
                a.body_only = true;
                i += 1;
            }
            other if !other.starts_with("--") && a.target.is_none() => {
                a.target = Some(other.to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    a
}

fn mode_to_i32(s: &str) -> Result<i32, String> {
    match s {
        "into" | "" => Ok(0),
        "before" => Ok(1),
        "after" => Ok(2),
        _ => Err(format!("unknown mode: {s} (into|before|after)")),
    }
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
        "insert" => {
            let a = parse_node_cmd_args(&parts);
            let iid = client.state.active_image_id.clone().ok_or("no active image")?;
            let target = a
                .target
                .or_else(|| app.selected_path())
                .ok_or("no target (select a node or pass TARGET)")?;
            let (ffs_path, artifact_id) = match (a.file, a.artifact_id) {
                (Some(p), None) => (p, String::new()),
                (None, Some(id)) => (String::new(), id),
                _ => {
                    return Err(
                        "usage: :insert [TARGET] (--file PATH | --artifact-id ID) [--mode into|before|after]"
                            .into(),
                    )
                }
            };
            let mode = mode_to_i32(a.mode.as_deref().unwrap_or(""))?;
            let req = ImageNodeInsertRequest {
                image_id: iid,
                target,
                ffs_path,
                artifact_id,
                mode,
            };
            let r = client
                .inner
                .image_node_insert(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("inserted {}", r.item_id);
            let _ = refresh_registry(app, client).await;
            Ok(r.item_id)
        }
        "replace" => {
            let a = parse_node_cmd_args(&parts);
            let iid = client.state.active_image_id.clone().ok_or("no active image")?;
            let target = a
                .target
                .or_else(|| app.selected_path())
                .ok_or("no target")?;
            let (ffs_path, artifact_id) = match (a.file, a.artifact_id) {
                (Some(p), None) => (p, String::new()),
                (None, Some(id)) => (String::new(), id),
                _ => {
                    return Err(
                        "usage: :replace [TARGET] (--file PATH | --artifact-id ID) [--body-only]"
                            .into(),
                    )
                }
            };
            let req = ImageNodeReplaceRequest {
                image_id: iid,
                target,
                ffs_path,
                artifact_id,
                body_only: a.body_only,
            };
            let r = client
                .inner
                .image_node_replace(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("replaced {}", r.item_id);
            let _ = refresh_registry(app, client).await;
            Ok(r.item_id)
        }
        "remove" => {
            let a = parse_node_cmd_args(&parts);
            let iid = client.state.active_image_id.clone().ok_or("no active image")?;
            let target = a
                .target
                .or_else(|| app.selected_path())
                .ok_or("no target")?;
            let req = ImageNodeRemoveRequest { image_id: iid, target: target.clone() };
            client
                .inner
                .image_node_remove(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("removed {target}");
            Ok(target)
        }
        "rebuild" => {
            let a = parse_node_cmd_args(&parts);
            let iid = client.state.active_image_id.clone().ok_or("no active image")?;
            let target = a
                .target
                .or_else(|| app.selected_path())
                .ok_or("no target")?;
            let req = ImageNodeRebuildRequest { image_id: iid, target: target.clone() };
            client
                .inner
                .image_node_rebuild(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("rebuilt {target}");
            Ok(target)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_node_cmd_target_and_flags() {
        let a = parse_node_cmd_args(&["insert", "1/0", "--file", "/x.bin", "--mode", "into"]);
        assert_eq!(a.target.as_deref(), Some("1/0"));
        assert_eq!(a.file.as_deref(), Some("/x.bin"));
        assert_eq!(a.mode.as_deref(), Some("into"));
        assert!(a.artifact_id.is_none());
        assert!(!a.body_only);
    }

    #[test]
    fn parse_node_cmd_artifact_and_body_only() {
        let a = parse_node_cmd_args(&["replace", "--artifact-id", "art1", "--body-only"]);
        assert_eq!(a.artifact_id.as_deref(), Some("art1"));
        assert!(a.body_only);
        assert!(a.target.is_none());
        assert!(a.file.is_none());
    }

    #[test]
    fn parse_node_cmd_target_is_first_non_flag() {
        let a = parse_node_cmd_args(&["remove", "0/3"]);
        assert_eq!(a.target.as_deref(), Some("0/3"));
    }
}
