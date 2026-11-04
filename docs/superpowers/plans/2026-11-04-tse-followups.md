# tse-followups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** WebUI-страница TSE (`tse/[id]`: отчёт + unhide, полный паритет CLI/TUI) + закрытие 5 миноров TODO-секции «Отложенное цикла tse-unhide».

**Architecture:** Движок/RPC не меняются (TseReport/TseUnhide уже в proto, 43 метода; gateway — descriptor-bridge без кода). Правки: два engine-минора (spf правдоподобие page_count, checked byte-guard), общий hex-хелпер `uefi_common::cli::parse_hex_offset` (CLI+TUI `--block-offset`), TUI-комплишен, gateway-интеграционный тест, WebUI-страница на inline-типах api.ts.

**Tech Stack:** Rust workspace (edition 2024), SvelteKit 5 (runes), msw/vitest.

**Спека:** `docs/superpowers/specs/2026-11-03-tse-followups-design.md` (с правкой от 2026-11-04: см. Task 0 — Tse*-типы в webui НЕ сгенерированы).

## Global Constraints

- Спека — authority; конфликты плана решаются в её пользу.
- TDD порядок (AGENTS.md п.5): тест (RED) → реализация (GREEN) → гейты → коммит. Новых модулей нет — module-first не требуется.
- Без narration-комментариев; короткие rustdoc `///` на pub-функциях (контракт + ссылка на спеку tse-followups §).
- Hex-семантика `--block-offset` (спека §4.5, едина для CLI/TUI): hex-only; `0x`-префикс срезается **однократно**; голый hex принят (`16` = 0x16); повтор префикса (`0x0x10`) — ошибка парсинга. Поведение пиннится тестами всех трёх слоёв (common/CLI/TUI).
- Гейты после каждой задачи: `cargo test -p <crate>` и `cargo clippy -p <crate> --all-targets -- -D warnings`.
- WebUI: Svelte 5 runes (`$state`/`$derived`/`$effect`/`$props`), proto3-JSON camelCase через `bridge`, ошибки — `role="alert"` с текстом ApiError. API-обёртки — inline-структурные типы в `api.ts` (идиома соседей; регенерация `engine.ts` вне цикла — см. Task 0).
- Один коммит на задачу; сообщения — из плана.
- Финальные гейты цикла: `cargo test --all && cargo clippy --all --all-targets -- -D warnings && cargo fmt --all -- --check && cd webui && npm run test && npm run check`. Живой гейт не требуется (спека §6).

## Review Focus

1. **Повтор `0x`-префикса / dangling `--block-offset`** — обе поверхности (CLI exit≠0, TUI Err) обязаны отказывать; тесты: Task 3 (CLI integration), Task 4 (TUI integration).
2. **pages_count bound не ломает живые значения** — граничное `count == body.len()/4` допустимо (отказ только при `>`); регресс `spf_formsets_and_var_catalog_read` (count=2) и tse-тестов (count=1) — Task 1.
3. **Точный вариант ошибки byte-guard** — InvalidSchema «guard mismatch», не паника и не NotFound; `pe_offset = usize::MAX - 8` (off+0x20 переполняет) — Task 2.
4. **WebUI block-offset за u32** (`0x100000000`) и `0x0x10` — клиентская валидация до RPC (TseUnhide не вызывается) — Task 6.
5. **WebUI refresh после unhide** — второй TseReport после успеха (иначе отчёт stale: зануленная запись всё ещё видна) — Task 6.

---

### Task 0: Правка спеки — Tse*-типы webui не сгенерированы

Дефект спеки §1: заявлено «proto-типы `webui/src/lib/proto/engine.ts` (Tse* уже сгенерированы)» — фактически файл (protoc v3.19.6, 100 типов) не содержит Tse* (регенерация в цикле tse-unhide не выполнялась). Решение (ruling): обёртки api.ts на inline-структурных типах — идиома большинства соседних обёрток (`nvarList`, `setValue`, …); регенерация engine.ts — вне цикла (требует protoc в окружении, дифф на 11k-строчный генерат).

- [ ] **Step 1: Правка спеки** — в `docs/superpowers/specs/2026-11-03-tse-followups-design.md` §1 заменить пункт о proto-типах на фактическое состояние + решение (inline-типы api.ts, без регенерации engine.ts).

- [ ] **Step 2: Коммит**

```bash
git add docs/superpowers/specs/2026-11-03-tse-followups-design.md
git commit -m "docs: fix tse-followups spec — Tse*-типы webui не сгенерированы (inline api.ts)"
```

---

### Task 1: engine — spf `pages_count` правдоподобие (класс silent-data)

Спека §4.1: после детекта мага валидировать правдоподобие `count > body.len()/4` → `tracing::warn!` + возврат 0. mz32-кейс: page_count = 0x496E7465 (ASCII "Inte") более не показывается как число страниц. `pages_fs_counts` уже fail-soft и вызывает `pages_count` — выигрывает автоматически; `scan_formsets`/`scan_var_catalog` не зависят от page_count.

**Files:**
- Modify: `crates/uefi-engine/src/hii/spf.rs` (pages_count, ~:99)

**Interfaces:**
- Produces: сигнатура `pages_count(&[u8]) -> u32` неизменна; контракт ужесточён (implausible → 0 + warn).

- [ ] **Step 1: Тест (RED)** — в `mod tests` spf.rs добавить:

```rust
    #[test]
    fn pages_count_implausible_value_returns_zero() {
        let mut b = vec![0u8; 0x100];
        b[0..4].copy_from_slice(b"$SPF");
        let mz32_false_count: u32 = 0x496E7465; // ASCII "Inte"
        b[SPF_PAGE_COUNT_OFFSET..SPF_PAGE_COUNT_OFFSET + 4]
            .copy_from_slice(&mz32_false_count.to_le_bytes());
        assert_eq!(pages_count(&b), 0);
        assert!(pages_fs_counts(&b).is_empty());
    }

    #[test]
    fn pages_count_boundary_len_div_4_accepted() {
        let mut b = vec![0u8; 0x100];
        b[0..4].copy_from_slice(b"$SPF");
        b[SPF_PAGE_COUNT_OFFSET..SPF_PAGE_COUNT_OFFSET + 4].copy_from_slice(&64u32.to_le_bytes());
        assert_eq!(pages_count(&b), 64, "count == len/4 — допустимо (отказ только при >)");
    }
```

Run: `cargo test -p uefi-engine --lib spf::` — Expected: 1 FAILED (implausible-тест: pages_count вернёт 0x496E7465, не 0) + 1 passed (boundary — pin-тест на допустимость `count == len/4`, на текущем коде уже зелёный).

- [ ] **Step 2: Реализация (GREEN)** — в `pages_count` после чтения raw-значения:

```rust
    let count = …;
    if count as usize > body.len() / 4 {
        tracing::warn!(count, len = body.len(), "spf page count implausible");
        return 0;
    }
    count
```

Run: `cargo test -p uefi-engine --lib spf::` — Expected: PASS (новые 2 + существующие).
Регресс: `cargo test -p uefi-engine --lib` (tse-тесты со spf count=1 — зелёные).

- [ ] **Step 3: Гейты + коммит**

```bash
cargo test -p uefi-engine --lib && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/src/hii/spf.rs
git commit -m "fix(engine): spf pages_count — правдоподобие против ложного \$SPF-мага mz32 (tse-followups §4.1)"
```

---

### Task 2: engine — tse byte-guard checked_add (класс errors)

Спека §4.2: `entry_bytes_match` — `off.checked_add(STRIDE_ENTRY_SIZE)`; переполнение → `false` (= guard mismatch, InvalidSchema, отказ без мутации). Сейчас `pe.get(off..off + STRIDE_ENTRY_SIZE)` паникует в debug на near-usize::MAX.

**Files:**
- Modify: `crates/uefi-engine/src/hii/tse.rs` (entry_bytes_match, ~:180)

**Interfaces:**
- Produces: приватная сигнатура `entry_bytes_match` неизменна; контракт — overflow = false.

- [ ] **Step 1: Тест (RED)** — в `mod tests` tse.rs добавить:

```rust
    #[test]
    fn tse_unhide_guard_overflow_is_mismatch_not_panic() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let err = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, Some(usize::MAX - 8))
            .unwrap_err();
        assert!(
            matches!(err, HiiError::InvalidSchema(ref m) if m.contains("guard mismatch")),
            "{err:?}"
        );
    }
```

(off = usize::MAX - 8: off + 0x20 переполняет usize.)

Run: `cargo test -p uefi-engine --lib tse:: -- --nocapture` — Expected: 1 FAILED (panic «attempt to add with overflow»).

- [ ] **Step 2: Реализация (GREEN)** — `entry_bytes_match`:

```rust
fn entry_bytes_match(pe: &[u8], off: usize, guid: &Guid, form_id: u16) -> bool {
    let Some(end) = off.checked_add(STRIDE_ENTRY_SIZE) else {
        return false;
    };
    let Some(rec) = pe.get(off..end) else {
        return false;
    };
    rec[..16] == guid.to_bytes()
        && rec[16..24] == u64::from(form_id).to_le_bytes()
        && rec[24..32] == [0u8; 8]
}
```

Run: `cargo test -p uefi-engine --lib tse::` — Expected: PASS (все существующие + 1 новый).

- [ ] **Step 3: Гейты + коммит**

```bash
cargo test -p uefi-engine --lib && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/src/hii/tse.rs
git commit -m "fix(engine): tse byte-guard — checked_add, overflow = guard mismatch (tse-followups §4.2)"
```

---

### Task 3: uefi-common + CLI — `parse_hex_offset`, 0x-префикс однократно (класс errors)

Спека §4.5 (CLI-половина): общий хелпер в `uefi-common::cli` (дом shared flag-parsing CLI/TUI), CLI-арма `Cmd::Tse::Unhide` переводится на него. `trim_start_matches("0x")` срезал все вхождения; теперь `strip_prefix` однократно.

**Files:**
- Modify: `crates/uefi-common/src/cli.rs`
- Modify: `crates/uefi-cli/src/main.rs` (~:622-632, парсинг в arm'е Cmd::Tse)
- Test: `crates/uefi-cli/tests/cli_integration.rs` (расширить `tse_report_and_unhide_roundtrip`, ~:786)

**Interfaces:**
- Produces: `pub fn parse_hex_offset(s: &str) -> Result<usize, String>` в `uefi_common::cli` (Err-текст содержит исходную строку). Используется Task 4 (TUI).

- [ ] **Step 1: Тесты хелпера (RED)** — в `mod tests` cli.rs (uefi-common) добавить:

```rust
    #[test]
    fn hex_offset_accepts_prefixed_and_bare() {
        assert_eq!(parse_hex_offset("0x1b40").unwrap(), 0x1b40);
        assert_eq!(parse_hex_offset("1b40").unwrap(), 0x1b40);
        assert_eq!(parse_hex_offset("16").unwrap(), 0x16, "голый hex принят: 16 = 0x16");
    }

    #[test]
    fn hex_offset_rejects_double_prefix_and_garbage() {
        assert!(parse_hex_offset("0x0x10").is_err());
        assert!(parse_hex_offset("zz").is_err());
        assert!(parse_hex_offset("").is_err());
        assert!(parse_hex_offset("0x").is_err());
    }
```

Run: `cargo test -p uefi-common` — Expected: компиляция падает (E0425, `parse_hex_offset` не существует) — это RED; после реализации PASS.

- [ ] **Step 2: Реализация хелпера (GREEN)** — в uefi-common/src/cli.rs:

```rust
/// Hex-only offset (CLI/TUI `--block-offset`): `0x`-префикс срезается
/// однократно; голый hex принят (`16` = 0x16); повтор префикса
/// (`0x0x10`) — ошибка парсинга. Спека tse-followups §4.5.
pub fn parse_hex_offset(s: &str) -> Result<usize, String> {
    let digits = s.strip_prefix("0x").unwrap_or(s);
    usize::from_str_radix(digits, 16).map_err(|e| format!("invalid hex offset {s:?}: {e}"))
}
```

Run: `cargo test -p uefi-common` — Expected: PASS.

- [ ] **Step 3: CLI wiring + integration-тест**

В `main.rs` arm `Cmd::Tse { sub: TseCmd::Unhide { .. } }` заменить inline-парсинг на:

```rust
let off = match block_offset.as_deref() {
    None => None,
    Some(s) => Some(uefi_common::cli::parse_hex_offset(s).map_err(|e| {
        error::AppError::new(error::ErrKind::RpcInvalidArgument, format!("--block-offset: {e}"))
    })?),
};
```

В `cli_integration.rs::tse_report_and_unhide_roundtrip` добавить (после кейса `not-a-guid`):

```rust
    cli(&sock, cwd)
        .args([
            "tse",
            "unhide",
            &image_id,
            "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9",
            "1",
            "--block-offset",
            "0x0x10",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--block-offset"));
```

Run: `cargo test -p uefi-cli` — Expected: PASS (integration упадёт, если wiring не сделан).

- [ ] **Step 4: Гейты + коммит**

```bash
cargo test -p uefi-common && cargo test -p uefi-cli && cargo clippy -p uefi-common -p uefi-cli --all-targets -- -D warnings
git add crates/uefi-common/src/cli.rs crates/uefi-cli/src/main.rs crates/uefi-cli/tests/cli_integration.rs
git commit -m "fix(cli,common): parse_hex_offset — 0x-префикс однократно, общий хелпер (tse-followups §4.5)"
```

---

### Task 4: TUI — dangling `--block-offset`, hex-префикс, COMMANDS (класс errors + discoverability)

Спека §4.3, §4.4, §4.5 (TUI-половина).

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (arm `"tse-unhide"` ~:1541-1560; `COMMANDS` ~:1612; mod tests ~:2992)
- Test: `crates/uefi-tui/tests/tui_integration.rs` (расширить `tse_commands_report_and_unhide`, ~:1519)

**Interfaces:**
- Consumes: `uefi_common::cli::parse_hex_offset` (Task 3).

- [ ] **Step 1: Тесты (RED)**

В `mod tests` commands.rs (uefi-tui) добавить:

```rust
    #[test]
    fn commands_completion_includes_tse() {
        assert!(COMMANDS.contains(&"tse"));
        assert!(COMMANDS.contains(&"tse-unhide"));
    }
```

В `tui_integration.rs::tse_commands_report_and_unhide` добавить (после кейса `tse-unhide bad`):

```rust
    assert!(
        uefi_tui::commands::execute_command(
            &mut app,
            "tse-unhide EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 1 --block-offset",
            &mut client
        )
        .await
        .is_err(),
        "dangling --block-offset без значения — usage-ошибка, не авто-режим"
    );
    assert!(
        uefi_tui::commands::execute_command(
            &mut app,
            "tse-unhide EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 1 --block-offset 0x0x10",
            &mut client
        )
        .await
        .is_err(),
        "повтор 0x-префикса — ошибка парсинга"
    );
    uefi_tui::commands::execute_command(
        &mut app,
        "tse-unhide EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 1 --block-offset 0x1b40",
        &mut client,
    )
    .await
    .unwrap();
    assert!(app.status_msg.contains("0x1b40"));
```

Run: `cargo test -p uefi-tui` — Expected: FAILED (dangling-кейс сейчас Ok — авто-режим; COMMANDS-тест падает — констант нет).

- [ ] **Step 2: Реализация (GREEN)**

Arm `"tse-unhide"`: заменить блок парсинга `off` на:

```rust
            let off = match parts.iter().position(|p| *p == "--block-offset") {
                None => None,
                Some(i) => {
                    let raw = parts.get(i + 1).ok_or(usage)?;
                    Some(uefi_common::cli::parse_hex_offset(raw).map_err(|_| usage)?)
                }
            };
```

`COMMANDS`: добавить `"tse"`, `"tse-unhide"` (после `"restore"`, перед `"help"`).

Run: `cargo test -p uefi-tui` — Expected: PASS.

- [ ] **Step 3: Гейты + коммит**

```bash
cargo test -p uefi-tui && cargo clippy -p uefi-tui --all-targets -- -D warnings
git add crates/uefi-tui/src/commands.rs crates/uefi-tui/tests/tui_integration.rs
git commit -m "fix(tui): tse-unhide — dangling --block-offset, hex-префикс, комплишен (tse-followups §4.3-4.5)"
```

---

### Task 5: gateway — интеграционный тест TseUnhide

Спека §3: кода нет; один тест `POST /api/v1/rpc/TseUnhide` с валидным телом → 200 + поля ответа. Mock-стаб уже есть (`crates/uefi-gateway/tests/mock_server.rs:385`: не-zero-guid → `{pe_offset: 0x1b40, entry_pe_offset: 0x1b40}`). bad-guid кейс не нужен.

**Files:**
- Test: `crates/uefi-gateway/tests/integration.rs`

- [ ] **Step 1: Тест** — добавить (по образцу `bridge_roundtrip_images_list`, ~:173):

```rust
#[tokio::test(flavor = "multi_thread")]
async fn bridge_tse_unhide_roundtrip() {
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
        .post(format!("{base}/api/v1/rpc/TseUnhide"))
        .header("cookie", format!("uefipatcher_session={sid}"))
        .json(&json!({
            "imageId": "img-1",
            "formsetGuid": "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9",
            "formId": 1,
            "blockPeOffset": 0x1b40
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(v["peOffset"], json!(0x1b40));
    assert_eq!(v["entryPeOffset"], json!(0x1b40));
}
```

Run: `cargo test -p uefi-gateway --test integration bridge_tse_unhide` — Expected: PASS (мост уже пробрасывает; тест фиксирует контракт).

- [ ] **Step 2: Гейты + коммит**

```bash
cargo test -p uefi-gateway && cargo clippy -p uefi-gateway --all-targets -- -D warnings
git add crates/uefi-gateway/tests/integration.rs
git commit -m "test(gateway): TseUnhide bridge-интеграция (tse-followups §3)"
```

---

### Task 6: WebUI — API-обёртки + страница `tse/[id]` + вход с image/[id]

Спека §2. Страница-просмотр с единственной интерактивностью отчёта (клик → префилл) + панель unhide с inline-подтверждением.

**Files:**
- Modify: `webui/src/lib/api.ts` (+типы, +2 обёртки)
- Create: `webui/src/routes/tse/[id]/+page.ts`
- Create: `webui/src/routes/tse/[id]/+page.svelte`
- Create: `webui/src/routes/tse/[id]/page.test.ts`
- Modify: `webui/src/routes/image/[id]/+page.svelte` (toolbar, ~:154)

**Interfaces:**
- Consumes: `bridge` (api.ts), RPC `TseReport`/`TseUnhide` (proto3-JSON camelCase).
- Produces:
  - `api.ts`: `export interface TseStrideEntry/StrideBlock`-набор + `export const tseReport(imageId)`, `export const tseUnhide(imageId, formsetGuid, formId, blockPeOffset?)`.
  - Маршрут `/tse/[imageId]`.

- [ ] **Step 1: API-обёртки (api.ts)** — inline-типы по образцу соседей:

```ts
export interface TseStrideEntry { formsetGuid: string; formId: string; }
export interface TseStrideBlock { peOffset: number; entries: TseStrideEntry[]; }
export interface TseSpfFormset { guid: string; rawU32: number; pages: number; }
export interface TseSpfVar { guid: string; name: string; attrs: number; size: number; }
export interface TseSpfSummary {
    pageCount: number; formsets: TseSpfFormset[]; vars: TseSpfVar[]; stringControls: number;
}
export interface TseReportResponse { blocks: TseStrideBlock[]; spf?: TseSpfSummary | null; peLen: number; }
```

(`formId` — строка: proto-поле `uint64 form_id`, gateway отдаёт канонический proto3-JSON (64-битные — строки; прецедент `NvarVarInfo.offset` в `nvar.ts`).)

export const tseReport = (imageId: string) =>
    bridge<{ imageId: string }, TseReportResponse>('TseReport', { imageId });

export const tseUnhide = (
    imageId: string,
    formsetGuid: string,
    formId: number,
    blockPeOffset?: number,
) =>
    bridge<
        { imageId: string; formsetGuid: string; formId: number; blockPeOffset?: number },
        { peOffset: number; entryPeOffset: number }
    >('TseUnhide', { imageId, formsetGuid, formId, blockPeOffset });
```

- [ ] **Step 2: `+page.ts`** — идентично nvar:

```ts
import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => ({ imageId: params.id });
```

- [ ] **Step 3: `+page.svelte`** — структура (runes, идиомы nvar/`image/[id]`; loadSeq-guard на загрузку):

  - `refresh()` → `tseReport(imageId)`; `$effect` на `imageId`. `pending` — «loading…» пока нет отчёта и нет ошибки; ошибка — `<p class="error" role="alert">`.
  - **Stride-блоки**: `<table>`; строка на запись: `@{block.peOffset.toString(16)}` (hex), `{entry.formsetGuid} #{entry.formId}`; запись — `<button>` (клик → префилл: `unhideGuid = entry.formsetGuid; unhideFid = entry.formId` — formId уже строка).
  - **$SPF-карточка**: при `spf` — список `pageCount`, `formsets` (`{guid} raw={rawU32} pages={pages}`), `vars` (`{guid} {name} attrs=#{attrs.toString(16)} size={size}`), `stringControls`, `peLen`; при `spf == null` — заметка `no $SPF` (не пустая карточка).
  - **Панель unhide** (`<section aria-label="tse unhide">`): поля `formset-guid` (text), `form-id` (text); экспертный `block-offset` в `<details><input placeholder="hex, e.g. 1b40">…`; inline-подтверждение на busy/disabled-идиоме ConfirmDialog: первый сабмит ставит `armed` (кнопка «Unhide — confirm»), повторный — RPC; смена любого поля сбрасывает `armed`; `busy` — disabled.
  - Валидация до RPC (ошибка в тот же `role="alert"`): guid пустой → «formset guid required»; fid не `/^\d+$/` → «form id must be a number»; block-offset: `strip one leading "0x"` → `/^[0-9a-f]+$/i` (case-insensitive — паритет с движковым `from_str_radix`, принимающим A-F; правка plan-review финального ревью) → `parseInt(·, 16)` → `> 0xFFFFFFFF` → «block offset exceeds u32»; `0x0x10` падает на regex.
  - Сабмит: `tseUnhide(...)` → результат `unhidden: entry @ 0x{entryPeOffset.toString(16)}` (`role="status"`) → `refresh()`. Ошибки RPC — текст ApiError как есть (движок — source of truth).
  - Кнопка-ссылка назад не нужна (layout nav); Download не входит (image/[id] уже имеет).

- [ ] **Step 4: Вход с image/[id]** — в toolbar после «Switch/upload…»:

```svelte
    <button onclick={() => goto(`/tse/${imageId}`)}>TSE</button>
```

- [ ] **Step 5: Тесты `page.test.ts` (msw)** — по образцу nvar page.test.ts; handlers:

```ts
const spf = {
    pageCount: 214,
    formsets: [{ guid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9', rawU32: 2, pages: 161 }],
    vars: [{ guid: '899407D7-99FE-43D8-9A21-79EC328CAC21', name: 'Setup', attrs: 3, size: 148 }],
    stringControls: 4,
};
const report = {
    blocks: [{
        peOffset: 0x1b40,
        entries: [
            { formsetGuid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9', formId: '1' },
            { formsetGuid: '7B59104A-C00D-4158-87FF-F04D6396A915', formId: '10000' },
        ],
    }],
    spf, peLen: 0x48860,
};
```

  (formId — строки: uint64 в proto3-JSON; см. Step 1.)

  Тесты (по спеке §5):
  1. **рендер с $SPF**: виден `@1b40`, обе записи `#1`/`#10000`, `raw=2 pages=161`, `Setup`, `no $SPF` отсутствует.
  2. **рендер без $SPF** (handler вариант `spf: null`): заметка `no $SPF`.
  3. **префилл**: клик по записи → input(guid) == entry.formsetGuid, input(fid) == '1'.
  4. **unhide success**: сабмит (дважды — armed) → `role="status"` содержит `unhidden: entry @ 0x1b40`; TseReport вызван повторно (счётчик ≥ 2 — refresh).
  5. **unhide error**: TseUnhide → 400 `{error: 'no AMITSE setup browser module', code: 'NOT_FOUND'}` → `role="alert"` содержит текст движка.
  6. **экспертный offset**: открыть details, ввести `1b40`, сабмит дважды → захваченное тело запроса содержит `blockPeOffset: 0x1b40`.
  7. **валидация offset**: `0x0x10` → alert, TseUnhide не вызывался (счётчик 0); `0x100000000` → alert «exceeds u32», вызовов 0.
  8. **валидация полей**: пустой guid / fid `abc` → alert до RPC (вызовов 0).

Run: `cd webui && npm run test -- tse` — Expected: PASS.
Run: `cd webui && npm run check` — Expected: 0 errors.

- [ ] **Step 6: Гейты + коммит**

```bash
cd webui && npm run test && npm run check
git add webui/src/lib/api.ts webui/src/routes/tse webui/src/routes/image/[id]/+page.svelte
git commit -m "feat(webui): TSE-страница tse/[id] — отчёт + unhide, вход с image (tse-followups §2)"
```

---

### Task 7: docs — закрытие TODO-записей + вердикт спеки

**Files:**
- Modify: `TODO.md` (секция «Отложенное цикла tse-unhide (2026-10-02)», :4871-4910)
- Modify: `docs/superpowers/specs/2026-11-03-tse-followups-design.md` (§8)

- [ ] **Step 1: TODO.md** — каждую из 5 записей секции пометить `[x]` + строка «Закрыто циклом tse-followups (2026-11-04): …» (одна строка на запись: spf правдоподобие / checked byte-guard / TUI dangling flag / COMMANDS / parse_hex_offset; WebUI-страница — не TODO-запись, в TODO не добавляется).

- [ ] **Step 2: Спека §8** — вердикт: «Реализован (цикл tse-followups, ветка feat/tse-followups)».

- [ ] **Step 3: Коммит**

```bash
git add TODO.md docs/superpowers/specs/2026-11-03-tse-followups-design.md
git commit -m "docs: закрытие TODO tse-followups + вердикт спеки"
```

---

### Финальные гейты цикла

```bash
cargo test --all
cargo clippy --all --all-targets -- -D warnings
cargo fmt --all -- --check
cd webui && npm run test && npm run check
```

Roadmap-запись мини-цикла (с PR# и merge-hash) — отдельным коммитом после мержа PR (паттерн tse-unhide: 3fdbf97), вне задач плана.
