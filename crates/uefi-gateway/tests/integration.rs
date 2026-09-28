mod mock_server;

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::json;
use tempfile::TempDir;

async fn setup_gateway() -> (TempDir, String) {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    mock_server::start_mock(&sock).await;
    let webui_dir = td.path().join("webui");
    std::fs::create_dir_all(&webui_dir).unwrap();
    std::fs::write(webui_dir.join("index.html"), "<html>e2e-marker</html>").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let webui = webui_dir.clone();
    tokio::spawn(async move {
        let _ = uefi_gateway::serve(listener, &sock, &webui).await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    (td, format!("http://{addr}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn static_spa_fallback() {
    let (_td, base) = setup_gateway().await;
    let resp = reqwest::get(format!("{base}/image/whatever"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let text = resp.text().await.unwrap();
    assert!(text.contains("e2e-marker"));
}

#[tokio::test(flavor = "multi_thread")]
async fn bridge_full_flow_upload_nodes_replace_download() {
    let (_td, base) = setup_gateway().await;
    let client = reqwest::Client::new();
    let cookie_header = |sid: &str| format!("uefipatcher_session={sid}");

    let resp = client
        .post(format!("{base}/api/v1/session"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let sid = resp.json::<serde_json::Value>().await.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let part = reqwest::multipart::Part::bytes(b"bios-bytes").file_name("live.bin");
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("mode", "write");
    let resp = client
        .post(format!("{base}/api/v1/image/upload"))
        .header("cookie", cookie_header(&sid))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let image_id = resp.json::<serde_json::Value>().await.unwrap()["image_id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = client
        .post(format!("{base}/api/v1/rpc/ImageNodesList"))
        .header("cookie", cookie_header(&sid))
        .json(&json!({"imageId": image_id, "filter": ""}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.json::<serde_json::Value>().await.unwrap()["nodes"].is_array());

    let resp = client
        .post(format!("{base}/api/v1/rpc/ImageNodeReplace"))
        .header("cookie", cookie_header(&sid))
        .json(&json!({"imageId": image_id, "target": "1", "artifactId": "a-1", "ffsPath": "", "bodyOnly": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = client
        .post(format!("{base}/api/v1/rpc/ImageSave"))
        .header("cookie", cookie_header(&sid))
        .json(&json!({"imageId": image_id, "outputPath": "/tmp/gw-e2e-save.bin"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = client
        .get(format!("{base}/api/v1/image/{image_id}/download"))
        .header("cookie", cookie_header(&sid))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.bytes().await.unwrap();
    assert!(!bytes.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn health() {
    let (_td, base) = setup_gateway().await;
    let resp = reqwest::get(format!("{base}/api/v1/health")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_session_and_list() {
    let (_td, base) = setup_gateway().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/api/v1/session"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookies = resp.cookies().collect::<Vec<_>>();
    assert!(cookies.iter().any(|c| c.name() == "uefipatcher_session"));
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["session_id"].as_str().is_some());

    let resp = client
        .get(format!("{base}/api/v1/sessions"))
        .header(
            "cookie",
            "uefipatcher_session=".to_string() + body["session_id"].as_str().unwrap(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn bridge_unknown_method_404() {
    let (_td, base) = setup_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/v1/rpc/NoSuchMethod"))
        .header("content-type", "application/json")
        .body(r#"{"x":1}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["code"], "NOT_FOUND");
}

#[tokio::test(flavor = "multi_thread")]
async fn bridge_without_cookie_401() {
    let (_td, base) = setup_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/v1/rpc/ImagesList"))
        .header("content-type", "application/json")
        .body(r#"{"sessionId":"s"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn bridge_roundtrip_images_list() {
    let (_td, base) = setup_gateway().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/api/v1/session"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let sid = resp.json::<serde_json::Value>().await.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = client
        .post(format!("{base}/api/v1/rpc/ImagesList"))
        .header("cookie", format!("uefipatcher_session={sid}"))
        .json(&json!({"sessionId": sid}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["images"].is_array());
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_image_opens_in_engine() {
    let (_td, base) = setup_gateway().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/api/v1/session"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let sid = resp.json::<serde_json::Value>().await.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let part = reqwest::multipart::Part::bytes(b"bios-image-bytes").file_name("test.bin");
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("mode", "write");
    let resp = client
        .post(format!("{base}/api/v1/image/upload"))
        .header("cookie", format!("uefipatcher_session={sid}"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["image_id"].as_str().is_some());
    assert_eq!(body["name"], "test.bin");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_artifact_returns_id() {
    let (_td, base) = setup_gateway().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/api/v1/session"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let sid = resp.json::<serde_json::Value>().await.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let part = reqwest::multipart::Part::bytes(vec![0xAA, 0xBB]).file_name("blob.bin");
    let form = reqwest::multipart::Form::new().part("file", part);
    let resp = client
        .post(format!("{base}/api/v1/artifact/upload"))
        .header("cookie", format!("uefipatcher_session={sid}"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["artifact_id"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn bridge_bad_field_type_400() {
    let (_td, base) = setup_gateway().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/api/v1/session"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let sid = resp.json::<serde_json::Value>().await.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = client
        .post(format!("{base}/api/v1/rpc/ImageOpen"))
        .header("cookie", format!("uefipatcher_session={sid}"))
        .json(&json!({"path": 123}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["code"], "INVALID_ARGUMENT");
}
