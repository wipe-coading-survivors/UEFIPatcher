# План: S4a — полный AMI TermSrc (резерв (а), кандидат E33)

Дата: 2026-09-06. Ветка: `serial-console`. Спека:
`docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md`
(§8.3-а, §11 — мини-pre-check). Прецедент исполнения:
`docs/superpowers/plans/2026-09-06-serial-s4-serialio-swap.md` (S4/E32).

**Tech Stack:** Rust (uefi-engine/uefi-cli), edk2 (UefiPatcherSerialPkg,
стенд S1, docker edk2-builder), python (hack/fv_audit.py).

## Цель и семантика

E32 доказал: донорский SerialIo (97C81E5D) resource-aware, glue v2
живая, baud-ручка действует; дефицит — цвет (TerminalDxe-путь рендерит
без красок, владелец связывает цвет rd450x с TermSrc). S4a = **одна
семантическая переменная против E32**: терминальный консьюмер
TerminalDxe (edk2, 9E863906) → донорский TermSrc (AMI, 54891A9E,
непатченный — §11-3: дефолты = порты 0/1 enabled, 115200 8N1,
тип PcAnsi) + glue v3 (search-all терминальных GUID, §11-5).

Цепочка E33: `[SerialIoAmiDxe, TermSrcAmiDxe, SerialConsoleGlueV3]`
в хвост FV1 @0xB63B18 (первый слот, free_tail ≈ 2 МБ; TermSrc 13 361 Б
< TerminalDxe 65 596 Б — цепочка СЖИМАЕТСЯ). Вопросы S3 q512/q513
не трогаем (q512 → glue SetAttributes; q513 после v3 — только
fallback-узел).

Гейт E33: (a) консоль жива с первого бута (бут-меню/баннер виден);
(b) **цвет** — ANSI-SGR в выводе (Setup-меню с атрибутами; эталон
rd450x); (c) q512-baud по-прежнему действует.

## Task 1. Артефакт донорского TermSrc (извлечение + структурный тест)

- [ ] **Step 1.** Извлечь FFS движком из `refs/amibcp/450x-4g-default.bin`
  (read-only): `session init --force` → `image open` →
  `node search TerminalSrc --mode utf16` (хит 4/118/0/1) →
  `node extract 4/118` → `artifact export <id>
  crates/uefi-engine/tests/data/serial/TermSrcAmiDxe.ffs`
  (13 361 Б, raw @0x981EE0). Сверка: sha256
  `463a6695dda31a078ff7d98b0a7ebb63c14a51c40ddd0d513ecd1197c482acfa`,
  побайтово == S0-блоб `/tmp/serial-s0/blobs/rd450x/TermSrc.ffs`
  (кросс-чек экстрактора; §11-1).
- [ ] **Step 2 (TDD).** Тест в `crates/uefi-engine/tests/serial_ffs.rs`
  (новый блок `serial_termsrc_ami`): (1) длина 13 361; FFS-заголовок
  валиден, GUID 54891A9E-763E-4377-8841-8D5C90D88CDE, type 0x07
  (DXE_DRIVER), size24, header checksum; (2) **DEPEX-секции НЕТ** —
  первая значащая секция GUIDED (subtype 0x02, EE4E5898-LZMA),
  contrast к SerialIoAmiDxe (DEPEX 0x13 первый); (3) UI-секция =
  `TerminalSrc` из guided-дерева (subtype 0x15, UTF-16 до NUL —
  имя в теле сжато, прецедент S4); (4) LZMA payload →
  `decompress(payload, 2)` = PE32 37 824 Б; (5) PE32: MZ/AMD64/
  PE32+ 0x20B/subsystem 11/entry RVA 0x1E30 (lfanew+40);
  (6) sha256-пин артефакта. Сначала тест (падает) → артефакт → зелёный.
- [ ] **Step 3.** `cargo test -p uefi-engine` + `cargo clippy -p
  uefi-engine --all-targets -- -D warnings` + `cargo fmt --all -- --check`.
- [ ] **Step 4.** Commit: `feat(uefi-engine): donor AMI TermSrc artifact (rd450x, unpatched default-path) + structural gate`.

## Task 2. Glue v3 — search-all терминальных GUID (edk2-стенд)

- [ ] **Step 1.** `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/
  SerialConsoleGlue.c`: FindTerminalChild перебирает ВСЕ 4 GUID из
  kTerminalGuidMap (точное совпадение any; mTerminalGuid первым —
  детерминизм), сигнатура без параметра-GUID; вызовы в
  AttachSerialConsole/OnReadyToBoot обновить. Остальное (ReadConfigBytes,
  SetAttributes, нотификация, маркеры) — без изменений. Никаких
  комментариев в коде (кроме file:line-ссылок).
- [ ] **Step 2.** Пересборка стендом S1 (`docker/edk2/build_serial.sh`;
  свежий контейнер + git-archive, прецедент S1/S4-Task2). Артефакт →
  `crates/uefi-engine/tests/data/serial/SerialConsoleGlueV3.ffs`
  (v2-файл НЕ трогать — гейт S4 пиннит его тело). Размер/SHA
  зафиксировать в выводе; тест serial_ffs для glue v2 не меняется.
- [ ] **Step 3.** Обновить `hack/edk2_build_check.py` ожидания, если
  он пиннит состав артефактов glue (проверить; v3 = доп. файл).
- [ ] **Step 4.** `cargo test -p uefi-engine` + clippy + fmt.
- [ ] **Step 5.** Commit: `feat(serial-glue): v3 — terminal child search across all 4 terminal GUIDs (TermSrc default tag is PcAnsi)`.

## Task 3. Гейт S4a — real_image_ops_insert_serial_s4a

- [ ] **Step 1 (TDD).** В `crates/uefi-engine/tests/real_image.rs` —
  `real_image_ops_insert_serial_s4a` (после s4-гейта, `#[ignore]`):
  serial = `[SerialIoAmiDxe.ffs, TermSrcAmiDxe.ffs,
  SerialConsoleGlueV3.ffs]`; вставки After по anchor (прецедент s4);
  add_question ×2 (s3_questions.json); проверки:
  - окно FIRST_SLOT 0xB63B18, span = 7 680-pad + 13 368-pad +
    glueV3-размер-pad (вычислить из фактических размеров файлов;
    выравнивание 8); non-tail = 0 (все изменения поверх 0xFF);
  - зоны (window + setup_slot + sd_slot) — вне 0;
  - tail GUIDs = [97C81E5D, 54891A9E, glueV3-GUID] — TerminalDxe
    9E863906 в FV1 ОТСУТСТВУЕТ (замена, не добавление);
    SerialDxe 9A5163E7 отсутствует (как в s4);
  - state 0xF8, align 8, body-identity с fixture-файлами;
  - донор TermSrc: DEPEX-байтов нет (первая секция GUIDED @+0x18),
    UI `TerminalSrc` из guided-дерева;
  - question_info round-trip q512/q513 (как s4);
  - стабильный rebuild (save → open → save побайтово).
- [ ] **Step 2.** `cargo test -p uefi-engine -- --ignored
  real_image_ops_insert_serial_s4a` (при наличии refs/fw-файла) +
  полный набор + clippy + fmt.
- [ ] **Step 3.** Commit: `test(uefi-engine): S4a gate — TermSrc swap + glue v3 on live image`.

## Task 4. Кандидат E33 + pack-отчёт

- [ ] **Step 1.** CLI-флоу на копии LIVE (refs/fw/
  HNX99TF_200525_original_E5C88C6F.bin, read-only; сокет свой
  `/tmp/serial-s4a/engine.sock`, `rm -f` перед стартом движка;
  session init --force): вставка тройки (anchor = последний файл
  FV1, прецедент S4-Task4), add_question q512/q513, `image save
  /tmp/serial-s4a/E33-candidate.bin`. Зафиксировать sha256, старты
  файлов, счётчик цепочки FV1 (ожидалось 216→219 в E32; тут база
  = LIVE-оригинал, не E32-образ).
- [ ] **Step 2.** Тройная валидация (прецедент E32): (1) гейт-тест
  Task 3 зелёный; (2) `python3 hack/fv_audit.py` — все FV целы,
  вставка только в хвост FV1; (3) зонный дифф кандидат vs оригинал
  — изменения только в [окно тройки] ∪ [Setup-слот] ∪
  [SetupData-слот], счётчик байт в отчёт.
- [ ] **Step 3.** Отчёт `docs/reports/2026-09-06-serial-s4a-e33-pack.md`:
  состав (таблица chain: файл/GUID/старт/размер/sha256/state),
  инварианты (гейт, fv_audit, зонный дифф), семантика против E32
  (одна переменная — терминальный консьюмер; ожидания §11-3:
  дефолты 115200/8N1/PcAnsi), протокол приёмки (ниже), контингенси.
- [ ] **Step 4.** roadmap.md (строка S4a: кандидат готов к E33) +
  TODO.md (резерв (а): pre-check §11 закрыт, минимальный вариант).
- [ ] **Step 5.** Commit: `docs(s4a): E33 candidate packed — TermSrc swap chain + triple validation`.

## Протокол приёмки E33 (за владельцем)

1. Прошивка E33 (решение и руки владельца; артефакт
   `/tmp/serial-s4a/E33-candidate.bin`).
2. (a) Первый бут: консоль жива? Баннер/бут-меню/Setup на COM
   (115200 8N1 — дефолты §11-3). Тайминг-контингенси S4 неактуален
   (glue v3 = та же нотификация, что доказана E32).
3. (b) Цвет: ANSI-SGR в выводе — Setup-меню с атрибутами/рамками
   как на rd450x. Терминал владельца в режиме ANSI.
4. (c) q512: смена baud → мусор на старой скорости после ранних
   строк = действует (SetAttributes glue последним, §11-5).
5. Наблюдения (даже негативные) — дословно в вердикт-отчёт.

## Контингенси (при глухом первом бута — §11-6)

- (i) Несоответствие типов узлов: байт-патч @0x2312 (01→04 =
  VT-UTF8) в PE32-теле артефакта + пересборка FFS; или q513=ANSI
  (рука владельца в Setup, без перепрошивки — если консоль хотя бы
  раз поднялась; иначе (i)).
- (ii) SioSerialPortsLocationVar=нули глушат Start: NVRAM-инъекция
  location-var (значения снять с боевого rd450x — единственный
  источник; на 450x-образе их нет, §11-4). Отдельная задача движка
  (op инициализации NVRAM-записи), вне этого плана.
- (iii) Полный патч-вариант §11-2 (19 байт + свободные оффсеты
  varstore 0x72) — если дефолты нерабочи в принципе.
- CMOS-клин исключён структурно; откат = перепрошивка E32/оригинала.

## Резервы (вне плана)

- Полный AMI TermSrc с патч-картой (§11-2) — фоллбэк.
- SerialIoSioDxe (путь б') — независимость от бинарей.
- Terminal 7A08CB98 — заметка S0 §8.5-6.
