# Serial Console S2 — вставка edk2-пары в живой образ + E30-pack (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Вставить S1-артефакты (SerialDxe + TerminalDxe + SerialConsoleGlue, 123 136 Б с прокладкой) append'ом в свободный хвост FV1 живого образа E5C88C6F движковой операцией `node insert --mode after` и подготовить флеш-кандидат E30 с протоколом приёмки.

**Architecture:** Движковая машинерия готова и доказана на этом образе (builder round-trip байт-идентичен, `real_image_ops_remove_last_file` показывает паттерн rebuild-с-сохранением длины тома) — S2 добавляет гейт-тест вставки трёх serial-.ffs в полный образ (инварианты «изменился только хвост FV1»), затем операционно собирает E30-кандидат через CLI движка и упаковывает протокол валидации. Код-дельта минимальна; основная ценность — верифицируемый артефакт и приёмочный протокол.

**Tech Stack:** Rust (uefi-engine: parser/ops/builder), uefi-cli (session/image/node), hack/fv_audit.py (независимая вторая проверка), .ffs-артефакты `crates/uefi-engine/tests/data/serial/` (blessed, sha256 в отчёте S1 §3).

**Spec:** `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` — §3 «S2 — Говорящий UART (флеш-гейт E30)», §7 «Решения S0» (входы) + аддендум «Мини-pre-check S2» (этот коммит). Отчёты-входы: `docs/reports/2026-09-05-serial-s0-recon.md` §8.1/§8.5, `docs/reports/2026-09-06-serial-s1-build.md` §5-6.

## Global Constraints

- BIOS-образы никогда не коммитятся: живой образ — только чтение из `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`; E30-кандидат живёт вне git (в артефактной директории), в отчёт — только sha256/офсеты.
- S1-артефакты blessed и не пересобираются: вставляются байт-в-байт файлы из `crates/uefi-engine/tests/data/serial/` (SerialDxe 32 848 Б, TerminalDxe 65 596 Б, SerialConsoleGlue 24 688 Б; sha256 — отчёт S1 §3). Решение по PE32-doubling: **принять** удвоенные ~123,1 КБ (запас 16,9×; чистка — TODO root-cause, не в этой ступени).
- Вставка: append в хвост FV1 (DXE) @0x890000, FvNameGuid `5C60F367-A505-419A-859E-2A4FF6CA6FE5`, первый свободный слот 0xB63B18; порядок SerialDxe → TerminalDxe → SerialConsoleGlue (S1 §6); FFS-файлы в FV 8-выровнены (между TerminalDxe и Glue возникнет 4-Б 0xFF-прокладка — её кладёт builder).
- **State-байт вставленных файлов (rule-11, вскрыто сборкой кандидата):** FV1 имеет EFI_FVB2_ERASE_POLARITY=1 (attrs 0x0004feff, бит 11; валидный файл = raw 0xF8, как все 216 существующих), а S1-артефакты несут GenFfs-форму polarity-0: `state[23]=0x07`. Verbatim-вставка даёт файлы, которые PI-DXE-core сочтёт HEADER_INVALID/DELETED. Движок обязан при insert адаптировать: **если enclosing-том empty_byte=0xFF (polarity 1) и state вставляемого == 0x07 → заменить на 0xF8** (узкое правило; иные state проходят verbatim). Header-checksum не затронут: [23] исключается из суммы (S1-семантика). Артефакты на диске НЕ перекодировать (blessed, polarity-0-форма — норма для standalone .ffs).
- Флеш — только решением владельца; план доводит до ready-to-flash артефакта и протокола, вердикт E30 — владельческий. Закрытие ступени S2 аддендумом спеки происходит ПОСЛЕ вердикта E30 (не в этом плане).
- Мини-pre-check S2 (см. спека §7, аддендум): (PC-A) disconnect SerialIo в CsmDxe — только CSM-рантайм, чистый UEFI-бут порт не отбирает; E30 обязан бутить UEFI-путём. (PC-B) консольные переменные LIVE — под `gEfiGlobalVariableGuid`; перезаписи `ConOut` в образе статически нет; NVRAM-инициализация НЕ нужна (glue создаёт переменные сам).
- Module-first rule, TDD, один коммит на шаг где указан `git commit`, никаких комментариев в коде (кроме `file:line`-референсов). Тесты на живом образе — `#[ignore]`, запуск явно: `cargo test -p uefi-engine -- --ignored real_image_ops_insert_serial_s2`.
- Перед запуском движка: `rm -f "$UEFIPATCHER_SOCK"`. Прокси для тестов шлюза чистить полностью (http/https/HTTP/HTTPS/ALL/all) — в S2 не задействовано, но при `cargo test --all` помнить.
- Push — только на remote `dsevosty`.

---

### Task 1: Гейт-тест вставки serial-пары в полный живой образ

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (добавить тест после `real_image_ops_remove_last_file`)
- Test: тот же файл, `#[ignore]`-тест `real_image_ops_insert_serial_s2`

**Interfaces:**
- Consumes: `parse_image`, `load_fw` (уже в файле); `uefi_engine::ops::{insert, InsertMode}`; `uefi_engine::builder::build_image`; `uefi_engine::types::{ImageMode, Target, FfsType}`; файлы `tests/data/serial/{SerialDxe,TerminalDxe,SerialConsoleGlue}.ffs`.
- Produces: эталонный инвариантный набор S2-вставки (используется Task 3 при валидации E30-кандидата: те же утверждения).

- [ ] **Step 1: Write the failing test**

Добавить в конец `crates/uefi-engine/tests/real_image.rs` (после `real_image_ops_remove_last_file`; константы первого слота и объёмов — из S0 §8.1 / S1 §6):

```rust
#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_s2() {
    use uefi_engine::builder::build_image;
    use uefi_engine::ops::{insert, InsertMode};
    use uefi_engine::types::{ImageMode, Target};

    const MAIN_FV_OFF: usize = 0x890000;
    const FIRST_SLOT: usize = 0xB63B18;
    const SERIAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/serial");

    let data = load_fw();
    let serial: Vec<&str> = vec![
        "SerialDxe.ffs",
        "TerminalDxe.ffs",
        "SerialConsoleGlue.ffs",
    ];

    let mut img = parse_image(&data, ImageMode::Read, "img1", "s1").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == MAIN_FV_OFF as u32)
        .expect("main FV @0x890000");
    let files_before = img.root.children[vol_idx].children.len();
    assert!(files_before > 100, "main FV files: {files_before}");

    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in &serial {
        let path = std::path::Path::new(SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }

    let rebuilt = build_image(&img).expect("build after insert");

    assert_eq!(rebuilt.len(), data.len(), "image size must be preserved");
    let mut regions = 0usize;
    let mut first_diff = usize::MAX;
    let mut last_diff = 0usize;
    for (i, (a, b)) in data.iter().zip(rebuilt.iter()).enumerate() {
        if a != b {
            first_diff = first_diff.min(i);
            last_diff = i;
            regions += 1;
        }
    }
    assert!(regions > 0, "insert must change bytes");
    assert_eq!(first_diff, FIRST_SLOT, "first changed byte must be first free slot");
    let expected_span = 32_848 + 4 + 65_596 + 24_688;
    assert!(
        last_diff < FIRST_SLOT + expected_span,
        "changes must stay inside {expected_span}-byte span: last_diff={last_diff:#x}"
    );
    let mut non_tail_changes = 0usize;
    for i in FIRST_SLOT..=last_diff {
        if rebuilt[i] != data[i] && data[i] != 0xFF {
            non_tail_changes += 1;
        }
    }
    assert_eq!(non_tail_changes, 0, "all changed bytes must lie over 0xFF free tail");

    let re_img = parse_image(&rebuilt, ImageMode::Read, "img1", "s1").unwrap();
    let vol = &re_img.root.children[vol_idx];
    assert_eq!(vol.children.len(), files_before + 3);
    let tail: Vec<String> = vol.children[files_before..]
        .iter()
        .map(|f| {
            f.guid
                .map(|g| g.to_string().to_ascii_uppercase())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        tail,
        vec![
            "9A5163E7-5C29-453F-825C-837A46A81E15".to_string(),
            "9E863906-A40F-4875-977F-5B93FF237FC6".to_string(),
            "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43".to_string(),
        ],
        "inserted files must sit in S1 order at chain end"
    );
    for f in &vol.children[files_before..] {
        assert_eq!(
            f.offset % 8,
            0,
            "FFS file must be 8-aligned in FV, got @{:#x}",
            f.offset
        );
    }

    let stable = build_image(&re_img).expect("stable rebuild");
    assert_eq!(stable, rebuilt, "rebuild of re-parsed tree must be stable");
}
```

После Task 1b тест расширяется двумя ассертами (state-байты вставленных == 0xF8, polarity-бит FV1) — код расширения в Task 1b.

**Уточнение по факту (rule-11, вскрыто сборкой кандидата в Task 2):** тест выше не проверял state-байты — verbatim-вставка 0x07 в polarity-1 FV даёт недиспетчеризуемые файлы. Требование адаптации — в Global Constraints; реализация и расширение теста — Task 1b ниже. Порядок span/bайтовых инвариантов не меняется: изменения по-прежнему все над 0xFF-хвостом (state-байт — внутри вставляемых файлов).

- [ ] **Step 2: Run test to verify it fails or passes honestly**

Run: `cargo test -p uefi-engine -- --ignored real_image_ops_insert_serial_s2`
Expected: PASS, если машинерия уже корректна (это допускается: тест закрывает гейт, а не новую функцию). Допустимые исходы: (а) PASS — зафиксировать вывод; (б) FAIL по реальному дефекту (падение assert с конкретикой) — диагностика по правилу systematic-debugging, минимальный фикс в движке отдельным шагом, затем PASS. НЕ ослаблять ассерты под FALL.

- [ ] **Step 3: Run crate checks**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check`
Expected: PASS (ignored-набор +1 новый; все не-ignored зелёные; абсолютные счётчики не фиксируются — крейт много меньше workspace).

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(s2): real-image insert gate for serial pair (tail-append invariants)"
```

Если понадобился фикс движка — СНАЧАЛА отдельный коммит фикса с пояснением в сообщении, затем этот тест-коммит.

---

### Task 1b: State-адаптация вставляемых FFS под erase-polarity тома (rule-11)

Добавлен по факту сборки кандидата (Task 2 concern №1, подтверждён контроллером побайтово: артефакты [23]=0x07, FV1 attrs=0x0004feff → ERASE_POLARITY=1, существующие файлы [23]=0xF8, кандидат после verbatim-вставки — [23]=0x07 → PI-семантика: HEADER_INVALID|DELETED, драйверы не диспетчеризуются).

**Files:**
- Modify: `crates/uefi-engine/src/ops.rs` (функция `insert`)
- Modify: `crates/uefi-engine/tests/real_image.rs` (расширение `real_image_ops_insert_serial_s2`)
- Test: юнит-тест рядом с существующими тестами insert (где они живут в крейте — найти grep'ом `mod tests`/существующие insert-тесты; если интеграционные — `tests/`)

**Interfaces:**
- Consumes: `ParsingData::Volume(vd).empty_byte` (парсер уже кладёт erase_polarity в каждый Volume и File узел, `parser/file.rs:54`, `parser/volume.rs:35`); сигнатуру `insert(root, target, ffs_bytes, mode)` не менять.
- Produces: семантика insert — вставляемый файл получает валидный для enclosing-тома state-байт: при empty_byte=0xFF и новом state 0x07 → 0xF8 (иначе verbatim). Хедж для последующих задач: артефакты на диске остаются blessed polarity-0-формой.

- [ ] **Step 1: Write the failing tests**

Юнит-тест (в месте существующих insert-тестов; фикстура — маленький polarity-1 FV: header c FvbAttributes битом EFI_FVB2_ERASE_POLARITY и empty_byte=0xFF, один дочерний файл; вставляем .ffs со state 0x07):

```rust
#[test]
fn insert_adapts_state_byte_to_erase_polarity() {
    // фикстура: volume node с ParsingData::Volume{empty_byte: 0xFF, ..},
    // один child-file; ffs_bytes с header[23] == 0x07
    // insert(.., InsertMode::After) в последний файл
    // assert: новый child.header[23] == 0xF8; header[16] (hdr-checksum) не изменён
}

#[test]
fn insert_keeps_state_byte_verbatim_in_polarity0_volume() {
    // та же вставка в volume с empty_byte = 0x00
    // assert: header[23] остался 0x07
}
```

Расширение `real_image_ops_insert_serial_s2` (после GUID-ассерта):

```rust
    let fv_attrs = u32::from_le_bytes(
        rebuilt[MAIN_FV_OFF + 0x2C..MAIN_FV_OFF + 0x30]
            .try_into()
            .unwrap(),
    );
    assert_eq!((fv_attrs >> 11) & 1, 1, "FV1 erase polarity must be 1");
    for f in &vol.children[files_before..] {
        assert_eq!(
            f.header[23],
            0xF8,
            "inserted file state must be polarity-1 valid"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p uefi-engine insert_adapts_state` и `cargo test -p uefi-engine -- --ignored real_image_ops_insert_serial_s2`
Expected: оба FAIL (0x07 вместо 0xF8).

- [ ] **Step 3: Implement adaptation in ops::insert**

В `insert`, после `parse_ffs_bytes`: определить enclosing-Volume для позиции вставки (Into → сам parent; Before/After → родитель таргета, т.е. `parent_path[..len-1]`; подниматься до ближайшего `FfsType::Volume`), взять его `ParsingData::Volume(vd).empty_byte`; если `== 0xFF` и `new_node.header[23] == 0x07` → `new_node.header[23] = 0xF8`. Нет Volume-предка — не трогать. Никаких комментариев в коде.

- [ ] **Step 4: Run tests to verify they pass + crate checks**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check`
Expected: PASS (ignored-набор: 27 → 27, счётчики passed не падают).

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/ops.rs crates/uefi-engine/tests/real_image.rs
git commit -m "fix(engine): adapt inserted FFS state byte to volume erase polarity (0x07->0xF8)"
```

---

### Task 2: Сборка E30-кандидата через CLI движка + двойная валидация

**Files:**
- Create: артефакты ВНЕ git: `$UEFIPATCHER_DATA` или `/tmp/serial-s2/` (E30-кандидат, sha256, логи)
- Consumes: движок на unix-сокете, `uefi-cli` (session/image/node insert/image save), `hack/fv_audit.py`, тест Task 1 как эталон инвариантов.

**Interfaces:**
- Produces: `E30-candidate.bin` (16 МБ, sha256 фиксируется для отчёта Task 3 и прошивки владельцем); лог валидации (движок + fv_audit.py).

- [ ] **Step 1: Поднять движок и открыть образ в write-режиме**

(rule-11 по факту: дефолт сокета бинаря — `/run/uefipatcher.sock`; везде задаём UEFIPATCHER_SOCK явно; режима `edit` нет — есть `write`; `session init` при stale-state требует `--force`)

```bash
export UEFIPATCHER_SOCK=/tmp/serial-s2/engine.sock
rm -f "$UEFIPATCHER_SOCK"
mkdir -p /tmp/serial-s2
cargo run -q -p uefi-engine --bin engine > /tmp/serial-s2/engine.log 2>&1 &
sleep 2
cargo run -q -p uefi-cli -- session init --force
cargo run -q -p uefi-cli -- image open \
    --mode write refs/fw/HNX99TF_200525_original_E5C88C6F.bin
```

Expected: session/image id в выводе; движок стартовал без ошибки сокета.

- [ ] **Step 2: Вставить три .ffs по путям из node list**

(rule-11 по факту: `node tree` и фильтр по type в CLI отсутствуют — адресация путями вида `3/215` из живого `node list`; `node insert` печатает id ТА RGETA, а не нового узла — путь нового узла = путь таргета + 1 по последнему индексу)

```bash
cargo run -q -p uefi-cli -- node list            # найти путь тома @0x890000 (напр. "3") и индекс его последнего файла (напр. "3/215")
cargo run -q -p uefi-cli -- node insert 3/215 --file crates/uefi-engine/tests/data/serial/SerialDxe.ffs --mode after
cargo run -q -p uefi-cli -- node insert 3/216 --file crates/uefi-engine/tests/data/serial/TerminalDxe.ffs --mode after
cargo run -q -p uefi-cli -- node insert 3/217 --file crates/uefi-engine/tests/data/serial/SerialConsoleGlue.ffs --mode after
```

(конкретные индексы — из фактического вывода `node list`; пример выше — фактические значения живого образа: том 3, последний файл 215)

Expected: каждый insert отрабатывает без RPC-ошибки; SerialDxe→TerminalDxe→SerialConsoleGlue в конце цепочки FV1.

- [ ] **Step 3: Сохранить кандидата и зафиксировать sha256**

```bash
cargo run -q -p uefi-cli -- image save /tmp/serial-s2/E30-candidate.bin
sha256sum /tmp/serial-s2/E30-candidate.bin refs/fw/HNX99TF_200525_original_E5C88C6F.bin | tee /tmp/serial-s2/sha256.txt
cmp -l /tmp/serial-s2/E30-candidate.bin refs/fw/HNX99TF_200525_original_E5C88C6F.bin | wc -l
```

Expected: размер обоих 16 МБ; differing-байты < 123 136 (часть вставленных байт может совпадать с 0xFF-хвостом) и ВСЕ строго внутри [0xB63B18, 0xB63B18+123 136) — точная проверка диапазона в Step 4 (cmp даёт только счётчик). Дополнительно (state-инвариант Task 1b): у вставленных файлов в кандидате `bytes[off+23] == 0xF8` для off ∈ {0xB63B18, 0xB63B18+32848+4, 0xB63B18+32848+4+65596}.

- [ ] **Step 4: Вторая независимая валидация (fv_audit.py) + сверка с инвариантами Task 1/1b**

```bash
python3 hack/fv_audit.py refs/fw/HNX99TF_200525_original_E5C88C6F.bin | tee /tmp/serial-s2/fv_orig.tsv
python3 hack/fv_audit.py /tmp/serial-s2/E30-candidate.bin | tee /tmp/serial-s2/fv_candidate.tsv
diff /tmp/serial-s2/fv_orig.tsv /tmp/serial-s2/fv_candidate.tsv
```

Expected (rule-11 по факту): diff только в строке FV @0x890000 — files +3, used/free_tail ±123 140 (НЕ 123 136: fv_audit меряет used до невыровненного конца последнего файла, вставка стартует с 8-выровненного 0xB63B18 — 4 Б разницы это выравнивание, не повреждение; байтовым авторитетом остаётся cmp-инвариант Step 3). Плюс прогон теста Task 1 (расширенного Task 1b) — зелёный.

- [ ] **Step 5: Заглушить движок**

```bash
pkill -x engine
```

Expected: процесс завершён (использовать `pkill -x` — не `-f`, правило AGENTS/владельца).

Коммита в этом Task нет (артефакты вне git); все результаты (sha256, diff, выводы) уходят в отчёт Task 3.

---

### Task 3: Отчёт S2 + E30-flashpack (протокол приёмки) + push

**Files:**
- Create: `docs/reports/2026-09-06-serial-s2-e30-pack.md`
- Modify: `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` (§7: пометка «S2 готова к E30» — БЕЗ закрытия ступени), `roadmap.md` (статус S2 «в ожидании E30»)

**Interfaces:**
- Consumes: результаты Tasks 1-2 (инварианты, sha256, diff), таблицу атрибуции маркеров S1 §5 (R-S1.1), находки мини-pre-check (спека §7), механику CsmDxe (S0 §7.4 + PC-A).

- [ ] **Step 1: Написать отчёт**

Секции отчёта (по стилю отчёта S1):
1. **Стенд** — версия движка/коммит, команда сборки кандидата (транскрипт Task 2 Steps 1-3 дословно).
2. **Артефакт** — `/tmp/serial-s2/E30-candidate.bin`, размер 16 МБ, sha256 (кандидат и база), офсет вставки 0xB63B18, состав (3 FFS + 4-Б прокладка), длина цепочки FV1 до/после.
3. **Инварианты** — вывод теста Task 1 (первый изменённый байт = 0xB63B18; все изменения над 0xFF-хвостом; 8-выравнивание; стабильный повторный rebuild) + fv_audit-diff (Task 2 Step 4).
4. **Протокол E30** — предусловия и приёмка:
   - Терминал: любой терминал 115200 8N1 без flow control (рекомендация `picocom /dev/ttyUSB0 -b 115200` или minicom); маркеры — чистый ASCII, VT-UTF8-специфика для маркеров несущественна (полноценная VT-UTF8-графика — вопрос S3+).
   - Предусловие PC-A: бут UEFI-путём (не CSM): CsmDxe-disconnect SerialIo — CSM-рантайм-механика (спека §7 аддендум), легаси-бут отберёт порт и даст ложный негатив.
   - Ожидаемая последовательность на COM1: маркер `SC-S1 glue: serial console attached (ConOut updated)` → UEFI-вывод (POST/баннер) → маркер `SC-S1 glue: ReadyToBoot`.
   - Таблица атрибуции (из S1 §5): нет ничего → смотри контингенси; только маркер 1 → вставка/SerialIo/ConOut OK, маршрут BDS под вопросом; маркер 1+2 без boot-вывода → консоль-энумерация AMITSE (контингенси: dump ConOut из Shell `dmpstore ConOut`, сверка инстансов); мусорный вывод → скорость/форматы (115200 8N1, VT-UTF8 vs ANSI).
   - Критерий успеха ступени (гейт спеки §3): UART-адаптер видит UEFI boot-вывод.
5. **Инструкция прошивки владельцу** — указать: файл-кандидат, sha256 для сверки перед прошивкой, инструмент по прецеденту E26-E29, сброс NVRAM после прошивки НЕ требуется (glue создаёт переменные сам; PC-B).
6. **Открытое** — вердикт E30 ждём; после него — закрывающий аддендум спеки + roadmap (следующая сессия).

- [ ] **Step 2: Аддендум спеки и roadmap**

В `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` §7 добавить пункт: «S2 готова к E30 (2026-09-06): кандидат собран (sha256 …, вставка @0xB63B18, 123 136 Б), инварианты движковые+fv_audit; вердикт E30 — за владельцем; закрытие ступени — отдельным аддендумом по вердикту». В `roadmap.md` — статус S2 «в ожидании E30» (стиль S0/S1).

- [ ] **Step 3: Проверки и коммит**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add docs/reports/2026-09-06-serial-s2-e30-pack.md \
        docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md \
        roadmap.md
git commit -m "docs(s2): E30 pack report + spec/roadmap ready-to-flash state"
```

- [ ] **Step 4: Push**

```bash
git push dsevosty fix/cycle6-reimplent:fix/cycle6-reimplent
```

Expected: ветка на dsevosty обновлена; origin не трогаем.

---

## Правила для исполнителей (из прецедентов S0/S1)

- Таргет `<id>` для node-команд — из вывода `node list`/`node tree`; при расхождении с планом (id-формат, флаги CLI) — не подгонять план молча: фиксировать в отчёте задачи, контроллер решает rule-11.
- `cargo test -p uefi-engine` всегда с полным набором; прокси-переменные чистить при `--all`.
- Движок глушить только `pkill -x engine`; перед стартом `rm -f "$UEFIPATCHER_SOCK"`.
- Если `image save`/`node insert` в CLI не хватает какого-то флага, который план считает существующим — это rule-11: СНАЧАЛА docs-коммит с правкой плана, потом реализация.
- Ветка та же: `fix/cycle6-reimplent`. Worktree не нужен (прецедент S1, рулинг владельца).

## Self-Review (выполнен при написании)

- Покрытие спеки: §3-S2 вставка (Task 1-2), NVRAM-инициализация — не нужна (PC-B, Global Constraints), гейт E30 — протокол (Task 3), «инженерные наработки: real-image edit insert» (Task 1). §5-ограничения (образы не коммитим; .ffs можно) — Global Constraints.
- Placeholder-скан: Steps операционные задачи содержат точные команды; идентификаторы узлов (`<id-…>`) — единственные переменные, они берутся из живого вывода на Step 2 той же задачи, алгоритм их получения указан.
- Консистентность: имена теста `real_image_ops_insert_serial_s2` одинаковы в Steps 1-3; sha256-файл `/tmp/serial-s2/sha256.txt` используется в Task 3 §2; GUID-строки в тесте соответствуют артефактам (сверено с файлами: SerialDxe 9A5163E7-5C29-453F-825C-837A46A81E15, TerminalDxe 9E863906-A40F-4875-977F-5B93FF237FC6, SerialConsoleGlue 1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43); uguid Display даёт lowercase — тест сравнивает через `.to_ascii_uppercase()` (правило AGENTS).
