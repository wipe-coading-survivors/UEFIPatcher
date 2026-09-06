# Мини-цикл «ami-patch-op» (patch_ami → payload-секции) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:test-driven-development — тест падает → реализация → проходит, один коммит на задачу.

**Goal:** `patch_ami` пишет setupdata-записи в $SPF-секцию (через LZMA slot-fit рекомпрессию билдером) и AMITSE-правки в PE32-секцию; записи находятся в **собранных** байтах после `build_image`. Инвариант атомарности `add_setup_formset` (precheck до мутаций) сохранён и усилен.

**Spec:** `docs/superpowers/specs/2026-09-03-ami-patch-op-design.md`.

**Files:** `crates/uefi-engine/src/hii/ami_patcher.rs` (rewrite patch-path), `crates/uefi-engine/src/hii/formset_add.rs` (только тесты-фикстуры), `crates/uefi-engine/tests/real_image.rs` (новый `#[ignore]`-гейт).

## Global Constraints

- AGENTS.md: TDD, один коммит на задачу, **без комментариев в коде**, `cargo test -p uefi-engine` + `cargo clippy -p uefi-engine --all-targets -- -D warnings` + `cargo fmt --all` после каждой задачи.
- Plan-defect (AGENTS.md п.11): расхождение плана с реальностью — сперва `docs: fix Task N in ami-patch-op plan (<дефекты>)`, затем реализация.
- Real-image: `#[ignore]`, образ `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`.

---

### Task 1: payload-finders + precheck (RED→GREEN)

`ami_patcher.rs`: `find_payload_path(file: &FfsNode, pred) -> Option<Vec<usize>>` — DFS за leaf-секцией по предикату, через обёртки; отдаёт также факт «кандидат за non-recompressable-обёрткой» → в `precheck_ami_modules` / `patch_ami`: `$SPF`-предикат (`body[..min(0x40,len)]` содержит `b"$SPF"`) для setupdata, PE32 (subtype 0x10, leaf) для AMITSE; ошибки `AmiFilesNotFound` / `MutationBehindCompression`.

Тесты: precheck ловит отсутствие payload (файл есть, секции нет) и барьер Tiano; payload за LZMA-обёрткой досягаем.

### Task 2: patch_ami пишет в секции, записи переживают build (RED→GREEN)

Мутации: `$SPF`-секция `body.extend(records)`; PE32 `body.splice(marker_pos, form_ids)`; `mark_rebuild_to_root_by_path` от пути секции. Старые unit-тесты patch-путей переписываются под новый контракт (фикстуры: файлы с payload-секциями + UI).

Тест-приёмка: `build_image` → `windows().any()` на байтах записи/form-id пар; re-parse валиден.

### Task 3: LZMA slot-fit (RED→GREEN)

Синтетическая GUIDed-LZMA-обёртка вокруг payload-секций (guided body = GUID(16)+dataoff(0x18)+attrs(1)+`compress_lzma(children)`), `parse_section` → дерево. Тест: patch → build → длина секции == слоту; re-parse: записи в детях ($SPF body вырос на 108×N; PE32 содержит form-id пары).

### Task 4: formset_add — фикстуры и атомарность

`formset_add.rs` тесты: AMI-файлы фикстур получают payload-секции (`gap_aware_image`, `resource_flash_image`, `wrapped_resource_node_image`, `plain_resource_node_image`, `two_candidate_resource_image`). Новый тест: precheck с payload-барьером блокирует ДО мутации строк (снапшот-ассерты).

### Task 5: real-image гейт (RED→GREEN)

`tests/real_image.rs`: `real_image_hii_formset_add_ami_records_in_built_bytes` — UI-резолюция AMITSESetupData/AMITSE → `add_setup_formset` → `build_image`: длина 16 МиБ; дифф-регионы == ровно два LZMA-слота AMI-файлов; re-parse: SDP-записи в хвосте $SPF-секции, form-id пары в PE32 AMITSE; collect_forms видит новый формсет.

### Task 6: docs

Отчёт `docs/reports/2026-09-03-ami-patch-op.md` (для параллельной сессии: разведка $SPF §2 спеки, что изменилось, что осталось — формат записей/контейнер), TODO.md: закрыть BLOCKER-пункт, добавить пункт следующего цикла («$PFS-формат + TSE-видимость»).
