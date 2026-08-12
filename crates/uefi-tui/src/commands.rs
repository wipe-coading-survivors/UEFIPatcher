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
            app.tree = parse_nodes_flat(&dump.nodes);
            app.cursor = 0;
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

fn parse_nodes_flat(nodes: &[Node]) -> Vec<crate::app::TreeNode> {
    nodes
        .iter()
        .map(|n| crate::app::TreeNode {
            path: n.path.clone(),
            depth: 0,
            node_type: n.r#type as u8,
            subtype: n.subtype as u8,
            guid: if n.guid.is_empty() {
                None
            } else {
                Some(n.guid.clone())
            },
            name: n.name.clone(),
            action: 50,
            expanded: true,
            has_children: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nodes_flat_basic() {
        let nodes = vec![
            Node {
                path: "0".into(),
                r#type: 62,
                subtype: 0,
                guid: String::new(),
                offset: 0,
                size: 256,
                name: "Image".into(),
            },
            Node {
                path: "0/0".into(),
                r#type: 65,
                subtype: 1,
                guid: "abc".into(),
                offset: 256,
                size: 1024,
                name: "Volume".into(),
            },
        ];
        let tree = parse_nodes_flat(&nodes);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].depth, 0);
        assert_eq!(tree[0].node_type, 62);
        assert_eq!(tree[0].guid, None);
        assert_eq!(tree[1].node_type, 65);
        assert_eq!(tree[1].subtype, 1);
        assert_eq!(tree[1].guid.as_deref(), Some("abc"));
    }
}
