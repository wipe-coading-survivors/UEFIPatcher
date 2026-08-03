# UEFI WebUI + gRPC Gateway (цикл 5+7) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать `uefi-gateway` (Rust/axum REST+WS прокси в движок через gRPC over unix-сокет, cookie-based сессии) и `webui/` (SvelteKit/TypeScript SPA: tree-view, операции, setup, upload/download образов).

**Architecture:** Новый крейт `uefi-gateway` в Cargo workspace: axum HTTP-сервер с REST API (`/api/v1/`) + WebSocket для streaming dump, gRPC-клиент к `EngineService` через unix-сокет. Cookie `uefipatcher_session` → in-memory map session_id→token → gRPC metadata. Отдельный npm-проект `webui/` (SvelteKit, SPA mode): tree-view, details, операции, setup. Docker: engine + gateway + webui.

**Tech Stack:** Rust (axum, tonic, tower-http, uefi-proto, tokio, serde, uuid, axum-extra), TypeScript (SvelteKit, vite, vitest, @testing-library/svelte, playwright, msw).

## Global Constraints

- Спека: `docs/superpowers/specs/2026-07-22-uefi-webui-design.md` — источник истины.
- API versioning: все эндпоинты под `/api/v1/`.
- gRPC-контракт: `uefi-proto` (из цикла 1). Шлюз — gRPC-клиент к `EngineService`.
- Cookie: `uefipatcher_session` (HttpOnly, SameSite=Strict) = session_id; `uefipatcher_image` = active image_id.
- Шлюз маппит: cookie session_id → in-memory token map → gRPC metadata `Authorization: Bearer <token>` + `x-session-id`.
- Поток: браузер ↔ HTTP/WS ↔ gateway ↔ gRPC over unix-сокет ↔ engine.
- SvelteKit SPA mode (`@sveltejs/adapter-static`), раздача через gateway (tower-http ServeDir) или nginx.
- Upload: multipart с лимитом (64MB), сохранение в `/tmp/uefipatcher-upload-<uuid>.bin`.
- Download: `SaveImage` в temp → binary file response.
- CORS: `tower-http::cors` в шлюзе (dev: allow `localhost:5173`).
- HTTP errors ← gRPC: 401 UNAUTHENTICATED, 404 NOT_FOUND, 400 INVALID_ARGUMENT, 500 INTERNAL. Тело: `{"error":"...","code":"ENUM"}`.
- WebSocket: `/api/v1/image/:id/dump/ws` — streaming dump.
- Кодстайл Rust: `cargo fmt`, `cargo clippy -- -D warnings`. Кодстайл TS: `eslint`, `prettier`, `svelte-check`.
- Без комментариев в коде.
- Docker: `docker/gateway.containerfile`, `docker/webui.containerfile` (на базе `docker/rust-builder.containerfile` из цикла 1, `registry.fedoraproject.org/fedora:44`), обновить `docker/docker-compose.yml`.

---

## Файлы плана

| Файл | Назначение |
|---|---|
| `Cargo.toml` | workspace: добавить uefi-gateway |
| `crates/uefi-gateway/Cargo.toml` | шлюз-крейт |
| `crates/uefi-gateway/src/main.rs` | axum server, listen, CORS, routes |
| `crates/uefi-gateway/src/config.rs` | env: UEFIPATCHER_GATEWAY_LISTEN, UEFIPATCHER_SOCK |
| `crates/uefi-gateway/src/client.rs` | gRPC-клиент к движку |
| `crates/uefi-gateway/src/session.rs` | cookie-маппинг, in-memory token map |
| `crates/uefi-gateway/src/error.rs` | AppError → HTTP status, JSON error body |
| `crates/uefi-gateway/src/routes/mod.rs` | router setup |
| `crates/uefi-gateway/src/routes/session.rs` | /api/v1/session(s) |
| `crates/uefi-gateway/src/routes/image.rs` | /api/v1/image/* |
| `crates/uefi-gateway/src/routes/edit.rs` | /api/v1/image/:id/{insert,remove,replace,rebuild} |
| `crates/uefi-gateway/src/routes/setup.rs` | /api/v1/image/:id/{set-visibility,setup-items,add-formset} |
| `crates/uefi-gateway/src/routes/upload.rs` | /api/v1/image/upload, /api/v1/image/:id/download |
| `crates/uefi-gateway/src/routes/artifact.rs` | /api/v1/image/:id/extract, /api/v1/artifact/{export,import}, /api/v1/artifacts |
| `crates/uefi-gateway/src/ws.rs` | WebSocket streaming dump |
| `crates/uefi-gateway/tests/mock_server.rs` | mock EngineService |
| `crates/uefi-gateway/tests/integration.rs` | integration-тесты |
| `webui/package.json` | SvelteKit project |
| `webui/svelte.config.js` | SvelteKit config (adapter-static) |
| `webui/vite.config.ts` | vite + proxy /api/v1 → gateway |
| `webui/tsconfig.json` | TypeScript config |
| `webui/src/app.html` | HTML shell |
| `webui/src/routes/+layout.svelte` | layout: header, навигация |
| `webui/src/routes/+page.svelte` | главная: session, image list |
| `webui/src/routes/image/[id]/+page.svelte` | дерево + детали + операции |
| `webui/src/routes/setup/+page.svelte` | setup: видимость + add-formset |
| `webui/src/lib/api.ts` | REST-клиент |
| `webui/src/lib/ws.ts` | WebSocket-клиент |
| `webui/src/lib/stores.ts` | svelte stores |
| `webui/src/lib/Tree.svelte` | tree-view компонент |
| `webui/src/lib/Details.svelte` | details panel |
| `docker/gateway.containerfile` | gateway образ (на базе rust-builder.containerfile, fedora:44) |
| `docker/webui.containerfile` | webui образ (fedora:44 + nginx/static) |
| `docker/docker-compose.yml` | engine + gateway + webui |

---

### Task 1: uefi-gateway скелет — Cargo.toml, config, main, health

**Files:**
- Modify: `Cargo.toml` (workspace)
- Create: `crates/uefi-gateway/Cargo.toml`
- Create: `crates/uefi-gateway/src/main.rs`
- Create: `crates/uefi-gateway/src/config.rs`
- Create: `crates/uefi-gateway/src/error.rs`

**Interfaces:**
- Consumes: `uefi-proto`, `axum`, `tonic`, `tokio`
- Produces:
  - `pub struct Config { listen: SocketAddr, sock_path: PathBuf }`
  - `pub fn load_config() -> Result<Config>`
  - `pub enum AppError { Auth, NotFound, BadRequest(String), Internal(String) }` с `IntoResponse`
  - HTTP server на `listen`, health endpoint `GET /api/v1/health` → `{ok:true}`

- [ ] **Step 1: Обновить workspace Cargo.toml**

Добавить в `Cargo.toml` workspace members: `"crates/uefi-gateway"`.

- [ ] **Step 2: Создать uefi-gateway/Cargo.toml**

`crates/uefi-gateway/Cargo.toml`:
```toml
[package]
name = "uefi-gateway"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
uefi-proto = { path = "../uefi-proto" }
axum = { version = "0.7", features = ["ws", "multipart"] }
axum-extra = { version = "0.9", features = ["cookie"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "fs"] }
tonic.workspace = true
tokio.workspace = true
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { workspace = true }
anyhow.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile = "3"
reqwest = { version = "0.12", features = ["json", "multipart"] }
```

- [ ] **Step 3: Создать config.rs**

`crates/uefi-gateway/src/config.rs`:
```rust
use std::net::SocketAddr;
use std::path::PathBuf;
use anyhow::Result;

pub struct Config {
    pub listen: SocketAddr,
    #[allow(dead_code)] // consumed in Task 2 (client::EngineClient::connect); remove then
    pub sock_path: PathBuf,
}

pub fn load_config() -> Result<Config> {
    let listen: SocketAddr = std::env::var("UEFIPATCHER_GATEWAY_LISTEN")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    let sock_path = std::env::var("UEFIPATCHER_SOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/run/uefipatcher.sock"));
    Ok(Config { listen, sock_path })
}
```

- [ ] **Step 4: Создать error.rs**

`crates/uefi-gateway/src/error.rs`:
```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
#[allow(dead_code)] // consumed in Task 4 (routes); remove then
pub enum AppError {
    Auth,
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, msg) = match self {
            AppError::Auth => (StatusCode::UNAUTHORIZED, "UNAUTHENTICATED", "invalid or missing session".into()),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "NOT_FOUND", m),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "INVALID_ARGUMENT", m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", m),
        };
        let body = json!({ "error": msg, "code": code });
        (status, axum::Json(body)).into_response()
    }
}

impl From<tonic::Status> for AppError {
    fn from(s: tonic::Status) -> Self {
        match s.code() {
            tonic::Code::Unauthenticated => AppError::Auth,
            tonic::Code::NotFound => AppError::NotFound(s.message().into()),
            tonic::Code::InvalidArgument => AppError::BadRequest(s.message().into()),
            _ => AppError::Internal(s.message().into()),
        }
    }
}
```

- [ ] **Step 5: Создать main.rs с health endpoint**

`crates/uefi-gateway/src/main.rs`:
```rust
mod config;
mod error;

use axum::routing::get;
use axum::Json;
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let app = axum::Router::new()
        .route("/api/v1/health", get(health))
        .layer(CorsLayer::very_permissive());
    let listener = tokio::net::TcpListener::bind(cfg.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}
```

- [ ] **Step 6: Проверить сборку**

Run: `cargo build -p uefi-gateway`
Expected: компиляция без ошибок

> **Note (clippy):** AGENTS.md rule 9 требует `cargo clippy -p uefi-gateway -- -D warnings`. В Task 1 `Config::sock_path` и `AppError` ещё не используются (потребуются в Task 2 и Task 4 соответственно), поэтому на них временно навешан `#[allow(dead_code)]` — удалить эти атрибуты при выполнении Task 2 (sock_path) и Task 4 (AppError). Аналогично `use axum::routing::get` без `post` (post понадобится в Task 4 — вернуть импорт тогда).

- [ ] **Step 7: Коммит**

```bash
git add Cargo.toml crates/uefi-gateway/
git commit -m "feat(gateway): scaffold uefi-gateway (axum, config, health, error mapping)"
```

---

### Task 2: gateway/client.rs — gRPC-клиент к движку

> ⚠️ **Дефекты (исправлены в плане):**
> - **A (зависимость):** `client.rs` импортирует `crate::session::SessionMap` (Task 3). Сначала выполни Task 3 (session.rs), затем этот task.
> - **B (unix-сокет):** `Endpoint::from_shared("unix://...")` НЕ работает в tonic 0.12. Использовать connector-override через `tower::service_fn` + `UnixStream` (как `uefi-cli/src/client.rs:30-39`, фикс cycle 3 `4a8ef43`).
> - **C (несуществующий RPC):** `add_setup_form_set` ссылается на `AddSetupFormSetRequest/Response` — этих типов **нет в proto** (добавляются в cycle 6 Task 7, ещё не выполнен). Метод отложен до cycle 6.
> - **D (dead_code):** `EngineClient`/`SessionMap` не используются до Task 4. Добавить crate-level `#![allow(dead_code)]` в main.rs, удалить в Task 4.
> - **E (deps):** для connector-override нужны `hyper-util` (tokio), `http`, `tower = "0.4"` (вместо 0.5 — tonic 0.12/axum 0.7 используют tower 0.4). Дополнить `Cargo.toml`.
> - **G (async):** `auth_req` вызывает `sessions.get_token()` (async, tokio Mutex) — `auth_req` должна быть `async fn`, а все call sites `Self::auth_req(...).await?`.
> - **H (clippy):** `insert`/`replace` имеют 8 аргументов → `clippy::too_many_arguments`. Добавить `#[allow(clippy::too_many_arguments)]` (дизайн sessions-per-call неизбежен для multi-session).

**Files:**
- Create: `crates/uefi-gateway/src/client.rs`
- Modify: `crates/uefi-gateway/src/main.rs`
- Modify: `crates/uefi-gateway/Cargo.toml` (deps для connector-override)

**Interfaces:**
- Consumes: `uefi-proto`, `tonic`, `config::Config`
- Produces:
  - `pub struct EngineClient { inner: EngineServiceClient<Channel> }`
  - `pub async fn connect(sock_path: &Path) -> Result<EngineClient>`
  - Методы-обёртки для всех RPC (с metadata auth): `create_session(name)`, `destroy_session`, `list_sessions`, `open_image`, `dump_tree`, `list_items`, `find_item`, `insert`, `remove`, `replace`, `rebuild`, `set_setup_visibility`, `save_image`, `extract_artifact`, `export_artifact`, `import_artifact`, `list_artifacts`. (`add_setup_form_set` отложен до cycle 6 — нет proto RPC)

- [ ] **Step 1: Реализовать client.rs**

`crates/uefi-gateway/src/client.rs`:
```rust
use std::path::Path;
use http::Uri;
use hyper_util::rt::TokioIo;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tower::service_fn;
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::*;
use crate::session::SessionMap;

pub struct EngineClient {
    inner: EngineServiceClient<Channel>,
}

impl EngineClient {
    pub async fn connect(sock_path: &Path) -> anyhow::Result<Self> {
        let sock_str = sock_path.display().to_string();
        let channel = Endpoint::try_from("http://localhost")?
            .connect_with_connector(service_fn(move |_: Uri| {
                let s = sock_str.clone();
                async move {
                    Ok::<_, std::io::Error>(TokioIo::new(
                        tokio::net::UnixStream::connect(s).await?,
                    ))
                }
            }))
            .await?;
        Ok(Self {
            inner: EngineServiceClient::new(channel),
        })
    }

    async fn auth_req<T>(sessions: &SessionMap, session_id: &str, body: T) -> Result<Request<T>, tonic::Status> {
        let token = sessions.get_token(session_id).await
            .ok_or_else(|| tonic::Status::unauthenticated("no token for session"))?;
        let mut req = Request::new(body);
        req.metadata_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
        req.metadata_mut().insert("x-session-id", session_id.parse().unwrap());
        Ok(req)
    }

    pub async fn create_session(&mut self, name: &str) -> anyhow::Result<(String, String)> {
        let r = self.inner.create_session(CreateSessionRequest { name: name.into() }).await?.into_inner();
        Ok((r.session_id, r.token))
    }
    pub async fn destroy_session(&mut self, id: &str) -> anyhow::Result<()> {
        self.inner.destroy_session(DestroySessionRequest { session_id: id.into() }).await?;
        Ok(())
    }
    pub async fn list_sessions(&mut self) -> anyhow::Result<Vec<SessionInfo>> {
        Ok(self.inner.list_sessions(ListSessionsRequest {}).await?.into_inner().sessions)
    }
    pub async fn open_image(&mut self, sessions: &SessionMap, session_id: &str, path: &str, mode: i32) -> anyhow::Result<OpenImageResponse> {
        let req = OpenImageRequest { session_id: session_id.into(), image_path: path.into(), mode };
        Ok(self.inner.open_image(Self::auth_req(sessions, session_id, req).await?).await?.into_inner())
    }
    pub async fn dump_tree(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, format: i32) -> anyhow::Result<String> {
        let req = DumpTreeRequest { image_id: image_id.into(), format };
        Ok(self.inner.dump_tree(Self::auth_req(sessions, session_id, req).await?).await?.into_inner().text)
    }
    pub async fn list_items(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, filter: &str) -> anyhow::Result<Vec<Item>> {
        let req = ListItemsRequest { image_id: image_id.into(), filter: filter.into() };
        Ok(self.inner.list_items(Self::auth_req(sessions, session_id, req).await?).await?.into_inner().items)
    }
    pub async fn find_item(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, target: &str) -> anyhow::Result<String> {
        let req = FindItemRequest { image_id: image_id.into(), target: target.into() };
        Ok(self.inner.find_item(Self::auth_req(sessions, session_id, req).await?).await?.into_inner().item_id)
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn insert(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, target: &str, ffs_path: &str, artifact_id: &str, mode: i32) -> anyhow::Result<String> {
        let req = InsertRequest { image_id: image_id.into(), target: target.into(), ffs_path: ffs_path.into(), artifact_id: artifact_id.into(), mode };
        Ok(self.inner.insert(Self::auth_req(sessions, session_id, req).await?).await?.into_inner().item_id)
    }
    pub async fn remove(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, target: &str) -> anyhow::Result<()> {
        let req = RemoveRequest { image_id: image_id.into(), target: target.into() };
        self.inner.remove(Self::auth_req(sessions, session_id, req).await?).await?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn replace(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, target: &str, data_path: &str, artifact_id: &str, body_only: bool) -> anyhow::Result<String> {
        let req = ReplaceRequest { image_id: image_id.into(), target: target.into(), ffs_path: data_path.into(), artifact_id: artifact_id.into(), body_only };
        Ok(self.inner.replace(Self::auth_req(sessions, session_id, req).await?).await?.into_inner().item_id)
    }
    pub async fn rebuild(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, target: &str) -> anyhow::Result<()> {
        let req = RebuildRequest { image_id: image_id.into(), target: target.into() };
        self.inner.rebuild(Self::auth_req(sessions, session_id, req).await?).await?;
        Ok(())
    }
    pub async fn set_setup_visibility(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, item_id: &str, visible: bool) -> anyhow::Result<()> {
        let req = SetSetupItemVisibilityRequest { image_id: image_id.into(), item_id: item_id.into(), visible };
        self.inner.set_setup_item_visibility(Self::auth_req(sessions, session_id, req).await?).await?;
        Ok(())
    }
    pub async fn save_image(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, output_path: &str) -> anyhow::Result<()> {
        let req = SaveImageRequest { image_id: image_id.into(), output_path: output_path.into() };
        self.inner.save_image(Self::auth_req(sessions, session_id, req).await?).await?;
        Ok(())
    }
    // add_setup_form_set отложен до cycle 6 (Task 7 добавляет AddSetupFormSet RPC в proto)
    pub async fn extract_artifact(&mut self, sessions: &SessionMap, session_id: &str, image_id: &str, target: &str, body_only: bool) -> anyhow::Result<String> {
        let req = ExtractArtifactRequest { image_id: image_id.into(), target: target.into(), body_only };
        Ok(self.inner.extract_artifact(Self::auth_req(sessions, session_id, req).await?).await?.into_inner().artifact_id)
    }
    pub async fn export_artifact(&mut self, sessions: &SessionMap, session_id: &str, artifact_id: &str, output_path: &str) -> anyhow::Result<()> {
        let req = ExportArtifactRequest { artifact_id: artifact_id.into(), output_path: output_path.into() };
        self.inner.export_artifact(Self::auth_req(sessions, session_id, req).await?).await?;
        Ok(())
    }
    pub async fn import_artifact(&mut self, sessions: &SessionMap, session_id: &str, file_path: &str) -> anyhow::Result<String> {
        let req = ImportArtifactRequest { session_id: session_id.into(), file_path: file_path.into() };
        Ok(self.inner.import_artifact(Self::auth_req(sessions, session_id, req).await?).await?.into_inner().artifact_id)
    }
    pub async fn list_artifacts(&mut self, sessions: &SessionMap, session_id: &str) -> anyhow::Result<Vec<ArtifactInfo>> {
        let req = ListArtifactsRequest { session_id: session_id.into() };
        Ok(self.inner.list_artifacts(Self::auth_req(sessions, session_id, req).await?).await?.into_inner().artifacts)
    }
}
```

- [ ] **Step 2: Подключить в main.rs**

`crates/uefi-gateway/src/main.rs`: добавить `#![allow(dead_code)]` (crate-level, удалить в Task 4), `mod client;` и `mod session;` (session — prerequisite из Task 3). Также обновить `Cargo.toml`: добавить `hyper-util = { version = "0.1", features = ["tokio"] }`, `http = "1"`, заменить `tower = "0.5"` → `tower = { version = "0.4", features = ["util"] }`.

- [ ] **Step 3: Проверить сборку + clippy**

Run: `cargo build -p uefi-gateway && cargo clippy -p uefi-gateway -- -D warnings`
Expected: компиляция без ошибок/предупреждений (модули dead_code до Task 4 — подавлены `#![allow(dead_code)]`)

- [ ] **Step 4: Коммит**

```bash
git add crates/uefi-gateway/src/client.rs crates/uefi-gateway/src/main.rs
git commit -m "feat(gateway): add gRPC client to engine (all EngineService methods with auth)"
```

---

### Task 3: gateway/session.rs — cookie-маппинг и token map

> ⚠️ **Порядок:** выполняется ПЕРЕД Task 2 (client.rs импортирует `crate::session::SessionMap`). См. дефект A в Task 2.
>
> ⚠️ **Дефект F (исправлен):** в axum-extra 0.9.6 модуль `cookie` находится по пути `axum_extra::extract::cookie` (НЕ `axum_extra::cookie`). Импорты: `use axum_extra::extract::cookie::{Cookie, SameSite};`. `CookieJar` остаётся `axum_extra::extract::CookieJar`.

**Files:**
- Create: `crates/uefi-gateway/src/session.rs`

**Interfaces:**
- Consumes: `axum-extra::extract::CookieJar`
- Produces:
  - `pub struct SessionMap { map: Arc<Mutex<HashMap<String, String>>> }`
  - `SessionMap::new()`, `SessionMap::insert(session_id, token)`, `SessionMap::get_token(session_id) -> Option<String>`, `SessionMap::remove(session_id)`
  - `pub fn extract_session_id(jar: &CookieJar) -> Option<String>`
  - `pub fn make_session_cookie(session_id: &str) -> Cookie`

- [ ] **Step 1: Реализовать session.rs**

`crates/uefi-gateway/src/session.rs`:
```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};

pub struct SessionMap {
    map: Arc<Mutex<HashMap<String, String>>>,
}

impl SessionMap {
    pub fn new() -> Self {
        Self { map: Arc::new(Mutex::new(HashMap::new())) }
    }
    pub async fn insert(&self, session_id: String, token: String) {
        self.map.lock().await.insert(session_id, token);
    }
    pub async fn get_token(&self, session_id: &str) -> Option<String> {
        self.map.lock().await.get(session_id).cloned()
    }
    pub async fn remove(&self, session_id: &str) {
        self.map.lock().await.remove(session_id);
    }
}

pub fn extract_session_id(jar: &CookieJar) -> Option<String> {
    jar.get("uefipatcher_session").map(|c| c.value().to_string())
}

pub fn make_session_cookie(session_id: &str) -> Cookie<'static> {
    Cookie::build(("uefipatcher_session", session_id.to_string()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .path("/")
        .build()
}

pub fn make_image_cookie(image_id: &str) -> Cookie<'static> {
    Cookie::build(("uefipatcher_image", image_id.to_string()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .path("/")
        .build()
}
```

- [ ] **Step 2: Подключить в main.rs**

`crates/uefi-gateway/src/main.rs`: добавить `mod session;`

- [ ] **Step 3: Коммит**

```bash
git add crates/uefi-gateway/src/session.rs crates/uefi-gateway/src/main.rs
git commit -m "feat(gateway): add session module (cookie mapping, in-memory token map)"
```

---

### Task 4: gateway/routes/ — REST эндпоинты (session, image, edit, setup, upload)

> ⚠️ **Дефекты (исправлены в плане):**
> - **I (Query-тип):** `items` handler использовал `Query<serde_json::Map<...>>` — хрупко/несовместимо с `serde_urlencoded`. Заменить на структуру `ItemsQuery { filter: Option<String> }`.
> - **C⁻ (наследник defect C):** route `/add-formset` + `setup::add_formset` вызывают `add_setup_form_set` (отложен в Task 2 до cycle 6). Убрать route и handler.
> - **J (dead_code):** `session::extract_image_id` не используется ни одним route (image_id берётся из URL `Path`, не из cookie). Удалить из session.rs.
> - **D⁻ (cleanup):** убрать `#![allow(dead_code)]` из main.rs (модули теперь wired) и per-item `#[allow(dead_code)]` из config.rs (`sock_path` теперь читается) и error.rs (`AppError` теперь используется).
> - **K (тип ошибки client):** RPC-методы client (кроме `connect`) возвращают `anyhow::Result<T>`, но routes вызывают `.map_err(AppError::from)` — а `AppError` имеет только `From<tonic::Status>`. Изменить возврат RPC-методов на `Result<T, tonic::Status>` (сохраняет gRPC-код маппинг 401/404/400). `connect` остаётся `anyhow::Result` (io-ошибки, только в main). Routes `create`/`list` тоже использовать `AppError::from` (не `Internal(e.to_string())`).
> - **L (json!):** `json!({ "path": path.display() })` — `path::Display` не реализует `Serialize`. Использовать `path.display().to_string()`.
> - **M (clippy let_underscore_future):** `let _ = tokio::fs::remove_file(&out_path);` не await-ит Future (и НЕ удаляет файл — баг). Исправить: `let _ = tokio::fs::remove_file(&out_path).await;`.

**Files:**
- Create: `crates/uefi-gateway/src/routes/mod.rs`
- Create: `crates/uefi-gateway/src/routes/session.rs`
- Create: `crates/uefi-gateway/src/routes/image.rs`
- Create: `crates/uefi-gateway/src/routes/edit.rs`
- Create: `crates/uefi-gateway/src/routes/setup.rs`
- Create: `crates/uefi-gateway/src/routes/upload.rs`
- Modify: `crates/uefi-gateway/src/main.rs` (router)

**Interfaces:**
- Consumes: `client::EngineClient`, `session::SessionMap`, `error::AppError`, `axum`
- Produces: все REST handlers из спеки (раздел API/Контракты)

- [ ] **Step 1: Создать routes/mod.rs (shared state + router)**

`crates/uefi-gateway/src/routes/mod.rs`:
```rust
pub mod session;
pub mod image;
pub mod edit;
pub mod setup;
pub mod upload;
pub mod artifact;

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::client::EngineClient;
use crate::session::SessionMap;

#[derive(Clone)]
pub struct AppState {
    pub client: Arc<Mutex<EngineClient>>,
    pub sessions: Arc<SessionMap>,
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/api/v1/health", axum::routing::get(|| async { axum::Json(serde_json::json!({"ok": true})) }))
        .route("/api/v1/session", axum::routing::post(session::create).delete(session::destroy))
        .route("/api/v1/sessions", axum::routing::get(session::list))
        .route("/api/v1/image/open", axum::routing::post(image::open))
        .route("/api/v1/image/upload", axum::routing::post(upload::upload))
        .route("/api/v1/image/:id/dump", axum::routing::get(image::dump))
        .route("/api/v1/image/:id/items", axum::routing::get(image::items))
        .route("/api/v1/image/:id/find", axum::routing::get(image::find))
        .route("/api/v1/image/:id/save", axum::routing::post(image::save))
        .route("/api/v1/image/:id/download", axum::routing::get(upload::download))
        .route("/api/v1/image/:id/insert", axum::routing::post(edit::insert))
        .route("/api/v1/image/:id/remove", axum::routing::post(edit::remove))
        .route("/api/v1/image/:id/replace", axum::routing::post(edit::replace))
        .route("/api/v1/image/:id/rebuild", axum::routing::post(edit::rebuild))
        .route("/api/v1/image/:id/set-visibility", axum::routing::post(setup::set_visibility))
        .route("/api/v1/image/:id/setup-items", axum::routing::get(setup::list_items))
        .route("/api/v1/image/:id/extract", axum::routing::post(artifact::extract))
        .route("/api/v1/artifact/:id/export", axum::routing::post(artifact::export))
        .route("/api/v1/artifact/import", axum::routing::post(artifact::import))
        .route("/api/v1/artifacts", axum::routing::get(artifact::list))
        .with_state(state)
}
```

- [ ] **Step 2: Реализовать routes/session.rs**

`crates/uefi-gateway/src/routes/session.rs`:
```rust
use axum::extract::State;
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::error::AppError;
use crate::session::{make_session_cookie, extract_session_id};
use super::AppState;

#[derive(Deserialize, Default)]
pub struct CreateBody {
    #[serde(default)]
    pub name: Option<String>,
}

pub async fn create(State(state): State<AppState>, Json(body): Json<CreateBody>) -> Result<(CookieJar, Json<Value>), AppError> {
    let mut c = state.client.lock().await;
    let name = body.name.filter(|n| !n.is_empty())
        .unwrap_or_else(|| std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default());
    let (sid, tok) = c.create_session(&name).await.map_err(|e| AppError::Internal(e.to_string()))?;
    state.sessions.insert(sid.clone(), tok).await;
    let jar = CookieJar::new().add(make_session_cookie(&sid));
    Ok((jar, Json(json!({ "session_id": sid }))))
}

pub async fn destroy(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.destroy_session(&sid).await.map_err(AppError::from)?;
    state.sessions.remove(&sid).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Value>, AppError> {
    let _sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let rows = c.list_sessions().await.map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(json!({ "sessions": rows })))
}
```

- [ ] **Step 3: Реализовать routes/image.rs**

`crates/uefi-gateway/src/routes/image.rs`:
```rust
use axum::extract::{Path, Query, State};
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::error::AppError;
use crate::session::{extract_session_id, make_image_cookie};
use super::AppState;

#[derive(Deserialize)]
pub struct OpenBody { pub path: String, pub mode: String }

pub async fn open(State(state): State<AppState>, jar: CookieJar, Json(body): Json<OpenBody>) -> Result<(CookieJar, Json<Value>), AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mode = match body.mode.as_str() { "read" => 0, "write" => 1, _ => return Err(AppError::BadRequest("mode must be read|write".into())) };
    let mut c = state.client.lock().await;
    let r = c.open_image(&state.sessions, &sid, &body.path, mode).await.map_err(AppError::from)?;
    let jar = jar.add(make_image_cookie(&r.image_id));
    Ok((jar, Json(json!({ "image_id": r.image_id, "root_guid": r.root_guid }))))
}

#[derive(Deserialize)]
pub struct DumpQuery { pub format: Option<String> }

pub async fn dump(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Query(q): Query<DumpQuery>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let fmt = if q.format.as_deref() == Some("tsv") { 1 } else { 0 };
    let mut c = state.client.lock().await;
    let text = c.dump_tree(&state.sessions, &sid, &id, fmt).await.map_err(AppError::from)?;
    Ok(Json(json!({ "text": text })))
}

#[derive(Deserialize)]
pub struct ItemsQuery { pub filter: Option<String> }

pub async fn items(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Query(q): Query<ItemsQuery>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let filter = q.filter.as_deref().unwrap_or("");
    let mut c = state.client.lock().await;
    let items = c.list_items(&state.sessions, &sid, &id, filter).await.map_err(AppError::from)?;
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
pub struct FindQuery { pub target: String }

pub async fn find(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Query(q): Query<FindQuery>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let item_id = c.find_item(&state.sessions, &sid, &id, &q.target).await.map_err(AppError::from)?;
    Ok(Json(json!({ "item_id": item_id })))
}

#[derive(Deserialize)]
pub struct SaveBody { pub output_path: String }

pub async fn save(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<SaveBody>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.save_image(&state.sessions, &sid, &id, &body.output_path).await.map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}
```

- [ ] **Step 4: Реализовать routes/edit.rs, routes/setup.rs, routes/upload.rs**

`crates/uefi-gateway/src/routes/edit.rs`:
```rust
use axum::extract::{Path, State};
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::error::AppError;
use crate::session::extract_session_id;
use super::AppState;

#[derive(Deserialize)]
pub struct InsertBody { pub target: String, pub ffs_path: Option<String>, pub artifact_id: Option<String>, pub mode: String }
pub async fn insert(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<InsertBody>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mode = match body.mode.as_str() { "into" => 0, "before" => 1, "after" => 2, _ => return Err(AppError::BadRequest("mode must be into|before|after".into())) };
    let mut c = state.client.lock().await;
    let item_id = c.insert(&state.sessions, &sid, &id, &body.target, body.ffs_path.as_deref().unwrap_or(""), body.artifact_id.as_deref().unwrap_or(""), mode).await.map_err(AppError::from)?;
    Ok(Json(json!({ "item_id": item_id })))
}

#[derive(Deserialize)]
pub struct TargetBody { pub target: String }
pub async fn remove(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<TargetBody>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.remove(&state.sessions, &sid, &id, &body.target).await.map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ReplaceBody { pub target: String, pub data_path: Option<String>, pub artifact_id: Option<String>, pub body_only: bool }
pub async fn replace(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<ReplaceBody>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let item_id = c.replace(&state.sessions, &sid, &id, &body.target, body.data_path.as_deref().unwrap_or(""), body.artifact_id.as_deref().unwrap_or(""), body.body_only).await.map_err(AppError::from)?;
    Ok(Json(json!({ "item_id": item_id })))
}

pub async fn rebuild(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<TargetBody>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.rebuild(&state.sessions, &sid, &id, &body.target).await.map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}
```

`crates/uefi-gateway/src/routes/setup.rs`:
```rust
use axum::extract::{Path, State};
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::error::AppError;
use crate::session::extract_session_id;
use super::AppState;

#[derive(Deserialize)]
pub struct VisibilityBody { pub item_id: String, pub visible: bool }
pub async fn set_visibility(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<VisibilityBody>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.set_setup_visibility(&state.sessions, &sid, &id, &body.item_id, body.visible).await.map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn list_items(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let items = c.list_items(&state.sessions, &sid, &id, "").await.map_err(AppError::from)?;
    let setup: Vec<_> = items.into_iter().filter(|i| i.r#type == 67).collect();
    Ok(Json(json!({ "items": setup })))
}
// add_formset отложен до cycle 6 (add_setup_form_set defer, defect C)
```

`crates/uefi-gateway/src/routes/upload.rs`:
```rust
use axum::extract::{Multipart, Path, State};
use axum::body::Body;
use axum::http::header;
use axum::response::Response;
use axum::Json;
use axum_extra::extract::CookieJar;
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;
use crate::error::AppError;
use crate::session::extract_session_id;
use super::AppState;

pub async fn upload(State(_state): State<AppState>, _jar: CookieJar, mut multipart: Multipart) -> Result<Json<Value>, AppError> {
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        if field.name() == Some("file") {
            let data = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
            let id = Uuid::new_v4();
            let path = PathBuf::from(format!("/tmp/uefipatcher-upload-{id}.bin"));
            tokio::fs::write(&path, &data).await.map_err(|e| AppError::Internal(e.to_string()))?;
            return Ok(Json(json!({ "path": path.display().to_string() })));
        }
    }
    Err(AppError::BadRequest("no file field in multipart".into()))
}

pub async fn download(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Response<Body>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let out_path = format!("/tmp/uefipatcher-download-{}.bin", Uuid::new_v4());
    let mut c = state.client.lock().await;
    c.save_image(&state.sessions, &sid, &id, &out_path).await.map_err(AppError::from)?;
    let data = tokio::fs::read(&out_path).await.map_err(|e| AppError::Internal(e.to_string()))?;
    let _ = tokio::fs::remove_file(&out_path).await;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"patched.bin\"")
        .body(Body::from(data))
        .unwrap())
}
```

`crates/uefi-gateway/src/routes/artifact.rs`:
```rust
use axum::extract::{Multipart, Path, Query, State};
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;
use crate::error::AppError;
use crate::session::extract_session_id;
use super::AppState;

#[derive(Deserialize)]
pub struct ExtractBody { pub target: String, pub body_only: bool }
pub async fn extract(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<ExtractBody>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let artifact_id = c.extract_artifact(&state.sessions, &sid, &id, &body.target, body.body_only).await.map_err(AppError::from)?;
    Ok(Json(json!({ "artifact_id": artifact_id })))
}

#[derive(Deserialize)]
pub struct ExportBody { pub output_path: String }
pub async fn export(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<ExportBody>) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.export_artifact(&state.sessions, &sid, &id, &body.output_path).await.map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn import(State(state): State<AppState>, jar: CookieJar, mut multipart: Multipart) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut file_path: Option<String> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        if field.name() == Some("file") {
            let data = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
            let path = PathBuf::from(format!("/tmp/uefipatcher-import-{}.bin", Uuid::new_v4()));
            tokio::fs::write(&path, &data).await.map_err(|e| AppError::Internal(e.to_string()))?;
            file_path = Some(path.display().to_string());
        }
    }
    let fp = file_path.ok_or_else(|| AppError::BadRequest("no file field in multipart".into()))?;
    let mut c = state.client.lock().await;
    let artifact_id = c.import_artifact(&state.sessions, &sid, &fp).await.map_err(AppError::from)?;
    Ok(Json(json!({ "artifact_id": artifact_id })))
}

#[derive(Deserialize)]
pub struct ListQuery { pub session_id: Option<String> }
pub async fn list(State(state): State<AppState>, jar: CookieJar, Query(q): Query<ListQuery>) -> Result<Json<Value>, AppError> {
    let sid = q.session_id.or_else(|| extract_session_id(&jar)).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let artifacts = c.list_artifacts(&state.sessions, &sid).await.map_err(AppError::from)?;
    Ok(Json(json!({ "artifacts": artifacts })))
}
```

- [ ] **Step 5: Обновить main.rs — использовать router**

`crates/uefi-gateway/src/main.rs`:
```rust
mod config;
mod error;
mod client;
mod session;
mod routes;

use std::sync::Arc;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let engine = client::EngineClient::connect(&cfg.sock_path).await?;
    let state = routes::AppState {
        client: Arc::new(tokio::sync::Mutex::new(engine)),
        sessions: Arc::new(session::SessionMap::new()),
    };
    let app = routes::router(state).layer(CorsLayer::very_permissive());
    let listener = tokio::net::TcpListener::bind(cfg.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 6: Проверить сборку**

Run: `cargo build -p uefi-gateway`
Expected: компиляция без ошибок

- [ ] **Step 7: Коммит**

```bash
git add crates/uefi-gateway/src/routes/ crates/uefi-gateway/src/main.rs
git commit -m "feat(gateway): add REST routes (session/image/edit/setup/upload/download)"
```

---

### Task 5: gateway/ws.rs — WebSocket для streaming dump

**Files:**
- Create: `crates/uefi-gateway/src/ws.rs`
- Modify: `crates/uefi-gateway/src/routes/mod.rs` (добавить WS route)

- [ ] **Step 1: Реализовать ws.rs**

`crates/uefi-gateway/src/ws.rs`:
```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use axum::extract::{Path, State, Query};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use crate::error::AppError;
use crate::session::extract_session_id;
use super::AppState;

#[derive(Deserialize)]
pub struct WsQuery { pub format: Option<String> }

pub async fn dump_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Query(q): Query<WsQuery>,
) -> Result<axum::response::Response, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let fmt = if q.format.as_deref() == Some("tsv") { 1 } else { 0 };
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, sid, id, fmt)))
}

async fn handle_ws(mut socket: WebSocket, state: AppState, sid: String, image_id: String, fmt: i32) {
    let mut c = state.client.lock().await;
    match c.dump_tree(&state.sessions, &sid, &image_id, fmt).await {
        Ok(text) => {
            let _ = socket.send(Message::Text(text)).await;
        }
        Err(e) => {
            let _ = socket.send(Message::Text(format!("error: {e}"))).await;
        }
    }
    let _ = socket.close(None).await;
}
```

- [ ] **Step 2: Добавить WS route в routes/mod.rs**

В `crates/uefi-gateway/src/routes/mod.rs` добавить `pub mod ws;` и route:
```rust
.route("/api/v1/image/:id/dump/ws", axum::routing::get(ws::dump_ws))
```

- [ ] **Step 3: Коммит**

```bash
git add crates/uefi-gateway/src/ws.rs crates/uefi-gateway/src/routes/mod.rs
git commit -m "feat(gateway): add WebSocket endpoint for streaming dump"
```

---

### Task 6: Mock-сервер и integration-тесты gateway

**Files:**
- Create: `crates/uefi-gateway/tests/mock_server.rs`
- Create: `crates/uefi-gateway/tests/integration.rs`

- [ ] **Step 1: Создать mock_server.rs (копия из uefi-cli/uefi-tUI)**

`crates/uefi-gateway/tests/mock_server.rs` — копировать `crates/uefi-cli/tests/mock_server.rs` (MockEngine с заглушками EngineService, `start_mock(sock)`).

- [ ] **Step 2: Написать integration-тесты**

`crates/uefi-gateway/tests/integration.rs`:
```rust
mod mock_server;

use reqwest::StatusCode;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

async fn setup_gateway() -> (TempDir, String) {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    mock_server::start_mock(&sock).await;
    std::env::set_var("UEFIPATCHER_SOCK", &sock);
    std::env::set_var("UEFIPATCHER_GATEWAY_LISTEN", "127.0.0.1:18080");
    let listen = "127.0.0.1:18080".to_string();
    tokio::spawn(async {
        let _ = uefi_gateway::main_inner().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    (td, listen)
}

#[tokio::test]
async fn health() {
    let (_td, listen) = setup_gateway().await;
    let resp = reqwest::get(format!("http://{listen}/api/v1/health")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_session_and_list() {
    let (_td, listen) = setup_gateway().await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("http://{listen}/api/v1/session"))
        .json(&json!({}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookies = resp.cookies().collect::<Vec<_>>();
    assert!(cookies.iter().any(|c| c.name() == "uefipatcher_session"));
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["session_id"].as_str().is_some());

    let resp = client.get(format!("http://{listen}/api/v1/sessions"))
        .header("cookie", "uefipatcher_session=".to_string() + body["session_id"].as_str().unwrap())
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
```

Добавить в `crates/uefi-gateway/src/main.rs`:
```rust
pub async fn main_inner() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let engine = client::EngineClient::connect(&cfg.sock_path).await?;
    let state = routes::AppState {
        client: Arc::new(tokio::sync::Mutex::new(engine)),
        sessions: Arc::new(session::SessionMap::new()),
    };
    let app = routes::router(state).layer(CorsLayer::very_permissive());
    let listener = tokio::net::TcpListener::bind(cfg.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```
И в `main()` просто вызвать `main_inner().await`.

- [ ] **Step 3: Запустить integration-тесты**

Run: `cargo test -p uefi-gateway --test integration`
Expected: PASS

- [ ] **Step 4: Коммит**

```bash
git add crates/uefi-gateway/tests/ crates/uefi-gateway/src/main.rs
git commit -m "test(gateway): add mock server and integration tests (health, session flow)"
```

---

### Task 7: webui/ скелет — SvelteKit init, api.ts, stores, layout

**Files:**
- Create: `webui/package.json`
- Create: `webui/svelte.config.js`
- Create: `webui/vite.config.ts`
- Create: `webui/tsconfig.json`
- Create: `webui/src/app.html`
- Create: `webui/src/routes/+layout.svelte`
- Create: `webui/src/routes/+page.svelte`
- Create: `webui/src/lib/api.ts`
- Create: `webui/src/lib/stores.ts`

- [ ] **Step 1: Инициализировать SvelteKit**

```bash
cd webui && npm create svelte@latest . -- --template skeleton --types typescript --no-prettier --no-eslint --no-playwright --no-vitest
```
(или вручную создать файлы ниже)

- [ ] **Step 2: package.json**

`webui/package.json`:
```json
{
  "name": "uefipatcher-webui",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview",
    "check": "svelte-check"
  },
  "devDependencies": {
    "@sveltejs/adapter-static": "^3.0.0",
    "@sveltejs/kit": "^2.0.0",
    "@sveltejs/vite-plugin-svelte": "^4.0.0",
    "svelte": "^5.0.0",
    "svelte-check": "^4.0.0",
    "typescript": "^5.5.0",
    "vite": "^6.0.0"
  }
}
```

- [ ] **Step 3: svelte.config.js (SPA mode)**

`webui/svelte.config.js`:
```javascript
import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({ fallback: 'index.html' }),
  },
};
```

- [ ] **Step 4: vite.config.ts (proxy /api/v1 → gateway)**

`webui/vite.config.ts`:
```typescript
import { sveltekit } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    proxy: {
      '/api/v1': 'http://localhost:8080',
      '/api/v1/image/:id/dump/ws': { target: 'ws://localhost:8080', ws: true },
    },
  },
});
```

- [ ] **Step 5: tsconfig.json**

`webui/tsconfig.json`:
```json
{
  "extends": "./.svelte-kit/tsconfig.json",
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "moduleResolution": "bundler"
  }
}
```

- [ ] **Step 6: app.html**

`webui/src/app.html`:
```html
<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>UEFIPatcher</title>
    %sveltekit.head%
</head>
<body data-sveltekit-preload-data="hover">
    <div style="display: contents">%sveltekit.body%</div>
</body>
</html>
```

- [ ] **Step 7: lib/stores.ts**

`webui/src/lib/stores.ts`:
```typescript
import { writable } from 'svelte/store';

export const sessionStore = writable<string | null>(null);
export const imageStore = writable<string | null>(null);
export const treeStore = writable<TreeNode[]>([]);
export const selectedStore = writable<TreeNode | null>(null);

export interface TreeNode {
    path: string;
    type: number;
    subtype: number;
    guid: string;
    offset: number;
    size: number;
    name: string;
}
```

- [ ] **Step 8: lib/api.ts**

`webui/src/lib/api.ts`:
```typescript
import { imageStore, sessionStore, treeStore, type TreeNode } from './stores';

const API = '/api/v1';

async function req(path: string, opts: RequestInit = {}): Promise<any> {
    const resp = await fetch(`${API}${path}`, { ...opts, headers: { 'Content-Type': 'application/json', ...opts.headers } });
    if (!resp.ok) {
        const body = await resp.json().catch(() => ({ error: resp.statusText }));
        throw new Error(body.error || resp.statusText);
    }
    return resp.json();
}

export async function createSession() {
    const r = await req('/session', { method: 'POST', body: '{}' });
    sessionStore.set(r.session_id);
    return r;
}

export async function destroySession() {
    await req('/session', { method: 'DELETE' });
    sessionStore.set(null);
    imageStore.set(null);
    treeStore.set([]);
}

export async function openImage(path: string, mode: string = 'read') {
    const r = await req('/image/open', { method: 'POST', body: JSON.stringify({ path, mode }) });
    imageStore.set(r.image_id);
    return r;
}

export async function uploadImage(file: File): Promise<string> {
    const form = new FormData();
    form.append('file', file);
    const resp = await fetch(`${API}/image/upload`, { method: 'POST', body: form });
    if (!resp.ok) throw new Error('upload failed');
    const r = await resp.json();
    return r.path;
}

export async function dumpTree(imageId: string, format: string = 'text') {
    return req(`/image/${imageId}/dump?format=${format}`);
}

export async function listItems(imageId: string, filter: string = '') {
    return req(`/image/${imageId}/items?filter=${filter}`);
}

export async function findItem(imageId: string, target: string) {
    return req(`/image/${imageId}/find?target=${encodeURIComponent(target)}`);
}

export async function insert(imageId: string, target: string, ffsPath: string, mode: string = 'into') {
    return req(`/image/${imageId}/insert`, { method: 'POST', body: JSON.stringify({ target, ffs_path: ffsPath, mode }) });
}

export async function remove(imageId: string, target: string) {
    return req(`/image/${imageId}/remove`, { method: 'POST', body: JSON.stringify({ target }) });
}

export async function replace(imageId: string, target: string, dataPath: string, bodyOnly: boolean = false) {
    return req(`/image/${imageId}/replace`, { method: 'POST', body: JSON.stringify({ target, data_path: dataPath, body_only: bodyOnly }) });
}

export async function rebuild(imageId: string, target: string) {
    return req(`/image/${imageId}/rebuild`, { method: 'POST', body: JSON.stringify({ target }) });
}

export async function setVisibility(imageId: string, itemId: string, visible: boolean) {
    return req(`/image/${imageId}/set-visibility`, { method: 'POST', body: JSON.stringify({ item_id: itemId, visible }) });
}

export async function saveImage(imageId: string, outputPath: string) {
    return req(`/image/${imageId}/save`, { method: 'POST', body: JSON.stringify({ output_path: outputPath }) });
}

export async function downloadImage(imageId: string): Promise<Blob> {
    const resp = await fetch(`${API}/image/${imageId}/download`);
    if (!resp.ok) throw new Error('download failed');
    return resp.blob();
}

export async function addFormSet(imageId: string, schemaJson: string, targetFfsGuid: string = '') {
    return req(`/image/${imageId}/add-formset`, { method: 'POST', body: JSON.stringify({ schema_json: schemaJson, target_ffs_guid: targetFfsGuid }) });
}
```

- [ ] **Step 9: +layout.svelte и +page.svelte**

`webui/src/routes/+layout.svelte`:
```svelte
<script lang="ts">
    import { sessionStore } from '$lib/stores';
    let { children } = $props();
</script>

<header>
    <h1>UEFIPatcher</h1>
    {#if $sessionStore}
        <span>Session: {$sessionStore.slice(0, 8)}...</span>
    {/if}
</header>
<main>{$children}</main>
```

`webui/src/routes/+page.svelte`:
```svelte
<script lang="ts">
    import { createSession, openImage, uploadImage } from '$lib/api';
    import { sessionStore, imageStore } from '$lib/stores';
    import { goto } from '$app/navigation';

    let filePath = $state('');
    let mode = $state('read');
    let uploading = $state(false);

    async function handleUpload(e: Event) {
        const input = e.target as HTMLInputElement;
        if (!input.files?.[0]) return;
        uploading = true;
        try {
            const path = await uploadImage(input.files[0]);
            await openImage(path, 'write');
            goto(`/image/${$imageStore}`);
        } finally {
            uploading = false;
        }
    }
</script>

<div>
    {#if !$sessionStore}
        <button onclick={createSession}>Create Session</button>
    {:else}
        <h2>Open Image</h2>
        <input type="file" onchange={handleUpload} disabled={uploading} />
        {#if uploading}<p>Uploading...</p>{/if}
        <hr />
        <input bind:value={filePath} placeholder="/path/to/bios.bin" />
        <select bind:value={mode}><option value="read">read</option><option value="write">write</option></select>
        <button onclick={async () => { await openImage(filePath, mode); goto(`/image/${$imageStore}`); }}>Open</button>
    {/if}
</div>
```

- [ ] **Step 10: Установить зависимости и проверить**

```bash
cd webui && npm install && npm run check
```
Expected: без ошибок

- [ ] **Step 11: Коммит**

```bash
git add webui/
git commit -m "feat(webui): scaffold SvelteKit (api.ts, stores, layout, upload page)"
```

---

### Task 8: webui/lib/Tree.svelte + Details.svelte — tree-view и детали

**Files:**
- Create: `webui/src/lib/Tree.svelte`
- Create: `webui/src/lib/Details.svelte`

- [ ] **Step 1: Tree.svelte**

`webui/src/lib/Tree.svelte`:
```svelte
<script lang="ts">
    import type { TreeNode } from './stores';
    import { selectedStore } from './stores';

    let { items, level = 0 }: { items: TreeNode[]; level?: number } = $props();

    function select(node: TreeNode) {
        selectedStore.set(node);
    }
</script>

<ul class="tree">
    {#each items as node (node.path)}
        <li class="tree-node" style="padding-left: {level * 20}px" onclick={() => select(node)}>
            <span class="icon"></span>
            <span class="name">{node.name || node.path}</span>
            <span class="guid">{node.guid}</span>
        </li>
    {/each}
</ul>

<style>
    .tree { list-style: none; padding: 0; margin: 0; }
    .tree-node { cursor: pointer; padding: 2px 4px; }
    .tree-node:hover { background: #333; }
    .icon { margin-right: 4px; }
    .name { font-weight: 500; }
    .guid { color: #666; font-size: 0.85em; margin-left: 8px; }
</style>
```

- [ ] **Step 2: Details.svelte**

`webui/src/lib/Details.svelte`:
```svelte
<script lang="ts">
    import { selectedStore } from './stores';
    let node = $derived($selectedStore);
</script>

<div class="details">
    {#if node}
        <h3>Details</h3>
        <table>
            <tr><td>Path</td><td>{node.path}</td></tr>
            <tr><td>Type</td><td>{node.type}</td></tr>
            <tr><td>Subtype</td><td>0x{node.subtype.toString(16)}</td></tr>
            <tr><td>GUID</td><td>{node.guid || '(none)'}</td></tr>
            <tr><td>Offset</td><td>0x{node.offset.toString(16)}</td></tr>
            <tr><td>Size</td><td>0x{node.size.toString(16)}</td></tr>
            <tr><td>Name</td><td>{node.name}</td></tr>
        </table>
    {:else}
        <p>Select a node from the tree</p>
    {/if}
</div>

<style>
    .details { padding: 12px; }
    table { border-collapse: collapse; }
    td { padding: 4px 8px; border: 1px solid #444; }
    td:first-child { color: #888; font-weight: 500; }
</style>
```

- [ ] **Step 3: Коммит**

```bash
git add webui/src/lib/Tree.svelte webui/src/lib/Details.svelte
git commit -m "feat(webui): add Tree and Details components"
```

---

### Task 9: webui/image/[id]/+page.svelte — страница образа с операциями

**Files:**
- Create: `webui/src/routes/image/[id]/+page.svelte`

- [ ] **Step 1: Реализовать страницу образа**

`webui/src/routes/image/[id]/+page.svelte`:
```svelte
<script lang="ts">
    import { page } from '$app/stores';
    import { dumpTree, listItems, insert, remove, rebuild, saveImage, downloadImage } from '$lib/api';
    import { treeStore, selectedStore, type TreeNode } from '$lib/stores';
    import Tree from '$lib/Tree.svelte';
    import Details from '$lib/Details.svelte';

    let imageId = $derived($page.params.id);
    let items = $state<TreeNode[]>([]);
    let target = $state('');
    let ffsPath = $state('');
    let status = $state('');

    async function load() {
        const r = await listItems(imageId);
        items = r.items;
        treeStore.set(r.items);
    }

    async function doInsert() {
        await insert(imageId, target, ffsPath, 'into');
        status = 'inserted';
        await load();
    }
    async function doRemove() {
        await remove(imageId, target);
        status = 'removed';
        await load();
    }
    async function doRebuild() {
        await rebuild(imageId, target);
        status = 'rebuilt';
    }
    async function doSave() {
        const out = `/tmp/patched-${Date.now()}.bin`;
        await saveImage(imageId, out);
        status = `saved to ${out}`;
    }
    async function doDownload() {
        const blob = await downloadImage(imageId);
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url; a.download = 'patched.bin'; a.click();
        URL.revokeObjectURL(url);
    }

    $effect(() => { load(); });
</script>

<div class="image-page">
    <div class="tree-panel">
        <h2>Tree</h2>
        <Tree items={items} />
    </div>
    <div class="right-panel">
        <Details />
        <div class="ops">
            <h3>Operations</h3>
            <input bind:value={target} placeholder="target (GUID/path)" />
            <input bind:value={ffsPath} placeholder="ffs path (for insert)" />
            <button onclick={doInsert}>Insert</button>
            <button onclick={doRemove}>Remove</button>
            <button onclick={doRebuild}>Rebuild</button>
            <hr />
            <button onclick={doSave}>Save</button>
            <button onclick={doDownload}>Download</button>
            {#if status}<p class="status">{status}</p>{/if}
        </div>
    </div>
</div>

<style>
    .image-page { display: flex; gap: 12px; height: calc(100vh - 60px); }
    .tree-panel { width: 40%; overflow-y: auto; border-right: 1px solid #444; padding: 8px; }
    .right-panel { flex: 1; padding: 8px; }
    .ops { margin-top: 16px; }
    input { display: block; margin: 4px 0; width: 100%; }
    button { margin: 4px 4px 4px 0; padding: 4px 12px; }
    .status { color: green; }
</style>
```

- [ ] **Step 2: Коммит**

```bash
git add webui/src/routes/image/
git commit -m "feat(webui): add image page (tree + details + operations + save/download)"
```

---

### Task 10: webui/setup/+page.svelte — setup (видимость + add-formset)

**Files:**
- Create: `webui/src/routes/setup/+page.svelte`

- [ ] **Step 1: Реализовать setup-страницу**

`webui/src/routes/setup/+page.svelte`:
```svelte
<script lang="ts">
    import { page } from '$app/stores';
    import { listItems, setVisibility, addFormSet } from '$lib/api';
    import { imageStore } from '$lib/stores';
    import type { TreeNode } from '$lib/stores';

    let setupItems = $state<TreeNode[]>([]);
    let schemaText = $state('');
    let status = $state('');

    async function loadSetup() {
        if (!$imageStore) return;
        const r = await listItems($imageStore);
        setupItems = r.items;
    }

    async function toggleVisibility(item: TreeNode, visible: boolean) {
        if (!$imageStore) return;
        await setVisibility($imageStore, item.path, visible);
        status = `visibility set for ${item.path}`;
    }

    async function doAddFormSet() {
        if (!$imageStore) return;
        await addFormSet($imageStore, schemaText, '');
        status = 'formset added';
    }

    $effect(() => { loadSetup(); });
</script>

<div class="setup-page">
    <h2>Setup</h2>
    <section>
        <h3>Visibility</h3>
        {#each setupItems as item (item.path)}
            <div class="setup-item">
                <span>{item.name || item.path}</span>
                <button onclick={() => toggleVisibility(item, true)}>Show</button>
                <button onclick={() => toggleVisibility(item, false)}>Hide</button>
            </div>
        {/each}
    </section>
    <section>
        <h3>Add FormSet (JSON schema)</h3>
        <textarea bind:value={schemaText} rows="15" cols="60" placeholder='{"formset_guid":"...","title":"...","forms":[...]}'></textarea>
        <br />
        <button onclick={doAddFormSet}>Add FormSet</button>
    </section>
    {#if status}<p class="status">{status}</p>{/if}
</div>

<style>
    .setup-page { padding: 12px; }
    .setup-item { display: flex; align-items: center; gap: 8px; padding: 4px 0; }
    textarea { font-family: monospace; }
    .status { color: green; }
</style>
```

- [ ] **Step 2: Коммит**

```bash
git add webui/src/routes/setup/
git commit -m "feat(webui): add setup page (visibility toggle + add-formset)"
```

---

### Task 11: Docker — gateway.containerfile, webui.containerfile, docker-compose

**Files:**
- Create: `docker/gateway.containerfile`
- Create: `docker/webui.containerfile`
- Create: `docker/webui-nginx.conf`
- Modify: `docker/docker-compose.yml`

> **Note:** Rust-сборки переиспользуют общий `docker/rust-builder.containerfile` (создан в Цикле 1, Task 18: `registry.fedoraproject.org/fedora:44` + `rust`/`cargo`/`protobuf-compiler`). Перед сборкой gateway образ нужно собрать базовый образ: `podman build -f docker/rust-builder.containerfile -t uefipatcher-rust-builder ..`

- [ ] **Step 1: gateway.containerfile**

`docker/gateway.containerfile`:
```dockerfile
# Builder переиспользует общий rust-builder.containerfile (Цикл 1: fedora:44 + rust/cargo/protobuf-compiler)
FROM uefipatcher-rust-builder AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p uefi-gateway

FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y ca-certificates && dnf clean all
COPY --from=builder /app/target/release/uefi-gateway /usr/local/bin/
ENV UEFIPATCHER_GATEWAY_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["uefi-gateway"]
```

- [ ] **Step 2: webui.containerfile (multi-stage: build SvelteKit + nginx)**

`docker/webui.containerfile`:
```dockerfile
FROM registry.fedoraproject.org/fedora:44 AS builder
RUN dnf install -y nodejs npm && dnf clean all
WORKDIR /app
COPY webui/package*.json ./
RUN npm install
COPY webui/ .
RUN npm run build

FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y nginx && dnf clean all
COPY --from=builder /app/build /usr/share/nginx/html
COPY docker/webui-nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
ENTRYPOINT ["nginx", "-g", "daemon off;"]
```

`docker/webui-nginx.conf`:
```nginx
server {
    listen 80;
    root /usr/share/nginx/html;
    location / {
        try_files $uri $uri/ /index.html;
    }
    location /api/v1/ {
        proxy_pass http://gateway:8080;
        proxy_set_header Host $host;
    }
    location /api/v1/image/ {
        proxy_pass http://gateway:8080;
        proxy_set_header Host $host;
    }
}
```

- [ ] **Step 3: Обновить docker-compose.yml**

Добавить сервисы `gateway` и `webui` к существующему `engine` (см. `docker/engine.containerfile` из Цикла 1):
```yaml
version: "3.8"
services:
  engine:
    build:
      context: ..
      dockerfile: docker/engine.containerfile
    volumes:
      - uefi-data:/data
      - uefi-sock:/run/uefipatcher
    environment:
      UEFIPATCHER_DATA: /data
      UEFIPATCHER_SOCK: /run/uefipatcher/uefipatcher.sock
  gateway:
    build:
      context: ..
      dockerfile: docker/gateway.containerfile
    volumes:
      - uefi-sock:/run/uefipatcher
    environment:
      UEFIPATCHER_SOCK: /run/uefipatcher/uefipatcher.sock
      UEFIPATCHER_GATEWAY_LISTEN: 0.0.0.0:8080
    ports:
      - "8080:8080"
    depends_on:
      - engine
  webui:
    build:
      context: ..
      dockerfile: docker/webui.containerfile
    ports:
      - "3000:80"
    depends_on:
      - gateway
volumes:
  uefi-data:
  uefi-sock:
```

- [ ] **Step 4: Проверить валидность compose**

Run: `podman-compose -f docker/docker-compose.yml config` (или `docker compose -f docker/docker-compose.yml config`)
Expected: корректный вывод

- [ ] **Step 5: Коммит**

```bash
git add docker/
git commit -m "feat: add gateway.containerfile, webui.containerfile (rust-builder + fedora:44), update docker-compose"
```

---

### Task 12: Финальные проверки — clippy, svelte-check, все тесты

- [ ] **Step 1: Запустить Rust-тесты**

Run: `cargo test --all`
Expected: PASS

- [ ] **Step 2: Запустить clippy**

Run: `cargo clippy --all -- -D warnings`
Expected: без warnings

- [ ] **Step 3: Запустить fmt check**

Run: `cargo fmt --all -- --check`
Expected: без diff

- [ ] **Step 4: Запустить svelte-check**

Run: `cd webui && npm run check`
Expected: без ошибок

- [ ] **Step 5: Коммит финальных правок**

```bash
git add -A
git commit -m "chore: final checks — all tests pass, clippy clean, svelte-check clean"
```

---

## Само-проверка плана (после написания)

**Спека-покрытие:**
- uefi-gateway (axum REST + WS): Tasks 1-5 ✓
- Cookie-based сессии (in-memory token map): Task 3 ✓
- REST API `/api/v1/` все эндпоинты: Task 4 ✓
- WebSocket streaming dump: Task 5 ✓
- Upload/download (multipart + binary): Task 4 (upload.rs) ✓
- Artifact endpoints (extract/export/import/list): Task 4 (artifact.rs) + client methods Task 2 ✓
- Insert/Replace с `artifact_id`-альтернативой: Task 4 (edit.rs) + client Task 2 ✓
- Session с `name` (CreateBody → CreateSessionRequest): Task 4 (session.rs) + client Task 2 ✓
- gRPC-клиент к движку (все методы, incl. artifacts): Task 2 ✓
- WebUI SvelteKit (SPA): Tasks 7-10 ✓
- api.ts (REST-клиент): Task 7 ✓
- Tree + Details компоненты: Task 8 ✓
- Image page (tree + ops + save/download): Task 9 ✓
- Setup page (visibility + add-formset): Task 10 ✓
- Docker (gateway + webui + compose, fedora:44 + rust-builder.containerfile): Task 11 ✓
- Тесты (gateway integration + svelte-check): Tasks 6, 12 ✓
- `/api/v1/` versioning: все routes Tasks 4-5 ✓
- CORS: Task 1 (CorsLayer) ✓
- Cookie attributes (HttpOnly, SameSite=Strict): Task 3 ✓

**Placeholder scan:** TBD/TODO нет; все шаги содержат код. ✓

**Type consistency:**
- `AppState` — единое в routes/mod.rs, всех route handlers ✓
- `EngineClient` — единое в client.rs, routes/* ✓
- `SessionMap` — единое в session.rs, client.rs, routes/* ✓
- `AppError` — единое в error.rs, routes/* ✓
- `TreeNode` — единое в stores.ts, Tree.svelte, Details.svelte, image page ✓
- API path `/api/v1/` — единый префикс во всех routes и api.ts ✓