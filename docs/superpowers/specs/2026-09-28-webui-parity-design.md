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

## Аддендум W1 (2026-09-28) — ступень закрыта

Исполнена 2026-09-28: PR #30 (`80bc55b`), план
`docs/superpowers/plans/2026-09-28-webui-w1.md` (15/15 задач). Все гейты
зелёные (cargo test/clippy/fmt, svelte-check, vitest, Playwright E2E);
владелец подтвердил выполнение. TODO-раздел «Gateway + WebUI rework» закрыт
(`20a09ea`), отложенное (bytes-out, palette) — TODO.md «WebUI Parity —
отложенное».

**Исполнено:** мост `/api/v1/rpc/{Method}` (DynCodec, prost-reflect,
auth-инъекция, 404 несуществующего метода без обращения к движку);
upload bytes→`ImageUpload` без temp-файлов; hand-written REST удалён
(routes: session/upload/health + мост); static-host (gateway-образ собирает
и раздаёт webui, nginx из compose выпилен); ts-proto генерация + typed
api-клиент + runes-state; shell (sidebar, W2/W3 disabled), лендинг Image,
дерево с иерархией по `path` (плоский `/dump` display-bug закрыт),
Inspector + диалоги операций, страница `/image/[id]` (операции, search,
save/download); Playwright E2E round-trip с sha256-паритетом против uefi-cli
на живом образе HNX99TF (гейт W1 пройден, `fda9aba`).

**Отклонения от спеки (легитимные):**
- `POST /api/v1/artifact/upload` (multipart → `ArtifactImport`) — добавлен
  в W1 Task 4: артефакт-импорту нужен bytes-in, симметрично image-upload;
  потребитель — W3 (раздел Artifacts).
- Docker: gateway-образ сам собирает webui (context = корень репо) и раздаёт
  статику; отдельного webui-образа/nginx нет.

**Уроки (учтены в W2/W3):**
- `ImageNodesSearch` матчит только Section-узлы (Region-узлы не ищутся),
  path-режима нет — поиск по имени.
- Мутации идут через `ensure_mutable`: цель через `FfsType::Region`
  (например ME) → `OpsError::ImmutableRegion`; тестовые цели — узлы
  мутабельного тома (рефенс W1 E2E: `4/2`).
- Playwright: `webServer` стартует до `globalSetup` — шлюз обязан
  retry-коннект к движку; vitest требует `exclude: ['e2e/**']`.
- SvelteKit+vitest: `+*.test.ts` ломают routes-analysis (имя
  `page.test.ts`); `$app/navigation` — alias-стаб; без `svelteTesting()`
  svelte резолвится в серверный рантайм.
- svelte-dialog: render-пропы обязаны оборачивать props в `{ props: ... }`.

**Вход в W2:** Forms — дерево форм (`HiiListForms` + REF-рёбра
`HiiFormTree`), вопросы (`HiiListQuestions`/`HiiQuestionInfo`), strings
(`HiiListStrings`), правки visibility/unlock/set-value. Гейт W2 — E2E
unlock + set-value + save на живом образе с вердиктом движком/CLI.

## Аддендум W2 (2026-09-29) — ступень закрыта

Исполнена 2026-09-29: PR #31 (`3d954b4`), план
`docs/superpowers/plans/2026-09-28-webui-w2.md` (10/10 задач; Rust не
тронут — все 9 HII-RPC уже проксированы мостом W1). Все гейты зелёные
(cargo test 1162/0, clippy/fmt, svelte-check 0/0, vitest 63/63,
Playwright 2/2); E2E-гейт W2 пройден на живом образе HNX99TF —
sha256-паритет WebUI-прогонки (unlock формы 10029 + set-value `0x3B=1`
+ download) и CLI-последовательности, byte-strict (+belt `[1-9]` flips,
`not.toBe(IMAGE)`). Владелец закрыл ступень по итогам зелёных гейтов
(мерж + аддендум); отложенное — TODO.md «WebUI Parity — отложенное
(после W1/W2)» с классами отказов.

**Исполнено:** api-обёртки 9 HII-RPC (proto3-JSON uint64 строкой);
`forms.ts` — item-id конвенции движка (форма `target#<dec-form-id>`,
вопрос `…#<form>:0x<QID-HEX-UPPER>`) + REF-дерево формсетов (корни без
входящих intra-формсетных рёбер, REF-цели — чайлды, cross-formset-цели —
листья в дереве источника + корень в своём формсете, дедуб рёбер,
циклы через on-path, first-wins дедуб ключей строк); компоненты
FormsTree (gates/show/expand, aria-контракты), GatesDialog (unlock +
applied flips), SetValueDialog (oneof-select/numeric-input,
decimal-only валидация `/^\d+$/`), QuestionTable, StringsPanel
(фильтр text/lang/id); страницы `/forms` + `/forms/[id]` (оркестратор,
stale-response гварды монотонным токеном) + sidebar Forms enable;
Playwright E2E `e2e/forms.spec.ts`.

**Отклонения от спеки (легитимные):**
- `buildFormRows` first-wins дедуб ключей строк — сверх спеки: на живом
  образе Setup HII-модуль лежит в twin-FFS файлах, дубли
  `<formsetGuid>#<formIdIfr>` молча валили Svelte keyed each пустым
  деревом (5d73349, +2 репро-теста).
- REF-дерево — зеркало TUI с тремя осознанными расхождениями
  (multi-parent «у каждого» → first-wins одно вхождение; dangling-цели
  пропускаются вместо DanglingRef-строк; dosed pure-cycle корни без
  детей) — класс limitation, TODO.md.
- E2E-флоу экспандит host-формы (вложенность 10000→10002→10029) кликом
  по aria-контракту `expand <key>` — авто-экспанд только формсетов.

**Уроки (учтены в W3):**
- Keyed-each дубли ключей = silent-crash класс (бил дважды: twin-FFS,
  кросс-формсетные REF) — до W3 нужен error boundary; известные
  векторы: strings `lang:id`, oneof `o.value`, applied-flips по значению.
- Svelte 5: union-аннотация на `let` + `$state` даёт never-narrowing
  (нужен `$state<T|null>(null)`); select `bind:value` требует
  инициализации значения (иначе `''` не матчит опции); Map/Set нельзя
  через пропсы.
- testing-library: `getByText` в негативных ассертах кидает исключение
  на отсутствующем элементе — только `queryByText`.
- Playwright параллельные workers: E2E_DIR общий → CLI-стейт
  `.uefipatcher` коллидирует между session-init — каждому спеку свой
  cwd; `sel.count()`-ветвление гонит против async-загрузки —
  `expect(sel.or(input)).toBeVisible()`.
- Продуктовая гонка session-bootstrap (upload до createSession → 401
  Auth без retry) — E2E закрыт сентинелом `#status-session`, гвард в
  продукте — TODO (класс: errors).
- msw-сниппеты планов обязаны включать server.listen/close; спеки
  страницы — `page.test.ts`.

**Вход в W3:** NVRAM / Snapshots / Artifacts / добавление — разделы
sidemenu, артефакт-импорт уже имеет bytes-in (`ArtifactImport`,
аддендум W1).

## Аддендум W3 (2026-09-29) — дуга W1–W3 закрыта

Исполнена 2026-09-29: PR #32 (`422b7f4`), план
`docs/superpowers/plans/2026-09-29-webui-w3.md` (13/13 задач, TDD,
субагент-исполнение с task-review на каждую + whole-branch ревью с
фикс-волной). Все гейты зелёные (cargo test 1164/0 +69 ignored,
clippy/fmt, svelte-check 0/0, vitest 89/89, build); Playwright 6/6 на
живом образе HNX99TF — sha256-паритет четырёх блоков: nvar-set ≡ CLI,
snapshot restore ≡ original, artifact download ≡ CLI export, question
add ≡ CLI. Rust тронут точечно: REST
`GET /api/v1/artifact/:id/download` (валидация id в
Content-Disposition, фикс-волна ревью).

**Исполнено:** api-обёртки 14 W3-RPC + artifact download; `nvar.ts`
(base64/hexDump/parseNum/hexN, BigInt-точность uint64); layout —
`<svelte:boundary>` error boundary + sidebar NVRAM/Snapshots/Artifacts
+ snapshot-индикатор со сбросом при смене образа; `/nvar/[id]`
(stores/vars/hex-dump + NvarSetDialog, hex/decimal нормализация);
`/snapshots/[id]` (create/list/restore-confirm + snapshotCount sync);
`/artifacts` (list/import/download/export с ошибками в модалке);
SchemaDialog/ExportDialog + Forms add-ops тулбар
(formset/form/question/page/hijack/export); index-страницы `/nvar`,
`/snapshots` (graceful no-image — фикс-волна); E2E-гейты W3.

**Ручной прогон владельца (2026-09-29):** функционально — дуга
работает (upload → NVRAM/Forms/мутации → download через
pod-контейнер, hostPort-фикс `563724a`). UX-находки — TODO.md
«WebUI Parity — отложенное (после W3)», блок «Находки ручного
прогона»: sidebar-Image ведёт на upload-индекс при открытом образе
(нет Download/Save без повторного Open); контекстное меню Image View
не гасится кликом вне/Escape; Forms/NVRAM без контекстных меню
(parity-gap). Чинятся отдельно.

**Уроки W3 (для будущих webui-циклов):**
- jsdom 25 без `Blob.text()` → FileReader (error-канал в role=alert);
  `URL.createObjectURL` в jsdom есть, но `a.click()` даёт ожидаемый
  navigation-noise.
- @testing-library/svelte v5: компонент с пропом `target` рендерится
  только через props-ворапер (mount-option конфликт); рендер страниц —
  `render(Page, { props: { data } })`.
- msw per-test `server.listen()/close()` каскадит падения («already
  enabled network») — паттерн `beforeAll(listen)/afterEach(reset)/
  afterAll(close)` обязателен в планах.
- svelte-check типизирует параметр `failed`-сниппета boundary как
  `unknown` — cast `(message as Error)`; `$state(prefill.x)` даёт
  `state_referenced_locally` warning → прагма (или $effect-паттерн).
- ts-proto `bytes`-поля приходят base64-строкой (proto3-JSON), тип —
  `Uint8Array`: row-типы + `as unknown as` отражают рантайм-истину.
- LSP svelte-language-server даёт stale-диагностику по свежим
  экспортам — гейт только `npm run check`.
- Плановый код верифицируется прогонами: 12 docs-fix коммитов плана
  (rule 11) за цикл — вербатим-сниппеты планов деградируют о
  тест-инфраструктуру быстрее, чем о продукт.

**Закрытие дуги W1–W3:** WebUI паритетен CLI по всем гейтам дуги
(roundtrip, forms, nvar, snapshots, artifacts, add-ops). Дальше —
по roadmap (цикл 6 / W3.1 UX-правки по TODO отдельно).

## Аддендум W4 (2026-09-29) — UX-полиш: навигация, контекстные меню, пиктограммы

Ступень-полиш поверх закрытой дуги W1–W3. Источник — находки ручного
прогона владельца (TODO, блок «Находки ручного прогона» после W3) +
пункт «пиктограммы как в TUI» (после W1/W2). Новых RPC и изменений
архитектуры нет — всё внутри существующих контрактов моста.

### W4-1. Sidebar «Image» при открытом образе

Дефект: `+layout.svelte` `href()` захардкожен для `image` → `/image`
(upload-индекс) даже при открытом образе; после ухода в Forms/NVRAM
вид образа (дерево, Save, Download) доступен только через повторный
`Open` в таблице.

Контракт: как Forms — `imageHref = /image/${imageId}` при открытом
образе, `/image` при отсутствии. На виде образа — кнопка
«Switch/upload…» → `/image` (возврат к индексу). Паритет поведения
sidebar-разделов: активный образ углубляет все разделы, кроме
Artifacts (индекс без образа — by design, артефакты сессионны).

### W4-2. Гашение контекстного меню

Дефект: `Tree.svelte` `menuFor` гасится только кликом по пункту меню;
клик по свободному месту и Escape меню оставляют.

Контракт (общий для всех меню W4-3): меню закрывается по (а) клику по
пункту, (б) клику вне меню (bubbling до `svelte:window`), (в) Escape,
(г) открытии другого меню (замена `menuFor`). Right-click не порождает
`click` — конфликта с открытием нет; пункты меню null-ят `menuFor`
раньше window-хендлера (порядок bubbling), двойное гашение безопасно.

### W4-3. Контекстные меню Forms и NVRAM (parity-gap)

`oncontextmenu` есть только в Image View. Извлекаем `Tree.svelte`
ctxmenu в переиспользуемый `lib/components/ContextMenu.svelte`
(items: `{id, label, disabled?}[]`, позиционирование, W4-2-контракт) —
Tree переходит на него без смены поведения (регрессия закрыта
существующими E2E).

Матрица действий (все — существующие хендлеры тулбаров/кнопок, новых
RPC нет):

| Цель (right-click) | Действия |
|---|---|
| Forms: форма | Set value, Gates, Export, Add question, Add page, Hijack |
| Forms: вопрос | Set value, Gates |
| NVRAM: переменная | Set…, Copy hex |

Disabled-пункты — по тем же условиям, что тулбар-кнопки (напр. Hijack
только на живой форме). Copy hex — `navigator.clipboard` + toast
(новая микро-операция, без RPC).

### W4-4. Пиктограммы как в TUI — woff2-сабсет Nerd Font

Решение (владелец, 2026-09-29): сабсет тех же PUA-кодпоинтов, что
`uefi-tui/theme.rs` (`type_icon`/`store_icon`/`question_icon`:
F2DB/F1C0/F15B/F1C9/F023/F1C6/F031/EAE8/F016/EB7D/F10C/F492/F0C7/
F0CA/F14A/F1EC) — байт-в-байт визуальный паритет с TUI, одна
иконография на оба UI. Альтернатива SVG-сет отклонена (расхождение с
TUI + вторая таблица соответствий).

Контракт:
- Сборка: скрипт сабсета (pyftsubset/npm subset-font) из полного
  Nerd Fonts-woff2 → `webui/static/icons.woff2` (~10KB), артефакт
  коммитится, скрипт — в `webui/scripts/` для регенерации.
- Таблица иконок: TS-модуль `lib/icons.ts`, зеркало `theme.rs`
  (ключ → codepoint); источник истины — `theme.rs`, TS-таблица
  снабжена ссылкой. Паритет кодпоинтов — юнит-тестом на
  неизменность набора (список захардкожен в тесте; смена `theme.rs`
  требует сознательной правки теста).
- CSS: `@font-face` NF + span.nf с `content`-кодпоинтом; иконки
  строго `aria-hidden` (декоративность — контракт E2E-селекторов
  W1–W3 не меняется).
- Применение (граница W4): дерево образа (type_icon), Forms
  (formset/form/вопросы), NVRAM (store). Artifacts/snapshots —
  без иконок (не в TUI-таблицах).

### Тесты и гейты W4

- vitest: href-логика layout; ContextMenu (гашение пункт/вне/Escape/
  замена); матрица пунктов Forms/NVRAM (disabled-условия); `icons.ts`
  (паритет набора кодпоинтов, отсутствие дублей); Clipboard-фолбэк.
- Playwright: навигация sidebar с открытым образом (Image → Forms →
  Image = вид образа, не индекс); ctxmenu в Forms и NVRAM (открытие
  действия из меню); гашение меню Escape. Существующие image-ctxmenu
  E2E — регрессия.
- Гейты ступени: cargo не трогаем (Rust-диффа нет, кроме нуля);
  `npm run check`, vitest, build; Playwright на живом образе;
  ручной прогон владельца.

### Закрытие TODO

По закрытии W4 закрываются: три «Находки ручного прогона» (после W3)
+ «пиктограммы как в TUI» (после W1/W2). Остальное разделов
«WebUI Parity — отложенное» — вне W4.
