# Serial Console S4 — resource-aware SerialIo (донорский свап + glue v2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ступень S4 лестницы Serial Console: serial-стек следует за ресурсами платформы (COM-переезд 2F8/IRQ3 не роняет консоль) + валидация донорского пути (готовые .ffs без сборочной инфраструктуры). Гейт E32: (a) регрессия на дефолтах 3F8 = уровню E31; (b) после Change Settings→2F8/IRQ3 и ребута консоль на новом базовом; (c) при глухоте — фиксация, лечение CMOS не меняется.

**Architecture:** Одна семантическая переменная против кандидата E31: продюсер SerialIo. Наш SerialDxe 9A5163E7 (фикс 0x3F8) заменяется донорским AMI SerialIo 97C81E5D (rd450x, без патчей): driver-model, привязка к AmiSio/EfiSio-чайлдам GenericSio, база UART — из SIO-ресурсов (дизасм-доказательство, спека §10-1). TerminalDxe 9E863906 и S3-вопросы — байт-идентичны E31. Glue v2 = надмножество E31-glue: immediate-путь как раньше + fallback-нотификация на установку gEfiSerialIoProtocolGuid (донор стартует по BDS ConnectAll, не в DXE-фазе — спека §10-3).

**Tech Stack:** Rust (uefi-engine/uefi-cli), edk2 (UefiPatcherSerialPkg, стенд S1), python (hack/fv_audit.py).

**Spec:** `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` §3-S4, §7 (решения S0/E30/E31), §8.3-п(в), аддендум §10 (мини-pre-check S4 — вход этого плана). Прецеденты: планы/отчёты S2 (`2026-09-06-serial-s2-e30-insert.md`) и S3 (`2026-09-06-serial-s3-setup-page.md`, `…-e31-pack.md`).

## Мини-pre-check S4 (вход плана; детально — аддендум спеки §10)

Проба `/tmp/serial-s4-precheck/pnp_scan.py` + дизасм-проходы (артефакты
S0). Закрытые развилки:

1. Донорский SerialIo — resource-aware (база только из SIO-ресурсов,
   скрытых фиксированных fallback-баз нет), ставит SerialIo+DevicePath на
   собственный чайлд; PNP0501-фильтр пути; DEPEX PcdProtocol в LIVE
   удовлетворён; патчей не требует.
2. Наш TerminalDxe привязывается к любому SerialIo-контроллеру; тип —
   PcdDefaultTerminalType при отсутствии терминал-узла в пути.
3. SIO-цепочка LIVE — driver-model, никто не самоконнектится в DXE-фазе
   (поднимается BDS ConnectAll) → glue v2 с нотификацией-fallback;
   кандидат ЗАМЕНЯЕТ SerialDxe (один SerialIo-продюсер).
4. Потребитель `PNP0501_0_NV[1]` статически не найден (имя/GUID — только
   IFR/NVRAM/TSE-зеркало) → применение COM-переезда дискриминируется
   самим E32 (п. (b) гейта); механизм клина — паркинг E31, не трогаем.
5. Своё resource-aware-ядро (путь б') — опора EFI_SIO_PROTOCOL
   (MdePkg/SuperIo.h, 215FDD18, GetResources→ACPI IO-дескриптор);
   резервируется за следующей итерацией (по вердикту E32).

## Task 1. Артефакт донорского SerialIo (извлечение + структурный тест)

- [ ] **Step 1.** Извлечь FFS движком из `refs/amibcp/450x-4g-default.bin`
  (read-only): `node search`/`find` по GUID `97C81E5D-8FA0-486A-AAEA-0EFDF090FE4F`
  → `node extract` артефакта → файл сохранить как
  `crates/uefi-engine/tests/data/serial/SerialIoAmiDxe.ffs` (7 675 Б,
  raw @0x985318). Сверить sha256 с S0-эталоном (`pe32_SerialIo.bin` внутри
  должен совпасть побайтово с извлечённым PE32 — кросс-чек экстрактора).
- [ ] **Step 2 (TDD).** Тест в `crates/uefi-engine/tests/serial_ffs.rs`
  (новый блок `serial_io_ami`): (1) FFS-заголовок валиден, type 0x07
  (DXE_DRIVER); (2) UI-секция = `SerialIo`; (3) DEPEX-секция 22 Б =
  `02 <gEfiPcdProtocolGuid 13A3F0F6…> 08`; (4) PE32 лежит в LZMA-guided
  EE4E5898-секции; (5) sha256-пин артефакта. Сначала тест (падает —
  артефакта нет) → артефакт из Step 1 → зелёный.
- [ ] **Step 3.** `cargo test -p uefi-engine` + `cargo clippy -p
  uefi-engine --all-targets -- -D warnings` + `cargo fmt --all -- --check`.
- [ ] **Step 4.** Commit: `feat(uefi-engine): donor AMI SerialIo artifact (rd450x, resource-aware) + structural gate`.

## Task 2. Glue v2 — нотификация-fallback (edk2-стенд)

- [ ] **Step 1.** `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/
  SerialConsoleGlue.c`: текущий attach-флоу после ReadConfigBytes
  (LocateHandleBuffer(SerialIo) → SetAttributes → ConnectController-цикл →
  FindTerminalChild → append Con*/маркеры → ReadyToBoot-событие) выделить
  в `AttachSerialConsole()`. Entry: если immediate-поиск нашёл хоть один
  SerialIo-хендл — вызвать сразу (поведение E31, байт-совместимо по
  смыслу); иначе `CreateEvent(EVT_NOTIFY_SIGNAL, TPL_CALLBACK)` +
  `RegisterProtocolNotify(&gEfiSerialIoProtocolGuid, …)` — колбэк зовёт
  `AttachSerialConsole()` (повторный вход защитить флагом; NOT_FOUND-выход
  колбэка не разрывает регистрацию — событие сработает на следующей
  установке протокола). Enable==0 и PNP0501_0_NV-чтение — без изменений.
- [ ] **Step 2.** Пересборка стендом `docker/edk2/build_serial.sh`
  (контейнер; git-archive `refs/edk2`); проверка `hack/edk2_build_check.py`
  — SerialDxe/TerminalDxe обязаны остаться побайтово идентичными S1
  (их не трогали), glue v2 = новый sha256; зафиксировать размер.
- [ ] **Step 3.** Обновить `crates/uefi-engine/tests/data/serial/
  SerialConsoleGlue.ffs` (коммит артефакта + исходника; S1-прецедент —
  .ffs тест-данных коммитятся). Тест `serial_ffs.rs` glue-блока — обновить
  sha256-пин и, если пинится размер, размер.
- [ ] **Step 4.** Commit: `feat(serial-glue): v2 — SerialIo protocol-notify fallback (driver-model producer support)`.

## Task 3. Движковый гейт `real_image_ops_insert_serial_s4`

- [ ] **Step 1 (TDD).** В `crates/uefi-engine/tests/real_image.rs` — новый
  `#[ignore]`-тест по образцу `real_image_ops_insert_serial_s3`
  (path-хелперы, `--ignored`, файл-гейт как у s3): open E5C88C6F (write) →
  insert `SerialIoAmiDxe.ffs` (замена первого элемента тройки) → insert
  `TerminalDxe.ffs` → insert `SerialConsoleGlue.ffs` (v2) → `hii question
  add` q512/q513 (тот же `tests/data/serial/s3_questions.json`, цель
  `3/28/1/0#10019`) → build → инварианты: (1) files FV1 216→219; (2)
  донорский FFS в re-parse (UI `SerialIo`, DEPEX PcdProtocol на месте);
  (3) `SerialDxe` 9A5163E7 в образе ОТСУТСТВУЕТ; (4) state-байты
  вставленных = 0xF8 (polarity-1 адаптация `ops::insert`); (5) вне окна
  вставки + двух Setup-слотов байт-дифф = 0; (6) вопросы q512/q513 в
  re-parse (`hii form list`/records). Сначала тест — падает (нет glue v2
  артефакта/донора) → после Task 1/2 зелёный.
- [ ] **Step 2.** `cargo test -p uefi-engine -- --ignored
  real_image_ops_insert_serial_s4` живьём (образ в `refs/fw/`), затем
  полный `cargo test -p uefi-engine` + clippy + fmt.
- [ ] **Step 3.** Commit: `test(uefi-engine): S4 gate — donor SerialIo swap + glue v2 on live image`.

## Task 4. Кандидат E32 + pack-отчёт

- [ ] **Step 1.** Сборка кандидата живым движком (CLI-скрипт по прецеденту
  S3, рабочий каталог `/tmp/serial-s4/`): та же последовательность, что в
  гейте Task 3. Выход `E32-candidate.bin` (16 МиБ), sha256; постоянная
  копия `~/E32/E32-candidate-<sha8>.bin`.
- [ ] **Step 2.** Независимые инварианты: `hack/fv_audit.py` (files
  216→219; used/free_tail ± = суммарный span вставки: донор 7 675 <
  SerialDxe 32 848 → тройка СТАДЁЖКУ против E31, пересчитать ожидание);
  байтовый дифф против E5C88C6F (окна: вставка FV1 + слоты Setup/SetupData;
  вне — 0).
- [ ] **Step 3.** Отчёт `docs/reports/2026-09-06-serial-s4-e32-pack.md`:
  состав (донорский SerialIo sha256, TerminalDxe = S1, glue v2 sha256,
  вопросы = S3), инварианты гейта/аудита/диффа, протокол приёмки:
  (a) дефолты 3F8/115200 — boot-вывод и Setup на COM1, смена baud из
  Setup действует (регрессия S3); (b) Change Settings→2F8/IRQ3 →
  сохранить (F10/явное) → ребут → смотреть **COM2 (0x2F8)** 115200:
  консоль там? на COM1 — тишина?; (c) при глухоте обоих — не
  реанимировать образ, лечение = CMOS-джампер, наблюдения в вердикт.
  Контингенси: «первая загрузка глухая, вторая живая» = timing-гипотеза
  (AMI BDS читает ConOut до ConnectAll) — фиксировать в вердикте,
  доработка glue по вердикту. Прошивка — только владельцем.
- [ ] **Step 4.** Commit: `docs(s4): E32 pack — donor SerialIo candidate, invariants, acceptance protocol` + push dsevosty.

## Task 5. После вердикта E32 (отдельные решения владельца — НЕ этот план)

- Резерв (б'): собственный `SerialIoSioDxe` в UefiPatcherSerialPkg
  (driver binding на EfiSio-чайлдах PNP0501, GetResources→IO-base→16550)
  — полная независимость от донорских бинарей; отдельный план.
- Резерв (а): полный AMI-стек TermSrc (imm32 0x72 + CR-оффсеты LIVE +
  2–3 NVRAM-записи) — патч-карта §8.3; отдельный план.
- Terminal 7A08CB98 — резерв-заметка (§10-6).
- Паркинг E31 (флаги 0x10/0x10 в IfrBuilder): E32 собирается из базы
  E5C88C6F — вопросы q512/q513 генерируются заново с текущими флагами
  `0x00`; зеркалирование `0x10/0x10` сознательно отложено — не вводить
  вторую переменную в E32 (UX-различие F10 известно и принято).

## Verification (после каждой задачи)

```bash
env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u all_proxy \
  cargo test -p uefi-engine
env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u all_proxy \
  cargo clippy -p uefi-engine --all-targets -- -D warnings
cargo fmt --all -- --check
```
