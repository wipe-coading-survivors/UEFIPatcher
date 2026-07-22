# UEFIPatcher — Дизайн цикла 5+7 (WebUI + gRPC-шлюз)

## Краткое описание

WebUI (SvelteKit/TypeScript) для управления UEFI-образами через браузер, плюс Rust gRPC-шлюз `uefi-gateway` (axum HTTP/WebSocket), проксирующий REST-запросы браузера в движок через gRPC over unix-сокет. Cookie-based сессии. Полный аналог CLI/TUI: загрузка/навигация/редактирование/setup/save + upload/download образов.

## Цели цикла 5+7

- Реализовать `uefi-gateway` (Rust, axum): REST API + WebSocket, прокси в движок через gRPC over unix-сокет, cookie-based сессии
- Реализовать `webui/` (SvelteKit, TypeScript): tree-view образа, детали узла, операции (insert/remove/replace/rebuild/set-visibility/save), upload/download образов, setup add-formset (цикл 6)
- Реализовать cookie-based хранение session_id (HttpOnly, SameSite=Strict) + шлюз маппит cookie → gRPC metadata (token + x-session-id)
- Реализовать REST API эндпоинты для всех операций движка
- Реализовать WebSocket для streaming dump
- Подготовить Dockerfile для gateway и webui, docker-compose (engine + gateway + webui)
- Объединить циклы 5 (WebUI) и 7 (gRPC-шлюз) — шлюз нужен именно для WebUI

## Архитектура

### Компоненты

```
репо/
├── crates/
│   ├── uefi-gateway/          # НОВЫЙ: Rust HTTP/WS шлюз
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs        # axum сервер, --listen 0.0.0.0:8080
│   │       ├── config.rs      # env: UEFIPATCHER_GATEWAY_LISTEN, UEFIPATCHER_SOCK
│   │       ├── client.rs      # gRPC-клиент к движку (tonic, uefi-proto)
│   │       ├── session.rs     # cookie-маппинг: session_id → token (in-memory map)
│   │       ├── routes/
│   │       │   ├── mod.rs
│   │       │   ├── session.rs # POST/DELETE/GET /api/session(s)
│   │       │   ├── image.rs   # POST /api/image/open, GET dump/items/find, POST save
│   │       │   ├── edit.rs    # POST insert/remove/replace/rebuild
│   │       │   ├── setup.rs   # POST set-visibility, GET setup-items, POST add-formset
│   │       │   └── upload.rs  # POST upload (multipart), GET download
│   │       └── ws.rs          # WebSocket для streaming dump
│   └── ... (существующие)
├── webui/                     # НОВЫЙ: SvelteKit (TypeScript)
│   ├── package.json
│   ├── svelte.config.js
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── src/
│   │   ├── routes/
│   │   │   ├── +layout.svelte     # layout: header (сессия/образ), навигация
│   │   │   ├── +page.svelte       # главная: создание сессии, список образов
│   │   │   ├── image/[id]/+page.svelte  # дерево + детали + операции
│   │   │   └── setup/+page.svelte      # setup: видимость + add-formset
│   │   ├── lib/
│   │   │   ├── api.ts            # REST-клиент (fetch wrapper)
│   │   │   ├── ws.ts             # WebSocket-клиент для dump
│   │   │   ├── tree.svelte       # TreeView компонент
│   │   │   ├── details.svelte    # DetailsPanel компонент
│   │   │   └── stores.ts         # svelte stores: session, image, tree
│   │   └── app.html
│   └── static/
└── docker/
    ├── Dockerfile.gateway
    ├── Dockerfile.webui
    └── docker-compose.yml
```

### Поток данных

1. Браузер → SvelteKit SPA → fetch REST к `uefi-gateway` (HTTP)
2. Шлюз: cookie `uefipatcher_session` → session_id → in-memory token map → gRPC metadata `Authorization: Bearer <token>` + `x-session-id`
3. Шлюз → движок (gRPC over unix-сокет)
4. Streaming dump → WebSocket (шлюз держит gRPC stream, ретранслирует в WS frames)
5. Upload образа: multipart POST → шлюз сохраняет во временный файл (`/tmp/uefipatcher-upload-<uuid>.bin`) → `OpenImage(path)`
6. Download: `SaveImage` во temp → шлюз отдаёт binary file

### Зависимости

**Gateway (Rust):**
- `axum` (latest) + `tower-http` (cors, static) — HTTP-сервер
- `tonic`, `tokio` — gRPC-клиент к движку
- `uefi-proto` (path) — контракт
- `serde`, `serde_json` — JSON
- `tower` — middleware
- `uuid` — upload filenames
- `axum-extra` — cookie extraction

**WebUI (TypeScript):**
- `svelte` (latest), `@sveltejs/kit`, `vite` — фреймворк
- `typescript` — язык
- `@testing-library/svelte` — unit-тесты компонентов
- `playwright` — E2E
- `vitest` — test runner

## Фронтенды

- [x] WebUI `webui/` (SvelteKit) — единственный UI-фронтенд цикла 5
- [x] gRPC-шлюз `uefi-gateway` (REST+WS) — бэкенд-фронтенд для браузера

## API/Контракты

Все эндпоинты под `/api/`. Cookie `uefipatcher_session` (HttpOnly, SameSite=Strict) = session_id. Cookie `uefipatcher_image` = active image_id.

### Session

| Метод | Path | Тело | Ответ | Описание |
|---|---|---|---|---|
| POST | `/api/session` | `{}` | `{session_id}` + Set-Cookie | CreateSession |
| DELETE | `/api/session` | — | `{ok}` | DestroySession |
| GET | `/api/sessions` | — | `[{session_id, created_at, last_activity}]` | ListSessions |

### Image

| Метод | Path | Тело | Ответ | Описание |
|---|---|---|---|---|
| POST | `/api/image/open` | `{path, mode: "read"\|"write"}` | `{image_id, root_guid}` + Set-Cookie | OpenImage |
| POST | `/api/image/upload` | multipart `file` | `{path}` | сохраняет во temp файл |
| GET | `/api/image/:id/dump?format=text\|tsv` | — | `{text}` | DumpTree |
| GET | `/api/image/:id/items?filter=` | — | `[{path, type, subtype, guid, offset, size, name}]` | ListItems |
| GET | `/api/image/:id/find?target=` | — | `{item_id}` | FindItem |
| POST | `/api/image/:id/save` | `{output_path}` | `{ok}` | SaveImage |
| GET | `/api/image/:id/download` | — | binary file | SaveImage в temp → отдаёт файл |

### Edit

| Метод | Path | Тело | Ответ |
|---|---|---|---|
| POST | `/api/image/:id/insert` | `{target, ffs_path, mode}` | `{item_id}` |
| POST | `/api/image/:id/remove` | `{target}` | `{ok}` |
| POST | `/api/image/:id/replace` | `{target, data_path, body_only}` | `{item_id}` |
| POST | `/api/image/:id/rebuild` | `{target}` | `{ok}` |

### Setup

| Метод | Path | Тело | Ответ |
|---|---|---|---|
| POST | `/api/image/:id/set-visibility` | `{item_id, visible}` | `{ok}` |
| GET | `/api/image/:id/setup-items` | — | `[{items}]` (Section type) |
| POST | `/api/image/:id/add-formset` | `{schema_json, target_ffs_guid}` | `{new_ffs_id, inserted_form_ids, string_ids}` |

### WebSocket

| Path | Описание |
|---|---|
| `ws://host/api/image/:id/dump/ws` | streaming dump |

### Ошибки

HTTP status ← gRPC code: `401` UNAUTHENTICATED, `404` NOT_FOUND, `400` INVALID_ARGUMENT, `500` INTERNAL.
Тело: `{"error": "message", "code": "ENUM"}`.

## WebUI (SvelteKit)

### Компоненты

| Компонент | Назначение |
|---|---|
| `+layout.svelte` | Глобальный layout: header (сессия/образ), навигация |
| `+page.svelte` | Главная: создание сессии, список/открытие образов |
| `lib/api.ts` | REST-клиент: `openImage`, `dumpTree`, `insert`, `remove`, `save`, `upload`, `download` |
| `lib/ws.ts` | WebSocket-клиент для streaming dump |
| `lib/tree.svelte` | TreeView: рекурсивный рендер, expand/collapse, выбор |
| `lib/details.svelte` | DetailsPanel: поля выбранного узла |
| `lib/stores.ts` | Svelte stores: `sessionStore`, `imageStore`, `treeStore` |
| `image/[id]/+page.svelte` | Страница образа: tree + details + кнопки операций |
| `setup/+page.svelte` | Setup: set-visibility list + add-formset (загрузка JSON) |

### Маршруты

- `/` — главная (создание сессии / список образов)
- `/image/[id]` — дерево образа + детали + операции
- `/setup` — setup (видимость + add-formset)

## Ошибки

- **401 Unauthorized**: невалидный/отсутствующий cookie сессии → редирект на `/` (пересоздать сессию)
- **404 Not Found**: образ/узел не найден → toast уведомление
- **400 Bad Request**: неверный target/path → toast с сообщением
- **500 Internal**: ошибка движка → toast, предложение проверить engine
- **Сетевые ошибки**: gateway недоступен → баннер "Engine unavailable, retry"

## Тестирование

### Gateway (Rust)
- **Unit**: cookie-маппинг (session_id → token), REST→gRPC трансляция (mock gRPC-клиент), error mapping.
- **Integration**: mock engine (как цикл 2) + HTTP-запросы через `axum::test` / `reqwest`. Полный flow: create session → open → dump → insert → save.

### WebUI (TypeScript)
- **Unit**: `api.ts` (моки fetch через `msw`), `tree.svelte` (component testing via `@testing-library/svelte`).
- **E2E** (Playwright): create session → upload image → dump → navigate tree → insert → save → download. Проверка round-trip.

### Цели покрытия
- Gateway: ≥70% (через integration).
- WebUI: smoke E2E покрывает главный flow.

## Этапы реализации

1. **uefi-gateway скелет**: Cargo.toml, axum server, config (env), health endpoint `/api/health`.
2. **gateway/client.rs**: gRPC-клиент к движку (tonic, uefi-proto).
3. **gateway/session.rs**: cookie-маппинг (session_id → token in-memory map).
4. **gateway/routes/**: session, image, edit, setup, upload — REST эндпоинты.
5. **gateway/ws.rs**: WebSocket для streaming dump.
6. **webui/ скелет**: SvelteKit init, api.ts, stores.ts, layout.
7. **webui/lib/tree.svelte + details.svelte**: tree-view + детали.
8. **webui/image/[id]/+page.svelte**: страница образа с операциями.
9. **webui/setup/+page.svelte**: setup (видимость + add-formset).
10. **Docker**: Dockerfile.gateway, Dockerfile.webui, docker-compose (engine + gateway + webui).
11. **Тесты**: gateway integration, WebUI E2E (Playwright).

## Риски и ограничения

- **CORS**: SvelteKit dev-сервер (vite, порт 5173) и gateway (порт 8080) — разные origin. Мера: `tower-http::cors` в шлюзе, или vite proxy в dev.
- **Большие образы upload/download**: BIOS-образ может быть 16-32MB. Мера: multipart upload с лимитом, streaming download (chunked transfer).
- **Cookie security**: HttpOnly + SameSite=Strict. Мера: шлюз устанавливает cookie с этими атрибутами; в dev — SameSite=Lax (для localhost).
- **WebSocket auth**: WS не может отправить cookie при upgrade в некоторых браузерах. Мера: cookie отправляется при WS handshake (браузер делает это автоматически для same-origin).
- **Два билд-стека**: Cargo + npm. Мера: docker-compose собирает оба; CI — два шага (cargo test + npm test).
- **SvelteKit SSR vs SPA**: SSR требует серверной части. Мера: SPA mode (`@sveltejs/adapter-static`), раздача статики через gateway или nginx.

## Решения (фиксация)

- Объединить циклы 5 (WebUI) и 7 (gRPC-шлюз) — шлюз нужен для WebUI.
- Бэкенд: `uefi-gateway` (Rust, axum) — REST + WebSocket прокси в движок через gRPC over unix-сокет.
- Фронтенд: SvelteKit (TypeScript), SPA mode (adapter-static).
- API: REST (JSON) + WebSocket для streaming.
- Сессии: cookie-based (HttpOnly, SameSite=Strict), шлюз маппит cookie → gRPC metadata.
- Функционал: полный аналог CLI/TUI + upload/download образов + setup add-formset.
- Архитектура: браузер ↔ HTTP/WS ↔ gateway ↔ gRPC ↔ engine.
- Docker: engine + gateway + webui в docker-compose.