# Спека мини-цикла `tse-followups`: TSE-страница WebUI + отложенные миноры

Follow-up цикла tse-unhide (PR #41, merge 4611247): интерактивная
WebUI-поверхность TSE (отложена спекой tse-unhide §3.4) + закрытие 5
записей TODO-секции «Отложенное цикла tse-unhide (2026-10-02)».

Дизайн утверждён владельцем 2026-11-03: полный паритет CLI/TUI (отчёт +
unhide), отдельная страница, один мини-цикл/один PR.

## 1. Контекст

- RPC `TseReport`/`TseUnhide` и proto-типы уже в `engine.proto` (43
  метода); gateway НЕ требует кода — descriptor-bridge уже пробрасывает
  `POST /api/v1/rpc/TseReport|TseUnhide` (bridge.rs, base64/JSON-конвенции).
- WebUI-прецеденты: страницы-просмотры `nvar`, `artifacts`, `forms/[id]`;
  mutation-диалоги GatesDialog/SetValueDialog/ConfirmDialog; API-слой
  `webui/src/lib/api.ts` (типизированные обёртки над bridge).
  Правка 2026-11-04: proto-типов Tse* в `webui/src/lib/proto/engine.ts`
  НЕТ (protoc v3.19.6, 100 типов; регенерация в цикле tse-unhide не
  выполнялась) — обёртки tseReport/tseUnhide строятся на
  inline-структурных типах в api.ts (идиома большинства соседних
  обёрток `nvarList`, `setValue`, …); регенерация engine.ts — вне
  цикла (требует protoc в окружении, дифф на 11k-строчный генерат).
- TODO-записи (TODO.md, fc785e9): spf silent-data (mz32 «Inte»),
  byte-guard unchecked-add, TUI dangling `--block-offset`, COMMANDS-
  комплишен, hex-префикс `trim_start_matches`.

## 2. WebUI — маршрут `tse/[id]`

**API** (`webui/src/lib/api.ts`): `tseReport(imageId)` →
`TseReportResponse`; `tseUnhide(imageId, formsetGuid, formId,
blockPeOffset?)` → `TseUnhideResponse`. camelCase proto3-JSON, как
соседние обёртки.

**Страница** (`webui/src/routes/tse/[id]/+page.svelte` + `page.test.ts`):
- Загрузка отчёта на mount; `pending`/`error` — идиомы соседних страниц.
- **Stride-блоки**: таблица `@offset` (hex) × записи `GUID #fid`; клик по
  записи → префилл панели unhide (единственная интерактивность отчёта).
- **$SPF-карточка**: page_count, формсеты (`GUID raw=N pages=M`),
  переменные (`GUID name attrs=#x size`), string-controls, pe_len; при
  `spf === null` — заметка «no $SPF» (не пустая карточка).
- **Панель unhide**: поля formset-guid + form-id; экспертный
  block-offset — опционально, свёрнут в `<details>`; сабмит → inline
  подтверждение (паттерн ConfirmDialog: busy/disabled + повторный клик)
  → `tseUnhide` → результат `unhidden: entry @ 0x…` (entry_pe_offset)
  → refresh отчёта. Ошибки — `role="alert"` с текстом ApiError
  (NotFound «нет AMITSE»/записи, InvalidSchema ambiguous/guard — текст
  движка, без клиентской интерпретации).
- Валидация полей: guid-пустота/fid-число — до RPC; block-offset — hex
  (`0x`-префикс однократно), u32-граница.
- Навигация: вход — кнопка-ссылка «TSE» со страницы `image/[id]`;
  конкретная идиома ссылки (существующий компонент/разметка)
  определяется по факту кода в плане.

## 3. Gateway

Кода нет. Один integration-тест (`crates/uefi-gateway/tests/
integration.rs`): `POST /api/v1/rpc/TseUnhide` c валидным телом →
200 + поля ответа (mock-стаб уже есть, mock_server.rs:385); bad-guid
кейс не нужен (валидация в engine/CLI, gateway — транспорт).

## 4. Миноры

1. **класс: silent-data, spf.rs** — после детекта мага `pages_count`
   валидирует правдоподобие: `count > body.len()/4` → `tracing::warn!`
   + возврат 0 (без паники, read-only канал). `pages_fs_counts` уже
   fail-soft (первый OOB-указатель → break); `scan_formsets`/
   `scan_var_catalog` не зависят от page_count. mz32: отчёт более не
   показывает «Inte» как число страниц.
2. **класс: errors, tse.rs** — `entry_bytes_match`: `off.
   checked_add(STRIDE_ENTRY_SIZE)`; переполнение → `false` (= guard
   mismatch, отказ без мутации). Тест: прямой вызов `tse_unhide` с
   `Some(usize::MAX - 8)` → InvalidSchema «guard mismatch».
3. **класс: errors, TUI** — dangling `--block-offset` (флаг без
   значения): явная usage-ошибка вместо молчаливого авто-режима.
   Тест: `tse-unhide <guid> 1 --block-offset` → Err(usage).
4. **discoverability, TUI** — COMMANDS += `"tse"`, `"tse-unhide"`
   (таб-комплишен). Тест: константная проверка вхождения.
5. **класс: errors, CLI+TUI** — hex-парсинг `--block-offset`:
   `strip_prefix("0x")` однократно (не `trim_start_matches`) —
   `0x0x10` (повтор префикса) становится ошибкой парсинга; голый hex
   (`16` = 0x16) остаётся принятым — поле hex-only, поведение
   задокументировано. Тесты: CLI parse + TUI-команда с `0x0x10` →
   ошибка парсинга.

## 5. Тесты

- engine: unit на п.1 (фикстура маг + огромный count) и п.2;
  регрессия существующих spf/tse-тестов.
- tui: интеграционные на п.3, п.4 (константа), п.5.
- cli: parse-тест на п.5.
- gateway: integration п.3-секции (1 тест).
- webui: `page.test.ts` (msw): рендер отчёта (блоки/$SPF/no-$SPF),
  префилл по клику, unhide success (результат + refresh) и error
  (role=alert), экспертный offset. `npm run check` чист.

## 6. Гейты

`cargo test --all`; `cargo clippy --all --all-targets -- -D warnings`;
`cargo fmt --all -- --check`; webui: `npm run test`, `npm run check`.
Живой гейт не требуется (мутационный путь движка не меняется;
WebUI ездит через тот же RPC, покрытый паритет-тестом v3).

## 7. За рамками

- Аннотация hide-кандидатов в отчёте (root-фильтр — привилегия
  движка-мутатора; UI не дублирует дискриминацию).
- Обратная операция hide; TUI-отчёт таблицей; навигационные правки
  TSE (см. tse-unhide спека §7).
- Пагинация/виртуализация таблиц (объёмы блоков малы: ≤14 на образе).

## 8. Вердикт

Реализован (цикл tse-followups, ветка feat/tse-followups).
