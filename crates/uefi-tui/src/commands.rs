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
            let req = OpenImageRequest {
                session_id: sid,
                image_path: path.to_string(),
                mode,
            };
            let r = client
                .inner
                .open_image(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.image_loaded = true;
            app.status_msg = format!("opened image {}", r.image_id);
            let dump_req = DumpTreeRequest {
                image_id: r.image_id.clone(),
                format: 0,
            };
            let dump = client
                .inner
                .dump_tree(auth_req(&client.state, dump_req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.tree = parse_tree_dump(&dump.text);
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
            let req = SaveImageRequest {
                image_id: iid,
                output_path: path.to_string(),
            };
            client
                .inner
                .save_image(auth_req(&client.state, req))
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
            let req = ExtractArtifactRequest {
                image_id: iid,
                target: target.to_string(),
                body_only,
            };
            let r = client
                .inner
                .extract_artifact(auth_req(&client.state, req))
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
            let req = ExportArtifactRequest {
                artifact_id: artifact_id.to_string(),
                output_path: path.clone(),
            };
            client
                .inner
                .export_artifact(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("exported {artifact_id} to {path}");
            Ok(artifact_id.to_string())
        }
        "import" => {
            let file = parts.get(1).ok_or("usage: :import FILE")?;
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let req = ImportArtifactRequest {
                session_id: sid,
                file_path: file.to_string(),
            };
            let r = client
                .inner
                .import_artifact(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("imported artifact {}", r.artifact_id);
            Ok(r.artifact_id)
        }
        "artifacts" => {
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let req = ListArtifactsRequest { session_id: sid };
            let r = client
                .inner
                .list_artifacts(auth_req(&client.state, req))
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

// TODO(2026-08-09): migrate to list_items RPC — current text-parser drops subtype, parses "File"/"Volume" as int (always 0), and reads "subtype=07" as name. See docs/superpowers/specs/2026-08-09-display-and-search-design.md
fn parse_tree_dump(text: &str) -> Vec<crate::app::TreeNode> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }
            Some(crate::app::TreeNode {
                path: parts[0].into(),
                depth: parts[0].matches('/').count(),
                node_type: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
                subtype: 0,
                guid: None,
                name: parts.get(2).unwrap_or(&"").to_string(),
                action: 50,
                expanded: true,
                has_children: false,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tree_simple() {
        let text = "0  Image\n  0/0  Volume\n  0/0/0  File\n";
        let nodes = parse_tree_dump(text);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].depth, 0);
        assert_eq!(nodes[1].depth, 1);
        assert_eq!(nodes[2].depth, 2);
    }
}
