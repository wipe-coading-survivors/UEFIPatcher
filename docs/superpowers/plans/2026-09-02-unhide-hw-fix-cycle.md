# Цикл фиксов по аппаратной валидации unhide (HNX99TF) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Заменить LZMA-кодировщик движка на liblzma-класс с приоритетом вписывания в исходный слот секции, добавить PRC-патч строк раскрытой формы, полный обход string-пакетов и три минора — по итогам аппаратной валидации 2026-09-02.

**Architecture:** Кодировщик `compress.rs` строит raw-LZMA1-поток через `liblzma` (системная библиотека) и собственный 13-байтовый alone-заголовок с точным size; свип параметров pb/lc с ранним выходом по бюджету слота интегрируется в `build_recompressed_guided`. PRC-патч: чистый планировщик (`plan_prc_entries`) + probe-on-clone для атомарности до IFR-мутации в `set_item_visibility` (PE32-ветка). Полный обход строк — устранение ранних `return` в walker'е + новый API `collect_string_packages`. Insert-at-id — разрезание SKIP-пропусков в SIBT для body- и resource-каналов.

**Tech Stack:** Rust workspace (edition 2024), крейт `liblzma` (новая зависимость, системная liblzma), `lzma-rs` (только декодер, без изменений), `r-efi` (IFR-константы/структуры), tonic RPC.

**Spec:** `docs/superpowers/specs/2026-09-02-unhide-hw-fix-cycle-design.md` (утверждена, коммит `edd7d30`). Источник находок: `docs/reports/2026-09-02-hw-validation-hnx99tf.md`.

## Global Constraints

- AGENTS.md обязательны: TDD-порядок шагов, один коммит на шаг с сообщением из задачи, **никаких комментариев в коде** (кроме ссылок `file:line` на референс), `cargo test -p <crate>` + `cargo clippy -p <crate> -- -D warnings` после каждой задачи.
- **Правило plan-defect (AGENTS.md п.11):** расхождение плана с реальностью (несуществующий API, неверный тип, тест не может пройти) — сначала отдельный коммит `docs: fix Task N in unhide-hw-fix-cycle plan (<дефекты>)`, затем реализация.
- `lzma-rs` остаётся **только декодером** (`decompress.rs` не меняется); кодирование — `liblzma`.
- IFR-структуры/константы — только `r_efi::hii::*`, свои не определять.
- Edition 2024: let-chains (`if let ... && ...`) разрешены и уже используются в кодовой базе.
- Real-image тесты — `#[ignore]`, запускаются явно при наличии `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`:
  `cargo test -p uefi-engine --test real_image -- --ignored`.
- Имена RPC-ошибок: `IdOccupied`/`PrcPatchUnsupported` → `failed_precondition`.
- Контейнер: если `/run/.containerenv` — сборка через `podman-remote` с `docker/rust-builder.containerfile`.

---

### Task 1: Зависимость liblzma + API-проба raw-LZMA1

**Files:**
- Modify: `Cargo.toml` (workspace, секция `[workspace.dependencies]`)
- Modify: `crates/uefi-engine/Cargo.toml`
- Modify: `docker/rust-builder.containerfile`
- Test: `crates/uefi-engine/src/compress.rs` (модуль `tests`)

**Interfaces:**
- Consumes: ничего (первая задача).
- Produces: зависимость `liblzma` в `uefi-engine`; доказательство API: `LzmaOptions` (сеттеры `dict_size`/`literal_context_bits`/`literal_position_bits`/`position_bits`/`nice_len`/`match_finder`/`mode`), `Filters::new().lzma1(&opts)`, `Stream::new_raw_encoder(&filters)`, `Stream::process_vec(input, &mut out, Action::Finish) -> Result<Status, _>`, `Stream::total_in()`. Побочное доказательство: lzma-rs декодирует поток raw-энкодера + ручной заголовок с точным size, включая добивку нулями за потоком.

Спека §4.1/§7: API-проверка — первый шаг; запасной путь при недоступности API — прямой `liblzma-sys` (`lzma_raw_encoder` + `lzma_options_lzma`), это тот же пакет. Версия крейта на docs.rs — 0.4.x, методы подтверждены: `Stream::new_raw_encoder`, `LzmaOptions::new_preset`, `Filters::lzma1`, `process_vec`, `total_in`; енумы `Action`, `Status`, `Mode`, `MatchFinder` в `liblzma::stream`.

- [ ] **Step 1: Добавить зависимость**

`Cargo.toml` (корень), в `[workspace.dependencies]` после строки `lzma-rs = "0.3"`:

```toml
liblzma = "0.4"
```

`crates/uefi-engine/Cargo.toml`, в `[dependencies]` после `lzma-rs = "0.3"`:

```toml
liblzma.workspace = true
```

`docker/rust-builder.containerfile`:

```dockerfile
FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y rust cargo protobuf-compiler make gcc xz-devel && dnf clean all
WORKDIR /app
```

- [ ] **Step 2: Написать проваливающуюся пробу**

В `crates/uefi-engine/src/compress.rs`, в конец модуля `mod tests`:

```rust
    #[test]
    fn liblzma_raw_lzma1_custom_props_decode_with_known_size_header() {
        use liblzma::stream::{Action, Filters, LzmaOptions, MatchFinder, Mode, Status, Stream};

        let input: Vec<u8> = DECOMPRESSED.to_vec();
        let mut opts = LzmaOptions::new_preset(6).unwrap();
        opts.dict_size(0x0100_0000)
            .literal_context_bits(0)
            .literal_position_bits(0)
            .position_bits(0)
            .nice_len(273)
            .match_finder(MatchFinder::Bt4)
            .mode(Mode::Normal);
        let mut filters = Filters::new();
        filters.lzma1(&opts);
        let mut enc = Stream::new_raw_encoder(&filters).unwrap();

        let mut raw = Vec::new();
        loop {
            raw.reserve(64 * 1024);
            let consumed = enc.total_in() as usize;
            let status = enc
                .process_vec(&input[consumed..], &mut raw, Action::Finish)
                .unwrap();
            if matches!(status, Status::StreamEnd) {
                break;
            }
        }
        assert!(!raw.is_empty());

        let mut alone = vec![0x00u8];
        alone.extend_from_slice(&0x0100_0000u32.to_le_bytes());
        alone.extend_from_slice(&(input.len() as u64).to_le_bytes());
        alone.extend_from_slice(&raw);
        alone.extend_from_slice(&[0u8; 16]);

        let decoded = crate::decompress::decompress(&alone, 2).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }
```

- [ ] **Step 3: Запустить пробу**

Run: `cargo test -p uefi-engine liblzma_raw_lzma1`
Expected: PASS (зависимость добавлена в Step 1, проба проверяет поведение, а не отсутствие кода). Если компиляция падает по именам API (сигнатуры/варианты енумов отличаются от docs.rs 0.4.x) — см. Step 4.

- [ ] **Step 4: Разбор неудачи пробы (только при падении)**

Если `Stream::process_vec`/`MatchFinder::Bt4`/etc. не компилируются: сверить точные имена на docs.rs `liblzma::stream` (крейт зеркалирует xz2) и исправить пробу. Это часть API-пробы, а не дефект плана — коммит один на задачу. Если безопасная обёртка вообще не экспонирует lc/lp/pb raw-LZMA1 — перейти на прямой `liblzma-sys` (`lzma_raw_encoder` с `lzma_options_lzma`) и зафиксировать это отдельным коммитом `docs: fix Task 1 in unhide-hw-fix-cycle plan (liblzma safe wrapper lacks raw lzma1 params)` до правки кода.

- [ ] **Step 5: Полный прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/uefi-engine/Cargo.toml docker/rust-builder.containerfile crates/uefi-engine/src/compress.rs
git commit -m "build: liblzma dependency + raw-LZMA1 API probe (xz-devel in rust-builder)"
```

---

### Task 2: Кодировщик LzmaEncodeParams + alone-заголовок (замена lzma-rs)

**Files:**
- Modify: `crates/uefi-engine/src/compress.rs`

**Interfaces:**
- Consumes: `liblzma` (Task 1), `crate::decompress::decompress`.
- Produces:
  - `pub enum LzmaMatchFinder { Bt2, Bt3, Bt4, Hc4 }` (дефолт `Bt4`)
  - `pub struct LzmaEncodeParams { pub lc: u32, pub lp: u32, pub pb: u32, pub dict_size: u32, pub nice_len: u32, pub mf: LzmaMatchFinder }` + `Default` (lc0/lp0/pb0, dict 16 МиБ = `0x0100_0000`, nice_len 273, Bt4)
  - `pub fn lzma_props_byte(lc: u32, lp: u32, pb: u32) -> u8` — `(pb*5 + lp)*9 + lc`
  - `pub fn compress_lzma(input: &[u8]) -> Result<Vec<u8>, CompressError>` — сигнатура/семантика прежние, внутри — дефолтные параметры нового кодировщика
  - приватные `fn encode_raw_lzma1(...)`, `fn alone_stream(...)`, `fn round_trip_check(...)`

Спека §4.1: props-байт `(pb*5+lp)*9+lc`, dict u32 LE, size u64 LE точный; дефолты lc0/lp0/pb0/dict 16МиБ/MF_BT4/nice 273; round-trip через `decompress::decompress` остаётся. Существующий тест `compress_lzma_writes_edk2_alone_header` зафиксировал параметры lzma-rs (props 0x5D, dict 8 МиБ) — по спеке дефолты меняются на 0x00/16 МиБ, тест переписывается в этой задаче (намеренное изменение поведения, цикл существует ради него).

- [ ] **Step 1: Переписать тесты под новые дефолты**

В `crates/uefi-engine/src/compress.rs` заменить тест `compress_lzma_writes_edk2_alone_header` целиком и добавить новые:

```rust
    #[test]
    fn compress_lzma_writes_alone_header_with_hw_proven_defaults() {
        let out = compress_lzma(&[0x00; 64]).unwrap();
        assert!(out.len() > 13);
        assert_eq!(out[0], 0x00);
        assert_eq!(&out[1..5], &0x0100_0000u32.to_le_bytes());
        assert_eq!(&out[5..13], &64u64.to_le_bytes());
    }

    #[test]
    fn lzma_props_byte_encoding() {
        assert_eq!(lzma_props_byte(0, 0, 0), 0x00);
        assert_eq!(lzma_props_byte(3, 0, 2), 0x5D);
        assert_eq!(lzma_props_byte(0, 0, 1), 0x2D);
        assert_eq!(lzma_props_byte(2, 0, 0), 0x02);
    }

    #[test]
    fn encode_raw_lzma1_respects_pb_param_in_props() {
        let p = LzmaEncodeParams {
            pb: 2,
            ..LzmaEncodeParams::default()
        };
        let out = alone_stream(&[0x41; 4096], &p).unwrap();
        assert_eq!(out[0], 0x5A);
    }
```

Тесты `compress_lzma_empty_input_errors`, `compress_lzma_round_trips_real_blob`, `compress_lzma_output_decodable_by_engine` и проба Task 1 остаются без изменений.

- [ ] **Step 2: Запустить, убедиться в провале новых**

Run: `cargo test -p uefi-engine compress_lzma_writes_alone_header`
Expected: FAIL — `assert_eq!(out[0], 0x00)` (сейчас 0x5D), и компиляция: `alone_stream`/`LzmaEncodeParams` не определены.

- [ ] **Step 3: Реализовать кодировщик**

Заменить тело `compress_lzma` и добавить структуры (старый код с `lzma_rs::lzma_compress_with_options` удаляется полностью):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LzmaMatchFinder {
    Bt2,
    Bt3,
    Bt4,
    Hc4,
}

impl LzmaMatchFinder {
    fn to_liblzma(self) -> liblzma::stream::MatchFinder {
        match self {
            LzmaMatchFinder::Bt2 => liblzma::stream::MatchFinder::BinaryTree2,
            LzmaMatchFinder::Bt3 => liblzma::stream::MatchFinder::BinaryTree3,
            LzmaMatchFinder::Bt4 => liblzma::stream::MatchFinder::BinaryTree4,
            LzmaMatchFinder::Hc4 => liblzma::stream::MatchFinder::HashChain4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LzmaEncodeParams {
    pub lc: u32,
    pub lp: u32,
    pub pb: u32,
    pub dict_size: u32,
    pub nice_len: u32,
    pub mf: LzmaMatchFinder,
}

impl Default for LzmaEncodeParams {
    fn default() -> Self {
        Self {
            lc: 0,
            lp: 0,
            pb: 0,
            dict_size: 0x0100_0000,
            nice_len: 273,
            mf: LzmaMatchFinder::Bt4,
        }
    }
}

pub fn lzma_props_byte(lc: u32, lp: u32, pb: u32) -> u8 {
    ((pb * 5 + lp) * 9 + lc) as u8
}

fn encode_raw_lzma1(input: &[u8], p: &LzmaEncodeParams) -> Result<Vec<u8>, CompressError> {
    use liblzma::stream::{Action, Filters, LzmaOptions, Status, Stream};
    let mut opts = LzmaOptions::new_preset(6).map_err(|_| CompressError::CompressFailed)?;
    opts.dict_size(p.dict_size)
        .literal_context_bits(p.lc)
        .literal_position_bits(p.lp)
        .position_bits(p.pb)
        .nice_len(p.nice_len)
        .match_finder(p.mf.to_liblzma())
        .mode(liblzma::stream::Mode::Normal);
    let mut filters = Filters::new();
    filters.lzma1(&opts);
    let mut enc =
        Stream::new_raw_encoder(&filters).map_err(|_| CompressError::CompressFailed)?;
    let mut out = Vec::new();
    loop {
        out.reserve(64 * 1024);
        let consumed = (enc.total_in() as usize).min(input.len());
        let status = enc
            .process_vec(&input[consumed..], &mut out, Action::Finish)
            .map_err(|_| CompressError::CompressFailed)?;
        if matches!(status, Status::StreamEnd) {
            break;
        }
    }
    Ok(out)
}

fn alone_stream(input: &[u8], p: &LzmaEncodeParams) -> Result<Vec<u8>, CompressError> {
    let mut out = Vec::with_capacity(13 + input.len() / 2);
    out.push(lzma_props_byte(p.lc, p.lp, p.pb));
    out.extend_from_slice(&p.dict_size.to_le_bytes());
    out.extend_from_slice(&(input.len() as u64).to_le_bytes());
    out.extend_from_slice(&encode_raw_lzma1(input, p)?);
    Ok(out)
}

fn round_trip_check(input: &[u8], stream: &[u8]) -> Result<(), CompressError> {
    let decoded = crate::decompress::decompress(stream, 2)
        .map_err(|_| CompressError::RoundTripFailed)?;
    if decoded.as_slice() != input {
        tracing::warn!(size = input.len(), "lzma round-trip mismatch");
        return Err(CompressError::RoundTripFailed);
    }
    Ok(())
}

pub fn compress_lzma(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    if input.is_empty() {
        return Err(CompressError::EmptyInput);
    }
    let out = alone_stream(input, &LzmaEncodeParams::default())?;
    round_trip_check(input, &out)?;
    Ok(out)
}
```

Варианты `MatchFinder` в liblzma 0.4.8 (проверено по исходникам крейта, `stream.rs`): `BinaryTree2/BinaryTree3/BinaryTree4`, `HashChain3/HashChain4` — коротких имён `Bt2/Bt3/Bt4/Hc4` нет, маппинг выше уже исправлен.

- [ ] **Step 4: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (все тесты compress.rs + проба Task 1), no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/compress.rs
git commit -m "feat(uefi-engine): liblzma raw-LZMA1 encoder with custom props replaces lzma-rs compressor"
```

---

### Task 3: Свип compress_lzma_fit с бюджетом слота

**Files:**
- Modify: `crates/uefi-engine/src/compress.rs`

**Interfaces:**
- Consumes: `alone_stream`, `round_trip_check`, `LzmaEncodeParams` (Task 2).
- Produces: `pub fn compress_lzma_fit(input: &[u8], budget: Option<usize>) -> Result<Vec<u8>, CompressError>` — сетка pb∈0..=2 × lc∈0..=4 (lp=0), старт pb0/lc0; при `budget` — ранний выход на первом влезающем кандидате, возврат добитым нулями до `budget`; иначе минимальный кандидат. Выбранный кандидат проходит round-trip.

Спека §4.1: свип с приоритетом вписывания; §5: «no candidate ≤ budget» — не ошибка.

- [ ] **Step 1: Написать проваливающиеся тесты**

В модуль `tests` файла `compress.rs`:

```rust
    fn incompressible(len: usize) -> Vec<u8> {
        let mut x: u64 = 0x243F_6A88_85A3_08D3;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 32) as u8
            })
            .collect()
    }

    #[test]
    fn compress_lzma_fit_fits_budget_and_pads_with_zeros() {
        let first = compress_lzma(DECOMPRESSED).unwrap();
        let budget = first.len();
        let out = compress_lzma_fit(DECOMPRESSED, Some(budget)).unwrap();
        assert_eq!(out.len(), budget);
        assert_eq!(&out[5..13], &(DECOMPRESSED.len() as u64).to_le_bytes());
        let decoded = crate::decompress::decompress(&out, 2).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }

    #[test]
    fn compress_lzma_fit_padded_tail_decodes() {
        let first = compress_lzma(DECOMPRESSED).unwrap();
        let padded = compress_lzma_fit(DECOMPRESSED, Some(first.len() + 64)).unwrap();
        assert_eq!(padded.len(), first.len() + 64);
        let decoded = crate::decompress::decompress(&padded, 2).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }

    #[test]
    fn compress_lzma_fit_without_budget_returns_minimal_candidate() {
        let data = incompressible(4096);
        let first = compress_lzma(&data).unwrap();
        let out = compress_lzma_fit(&data, None).unwrap();
        assert!(out.len() <= first.len());
        let decoded = crate::decompress::decompress(&out, 2).unwrap();
        assert_eq!(decoded.as_slice(), data);
    }

    #[test]
    fn compress_lzma_fit_unreachable_budget_still_returns_stream() {
        let data = incompressible(512);
        let out = compress_lzma_fit(&data, Some(13)).unwrap();
        assert!(out.len() > 13);
        let decoded = crate::decompress::decompress(&out, 2).unwrap();
        assert_eq!(decoded.as_slice(), data);
    }

    #[test]
    fn compress_lzma_fit_prefers_pb0_for_aligned_payload() {
        let data: Vec<u8> = (0..2048u32).flat_map(|i| i.to_le_bytes()).collect();
        let out = compress_lzma_fit(&data, None).unwrap();
        assert_eq!(out[0], 0x00);
    }
```

- [ ] **Step 2: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine compress_lzma_fit`
Expected: FAIL — `compress_lzma_fit` не определена.

- [ ] **Step 3: Реализовать свип**

В `compress.rs` после `compress_lzma`:

```rust
pub fn compress_lzma_fit(
    input: &[u8],
    budget: Option<usize>,
) -> Result<Vec<u8>, CompressError> {
    if input.is_empty() {
        return Err(CompressError::EmptyInput);
    }
    let mut best: Option<Vec<u8>> = None;
    for pb in 0..=2u32 {
        for lc in 0..=4u32 {
            let params = LzmaEncodeParams {
                lc,
                lp: 0,
                pb,
                ..LzmaEncodeParams::default()
            };
            let stream = alone_stream(input, &params)?;
            if let Some(b) = budget
                && stream.len() <= b
            {
                let mut padded = stream;
                padded.resize(b, 0x00);
                round_trip_check(input, &padded)?;
                return Ok(padded);
            }
            if best.as_ref().is_none_or(|s| stream.len() < s.len()) {
                best = Some(stream);
            }
        }
    }
    let stream = best.expect("sweep grid is non-empty");
    if let Some(b) = budget
        && stream.len() > b
    {
        tracing::debug!(
            size = stream.len(),
            budget = b,
            "lzma sweep: no candidate fits budget, using minimal"
        );
    }
    round_trip_check(input, &stream)?;
    Ok(stream)
}
```

- [ ] **Step 4: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/compress.rs
git commit -m "feat(uefi-engine): compress_lzma_fit sweep with slot budget priority"
```

---

### Task 4: Интеграция slot-fit в билдер (build_recompressed_guided)

**Files:**
- Modify: `crates/uefi-engine/src/builder/mod.rs:162-185`

**Interfaces:**
- Consumes: `compress_lzma_fit` (Task 3).
- Produces: `build_recompressed_guided` пересжимает с бюджетом `node.body.len() - prefix_len`; влезли — размер секции байт-в-байт равен исходному (total из `set_section_size` совпадает с оригинальным, файлы не сдвигаются, pad-file-механика не участвует); не влезли — прежняя раскладка роста (E7-стиль).

Спека §4.1 (интеграция в билдер), §6(c)/(e).

- [ ] **Step 1: Написать проваливающиеся тесты**

В модуль `tests` файла `builder/mod.rs` (по образцу `guided_lzma_dirty_child_recompresses_and_materializes`, `builder/mod.rs:470`):

```rust
    #[test]
    fn guided_lzma_rebuild_fits_original_slot() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let mut node = crate::parser::section::parse_section(section, 0).unwrap();
        assert!(!node.children.is_empty());
        let child = &mut node.children[0];
        child.action = Action::Replace;
        child.children.clear();
        child.body = vec![0x42; 24];
        node.action = Action::Rebuild;
        let mut out = Vec::new();
        build_section(&node, &mut out).unwrap();
        assert_eq!(
            out.len(),
            section.len(),
            "recompressed section must occupy the original slot"
        );
        let rebuilt = crate::parser::section::parse_section(&out, 0).unwrap();
        assert_eq!(rebuilt.body.len(), node.body.len());
        assert_eq!(rebuilt.children.len(), node.children.len());
        assert_eq!(rebuilt.children[0].body, node.children[0].body);
        assert_eq!(rebuilt.children[0].subtype, node.children[0].subtype);
    }

    #[test]
    fn guided_lzma_growth_layout_when_payload_exceeds_slot() {
        let mut body = vec![0u8; 24];
        body[16..18].copy_from_slice(&24u16.to_le_bytes());
        let mut payload = Vec::new();
        let mut x: u64 = 0x243F_6A88_85A3_08D3;
        for _ in 0..4096 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            payload.push((x >> 32) as u8);
        }
        let mut node = guided_node(crate::ffs::lzma_guid(), Action::Rebuild);
        node.body = body;
        node.children = vec![FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_RAW,
            offset: 0,
            header: vec![0u8; 4],
            body: payload,
            tail: vec![],
            children: vec![],
            action: Action::Replace,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }];
        let mut out = Vec::new();
        build_section(&node, &mut out).unwrap();
        assert!(out.len() > node.header.len() + 20);
        assert!(crate::parser::section::parse_section(&out, 0).is_ok());
    }
```

(`crate::ffs::lzma_guid()` — публичный хелпер, `crates/uefi-engine/src/ffs.rs:22`; бюджет слота = `body.len() - prefix_len` = 4 байта — несжимаемый payload обязан не влезть и включить раскладку роста. Payload исходного фикстура (210 байт 16-битного кода) liblzma не помещает обратно в 162-байтный слот — минимум по всей сетке pb0..2 × lc0..4 × lp0..2 × mf{bt2,bt3,bt4,hc4} × nice{64,128,273} = 166/167 байт; исходный поток создан более экономным OEM-энкодером. Поэтому слот-фит закреплён на сжимаемом payload `0x42×24` с `Action::Replace` (Replace также перезаписывает size в заголовке дочерней секции — verbatim NoAction оставил бы устаревший размер 0xD2 и re-parse падал).)

- [ ] **Step 2: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine guided_lzma_rebuild_fits`
Expected: FAIL — `out.len() != section.len()` (текущая рекомпрессия не сохраняет слот: поток другого размера без добивки).

- [ ] **Step 3: Реализовать**

В `builder/mod.rs` в `build_recompressed_guided` (`builder/mod.rs:162-185`): перенести гварды размера выше компрессии и заменить вызов `compress_lzma` на `compress_lzma_fit` с бюджетом:

```rust
fn build_recompressed_guided(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    if node.body.len() < 18 {
        return Err(BuilderError::RecompressionUnsupported);
    }
    let data_offset = u16::from_le_bytes([node.body[16], node.body[17]]) as usize;
    let prefix_len = data_offset.wrapping_sub(4);
    if !(20..=node.body.len()).contains(&prefix_len) {
        return Err(BuilderError::RecompressionUnsupported);
    }
    let mut children = Vec::new();
    for child in &node.children {
        build_node(child, &mut children)?;
        let target = align4(children.len());
        pad_to(&mut children, target, 0x00);
    }
    let budget = node.body.len() - prefix_len;
    let stream = compress_lzma_fit(&children, Some(budget))?;
    let mut header = node.header.clone();
    let total = header.len() + prefix_len + stream.len();
    set_section_size(&mut header, total);
    out.extend_from_slice(&header);
    out.extend_from_slice(&node.body[..prefix_len]);
    out.extend_from_slice(&stream);
    Ok(())
}
```

Импорт вверху файла: `use crate::compress::compress_lzma;` заменить на `use crate::compress::compress_lzma_fit;`.

- [ ] **Step 4: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (включая `guided_lzma_dirty_child_recompresses_and_materializes`, `guided_lzma_remove_only_child_errors_empty_input`), no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/builder/mod.rs
git commit -m "feat(uefi-engine): guided LZMA rebuild fits original section slot (E3-style minimal diff)"
```

---

### Task 5: Полный обход string-пакетов + collect_string_packages

**Files:**
- Modify: `crates/uefi-engine/src/hii/strings.rs:230-284`

**Interfaces:**
- Consumes: `parse_string_package`, `is_string_package`, `declared_len_sane`, `hii_resource_blobs` (существующие).
- Produces:
  - `pub(crate) enum StringPackageChannel { Bare, Resource }`
  - `pub(crate) struct StringPackageRef { pub file_guid: Option<Guid>, pub channel: StringPackageChannel, pub section_path: Vec<usize>, pub language: String, pub strings: Vec<(u16, String)> }`
  - `pub(crate) fn collect_string_packages(image: &Image) -> Vec<StringPackageRef>`
  - `collect_strings(&Image) -> Vec<StringInfo>` — плоская проекция над `collect_string_packages` (протокол RPC `HiiListStrings` не меняется)

Спека §4.2: все bare string-package секции **и** все STRING-пакеты всех resource-блобов всех PE32-файлов; закрывает minor «found-флаг». `string_package_section_path` (для `add_strings`) не трогать — у неё своя семантика «первый bare-пакет».

- [ ] **Step 1: Написать проваливающиеся тесты**

В модуль `tests` файла `hii/strings.rs` (хелпер `make_pkg` уже есть, `strings.rs:296`):

```rust
    fn pkg_wrap(kind: u8, payload: &[u8]) -> Vec<u8> {
        let len = 4 + payload.len();
        let mut b = vec![
            (len & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            ((len >> 16) & 0xFF) as u8,
            kind,
        ];
        b.extend_from_slice(payload);
        b
    }

    fn res_list(guid: &crate::types::Guid, pkgs: &[&[u8]]) -> Vec<u8> {
        let mut b = guid.to_bytes().to_vec();
        let total: usize = 20 + pkgs.iter().map(|p| p.len()).sum::<usize>() + 4;
        b.extend_from_slice(&(total as u32).to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        b
    }

    #[test]
    fn collect_strings_traverses_all_packages_and_files() {
        let sibt_a = [
            SIBT_STRING_SCSU, b'A', 0,
            SIBT_END,
        ];
        let pkg_a = make_pkg("en", &sibt_a);
        let sibt_en = [
            SIBT_STRING_SCSU, b'H', b'i', 0,
            SIBT_END,
        ];
        let pkg_en = make_pkg("en-US", &sibt_en);
        let sibt_tok = [
            SIBT_STRING_SCSU, b'P', b'R', b'C', 0,
            SIBT_END,
        ];
        let pkg_tok = make_pkg("x-UEFI-AMI", &sibt_tok);
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let blob = res_list(&g, &[&pkg_en, &pkg_tok]);
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);

        let ga = Guid::try_parse("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let bare = mk_section(0x19, pkg_a.clone());
        let mut file_a = mk_file(Some(ga), vec![bare]);
        let gb = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
        let pe_section = mk_section(EFI_SECTION_PE32, pe);
        let file_b = mk_file(Some(gb), vec![pe_section]);
        file_a.children.push(mk_section(0x19, pkg_a.clone()));
        let image = mk_image(vec![file_a, file_b]);

        let strings = collect_strings(&image);
        assert_eq!(strings.len(), 4);
        assert!(strings.iter().any(|s| s.language == "en" && s.text == "A"));
        assert!(strings.iter().any(|s| s.language == "en-US" && s.text == "Hi"));
        assert!(strings
            .iter()
            .any(|s| s.language == "x-UEFI-AMI" && s.text == "PRC"));

        let pkgs = collect_string_packages(&image);
        assert_eq!(pkgs.len(), 4);
        assert_eq!(pkgs[0].channel, StringPackageChannel::Bare);
        assert_eq!(pkgs[0].file_guid, Some(ga));
        assert_eq!(pkgs[1].channel, StringPackageChannel::Bare);
        assert_eq!(pkgs[2].channel, StringPackageChannel::Resource);
        assert_eq!(pkgs[2].file_guid, Some(gb));
        assert_eq!(pkgs[2].language, "en-US");
        assert_eq!(pkgs[3].language, "x-UEFI-AMI");
    }
```

Хелперы `mk_section`/`mk_file`/`mk_image` — если их ещё нет в `tests` модулях `strings.rs`, добавить (по образцу `mk_node`/`make_image` из `string_pack.rs:328-349`):

```rust
    fn mk_section(subtype: u8, body: Vec<u8>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn mk_file(guid: Option<Guid>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid,
            node_type: FfsType::File,
            subtype: 0x07,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn mk_image(children: Vec<FfsNode>) -> Image {
        let volume = FfsNode {
            guid: None,
            node_type: FfsType::Volume,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let root = FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![volume],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        }
    }
```

- [ ] **Step 2: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine collect_strings_traverses`
Expected: FAIL — `collect_string_packages`/`StringPackageChannel` не определены; `strings.len()` сейчас далёко от 4 (walker останавливается на первом пакете).

- [ ] **Step 3: Реализовать**

В `hii/strings.rs` заменить `collect_strings`/`walk_for_string_package`/`first_resource_string_package` (`strings.rs:230-284`) на (импорт типов расширить: `use crate::types::{FfsNode, FfsType, Guid, Image};`):

```rust
pub fn collect_strings(image: &Image) -> Vec<StringInfo> {
    let mut out = Vec::new();
    for pkg in collect_string_packages(image) {
        for (sid, text) in pkg.strings {
            out.push(StringInfo {
                language: pkg.language.clone(),
                string_id: sid as u32,
                text,
            });
        }
    }
    out
}

pub(crate) enum StringPackageChannel {
    Bare,
    Resource,
}

pub(crate) struct StringPackageRef {
    pub file_guid: Option<Guid>,
    pub channel: StringPackageChannel,
    pub section_path: Vec<usize>,
    pub language: String,
    pub strings: Vec<(u16, String)>,
}

pub(crate) fn collect_string_packages(image: &Image) -> Vec<StringPackageRef> {
    let mut out = Vec::new();
    walk_string_packages(&image.root, None, &mut Vec::new(), &mut out);
    out
}

fn walk_string_packages(
    node: &FfsNode,
    owner: Option<&Guid>,
    path: &mut Vec<usize>,
    out: &mut Vec<StringPackageRef>,
) {
    if node.node_type == FfsType::Section {
        if declared_len_sane(&node.body)
            && is_string_package(&node.body)
            && let Some(pkg) = parse_string_package(&node.body)
        {
            out.push(StringPackageRef {
                file_guid: owner.cloned(),
                channel: StringPackageChannel::Bare,
                section_path: path.clone(),
                language: pkg.language,
                strings: pkg.strings,
            });
        }
        if node.subtype == EFI_SECTION_PE32 {
            for pkg in resource_string_packages(&node.body) {
                out.push(StringPackageRef {
                    file_guid: owner.cloned(),
                    channel: StringPackageChannel::Resource,
                    section_path: path.clone(),
                    language: pkg.language,
                    strings: pkg.strings,
                });
            }
        }
    }
    for (i, child) in node.children.iter().enumerate() {
        let child_owner = if child.node_type == FfsType::File {
            child.guid.as_ref()
        } else {
            owner
        };
        path.push(i);
        walk_string_packages(child, child_owner, path, out);
        path.pop();
    }
}

pub(crate) fn resource_string_packages(pe: &[u8]) -> Vec<ParsedStringPackage> {
    let mut out = Vec::new();
    for blob in hii_resource_blobs(pe) {
        let Some(list) = parse_package_list(blob) else {
            continue;
        };
        for pkg in &list.packages {
            if pkg.kind == PACKAGE_STRINGS
                && let Some(sp) = parse_string_package(pkg.bytes)
            {
                out.push(sp);
            }
        }
    }
    out
}
```

Удалить `first_resource_string_package` (единственный потребитель — старый walker). Существующие тесты `strings.rs`, использующие первый-пакет-только (если отображают изменение количества строк), правятся по фактическому смыслу полного обхода: тест, где один файл с bare-пакетом + тот же файл содержит PE32 — теперь строки обоих каналов. Прогнать весь крейт и сверить каждый упавший тест с новой семантикой (изменение охвата — цель задачи; если тест утверждал «только первый пакет» — это устаревшее утверждение, переписать под полный обход).

- [ ] **Step 4: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/strings.rs
git commit -m "fix(uefi-engine): hii string traversal collects all string packages (bare + resource)"
```

---

### Task 6: insert_strings_at_ids — вставка SIBT-записей на точные id (body-канал)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (новый вариант ошибки)
- Modify: `crates/uefi-engine/src/hii/string_pack.rs`

**Interfaces:**
- Consumes: `string_info_offset`, `scan_sibt`, `update_package_length`, опкоды SIBT (существующие в `string_pack.rs`).
- Produces:
  - `HiiError::IdOccupied(u16)` — `#[error("string id {0} is already occupied")]`
  - `pub(crate) fn insert_strings_at_ids(body: &mut Vec<u8>, entries: &[(u16, &str)]) -> Result<(), HiiError>` — вставка SIBT_STRING_SCSU-записей на заданные id: разрезание SKIP-пропусков (`SKIP(n)` → `[skip(before)][новые записи][skip(after)]`, класс опкода по размеру: n ≤ 0xFF → SKIP1 `[0x22, n]`, иначе SKIP2 `[0x21, lo, hi]`); вставка между записями; id за хвостом — skip-прогал + запись перед `SIBT_END`; занятый id (строка/DUPLICATE/…) → `Err(HiiError::IdOccupied(id))` без мутации; пересчёт package length.

Спека §4.3. Записи — SIBT_STRING_SCSU (ASCII-токены; reader понимает оба кодирования — writer поддерживает фактически найденный в пробниках формат, см. Task 9 synthetic + real-image (b)).

- [ ] **Step 1: Добавить вариант ошибки**

В `hii/mod.rs` в `enum HiiError` (после `StringPackageNotFound`, `hii/mod.rs:31-32`):

```rust
    #[error("string id {0} is already occupied")]
    IdOccupied(u16),
```

- [ ] **Step 2: Написать проваливающиеся тесты**

В модуль `tests` файла `string_pack.rs` (хелпер `make_string_package` есть, `string_pack.rs:308`; для SKIP-пакетов собрать SIBT вручную):

```rust
    fn make_sppkg(language: &str, sibt: &[u8]) -> Vec<u8> {
        let hdr_size: u32 = (46 + language.len() + 1) as u32;
        let info_off = hdr_size;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 3]);
        buf.push(PACKAGE_STRINGS);
        buf.extend_from_slice(&hdr_size.to_le_bytes());
        buf.extend_from_slice(&info_off.to_le_bytes());
        while buf.len() < 44 {
            buf.push(0);
        }
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(language.as_bytes());
        buf.push(0);
        while buf.len() < info_off as usize {
            buf.push(0);
        }
        buf.extend_from_slice(sibt);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn pkg_len(body: &[u8]) -> usize {
        body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16
    }

    #[test]
    fn insert_at_id_cuts_skip2_block() {
        let sibt = [
            SIBT_STRING_SCSU, b'A', 0,
            SIBT_SKIP2, 0x03, 0x00,
            SIBT_STRING_SCSU, b'B', 0,
            SIBT_END,
        ];
        let mut pkg = make_sppkg("en", &sibt);
        insert_strings_at_ids(&mut pkg, &[(3, "X")]).unwrap();
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(
            parsed.strings,
            vec![
                (1, "A".to_string()),
                (3, "X".to_string()),
                (5, "B".to_string()),
            ]
        );
        assert_eq!(pkg_len(&pkg), pkg.len());
    }

    #[test]
    fn insert_at_id_splits_skip_into_size_classes() {
        let sibt = [
            SIBT_STRING_SCSU, b'A', 0,
            SIBT_SKIP2, 0x50, 0x01,
            SIBT_END,
        ];
        let mut pkg = make_sppkg("en", &sibt);
        insert_strings_at_ids(&mut pkg, &[(3, "X")]).unwrap();
        let info = string_info_offset(&pkg);
        assert_eq!(&pkg[info + 3..info + 5], &[SIBT_SKIP1, 0x01]);
        assert_eq!(&pkg[info + 5..info + 8], &[SIBT_STRING_SCSU, b'X', 0]);
        let after = info + 8;
        assert_eq!(pkg[after], SIBT_SKIP2);
        assert_eq!(
            u16::from_le_bytes([pkg[after + 1], pkg[after + 2]]),
            0x150 - 3
        );
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
        assert_eq!(parsed.strings[1], (3, "X".to_string()));
    }

    #[test]
    fn insert_at_id_rejects_occupied_id_without_mutation() {
        let mut pkg = make_string_package(&["A", "B"]);
        let snapshot = pkg.clone();
        let err = insert_strings_at_ids(&mut pkg, &[(2, "X")]).unwrap_err();
        assert!(matches!(err, crate::hii::HiiError::IdOccupied(2)));
        assert_eq!(pkg, snapshot);
    }

    #[test]
    fn insert_at_id_appends_with_gap_before_end() {
        let mut pkg = make_string_package(&["A"]);
        insert_strings_at_ids(&mut pkg, &[(4, "Z")]).unwrap();
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(
            parsed.strings,
            vec![(1, "A".to_string()), (4, "Z".to_string())]
        );
    }

    #[test]
    fn insert_at_id_multiple_entries_in_one_walk() {
        let sibt = [
            SIBT_STRING_SCSU, b'A', 0,
            SIBT_SKIP2, 0x04, 0x00,
            SIBT_STRING_SCSU, b'B', 0,
            SIBT_END,
        ];
        let mut pkg = make_sppkg("en", &sibt);
        insert_strings_at_ids(&mut pkg, &[(3, "X"), (5, "Y")]).unwrap();
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(
            parsed.strings,
            vec![
                (1, "A".to_string()),
                (3, "X".to_string()),
                (5, "Y".to_string()),
                (6, "B".to_string()),
            ]
        );
    }

    #[test]
    fn insert_at_id_rejects_duplicate_request_ids() {
        let mut pkg = make_string_package(&["A"]);
        let err = insert_strings_at_ids(&mut pkg, &[(2, "X"), (2, "Y")]).unwrap_err();
        assert!(matches!(err, crate::hii::HiiError::IdOccupied(2)));
    }
```

(`make_string_package(&["A", "B"])` даёт id 1 и 2 — занятый id в третьем тесте это 2.)

- [ ] **Step 3: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine insert_at_id`
Expected: FAIL — `insert_strings_at_ids` не определена.

- [ ] **Step 4: Реализовать**

В `hii/string_pack.rs` (рядом с `add_strings_to_body`, `string_pack.rs:46`):

```rust
fn push_skip(out: &mut Vec<u8>, count: u16) {
    if count <= 0xFF {
        out.push(SIBT_SKIP1);
        out.push(count as u8);
    } else {
        out.push(SIBT_SKIP2);
        out.extend_from_slice(&count.to_le_bytes());
    }
}

fn push_string(out: &mut Vec<u8>, text: &str) {
    out.push(SIBT_STRING_SCSU);
    out.extend_from_slice(text.as_bytes());
    out.push(0x00);
}

fn block_id_count(body: &[u8], pos: usize) -> usize {
    match body[pos] {
        SIBT_STRING_SCSU | SIBT_STRING_SCSU_FONT | SIBT_STRING_UCS2 | SIBT_STRING_UCS2_FONT
        | SIBT_DUPLICATE => 1,
        SIBT_STRINGS_SCSU | SIBT_STRINGS_UCS2 => usize::from(read_u16_count(body, pos + 1).0),
        SIBT_STRINGS_SCSU_FONT | SIBT_STRINGS_UCS2_FONT => {
            usize::from(read_u16_count(body, pos + 2).0)
        }
        SIBT_SKIP2 => usize::from(read_u16_count(body, pos + 1).0),
        SIBT_SKIP1 => body.get(pos + 1).copied().unwrap_or(0) as usize,
        _ => 0,
    }
}

fn strings_block_extent(body: &[u8], pos: usize, count_off: usize, ucs2: bool) -> usize {
    if pos + count_off + 2 > body.len() {
        return body.len();
    }
    let count = u16::from_le_bytes([body[pos + count_off], body[pos + count_off + 1]]);
    let mut p = pos + count_off + 2;
    for _ in 0..count {
        p = if ucs2 {
            skip_ucs2(body, p)
        } else {
            skip_scsu(body, p)
        };
    }
    p
}

fn block_end(body: &[u8], pos: usize) -> Option<usize> {
    let end = match body[pos] {
        SIBT_STRING_SCSU => skip_scsu(body, pos + 1),
        SIBT_STRING_SCSU_FONT => skip_scsu(body, pos + 2),
        SIBT_STRING_UCS2 => skip_ucs2(body, pos + 1),
        SIBT_STRING_UCS2_FONT => skip_ucs2(body, pos + 2),
        SIBT_DUPLICATE | SIBT_SKIP2 => (pos + 3).min(body.len()),
        SIBT_SKIP1 => (pos + 2).min(body.len()),
        SIBT_STRINGS_SCSU => strings_block_extent(body, pos, 1, false),
        SIBT_STRINGS_SCSU_FONT => strings_block_extent(body, pos, 2, false),
        SIBT_STRINGS_UCS2 => strings_block_extent(body, pos, 1, true),
        SIBT_STRINGS_UCS2_FONT => strings_block_extent(body, pos, 2, true),
        SIBT_END => pos,
        _ => return None,
    };
    Some(end)
}

pub(crate) fn insert_strings_at_ids(
    body: &mut Vec<u8>,
    entries: &[(u16, &str)],
) -> Result<(), crate::hii::HiiError> {
    let mut pending: Vec<(u16, &str)> = entries.iter().map(|(i, t)| (*i, *t)).collect();
    pending.sort_by_key(|(id, _)| *id);
    let mut pi = 0usize;
    let info_off = string_info_offset(body);
    let mut result: Vec<u8> = Vec::with_capacity(body.len() + 16);
    result.extend_from_slice(&body[..info_off]);
    let mut next_id: u16 = 1;
    let mut pos = info_off;

    while pos < body.len() && body[pos] != SIBT_END {
        let Some(end) = block_end(body, pos) else {
            break;
        };
        let count = block_id_count(body, pos) as u16;
        let span_end = next_id.wrapping_add(count);
        if body[pos] == SIBT_SKIP1 || body[pos] == SIBT_SKIP2 {
            let mut cursor = next_id;
            while pending
                .get(pi)
                .is_some_and(|(id, _)| *id >= cursor && *id < span_end)
            {
                let (id, text) = pending[pi];
                if id > cursor {
                    push_skip(&mut result, id - cursor);
                }
                push_string(&mut result, text);
                pi += 1;
                cursor = id.wrapping_add(1);
            }
            if span_end > cursor {
                push_skip(&mut result, span_end - cursor);
            }
        } else {
            if pending
                .get(pi)
                .is_some_and(|(id, _)| *id >= next_id && *id < span_end)
            {
                let (id, _) = pending[pi];
                return Err(crate::hii::HiiError::IdOccupied(id));
            }
            result.extend_from_slice(&body[pos..end]);
        }
        next_id = span_end;
        pos = end;
    }

    while let Some(&(id, text)) = pending.get(pi) {
        if id < next_id {
            return Err(crate::hii::HiiError::IdOccupied(id));
        }
        if id > next_id {
            push_skip(&mut result, id - next_id);
        }
        push_string(&mut result, text);
        pi += 1;
        next_id = next_id.wrapping_add(1);
    }

    result.extend_from_slice(&body[pos.min(body.len())..]);
    *body = result;
    update_package_length(body);
    Ok(())
}
```

Семантика прохода: каждый блок задаёт непрерывный диапазон id `[next_id, next_id+count)`; SKIP-блок разрезается вставками (`[skip(before)][записи][skip(after)]`, класс опкода по размеру), любой другой блок с попаданием целевого id в его диапазон — `IdOccupied` (ошибки возвращаются до `*body = result`, тело не мутируется); после `SIBT_END` (или неизвестного опкода) оставшиеся записи добираются skip-прогалом и вставляются перед скопированным хвостом. Хелперы `skip_scsu`/`skip_ucs2`/`read_u16_count`/`update_package_length`/`string_info_offset` уже есть в `string_pack.rs:253-302`.

- [ ] **Step 5: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/string_pack.rs
git commit -m "feat(uefi-engine): insert SIBT strings at exact ids (skip-run cutting)"
```

---

### Task 7: insert_strings_at_ids_in_resource — resource-канал по языку

**Files:**
- Modify: `crates/uefi-engine/src/hii/string_pack.rs`

**Interfaces:**
- Consumes: `insert_strings_at_ids` (Task 6), `hii_entry_locations`, `parse_package_list`, `plan_rsrc_blob_growth`, `try_grow_rsrc_tail`, `write_length_chain`, `parse_string_package` (существующие).
- Produces: `pub(crate) fn insert_strings_at_ids_in_resource(pe: &mut Vec<u8>, language: &str, entries: &[(u16, &str)]) -> Result<(), crate::hii::HiiError>` — обобщение `add_strings_to_resource` (`string_pack.rs:66-107`): пакет выбирается по распарсенному языку; ошибки: `StringPackageNotFound` (нет блоба/листа/пакета с языком), `PeGrowthUnsupported` (рост невыполним), `IdOccupied` (сквозная).

Спека §4.3 (resource-канал). Существующий `add_strings_to_resource` не удаляется (потребитель — `form_add`).

- [ ] **Step 1: Написать проваливающиеся тесты**

В модуль `tests` файла `string_pack.rs` (паттерны из `add_strings_to_resource_refuses_cert_blocked_growth`, `string_pack.rs:652`; хелперы `pkg`/`res_list` есть в этом модуле, `make_sppkg` добавлен Task 6):

```rust
    fn two_language_blob() -> (Vec<u8>, Guid) {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let display_sibt = [
            SIBT_STRING_SCSU, b'H', b'i', 0,
            SIBT_END,
        ];
        let token_sibt = [
            SIBT_STRING_SCSU, b'P', b'R', b'C', 0,
            SIBT_END,
        ];
        let display = make_sppkg("en-US", &display_sibt);
        let token = make_sppkg("x-UEFI-AMI", &token_sibt);
        (res_list(&g, &[&form, &display, &token]), g)
    }

    fn resource_pkgs(pe: &[u8]) -> Vec<crate::hii::strings::ParsedStringPackage> {
        let (_, blob_off, blob_len) = crate::hii::pe_resource::hii_entry_locations(pe)[0];
        let list = crate::hii::package_list::parse_package_list(&pe[blob_off..blob_off + blob_len])
            .unwrap();
        list.packages
            .iter()
            .filter(|p| p.kind == PACKAGE_STRINGS)
            .filter_map(|p| crate::hii::strings::parse_string_package(p.bytes))
            .collect()
    }

    #[test]
    fn insert_in_resource_targets_language_package_only() {
        let (blob, _) = two_language_blob();
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        insert_strings_at_ids_in_resource(&mut pe, "x-UEFI-AMI", &[(3, "UPG0001")]).unwrap();
        let pkgs = resource_pkgs(&pe);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].language, "en-US");
        assert_eq!(pkgs[0].strings, vec![(1, "Hi".to_string())]);
        assert_eq!(pkgs[1].language, "x-UEFI-AMI");
        assert_eq!(
            pkgs[1].strings,
            vec![(1, "PRC".to_string()), (3, "UPG0001".to_string())]
        );
    }

    #[test]
    fn insert_in_resource_missing_language_errors() {
        let (blob, _) = two_language_blob();
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let err =
            insert_strings_at_ids_in_resource(&mut pe, "x-UEFI-AMI-2", &[(3, "UPG0001")])
                .unwrap_err();
        assert!(matches!(
            err,
            crate::hii::HiiError::StringPackageNotFound
        ));
    }

    #[test]
    fn insert_in_resource_refuses_cert_blocked_growth() {
        let (blob, _) = two_language_blob();
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        pe[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        let snapshot = pe.clone();
        let err =
            insert_strings_at_ids_in_resource(&mut pe, "x-UEFI-AMI", &[(3, "UPG0001")])
                .unwrap_err();
        assert!(matches!(err, crate::hii::HiiError::PeGrowthUnsupported));
        assert_eq!(pe, snapshot);
    }
```

(константы `SIBT_*` и хелперы `pkg`/`res_list`/`make_sppkg` уже в tests-модуле после Task 6.)

- [ ] **Step 2: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine insert_in_resource`
Expected: FAIL — функция не определена.

- [ ] **Step 3: Реализовать**

В `hii/string_pack.rs` после `add_strings_to_resource` (`string_pack.rs:107`):

```rust
pub(crate) fn insert_strings_at_ids_in_resource(
    pe: &mut Vec<u8>,
    language: &str,
    entries: &[(u16, &str)],
) -> Result<(), crate::hii::HiiError> {
    use crate::hii::HiiError;
    let (_, blob_off, blob_len) = hii_entry_locations(pe)
        .first()
        .copied()
        .ok_or(HiiError::StringPackageNotFound)?;
    let blob_end = blob_off
        .checked_add(blob_len)
        .ok_or(HiiError::StringPackageNotFound)?;
    let blob = pe
        .get(blob_off..blob_end)
        .ok_or(HiiError::StringPackageNotFound)?;
    let parsed = parse_package_list(blob).ok_or(HiiError::StringPackageNotFound)?;
    let idx = parsed
        .packages
        .iter()
        .position(|p| {
            p.kind == PACKAGE_STRINGS
                && is_string_package(p.bytes)
                && crate::hii::strings::parse_string_package(p.bytes)
                    .is_some_and(|sp| sp.language == language)
        })
        .ok_or(HiiError::StringPackageNotFound)?;
    let prefix: usize = parsed.packages[..idx].iter().map(|p| p.bytes.len()).sum();
    let old_len = parsed.packages[idx].bytes.len();
    let mut grown = blob[20 + prefix..20 + prefix + old_len].to_vec();
    insert_strings_at_ids(&mut grown, entries)?;
    let delta = grown.len() - old_len;
    let sum: usize = parsed.packages.iter().map(|p| p.bytes.len()).sum();
    let new_blob_len = blob_len
        .checked_add(delta)
        .ok_or(HiiError::PeGrowthUnsupported)?;
    let new_total = 20u64 + sum as u64 + delta as u64 + 4;
    if new_total > u32::MAX as u64 || new_blob_len > u32::MAX as usize {
        tracing::debug!("string package growth overflows list lengths");
        return Err(HiiError::PeGrowthUnsupported);
    }
    let pkg_off = blob_off + 20 + prefix;
    let plan = plan_rsrc_blob_growth(pe, delta).ok_or(HiiError::PeGrowthUnsupported)?;
    if plan.grow > 0 && !try_grow_rsrc_tail(pe, plan.grow) {
        return Err(HiiError::PeGrowthUnsupported);
    }
    pe.copy_within(pkg_off + old_len..blob_end, pkg_off + grown.len());
    pe[pkg_off..pkg_off + grown.len()].copy_from_slice(&grown);
    write_length_chain(
        pe,
        plan.entry_off,
        plan.blob_off,
        new_blob_len as u32,
        new_total as u32,
    );
    Ok(())
}
```

(хелпер `make_sppkg` добавлен в Task 6; `hii_entry_locations`/`plan_rsrc_blob_growth`/`write_length_chain` — существующие `pub(crate)` из `pe_resource.rs`.)

- [ ] **Step 4: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/string_pack.rs
git commit -m "feat(uefi-engine): insert-at-id in PE resource string packages by language"
```

---

### Task 8: collect_form_string_ids — IFR-walker идов строк формы

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs`

**Interfaces:**
- Consumes: `is_form_package`, опкод-константы `r_efi::hii::*` (`IFR_FORM_OP`, `IFR_SUBTITLE_OP`, `IFR_TEXT_OP`, `IFR_ONE_OF_OPTION_OP`, и опкоды вопросов), макет записей r-efi (`IfrForm`: form_id@2, form_title@4; `IfrStatementHeader`: prompt@0, help@2 → в op: prompt@2, help@4; `IfrText`: text_two@6; `IfrOneOfOption`: option@2).
- Produces: `pub fn collect_form_string_ids(body: &[u8], form_id: u16) -> Vec<u16>` — иды строк формы: титул FORM, промпты/хелпы субтитлов и вопросов (ONE_OF 0x05, CHECKBOX 0x06, NUMERIC 0x07, PASSWORD 0x08, ACTION 0x0C, REF 0x0F, DATE 0x1A, TIME 0x1B, STRING 0x1C, ORDERED_LIST 0x23), три ид TEXT (0x03), тексты опций ONE_OF_OPTION (0x09); id 0 отфильтрован; дедуп с сохранением порядка; форма не найдена → пустой вектор.

Обход: как в `find_form_suppress_scope` (`ifr.rs:60-113`) — если `is_form_package`, границы `(4, plen)`; иначе `(0, len)`; `[op, len|scope<<7]`; найдя `IFR_FORM_OP` с form_id@2, собирать иды до закрывающего END (относительная глубина: scoped-оп → +1, END → −1, END при 0 — конец формы).

- [ ] **Step 1: Написать проваливающиеся тесты**

В модуль `tests` файла `ifr.rs`:

```rust
    fn ifr_op(opc: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(2 + payload.len());
        v.push(opc);
        v.push((2 + payload.len()) as u8 | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn u16p(x: u16) -> Vec<u8> {
        x.to_le_bytes().to_vec()
    }

    fn two_form_package() -> Vec<u8> {
        let mut ifr = Vec::new();
        ifr.extend(ifr_op(0x0E, true, &[&u16p(0x0100), &u16p(0x0101)].concat()));
        ifr.extend(ifr_op(0x0A, true, &[]));
        ifr.extend(ifr_op(0x12, false, &[0x40]));
        ifr.extend(ifr_op(
            IFR_FORM_OP,
            true,
            &[&u16p(901), &u16p(0x1325)].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_SUBTITLE_OP,
            false,
            &[&u16p(4894), &u16p(4895)].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_ONE_OF_OP,
            true,
            &[&u16p(4965), &u16p(4966), &u16p(0x0D5F), &u16p(1)].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_ONE_OF_OPTION_OP,
            false,
            &[&u16p(4969), &[0x00, 0x05], &u16p(1).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_ONE_OF_OPTION_OP,
            false,
            &[&u16p(2638), &[0x00, 0x05], &u16p(0).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(
            IFR_FORM_OP,
            true,
            &[&u16p(902), &u16p(0x1400)].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_TEXT_OP,
            false,
            &[&u16p(0x1401), &u16p(0x1402), &u16p(0x1403)].concat(),
        ));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        let mut pkg = vec![
            (4 + ifr.len()) as u8 & 0xFF,
            0,
            0,
            r_efi::hii::PACKAGE_FORMS,
        ];
        let len = 4 + ifr.len();
        pkg[0] = (len & 0xFF) as u8;
        pkg[1] = ((len >> 8) & 0xFF) as u8;
        pkg[2] = ((len >> 16) & 0xFF) as u8;
        pkg.extend(ifr);
        pkg
    }

    #[test]
    fn collect_form_string_ids_walks_target_form_only() {
        let pkg = two_form_package();
        assert_eq!(
            collect_form_string_ids(&pkg, 901),
            vec![0x1325, 4894, 4895, 4965, 4966, 4969, 2638]
        );
        assert_eq!(
            collect_form_string_ids(&pkg, 902),
            vec![0x1400, 0x1401, 0x1402, 0x1403]
        );
    }

    #[test]
    fn collect_form_string_ids_unknown_form_is_empty() {
        let pkg = two_form_package();
        assert!(collect_form_string_ids(&pkg, 9999).is_empty());
    }

    #[test]
    fn collect_form_string_ids_filters_zero_and_dedups() {
        let mut ifr = Vec::new();
        ifr.extend(ifr_op(
            IFR_FORM_OP,
            true,
            &[&u16p(7), &u16p(0x22)].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_SUBTITLE_OP,
            false,
            &[&u16p(0), &u16p(0x22)].concat(),
        ));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        assert_eq!(collect_form_string_ids(&ifr, 7), vec![0x22]);
    }
```

(если в `ifr.rs` ещё нет `mod tests` — создать; модуль объявлен в `hii/mod.rs:6` уже как `pub mod ifr`.)

- [ ] **Step 2: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine collect_form_string_ids`
Expected: FAIL — функция не определена.

- [ ] **Step 3: Реализовать**

В `hii/ifr.rs` после `find_form_suppress_scope` (`ifr.rs:113`):

```rust
pub fn collect_form_string_ids(body: &[u8], form_id: u16) -> Vec<u16> {
    use r_efi::hii::{
        IFR_ACTION_OP, IFR_CHECKBOX_OP, IFR_DATE_OP, IFR_NUMERIC_OP, IFR_ONE_OF_OP,
        IFR_ORDERED_LIST_OP, IFR_PASSWORD_OP, IFR_REF_OP, IFR_STRING_OP, IFR_SUBTITLE_OP,
        IFR_TEXT_OP, IFR_TIME_OP,
    };
    let (start, end) = if is_form_package(body) {
        let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
        (4, plen.min(body.len()))
    } else {
        (0, body.len())
    };
    let mut out: Vec<u16> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |out: &mut Vec<u16>, seen: &mut std::collections::HashSet<u16>, id: u16| {
        if id != 0 && seen.insert(id) {
            out.push(id);
        }
    };
    let mut i = start;
    while i + 2 <= end {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            return out;
        }
        if op_code == IFR_FORM_OP
            && length >= 6
            && u16::from_le_bytes([body[i + 2], body[i + 3]]) == form_id
        {
            let read_id = |off: usize| -> u16 {
                u16::from_le_bytes([body[i + off], body[i + 1 + off]])
            };
            push(&mut out, &mut seen, read_id(4));
            let mut depth = 0usize;
            let mut j = i + length;
            while j + 2 <= end {
                let inner_op = body[j];
                let inner_ls = body[j + 1];
                let inner_len = (inner_ls & 0x7F) as usize;
                if inner_len < 2 || j + inner_len > end {
                    return out;
                }
                if inner_op == IFR_END_OP {
                    if depth == 0 {
                        return out;
                    }
                    depth -= 1;
                } else {
                    match inner_op {
                        IFR_SUBTITLE_OP | IFR_ONE_OF_OP | IFR_CHECKBOX_OP | IFR_NUMERIC_OP
                        | IFR_PASSWORD_OP | IFR_ACTION_OP | IFR_REF_OP | IFR_DATE_OP
                        | IFR_TIME_OP | IFR_STRING_OP | IFR_ORDERED_LIST_OP => {
                            if inner_len >= 6 {
                                let p = u16::from_le_bytes([body[j + 2], body[j + 3]]);
                                let h = u16::from_le_bytes([body[j + 4], body[j + 5]]);
                                push(&mut out, &mut seen, p);
                                push(&mut out, &mut seen, h);
                            }
                        }
                        IFR_TEXT_OP => {
                            if inner_len >= 8 {
                                push(&mut out, &mut seen, u16::from_le_bytes([body[j + 2], body[j + 3]]));
                                push(&mut out, &mut seen, u16::from_le_bytes([body[j + 4], body[j + 5]]));
                                push(&mut out, &mut seen, u16::from_le_bytes([body[j + 6], body[j + 7]]));
                            }
                        }
                        IFR_ONE_OF_OPTION_OP => {
                            if inner_len >= 4 {
                                push(&mut out, &mut seen, u16::from_le_bytes([body[j + 2], body[j + 3]]));
                            }
                        }
                        _ => {}
                    }
                    if inner_ls & 0x80 != 0 {
                        depth += 1;
                    }
                }
                j += inner_len;
            }
            return out;
        }
        i += length;
    }
    out
}
```

`IFR_ONE_OF_OPTION_OP` и `IFR_TEXT_OP` — импортировать в use-список наравне с остальными (в блоке use внутри функции добавить `IFR_ONE_OF_OPTION_OP`).

- [ ] **Step 4: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs
git commit -m "feat(uefi-engine): collect_form_string_ids IFR walker"
```

---

### Task 9: PRC-патч в set_item_visibility + RPC-маппинг

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs:49-130` (PE32-ветка), `hii/mod.rs` tests
- Modify: `crates/uefi-engine/src/rpc/server.rs:42-53` (+ тест)

**Interfaces:**
- Consumes: `collect_form_string_ids` (Task 8), `insert_strings_at_ids_in_resource` (Task 7), `pe_resource_form_packages`, `ifr::find_form_suppress_scope`/`unsuppress`, `hii_resource_ranges`, `parse_package_list`, `parse_string_package` (существующие).
- Produces:
  - `HiiError::PrcPatchUnsupported` — `#[error("PRC token patch unsupported for this target")]`
  - приватные `const PRC_TOKEN_LANGUAGE: &str = "x-UEFI-AMI"`, `const PRC_NAME_PREFIX: &str = "UPG"`
  - приватный `fn plan_prc_entries(pe: &[u8], form_id: u16) -> Result<Option<Vec<(u16, String)>>, HiiError>` — чистый (без мутаций): по блобам resource ищет лист, где есть FORMS-пакет с непустым `collect_form_string_ids(..., form_id)` и STRING-пакеты обоих языков (токен `x-UEFI-AMI`, display — первый STRING с другим языком); записи: id с непустым текстом в display и без записи в токен-пакете; имена `UPG` + hex-счётчик `{:04X}` с пропуском коллизий по существующим текстам токен-пакета; нет токен-пакета нигде → `Ok(None)` (no-op, не-AMI образы)
  - поведение `set_item_visibility` PE32-ветки: при `visible && form_id.is_some()` — `plan_prc_entries`, затем **probe-on-clone** (`insert_strings_at_ids_in_resource` на клоне `node.body`; ошибка → `PrcPatchUnsupported`, образ не тронут), затем unsuppress (длина-сохраняющая), затем применение патча на `node.body`
  - RPC: `HiiError::IdOccupied(_)` | `HiiError::PrcPatchUnsupported` → `Status::failed_precondition`

Спека §4.4 (шаги 1–5), §5 (таблица ошибок), §Отложенное (bare-канал PRC — вне цикла: bare/RAW-ветка `set_item_visibility` не меняется). Отступление от буквы §4.2: PRC-планировщик не строится поверх `collect_string_packages` (Task 5), а обходит resource-блобы сам — ему нужна связка «FORMS + display + токен в одном package-list», которую плоский `StringPackageRef` не группирует; API Task 5 остаётся для `collect_strings` и будущих потребителей.

- [ ] **Step 1: Добавить вариант ошибки**

В `enum HiiError` (`hii/mod.rs:45-46`, после `PeGrowthUnsupported`):

```rust
    #[error("PRC token patch unsupported for this target")]
    PrcPatchUnsupported,
```

- [ ] **Step 2: Написать проваливающиеся тесты**

В модуль `tests` файла `hii/mod.rs` (хелперы `mk_node` есть, `hii/mod.rs:189`; для IFR/пакетов — локальные):

```rust
    fn ifr_op(opc: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(2 + payload.len());
        v.push(opc);
        v.push((2 + payload.len()) as u8 | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn u16p(x: u16) -> Vec<u8> {
        x.to_le_bytes().to_vec()
    }

    fn suppressed_form_901_pkg() -> Vec<u8> {
        let mut ifr = Vec::new();
        ifr.extend(ifr_op(0x0E, true, &[&u16p(1), &u16p(2)].concat()));
        ifr.extend(ifr_op(0x0A, true, &[]));
        ifr.extend(ifr_op(0x12, false, &[0x40]));
        ifr.extend(ifr_op(0x01, true, &[&u16p(901), &u16p(0x1325)].concat()));
        ifr.extend(ifr_op(0x02, false, &[&u16p(4894), &u16p(4895)].concat()));
        ifr.extend(ifr_op(
            0x05,
            true,
            &[&u16p(4965), &u16p(4966), &u16p(0x0D5F), &u16p(1)].concat(),
        ));
        ifr.extend(ifr_op(
            0x09,
            false,
            &[&u16p(4969), &[0x00, 0x05], &u16p(1).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            0x09,
            false,
            &[&u16p(2638), &[0x00, 0x05], &u16p(0).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(0x29, false, &[]));
        ifr.extend(ifr_op(0x29, false, &[]));
        ifr.extend(ifr_op(0x29, false, &[]));
        ifr.extend(ifr_op(0x29, false, &[]));
        let len = 4 + ifr.len();
        let mut pkg = vec![
            (len & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            ((len >> 16) & 0xFF) as u8,
            r_efi::hii::PACKAGE_FORMS,
        ];
        pkg.extend(ifr);
        pkg
    }

    fn sppkg(language: &str, sibt: &[u8]) -> Vec<u8> {
        let hdr_size: u32 = (46 + language.len() + 1) as u32;
        let info_off = hdr_size;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 3]);
        buf.push(0x04);
        buf.extend_from_slice(&hdr_size.to_le_bytes());
        buf.extend_from_slice(&info_off.to_le_bytes());
        while buf.len() < 44 {
            buf.push(0);
        }
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(language.as_bytes());
        buf.push(0);
        while buf.len() < info_off as usize {
            buf.push(0);
        }
        buf.extend_from_slice(sibt);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn display_sibt() -> Vec<u8> {
        vec![
            0x10, b'b', b'o', b'o', b't', 0,
            0x21, 0x4C, 0x0A,
            0x10, b'A', b'u', b't', b'o', 0,
            0x21, 0xCF, 0x08,
            0x10, b'H', b'W', b'P', b'M', 0,
            0x21, 0x46, 0x00,
            0x10, b'W', b'S', b'u', b'p', 0,
            0x21, 0x03, 0x00,
            0x10, b'E', b'n', b'a', 0,
            0x21, 0x65, 0x00,
            0x10, 0x00,
            0x00,
        ]
    }

    fn token_sibt() -> Vec<u8> {
        vec![
            0x10, b'P', b'R', b'C', b'0', b'1', 0,
            0x21, 0x4C, 0x0A,
            0x10, b'P', b'R', b'C', b'0', b'A', b'E', 0,
            0x00,
        ]
    }

    fn prc_blob(forms: Vec<u8>, display: Vec<u8>, token: Vec<u8>) -> Vec<u8> {
        let g = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
        let mut b = g.to_bytes().to_vec();
        let total = 20 + forms.len() + display.len() + token.len() + 4;
        b.extend_from_slice(&(total as u32).to_le_bytes());
        b.extend_from_slice(&forms);
        b.extend_from_slice(&display);
        b.extend_from_slice(&token);
        b.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        b
    }

    fn pe32_image_with(blob: &[u8]) -> Image {
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", blob);
        let mut section = mk_node(FfsType::Section, pe, vec![]);
        section.subtype = EFI_SECTION_PE32;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::try_parse("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    fn resource_string_pkgs(image: &Image) -> Vec<crate::hii::strings::ParsedStringPackage> {
        let node = &image.root.children[0].children[0].children[0];
        crate::hii::strings::resource_string_packages(&node.body)
    }

    fn image_snapshot(image: &Image) -> Vec<u8> {
        image.root.children[0].children[0].children[0].body.clone()
    }

    #[test]
    fn set_item_visibility_patches_prc_tokens_for_unhidden_form() {
        let blob = prc_blob(
            suppressed_form_901_pkg(),
            sppkg("en-US", &display_sibt()),
            sppkg("x-UEFI-AMI", &token_sibt()),
        );
        let mut image = pe32_image_with(&blob);
        set_item_visibility(
            &mut image,
            "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#901",
            true,
        )
        .unwrap();
        let pkgs = resource_string_pkgs(&image);
        assert_eq!(pkgs.len(), 2);
        let display = pkgs.iter().find(|p| p.language == "en-US").unwrap();
        let token = pkgs.iter().find(|p| p.language == "x-UEFI-AMI").unwrap();
        let tok_by_id: std::collections::HashMap<u16, &String> =
            token.strings.iter().map(|(i, t)| (*i, t)).collect();
        assert_eq!(tok_by_id.get(&4894).map(|s| s.as_str()), Some("UPG0001"));
        assert_eq!(tok_by_id.get(&4965).map(|s| s.as_str()), Some("UPG0002"));
        assert_eq!(tok_by_id.get(&4969).map(|s| s.as_str()), Some("UPG0003"));
        assert!(!tok_by_id.contains_key(&5071));
        assert_eq!(tok_by_id.get(&2638).map(|s| s.as_str()), Some("PRC0AE"));
        let disp_by_id: std::collections::HashMap<u16, &String> =
            display.strings.iter().map(|(i, t)| (*i, t)).collect();
        assert_eq!(disp_by_id.get(&4894).map(|s| s.as_str()), Some("HWPM"));
        assert_eq!(disp_by_id.get(&5071).map(|s| s.as_str()), Some(""));
        assert_eq!(
            image.root.children[0].children[0].children[0].action,
            Action::Rebuild
        );
    }

    #[test]
    fn set_item_visibility_no_token_package_is_noop_unhide() {
        let g = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
        let forms = suppressed_form_901_pkg();
        let display = sppkg("en-US", &display_sibt());
        let mut blob = g.to_bytes().to_vec();
        let total = 20 + forms.len() + display.len() + 4;
        blob.extend_from_slice(&(total as u32).to_le_bytes());
        blob.extend_from_slice(&forms);
        blob.extend_from_slice(&display);
        blob.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        let mut image = pe32_image_with(&blob);
        set_item_visibility(
            &mut image,
            "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#901",
            true,
        )
        .unwrap();
        let pkgs = resource_string_pkgs(&image);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].language, "en-US");
        assert_eq!(pkgs[0].strings.len(), 7);
    }

    #[test]
    fn set_item_visibility_prc_growth_failure_leaves_image_untouched() {
        let blob = prc_blob(
            suppressed_form_901_pkg(),
            sppkg("en-US", &display_sibt()),
            sppkg("x-UEFI-AMI", &token_sibt()),
        );
        let mut image = pe32_image_with(&blob);
        {
            let node = &mut image.root.children[0].children[0].children[0];
            node.body[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        }
        let snapshot = image_snapshot(&image);
        let err = set_item_visibility(
            &mut image,
            "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#901",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::PrcPatchUnsupported));
        assert_eq!(image_snapshot(&image), snapshot);
    }
```

(`display_sibt`: id1 «boot»; SKIP2 2636 (0x0A4C) → «Auto»=2638; SKIP2 2255 (0x08CF) → «HWPM»=4894; SKIP2 70 → «WSup»=4965; SKIP2 3 → «Ena»=4969; SKIP2 101 (0x65) → пустая =5071; END — 7 записей. `token_sibt`: «PRC01»=1; SKIP2 2636 → «PRC0AE»=2638; END. Проверка PRC: 4894/4965/4969 получают `UPG0001/2/3`, пустой 5071 пропущен, вендорский 2638 нетронут. Блокировка роста — сертификатная запись по смещению 0xe8, паттерн `string_pack.rs:660`.)

- [ ] **Step 3: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine set_item_visibility_`
Expected: FAIL — PRC-записи не появляются (`tok_by_id.get(&4894)` → None); `HiiError::PrcPatchUnsupported` не определён; `plan_prc_entries` не существует.

- [ ] **Step 4: Реализовать**

4a. В `hii/mod.rs` — константы и планировщик:

```rust
const PRC_TOKEN_LANGUAGE: &str = "x-UEFI-AMI";
const PRC_NAME_PREFIX: &str = "UPG";

fn plan_prc_entries(
    pe: &[u8],
    form_id: u16,
) -> Result<Option<Vec<(u16, String)>>, HiiError> {
    for (off, len) in crate::hii::pe_resource::hii_resource_ranges(pe) {
        let Some(blob) = pe.get(off..off + len) else {
            continue;
        };
        let Some(list) = crate::hii::package_list::parse_package_list(blob) else {
            continue;
        };
        let mut form_ids: Option<Vec<u16>> = None;
        let mut display: Option<crate::hii::strings::ParsedStringPackage> = None;
        let mut token: Option<crate::hii::strings::ParsedStringPackage> = None;
        for pkg in &list.packages {
            if pkg.kind == r_efi::hii::PACKAGE_FORMS
                && form_ids.is_none()
            {
                let ids = ifr::collect_form_string_ids(pkg.bytes, form_id);
                if !ids.is_empty() {
                    form_ids = Some(ids);
                }
            }
            if pkg.kind == r_efi::hii::PACKAGE_STRINGS
                && let Some(sp) = crate::hii::strings::parse_string_package(pkg.bytes)
            {
                if sp.language == PRC_TOKEN_LANGUAGE {
                    token = Some(sp);
                } else if display.is_none() {
                    display = Some(sp);
                }
            }
        }
        let (Some(ids), Some(display), Some(token)) = (form_ids, display, token) else {
            continue;
        };
        let display_by_id: std::collections::HashMap<u16, &String> =
            display.strings.iter().map(|(i, t)| (*i, t)).collect();
        let token_ids: std::collections::HashSet<u16> =
            token.strings.iter().map(|(i, _)| *i).collect();
        let token_texts: std::collections::HashSet<&str> =
            token.strings.iter().map(|(_, t)| t.as_str()).collect();
        let mut counter = 1u32;
        let mut entries = Vec::new();
        for id in ids {
            if token_ids.contains(&id) {
                continue;
            }
            let Some(text) = display_by_id.get(&id) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let mut name = format!("{PRC_NAME_PREFIX}{counter:04X}");
            while token_texts.contains(name.as_str()) {
                counter += 1;
                name = format!("{PRC_NAME_PREFIX}{counter:04X}");
            }
            counter += 1;
            entries.push((id, name));
        }
        return Ok(Some(entries));
    }
    Ok(None)
}
```

4b. PE32-ветка `set_item_visibility` (`hii/mod.rs:101-120`) — заменить на:

```rust
        } else if node.subtype == EFI_SECTION_PE32 {
            let Some(packages) = pe_resource_form_packages(&node.body) else {
                return Err(HiiError::NotASetupItem);
            };
            let prc_entries = if visible {
                match form_id {
                    Some(fid) => plan_prc_entries(&node.body, fid)?,
                    None => None,
                }
            } else {
                None
            };
            let prc_entry_refs: Option<Vec<(u16, &str)>> = prc_entries
                .as_ref()
                .map(|es| es.iter().map(|(i, t)| (*i, t.as_str())).collect());
            if let Some(entries) = &prc_entry_refs
                && !entries.is_empty()
            {
                let mut probe = node.body.clone();
                string_pack::insert_strings_at_ids_in_resource(
                    &mut probe,
                    PRC_TOKEN_LANGUAGE,
                    entries,
                )
                .map_err(|e| match e {
                    HiiError::IdOccupied(id) => HiiError::IdOccupied(id),
                    _ => HiiError::PrcPatchUnsupported,
                })?;
            }
            if visible {
                for (start, len) in packages {
                    let scope = {
                        let seg = &node.body[start..start + len];
                        match form_id {
                            Some(fid) => ifr::find_form_suppress_scope(seg, fid),
                            None => ifr::find_suppress_if_scopes(seg).into_iter().next(),
                        }
                    };
                    if let Some(scope) = scope {
                        ifr::unsuppress(&mut node.body[start..start + len], &scope);
                        changed = true;
                        break;
                    }
                }
            }
            if let Some(entries) = &prc_entry_refs
                && !entries.is_empty()
            {
                string_pack::insert_strings_at_ids_in_resource(
                    &mut node.body,
                    PRC_TOKEN_LANGUAGE,
                    entries,
                )
                .map_err(|_| HiiError::PrcPatchUnsupported)?;
                changed = true;
            }
        } else {
```

4c. RPC-маппинг — в `rpc/server.rs` в `hii_error_status` (`server.rs:42-53`) добавить в arm `failed_precondition`:

```rust
        crate::hii::HiiError::NotWritable
        | crate::hii::HiiError::MutationBehindCompression
        | crate::hii::HiiError::PeGrowthUnsupported
        | crate::hii::HiiError::IdOccupied(_)
        | crate::hii::HiiError::PrcPatchUnsupported => Status::failed_precondition(e.to_string()),
```

И тест в `mod tests` `server.rs` (по образцу `hii_error_status_maps_preconditions_not_found_and_bad_target`, `server.rs:906`):

```rust
    #[test]
    fn hii_error_status_maps_id_occupied_and_prc_patch_unsupported() {
        let st = hii_error_status(crate::hii::HiiError::IdOccupied(7));
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        let st = hii_error_status(crate::hii::HiiError::PrcPatchUnsupported);
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
    }
```

- [ ] **Step 5: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/strings.rs crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): PRC token patch in set_item_visibility (x-UEFI-AMI, UPG names)"
```

---

### Task 10: Минор — ops::rebuild принимает GuidSection-таргеты

**Files:**
- Modify: `crates/uefi-engine/src/ops.rs:130-135` (+ вызовы в `remove`/`replace`/`rebuild`)
- Test: `crates/uefi-engine/src/ops.rs` (модуль `tests`)

**Interfaces:**
- Consumes: `crate::parser::target::find_item_path` (`parser/target.rs:88`).
- Produces: `fn target_path(root: &FfsNode, target: &Target) -> Result<Vec<usize>, OpsError>` — делегирует `find_item_path`; `Target::Guid` и `Target::GuidSection` теперь валидны для `remove`/`replace`/`rebuild`.

Спека §4.5.2: GuidSection-arm зеркально DFS-обходу `find_item`/`find_item_path` (коммит `7e654b0`). Семантика `Path`-таргетов не меняется (ненайденный путь → `OpsError::NotFound` как раньше через `find_mut`).

- [ ] **Step 1: Написать проваливающийся тест**

В модуль `tests` файла `ops.rs`:

```rust
    #[test]
    fn rebuild_accepts_guid_section_target() {
        let buf = make_simple_image();
        let file_bytes = make_ffs_file();
        let mut file = parse_ffs_bytes(&file_bytes).unwrap();
        let guid = file.guid.unwrap();
        file.children.push(FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x19,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![0xAA; 8],
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        });
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        img.root.children[0].children.push(file);

        let t = parse_target(&format!("{guid}:0x19:0")).unwrap();
        rebuild(&mut img.root, &t).unwrap();
        assert_eq!(img.root.action, Action::Rebuild);
        assert_eq!(img.root.children[0].children[0].action, Action::Rebuild);
        assert_eq!(
            img.root.children[0].children[0].children[0].action,
            Action::Rebuild
        );

        let t_missing = parse_target(&format!("{guid}:0x15:0")).unwrap();
        assert!(matches!(
            rebuild(&mut img.root, &t_missing),
            Err(OpsError::NotFound)
        ));
    }
```

(0x19 = RAW-секция; 0x15 = UI — такого ребёнка нет → NotFound. `FfsType`/`ParsingData` доступны через `use crate::types::*` в `ops.rs`.)

- [ ] **Step 2: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine rebuild_accepts_guid_section`
Expected: FAIL — `OpsError::NotFound` на первом же `rebuild` (target_path знает только `Path`).

- [ ] **Step 3: Реализовать**

`ops.rs:130-135`:

```rust
fn target_path(root: &FfsNode, target: &Target) -> Result<Vec<usize>, OpsError> {
    crate::parser::target::find_item_path(root, target).ok_or(OpsError::NotFound)
}
```

Вызовы `target_path(target)` в `remove` (`ops.rs:64`), `replace` (`ops.rs:79`), `rebuild` (`ops.rs:103`) заменить на `target_path(root, target)`.

- [ ] **Step 4: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (включая существующие тесты `rebuild_marks_node_and_cascade` и др.), no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/ops.rs
git commit -m "fix(uefi-engine): ops rebuild accepts GuidSection targets"
```

---

### Task 11: Минор — artifact export по CWD клиента

**Files:**
- Modify: `crates/uefi-cli/src/commands/artifact.rs:44-58`
- Modify: `crates/uefi-engine/src/rpc/server.rs:557-582`
- Test: обе точки (unit)

**Interfaces:**
- Consumes: `AppError::new(ErrKind::IoError, ...)` (`uefi-common/src/error.rs:54-59`), `Status::invalid_argument`.
- Produces: CLI-хелпер `fn resolve_output_path(path: &str) -> Result<std::path::PathBuf, AppError>` (абсолютизация относительно CWD клиента); engine-гейт: относительный `output_path` → `invalid_argument` (защита от регресса).

Спека §4.5.3.

- [ ] **Step 1: Написать проваливающиеся тесты**

В `crates/uefi-cli/src/commands/artifact.rs` добавить модуль и тесты:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_output_path_is_absolutized_under_cwd() {
        let p = resolve_output_path("out.bin").unwrap();
        assert!(p.is_absolute());
        assert_eq!(p.parent(), std::env::current_dir().ok().as_deref());
        assert_eq!(p.file_name().map(|f| f.to_str()), Some("out.bin"));
    }

    #[test]
    fn absolute_output_path_is_untouched() {
        let p = resolve_output_path("/tmp/x/out.bin").unwrap();
        assert_eq!(p, std::path::PathBuf::from("/tmp/x/out.bin"));
    }
}
```

В `rpc/server.rs` в `mod tests`:

```rust
    #[test]
    fn artifact_export_rejects_relative_output_path() {
        assert!(artifact_output_path_is_relative("out.bin"));
        assert!(artifact_output_path_is_relative("./out.bin"));
        assert!(!artifact_output_path_is_relative("/tmp/out.bin"));
    }
```

- [ ] **Step 2: Запустить, убедиться в провале**

Run: `cargo test -p uefi-cli relative_output_path && cargo test -p uefi-engine artifact_export_rejects`
Expected: FAIL — `resolve_output_path` не определена; `artifact_output_path_is_relative` не определена.

- [ ] **Step 3: Реализовать**

CLI (`commands/artifact.rs`): хелпер и использование в `export`:

```rust
fn resolve_output_path(path: &str) -> Result<std::path::PathBuf, AppError> {
    let p = std::path::PathBuf::from(path);
    if p.is_absolute() {
        return Ok(p);
    }
    let cwd = std::env::current_dir().map_err(|e| {
        AppError::new(uefi_common::error::ErrKind::IoError, format!("CWD: {e}"))
    })?;
    Ok(cwd.join(p))
}
```

В `export` заменить `let out = output_path.unwrap_or(artifact_id);` на:

```rust
    let out = resolve_output_path(output_path.unwrap_or(artifact_id))?;
    client
        .artifact_export(artifact_id, &out.to_string_lossy())
        .await?;
```

Engine (`rpc/server.rs`, рядом с `hii_error_status`):

```rust
fn artifact_output_path_is_relative(p: &str) -> bool {
    std::path::Path::new(p).is_relative()
}
```

В `artifact_export` (`server.rs:558-567`, сразу после `let r = req.into_inner();`):

```rust
        if artifact_output_path_is_relative(&r.output_path) {
            return Err(Status::invalid_argument(
                "output_path must be absolute",
            ));
        }
```

- [ ] **Step 4: Прогон + lint**

Run: `cargo test -p uefi-cli && cargo clippy -p uefi-cli -- -D warnings && cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-cli/src/commands/artifact.rs crates/uefi-engine/src/rpc/server.rs
git commit -m "fix(uefi-cli,uefi-engine): artifact export resolves client CWD; engine rejects relative paths"
```

---

### Task 12: Минор — хвостовые байты билдера (+2/+1)

**Files:**
- Modify: `crates/uefi-engine/src/builder/mod.rs:97-101` (`build_file`), `builder/mod.rs:142-146` (`build_section`), `builder/mod.rs:164-168` (`build_recompressed_guided`)

**Interfaces:**
- Consumes: `align4`/`pad_to` (существующие).
- Produces: меж-детское выравнивание сохраняется, паддинг после **последнего** ребёнка убран во всех трёх местах (семантика оригинала); дифф к оригиналу теряет +2 хвоста распакованного потока и +1 хвост FFS-файла.

Спека §4.5.1: research-шаг подтверждает источник по диффу E7 (образ вне репо, `refs/amibcp/`); фикс кодово-очевиден (безусловный `pad_to(align4(...))` в хвосте цикла), тесты ниже фиксируют семантику и на живом fixture.

- [ ] **Step 1: Research-подтверждение (наличие E7-диффа)**

Если `refs/amibcp/` доступен — сверить, что +2 байта диффа E7 в распакованном потоке и +1 в FFS-файле приходятся на нулевой хвост за последней секцией (по карте `probes/fvmap.py`). Если образов нет — пропустить, тест Step 2 на fixture доказывает то же.

- [ ] **Step 2: Написать проваливающиеся тесты**

В модуль `tests` файла `builder/mod.rs`:

```rust
    #[test]
    fn build_file_does_not_pad_after_last_section() {
        let mk_raw = |fill: u8| FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_RAW,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![fill; 5],
            tail: vec![],
            children: vec![],
            action: Action::Rebuild,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let file = FfsNode {
            guid: None,
            node_type: FfsType::File,
            subtype: 0x07,
            offset: 0,
            header: vec![0u8; 24],
            body: vec![],
            tail: vec![],
            children: vec![mk_raw(0xA1), mk_raw(0xB2)],
            action: Action::Rebuild,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let mut out = Vec::new();
        build_node(&file, &mut out).unwrap();
        assert_eq!(out.len(), 24 + 9 + 3 + 9);
        assert_eq!(&out[24 + 9..24 + 12], &[0x00, 0x00, 0x00]);
        assert_eq!(&out[out.len() - 5..], &[0xB2; 5]);
    }

    #[test]
    fn build_section_does_not_pad_after_last_child() {
        let mk_raw = |fill: u8| FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_RAW,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![fill; 5],
            tail: vec![],
            children: vec![],
            action: Action::Rebuild,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let mut section = mk_raw(0xC3);
        section.node_type = FfsType::Section;
        section.subtype = 0x03;
        section.children = vec![mk_raw(0xD4), mk_raw(0xE5)];
        section.action = Action::Rebuild;
        let mut out = Vec::new();
        build_node(&section, &mut out).unwrap();
        assert_eq!(out.len(), 4 + 9 + 3 + 9);
    }

    #[test]
    fn guided_lzma_payload_has_no_padding_after_last_child() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let mut node = crate::parser::section::parse_section(section, 0).unwrap();
        let data_offset = u16::from_le_bytes([node.body[16], node.body[17]]) as usize;
        let orig_payload =
            crate::decompress::decompress(&node.body[data_offset..], 2).unwrap();
        assert_eq!(&orig_payload[orig_payload.len() - 2..], &[0x00, 0x00]);
        node.action = Action::Rebuild;
        let mut out = Vec::new();
        build_section(&node, &mut out).unwrap();
        let rebuilt = crate::parser::section::parse_section(&out, 0).unwrap();
        let new_data_offset = u16::from_le_bytes([rebuilt.body[16], rebuilt.body[17]]) as usize;
        let new_payload =
            crate::decompress::decompress(&rebuilt.body[new_data_offset..], 2).unwrap();
        assert_eq!(new_payload.as_slice(), &orig_payload[..orig_payload.len() - 2]);
    }
```

(Утверждение `assert_eq!(&orig_payload[...-2..], &[0x00, 0x00])` — research-фиксация: оригинальный payload fixture действительно несёт 2 нулевых хвостовых байта; если ассерт падает иначе — остановиться и разобраться (systematic-debugging), это означало бы другую геометрию fixture.)

- [ ] **Step 3: Запустить, убедиться в провале**

Run: `cargo test -p uefi-engine build_file_does_not_pad build_section_does_not_pad guided_lzma_payload_has_no`
Expected: FAIL — текущий код паддит после последнего (размеры больше на 3/3, payload длиннее на 2).

- [ ] **Step 4: Реализовать**

Три цикла (в `build_file` `builder/mod.rs:97-101`, `build_section` `builder/mod.rs:142-146`, `build_recompressed_guided` `builder/mod.rs:164-168`) привести к виду:

```rust
    for (i, child) in node.children.iter().enumerate() {
        build_node(child, &mut body)?;
        if i + 1 < node.children.len() {
            let target = align4(body.len());
            pad_to(&mut body, target, 0x00);
        }
    }
```

(в `build_recompressed_guided` буфер называется `children` — соответственно `pad_to(&mut children, ...)`; гварды размера и бюджет из Task 4 не трогать).

- [ ] **Step 5: Прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS — включая `guided_lzma_rebuild_fits_original_slot` (Task 4: поток стал на 2 байта короче, слот держит), `round_trip_volume`, `real_image_*` не запускаются (ignored). No warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/builder/mod.rs
git commit -m "fix(uefi-engine): builder no longer pads after last child (clean +2/+1 diff tails)"
```

---

### Task 13: Real-image приёмка на HNX99TF (a)–(e)

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (продление живых `#[ignore]`-тестов)

**Interfaces:**
- Consumes: `collect_strings`/`collect_string_packages` (Task 5), `set_item_visibility` с PRC (Task 9), `build_image` со slot-fit (Tasks 4, 12), `parse_target`/`find_item_path`.
- Produces: тесты `real_image_string_list_full_traversal`, `real_image_unhide_patches_prc_tokens`, `real_image_unhide_rebuild_keeps_layout` — `#[ignore]`, константы: модуль `ABBCE13D-E25A-4D9F-A1F9-2F7710786892`, форма `901`, иды `4894/4965/4969` (пустой `5071`), слот `0x161a1`.

Спека §6 (real-image (a)–(e)). Запуск: `cargo test -p uefi-engine --test real_image -- --ignored` при наличии `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`.

- [ ] **Step 1: Написать тесты**

В `tests/real_image.rs` (в шапке расширить импорт типов: `use uefi_engine::types::{Action, FfsNode, FfsType, Guid, Image, ImageMode, ParsingData};`):

```rust
const SETUP_MODULE_GUID: &str = "abbce13d-e25a-4d9f-a1f9-2f7710786892";
const HIDDEN_FORM_ID: u16 = 901;

#[ignore]
#[test]
fn real_image_string_list_full_traversal() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let strings = uefi_engine::hii::strings::collect_strings(&img);
    assert!(
        strings.len() > 5000,
        "expected ~5.7k strings, got {}",
        strings.len()
    );
    assert!(strings.iter().any(|s| s.language == "en-US"));
    assert!(strings.iter().any(|s| s.language == "x-UEFI-AMI"));
    let prc = strings.iter().filter(|s| s.language == "x-UEFI-AMI").count();
    assert!(prc > 100);
}

fn setup_pe32_node_path(img: &Image) -> Vec<usize> {
    let target =
        uefi_engine::parser::target::parse_target(&format!("{SETUP_MODULE_GUID}:0x10:0"))
            .unwrap();
    uefi_engine::parser::target::find_item_path(&img.root, &target).expect("setup PE32 node")
}

fn node_at_path<'a>(img: &'a Image, path: &[usize]) -> &'a FfsNode {
    let mut node = &img.root;
    for &i in path {
        node = &node.children[i];
    }
    node
}

fn resource_string_ids(img: &Image, path: &[usize], language: &str) -> Vec<(u16, String)> {
    let pe = &node_at_path(img, path).body;
    for (off, len) in uefi_engine::hii::pe_resource::hii_resource_ranges(pe) {
        let Some(blob) = pe.get(off..off + len) else {
            continue;
        };
        let Some(list) = uefi_engine::hii::package_list::parse_package_list(blob) else {
            continue;
        };
        for pkg in &list.packages {
            if pkg.kind == r_efi::hii::PACKAGE_STRINGS
                && let Some(sp) = uefi_engine::hii::strings::parse_string_package(pkg.bytes)
                && sp.language == language
            {
                return sp.strings;
            }
        }
    }
    Vec::new()
}

fn form_901_suppressed(img: &Image, path: &[usize]) -> bool {
    let pe = &node_at_path(img, path).body;
    for (off, len) in uefi_engine::hii::pe_resource::hii_resource_ranges(pe) {
        let Some(blob) = pe.get(off..off + len) else {
            continue;
        };
        let Some(list) = uefi_engine::hii::package_list::parse_package_list(blob) else {
            continue;
        };
        for pkg in &list.packages {
            if pkg.kind == r_efi::hii::PACKAGE_FORMS
                && uefi_engine::hii::ifr::find_form_suppress_scope(pkg.bytes, HIDDEN_FORM_ID)
                    .is_some()
            {
                return true;
            }
        }
    }
    false
}

#[ignore]
#[test]
fn real_image_unhide_patches_prc_tokens() {
    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");
    let path = setup_pe32_node_path(&img);
    let display_before = resource_string_ids(&img, &path, "en-US");
    let token_before = resource_string_ids(&img, &path, "x-UEFI-AMI");
    assert!(!token_before.iter().any(|(i, _)| *i == 4894));
    assert!(form_901_suppressed(&img, &path));

    uefi_engine::hii::set_item_visibility(
        &mut img,
        &format!("{SETUP_MODULE_GUID}:0x10:0#{HIDDEN_FORM_ID}"),
        true,
    )
    .expect("unhide with PRC patch");

    assert!(!form_901_suppressed(&img, &path));
    let display_after = resource_string_ids(&img, &path, "en-US");
    let token_after = resource_string_ids(&img, &path, "x-UEFI-AMI");
    assert_eq!(display_after, display_before);
    for id in [4894u16, 4965, 4969] {
        assert!(
            token_after.iter().any(|(i, t)| *i == id && t.starts_with("UPG")),
            "missing PRC token for id {id}"
        );
    }
    assert!(!token_after.iter().any(|(i, _)| *i == 5071));
    for (id, text) in &token_before {
        assert!(token_after.contains(&(*id, text.clone())));
    }
}

fn file_extent(img: &Image, path: &[usize]) -> (usize, usize) {
    let mut node = &img.root;
    let mut vol = &img.root;
    let mut file_idx = 0usize;
    for &i in path {
        let child = &node.children[i];
        if child.node_type == FfsType::File {
            vol = node;
            file_idx = i;
        }
        node = child;
    }
    let file_start = vol.children[file_idx].offset;
    let file_end = vol.children[file_idx + 1].offset;
    (file_start, file_end)
}

fn guided_body_len(img: &Image, path: &[usize]) -> usize {
    let mut node = &img.root;
    let mut guided: Option<&FfsNode> = None;
    for &i in path {
        node = &node.children[i];
        if node.node_type == FfsType::Section
            && node.subtype == uefi_engine::ffs::EFI_SECTION_GUID_DEFINED
        {
            guided = Some(node);
        }
    }
    guided.expect("guided LZMA ancestor of the PE32 section").body.len()
}

#[ignore]
#[test]
fn real_image_unhide_rebuild_keeps_layout() {
    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");
    let pe_path = setup_pe32_node_path(&img);
    let (file_start, file_end) = file_extent(&img, &pe_path);
    let guided_before = guided_body_len(&img, &pe_path);

    uefi_engine::hii::set_item_visibility(
        &mut img,
        &format!("{SETUP_MODULE_GUID}:0x10:0#{HIDDEN_FORM_ID}"),
        true,
    )
    .expect("unhide");
    let built = uefi_engine::builder::build_image(&img).expect("build_image");

    assert_eq!(built.len(), data.len());
    assert_eq!(&built[..file_start], &data[..file_start]);
    assert_eq!(&built[file_end..], &data[file_end..]);

    let rebuilt = parse_image(&built, ImageMode::Read, "img1", "s1").expect("re-parse");
    let new_path = setup_pe32_node_path(&rebuilt);
    let (_, new_file_end) = file_extent(&rebuilt, &new_path);
    assert_eq!(new_file_end, file_end);
    assert_eq!(guided_body_len(&rebuilt, &new_path), guided_before);
    let token = resource_string_ids(&rebuilt, &new_path, "x-UEFI-AMI");
    assert!(token.iter().any(|(i, t)| *i == 4894 && t.starts_with("UPG")));
}
```

(`file_extent` опирается на абсолютные `node.offset` файлов и наличие следующего файла в FV2 после ABBCE13D — подтверждено отчётом E6: «все файлы FV2 после него»; `guided_body_len` — длина body GUIDed-секции, при slot-fit инвариантна. (d) — без нового теста: существующие `real_image_full_flash_round_trip` и `real_image_full_flash_repatch_stability` должны остаться зелёными (прогон в Step 2).)

- [ ] **Step 2: Запустить живые тесты**

Run: `cargo test -p uefi-engine --test real_image -- --ignored`
Expected: PASS — новые три + все существующие живые (включая (d) round-trip/repatch-stability). Ожидания по числам: строк > 5000; слот удержан (`guided_len_after == guided_len_before`); иды 4894/4965/4969 с префиксом `UPG`.

Если PRC-записи не появились (id не входят в `collect_form_string_ids` формы 901 — например, IFR формы использует другие поля) — остановиться и диагностировать парсингом реального формсета (пробники `refs/amibcp/probes/`), при расхождении плана с фактикой — правило plan-defect (коммит `docs: fix Task 13 ...`).

- [ ] **Step 3: Прогон синтетики + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (ignored не запускаются), no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): HNX99TF real-image acceptance for unhide fix cycle"
```

---

### Task 14: HW-кандидат + чеклист + актуализация TODO

**Files:**
- Create: кандидат-образ вне репо: `refs/amibcp/e8-unhide-fix-candidate.bin`
- Modify: `docs/reports/2026-09-02-hw-validation-hnx99tf.md` (аддендум)
- Modify: `TODO.md`

**Interfaces:**
- Consumes: всё цикло (Task 13 зелёный — обязательное условие).
- Produces: образ-кандидат E8, собранный чистым движковым путём; аддендум отчёта с чеклистом прошивки (статус pending); TODO-актуализация.

Спека §6 (финальная HW-фаза: прошивает пользователь, движок готовит), §7 (каузальность PRC закрывается этой фазой).

- [ ] **Step 1: Собрать кандидат движком**

Движок запущен (`UEFIPATCHER_SOCK`/state), CLI из репо:

```bash
cd crates/uefi-cli
cargo run -- image open --path ../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin --name hnx-fix --mode write
cargo run -- hii form set-visibility 'abbce13d-e25a-4d9f-a1f9-2f7710786892:0x10:0#901' --visible
cargo run -- image save --output ../../../refs/amibcp/e8-unhide-fix-candidate.bin
cargo run -- image close hnx-fix
```

(точные флаги `image open` — `--path/--name/--mode`, `hii form set-visibility <form_id> --visible`, `image save --output`; сверить с `crates/uefi-cli/src/main.rs:96-120,185-200`.)

- [ ] **Step 2: Самопроверка кандидата**

```bash
sha256sum refs/amibcp/e8-unhide-fix-candidate.bin
```

Побайтово сверить с ожиданием теста `real_image_unhide_rebuild_keeps_layout`: кандидат должен совпадать с выходом `build_image` из теста (длина 16 МиБ, дифф только внутри файла ABBCE13D). При расхождении — разобрать дифф до прошивки (fvmap-пробник `refs/amibcp/probes/fvmap.py`, если доступен): карта FV, дифф декодированных потоков, FIT не задет.

- [ ] **Step 3: Аддендум отчёта**

В конец `docs/reports/2026-09-02-hw-validation-hnx99tf.md`:

```markdown
## 8. Финальная валидация цикла фиксов (E8) — pending

- Кандидат: `refs/amibcp/e8-unhide-fix-candidate.bin`
  (sha256: `<вставить из Step 2>`, длина 16 МиБ).
- Путь сборки: чистый движковый (`image open` Write → `hii form set-visibility
  'abbce13d-e25a-4d9f-a1f9-2f7710786892:0x10:0#901' --visible` → `image save`);
  кодировщик liblzma raw-LZMA1 (slot-fit), PRC-патч `x-UEFI-AMI` (UPG-токены).
- Движковые самопроверки: real-image (a)–(e) зелёные
  (`cargo test -p uefi-engine --test real_image -- --ignored`).
- Чеклист прошивки (внешним программатором):
  - [ ] POST проходит;
  - [ ] видео есть (нет звукового кода «нет видеокарты»);
  - [ ] Setup → «CPU HWPM State Control»: заголовок формы непустой;
  - [ ] подписи обоих вопросов и опций непустые (PRC-каузальность);
  - [ ] опции переключаются, значения сохраняются.
- Опционально (до прошивки E8): подтверждение предсказания §3.2 на уже-прошитом
  E7 — пустой заголовок формы 901 при видимых значениях в скобках.
- Статус: ожидает прошивки пользователем; результат — аддендум §9.
```

- [ ] **Step 4: Актуализация TODO**

В `TODO.md`, секция «Аппаратная валидация unhide на Huananzhi X99-TF (2026-09-02)»:

- «ENGINE: LZMA-поток lzma-rs валит плату» — оставить `[ ]`, дописать в конец абзаца: `Фикс-цикл: кодировщик liblzma + slot-fit готовы (real-image (c)/(e) зелёные), HW-подтверждение — E8.`
- «ENGINE: пустые подписи … PRC-токенов» — оставить `[ ]`, дописать: `Фикс-цикл: insert-at-id + PRC-патч готовы (real-image (b) зелёный), HW-подтверждение — E8.`
- «hii string list отдаёт только первый string-пакет» — оставить `[ ]`, дописать: `Фикс-цикл: полный обход готов (real-image (a): >5k строк), закрытие после E8-ревью Setup.`
- Миноры: «ops::rebuild не принимает GuidSection-таргеты» → `[x]`; «artifact export пишется в CWD движка» → `[x]`; «Билдер: +2/+1 байта хвоста» → `[x]` (каждый с пометкой `(цикл 2026-09-02)`).

- [ ] **Step 5: Прогон + commit**

Run: `cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check`
Expected: PASS (кода не меняли — контрольный прогон).

```bash
git add docs/reports/2026-09-02-hw-validation-hnx99tf.md TODO.md
git commit -m "docs: HW candidate E8 checklist + report addendum (unhide fix cycle)"
```

- [ ] **Step 6: Передача пользователю**

Финальное сообщение исполнителя: кандидат готов (`refs/amibcp/e8-unhide-fix-candidate.bin`, sha256), чеклист в отчёте §8; после прошивки — заполнить чеклист и закрыть критические пункты TODO (аддендум §9). Если подписи остались пустыми при наличии токенов — возврат к реверсу AMITSE (спека §7, вне этого цикла).

---

## Зависимости задач

```
Task 1 → Task 2 → Task 3 → Task 4
Task 5 (независим от 1-4)
Task 6 → Task 7
Task 8 (независим)
Task 5+6+7+8 → Task 9
Task 10, 11, 12 (независимы)
все → Task 13 → Task 14
```
