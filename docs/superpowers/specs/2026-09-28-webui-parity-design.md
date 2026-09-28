# UEFIPatcher — WebUI Parity (дуга W1–W3): дизайн

## Краткое описание

Довести WebUI (SvelteKit) до **полного функционального паритета с TUI** через
универсальный RPC-мост в шлюзе. Лестница из трёх ступеней: W1 — мост + каркас
+ раздел Image; W2 — Forms (просмотр + правки); W3 — NVRAM, снапшоты,
артефакты, операции добавления. UX — нативный web (IDE-стиль, без командного
режима). Закрывает TODO-раздел «Gateway + WebUI rework» целиком (в W1).

## Контекст и мотивация

Цикл 5+7 (2026-07-22) реализовал `uefi-gateway` (axum REST) + `webui/`
(SvelteKit) на 15 hand-written эндпоинтов. С тех пор контракт движка вырос до
40 RPC: HII-семейство (forms/varstores/strings/gates/unlock/questions/
setValue/questionAdd/pageAdd/hijack/formExport), NvarList/NvarSet, снапшоты,
ImageUpload, поиск. TUI потребил всё это (дуги TUI Revival, Forms View V1–V3,
formset-unlock), WebUI — нет. Известные дефекты текущего WebUI — в TODO.md,
раздел «Gateway + WebUI rework»: `/dump` отдаёт плоский список без иерархии,
`addFormSet()` зовёт несуществующий маршрут (404), dead imports, a11y
warnings. Продолжать hand-written REST бессмысленно: каждый новый RPC требует
правок в трёх местах (proto → gateway → webui).

## Решения (фиксация, 2026-09-28)

1. **Полный паритет с TUI** — масштаб дуги (решение владельца).
2. **Универсальный RPC-мост** вместо hand-written REST: один эндпоинт
   `POST /api/v1/rpc/{Method}`, JSON ↔ proto через prost-reflect по
   дескриптору. REST остаётся только там, где семантика HTTP-тела ≠ JSON:
   upload (multipart), download (binary), session (Set-Cookie), health.
3. **Нативный web-UX**: sidebar-навигация по разделам (Image/Forms/NVRAM/
   Snapshots/Artifacts) + inspector-панель, а не зеркало TUI-раскладки.
4. **Без командного режима**: все операции через меню/тулбары/контекстные
   меню/диалоги (IDE-стиль). Command palette (Ctrl+K) — возможное будущее,
   не W1–W3 (TODO).
5. **Upload — bytes-in напрямую в движок**: multipart → `ImageUpload` RPC →
   образ уже открыт (без temp-файлов и второго round-trip `/image/open`).
   Единый контракт с TUI `:upload` (контейнерное окружение). Экстраполяция
   bytes-out (download без temp) — вне скоупа (TODO).
6. **Лестница W1–W3**, каждая ступень — свой план (TDD) и свой гейт.
7. **Полный тестовый набор WebUI**: vitest + @testing-library/svelte +
   Playwright E2E на живом образе (в исходной спеке 5+7 задумано, но не
   реализовано).
8. **TS-типы генерируются** из `engine.proto` (ts-proto, proto3-JSON
   конвенции) — клиент и сервер не расходятся.

## Архитектура

### Компоненты

```
репо/
├── crates/
│   ├── uefi-proto/            # + FileDescriptorSet (build.rs) + DescriptorPool
│   └── uefi-gateway/          # мост вместо REST-роутов; static-host webui/build
│       └── src/
│           ├── bridge.rs      # НОВЫЙ: /api/v1/rpc/{Method} — JSON↔DynamicMessage
│           ├── routes/
│           │   ├── mod.rs     # router: bridge + upload/download/session/health + static
│           │   ├── upload.rs  # multipart → ImageUpload RPC; download — как сейчас
│           │   └── session.rs # как сейчас (Set-Cookie)
│           │   └── (image.rs, edit.rs, hii.rs, artifact.rs — УДАЛЯЮТСЯ)
│           └── (client.rs сокращается: generic-вызовы вместо 15 обёрток)
├── webui/
│   ├── package.json           # + vitest, @testing-library/svelte, playwright, msw
│   ├── playwright.config.ts   # НОВЫЙ: поднимает engine+gateway сам
│   └── src/                   # (engine.proto НЕ копируется: ts-proto-скрипт читает crates/uefi-proto/proto/)
│       ├── lib/
│       │   ├── proto.ts       # ГЕНЕРЕН: ts-proto из engine.proto
│       │   ├── api.ts         # typed bridge-клиент
│       │   └── components/    # Tree, Inspector, QuestionTable, HexEditor, диалоги
│       └── routes/
│           ├── +layout.svelte # shell: sidebar + status-bar
│           ├── image/         # раздел Image
│           ├── forms/         # раздел Forms (W2)
│           ├── nvar/          # раздел NVRAM (W3)
│           ├── snapshots/     # раздел Snapshots (W3)
│           └── artifacts/     # раздел Artifacts (W3)
└── docker/                    # gateway.containerfile, webui.containerfile, compose — обновить в W1
```

### Поток данных

```
Браузер ── fetch JSON ──► gateway /api/v1/rpc/{Method}
                            │ cookie → token → metadata Authorization + x-session-id
                            │ JSON → prost_reflect::DynamicMessage (дескриптор)
                            │ gRPC unary, raw path /engine.EngineService/{Method}
                            ▼
                         engine (unix-сокет)
                            │ ответ → DynamicMessage → proto3-JSON
                            ▼
Браузер ◄── JSON ────────── gateway
```

- Метод резолвится по `DescriptorPool`: несуществующий метод → 404 на шлюзе,
  без обращения к движку.
- `session_id` передаётся в теле запроса (конвенция движка), токен — в
  metadata (конвенция шлюза, как сейчас).
- proto3-JSON конвенции обеих сторон: camelCase, base64 для bytes,
  int64-as-string. ts-proto (клиент) и prost-reflect (шлюз) следуют одному
  стандарту.
- Upload: `POST /api/v1/image/upload` multipart (`file`, `mode`) → шлюз
  читает bytes → `ImageUpload` RPC → `{image_id, root_guid, name}`. Образ
  открыт. Temp-файлы upload-пути умирают.
- Download: `GET /image/:id/download` → `ImageSave` в temp на стороне шлюза →
  chunked binary (без изменений до будущей bytes-out экстраполяции).
- Раздача статики: gateway отдаёт `webui/build` (SPA fallback). Dev — vite
  dev-сервер + proxy `/api/v1` (уже настроен в vite.config.ts).

## API / контракты

### Мост

| Метод | Path | Тело | Ответ |
|---|---|---|---|
| POST | `/api/v1/rpc/{Method}` | proto3-JSON тела запроса RPC | proto3-JSON ответа |

`{Method}` — имя метода `engine.EngineService` (CamelCase, напр.
`ImageNodesList`, `HiiUnlock`). Ошибки — как сейчас:
`{"error": "message", "code": "ENUM"}`, HTTP-статусы из gRPC-кодов
(401/404/400/500).

### REST-остаток

| Метод | Path | Назначение |
|---|---|---|
| GET | `/api/v1/health` | живость шлюза |
| POST/DELETE | `/api/v1/session` | Create/Destroy + Set-Cookie |
| GET | `/api/v1/sessions` | ListSessions |
| POST | `/api/v1/image/upload` | multipart → `ImageUpload` → образ открыт |
| GET | `/api/v1/image/:id/download` | SaveImage → binary stream |

Все прочие маршруты текущего шлюза (`/dump`, `/items`, `/insert`, `/remove`,
`/replace`, `/rebuild`, `/set-visibility`, `/setup-items`, `/artifacts`,
`/artifact/*`) удаляются в W1; mock-тесты шлюза переписываются против моста в
той же задаче.

## WebUI (нативный web-UX)

### Каркас

- SvelteKit SPA (adapter-static), Svelte 5 runes.
- Shell: фиксированный sidebar слева (Image / Forms / NVRAM / Snapshots /
  Artifacts), status-bar сверху (сессия, активный образ, pending-индикатор),
  основная область — активный раздел.
- Разделы активны только при открытом образе; переключение/открытие образов —
  из status-bar (ImagesList/ImageOpen/ImageUpload/ImageClose).

### Разделы

| Раздел | Содержимое | Ступень |
|---|---|---|
| **Image** | Дерево образа (иерархия по `path`, лениво), inspector (детали узла, region, pending action), тулбар + контекстное меню узла (insert/remove/replace/rebuild/extract), upload/open, save/download, search (ImageNodesSearch) | W1 |
| **Forms** | Дерево формсетов/форм (`HiiListForms` + REF-рёбра `HiiFormTree`), таблица вопросов (`HiiListQuestions` → `HiiQuestionInfo`), strings-браузер с фильтром (`HiiListStrings`); правки: visibility, unlock (с `HiiGatesList`), set-value | W2 |
| **NVRAM** | Список NVRAM-store'ов и переменных (`NvarList`), hex-редактор значения (`NvarSet`) | W3 |
| **Snapshots** | Список, create, restore; индикатор «есть снапшот → безопасно править» | W3 |
| **Artifacts** | Список, extract/export/import, download | W3 |
| **Forms (добавление)** | Диалоги `HiiFormSetAdd`/`HiiFormAdd`/`HiiQuestionAdd`/`HiiPageAdd`/`HiiFormHijack`/`HiiFormExport` — JSON-схема через file-upload, валидация движком, ошибки inline | W3 |

### Интеракция — IDE-стиль, без командного режима

- Все операции через тулбары, контекстные меню (правый клик на узле дерева),
  кнопки inspector.
- Параметрические операции — диалоги с формами: insert (target/mode/
  artifact), replace (body-only), add-операции (JSON schema), nvar set
  (hex-редактор).
- Keyboard shortcuts на частые действия (Del, Ctrl+S), без cmdline.
- Command palette (Ctrl+K) — возможное будущее, вне скоупа.

## Ступени и гейты

Каждая ступень: план (TDD) → реализация → гейт. Спека W1 — этот документ;
W2/W3 — аддендумы к нему по итогам ступеней (паттерн serial-ladder).

### W1 — Мост + каркас + Image

- uefi-proto: build.rs эмитит FileDescriptorSet, крейт экспонирует
  `DescriptorPool` (prost-reflect).
- gateway: мост; upload-переделка (bytes → ImageUpload); static-host;
  удаление старых маршрутов; переписанные mock-тесты; обновление
  containerfile/compose.
- webui: тест-scaffolding (vitest + Playwright + msw), ts-proto генерация,
  typed api-клиент, shell, раздел Image целиком.
- **Гейт W1**: Playwright E2E round-trip на живом образе HNX99TF
  (`refs/fw/HNX99TF_200525_original_E5C88C6F.bin`): upload → дерево →
  replace → save → download → **sha256-паритет с CLI-прогоном** той же
  последовательности + ручной прогон владельца.
- Закрывает TODO-раздел «Gateway + WebUI rework».

### W2 — Forms

- Просмотр: дерево форм (`HiiListForms` + `HiiFormTree`), вопросы
  (`HiiListQuestions`/`HiiQuestionInfo`), strings (`HiiListStrings`).
- Правки: `HiiSetFormVisibility`, `HiiUnlock` (+ `HiiGatesList` — показать,
  что именно отпирается), `HiiSetValue`.
- **Гейт W2**: E2E — unlock скрытого формсета + set-value + save на живом
  образе; вердикт движком/CLI (паттерн U4/formset-unlock), ручной прогон.

### W3 — NVRAM + Snapshots + Artifacts + добавление

- NVRAM: `NvarList`/`NvarSet` + hex-редактор.
- Snapshots: create/list/restore + индикация.
- Artifacts: extract/export/import/download.
- Добавление: HiiFormSetAdd/HiiFormAdd/HiiQuestionAdd/HiiPageAdd/
  HiiFormHijack/HiiFormExport через диалоги.
- **Гейт W3**: E2E на каждый блок + ручной прогон владельца.

## Тестирование

### Gateway (Rust)

- **Unit**: резолв метода по дескриптору (валидный/несуществующий),
  JSON↔DynamicMessage конверсия (enum'ы, base64 bytes, uint64-as-string,
  unknown поля), auth-инъекция metadata.
- **Integration** (mock engine, паттерн текущего `tests/mock_server.rs`):
  полный flow через мост — session → upload → nodes list → replace → save;
  upload/download binary round-trip; error mapping (401/404/400/500).

### WebUI (TypeScript)

- **vitest + @testing-library/svelte**: api-клиент (msw-моки), дерево
  (иерархия/ленивость/контекстное меню), таблица вопросов, диалоги.
- **Playwright E2E** (гейт ступени): конфиг поднимает engine + gateway сам
  (temp data-dir, unix-сокет); W1 round-trip sha256-паритет; W2 unlock+
  set-value; W3 nvar/snapshot/artifact. Живой образ HNX99TF с хоста
  (паттерн `#[ignore]`-гейтов engine); в контейнере — только unit+check.
- `npm run check` (svelte-check) — межзадачный гейт.

## Риски и меры

| Риск | Мера |
|---|---|
| proto3-JSON конвенции (camelCase, base64, int64-as-string) | ts-proto и prost-reflect следуют одному стандарту; unit-тесты конверсии в шлюзе |
| Разрыв старых REST-маршрутов | Старый WebUI переписывается в W1 синхронно с удалением; внешних потребителей шлюза нет |
| Два билд-стека (cargo+npm) в гейтах | playwright-конфиг поднимает engine+gateway сам; док-команда единого гейта |
| Playwright на живом образе в контейнере | E2E — на хосте (паттерн engine real-гейтов); в контейнере unit+check |
| Удаление маршрутов ломает mock-тесты | Тесты переписываются в той же задаче, что и удаление |
| Мост = тонкая поверхность для регрессий сериализации | Integration-гейт полного flow + E2E sha256-паритет ловит молчаливые искажения |

## Отложенное (фиксируется в TODO.md)

- bytes-out экстраполяция: download без temp (RPC с bytes-ответом) и общий
  bytes-in/out для контейнерного окружения (TUI-переиспользование upload уже
  есть через ImageUpload RPC).
- Command palette (Ctrl+K, VS Code-стиль) как web-аналог «командности».
- Persistent search-panel и unified `/`-адресация — общие с TUI TODO пункты,
  затрагивают gateway/webui при исполнении.

## Связи

- Дуга стоит на цикле 1 (контракт uefi-proto), потребляет все RPC-дуги:
  TUI Revival (upload, снапшоты, поиск), Forms View V1–V3 (HII-семейство),
  formset-unlock (gates/unlock), nvar-op, hii-*-циклы.
- Инвалидация: при добавлении новых RPC движка WebUI получает их через мост
  автоматически (нужен только UI-раздел).
