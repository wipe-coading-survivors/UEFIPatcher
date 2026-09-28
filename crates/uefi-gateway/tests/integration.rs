mod mock_server;

use std::time::Duration;

use reqwest::StatusCode;
use serde_json::json;
use tempfile::TempDir;

async fn setup_gateway() -> (TempDir, String) {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    mock_server::start_mock(&sock).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = uefi_gateway::serve(listener, &sock).await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    (td, format!("http://{addr}"))
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
