# План: S4b — дискриминация виса E33 (TermSrc vs glue-цепочка vs NVRAM-грязь)

Дата: 2026-09-07. Ветка: `serial-console`. Спека §11, вердикт E33
`docs/reports/2026-09-07-serial-s4a-e33-verdict.md` (гипотезы A/B/C).
Хост тестовый, выключен; владелец готов шить (решение 2026-09-07
«шмаляй план, хост оживим следующим рефлешем»).

## Фактор, отсутствовавший в анализе E33 (гипотеза D)

ConOut/ConIn — **NV-переменные**: glue append'ит пути, прошивки их не
стирают. К буту E33 в ConOut уже лежал VT-UTF8-путь эры E32
(TerminalDxe). TerminalDxe удалён, но **TermSrc.Supported принимает
любой из 4 терминальных vendor-GUID** (разбор вердикта §2-3,
сравнения @0x2186–0x21DD) → при консоль-инициализации BDS/TSE
дёргает ConnectController по мёртвому пути, TermSrc клеймит и делает
**повторный Start на том же порту**; dedup-реестр портов @0x8E78
(разбор: скипается только конфиг-чтение @0x229A, создание чайлда
продолжается) → второй чайлд/ошибка → кандидат на ERROR
DXE_BS_DRIVER и вис. E30–E32 этот сценарий не встречали:
TerminalDxe корректно обслуживал любые дубли терминальных путей.

## Дизайн дискриминации (≤2 прошивки)

- **E34** = `[SerialIoAmiDxe, TermSrcAmiDxe]` — **без glue, без
  вопросов**. TermSrc-чайлд создаётся BDS ConnectAll сам (RDP=NULL,
  Supported не требует vendor-узла). Консоль не подключена (glue
  нет) → serial молчит; индикатор — VGA-Setup и **ERROR-строка**
  (RAW-статускод-принтер работает всегда, доказано E33).
- **E35** = `[SerialIoAmiDxe, TerminalDxe, SerialConsoleGlueV3]` —
  тройка E32 с glue v2→v3 (glue-v3 против доказанного TerminalDxe).

Дерево решений (первый бут кандидата — после CMOS-clear, база чистая):

| E34 (без glue) | Вывод |
|---|---|
| Setup входит, ERROR нет | TermSrc сам безвреден → E33-вис = glue-цепочка или NVRAM-грязь → шить E35: чисто ⇒ D (грязная ConOut); висит ⇒ glue-v3/TSE-вывод в TermSrc (A/B) |
| Setup висит и/или ERROR в serial | TermSrc сам токсичен в LIVE-гибриде (A/C) → резерв (а) закрыт; дуга — путь (б) / патч рендера |

CMOS-clear перед E34 обязателен: иначе мёртвые пути E32/E33 в ConOut
маскируют результат (гипотеза D смешивается с A/C).

## Task 1. Гейты движка (s4b/s4c)

- [ ] **Step 1 (TDD).** `crates/uefi-engine/tests/real_image.rs`:
  `real_image_ops_insert_serial_s4b` (#[ignore]) — вставка двойки
  [SerialIoAmiDxe.ffs, TermSrcAmiDxe.ffs] After последнего файла FV1;
  инварианты по прецеденту s4a: окно FIRST_SLOT 0xB63B18, span =
  7 680 + 13 368 = 21 048, non-tail=0; tail GUIDs [97C81E5D,
  54891A9E]; TerminalDxe 9E863906 и SerialDxe 9A5163E7 в FV1
  отсутствуют; **glue-GUID 1EF3A7C2 в FV1 отсутствует** (кандидат без
  glue); state 0xF8/align 8/body-identity; TermSrc-структура (нет
  DEPEX, GUIDED первый, UI TerminalSrc); стабильный rebuild.
  Вопросы НЕ добавляются (кандидат минимальный).
- [ ] **Step 2 (TDD).** `real_image_ops_insert_serial_s4c` — тройка
  [SerialIoAmiDxe, TerminalDxe, SerialConsoleGlueV3] + вопросы
  s3_questions.json; инвариенты как s4-гейт, отличия: glue-файл
  SerialConsoleGlueV3.ffs (body-identity против V3-фикстуры), span =
  7 680 + 65 600 + 41 072.
- [ ] **Step 3.** `cargo test -p uefi-engine -- --ignored
  real_image_ops_insert_serial_s4b real_image_ops_insert_serial_s4c`
  + полный набор + clippy + fmt.
- [ ] **Step 4.** Commit: `test(uefi-engine): S4b/S4c gates — E34 no-glue discriminator pair + E35 glue-v3-vs-TerminalDxe`.

## Task 2. Кандидаты E34/E35 + валидация

- [ ] **Step 1.** CLI-флоу (сокет /tmp/serial-s4b/engine.sock, rm -f
  перед стартом, session init --force, image open --mode write):
  E34 = двойка After 3/215 → save `/tmp/serial-s4b/E34-candidate.bin`;
  E35 = тройка + add_question → save
  `/tmp/serial-s4b/E35-candidate.bin`. sha256, старты, счётчик FV1
  (216→218 / 216→219).
- [ ] **Step 2.** Валидация каждого: гейт (Task 1) зелёный;
  `hack/fv_audit.py` (только FV1 меняется); зонный дифф против
  E5C88C6F — изменения только [окно вставки] (E34) /
  [окно ∪ Setup ∪ SetupData] (E35); счётчики в отчёт.
- [ ] **Step 3.** Постоянные копии `~/E34/…`, `~/E35/…`.
- [ ] **Step 4.** Отчёт `docs/reports/2026-09-07-serial-s4b-e34-e35-pack.md`:
  составы, инварианты, дерево решений (выше), протокол приёмки
  (CMOS-clear → E34 → наблюдение → E35 по дереву).

## Task 3. Протокол приёмки (руки владельца)

1. (при выключенном хосте) CMOS/NVRAM-clear джампером — убрать
   накопленные ConOut/ConIn-пути эры E30–E33 (гипотеза D).
2. Прошить `E34-candidate.bin` → бут:
   - VGA: POST жив? Вход в Setup (DEL/F2) — входит или вис?
   - serial: тишина (консоль не подключена — норма); появление
     `ERROR: Class:…` — фиксировать дословно.
3. По дереву решений — E34 чист ⇒ сброс CMOS не обязателен перед
   E35 (переменная уже чистая), прошить `E35-candidate.bin` → бут:
   - serial жива (TerminalDxe+glue как E32) — регрессия E32;
   - Setup входит/висит; цвет (не должен измениться — TerminalDxe);
   - ERROR-строка?
4. Все наблюдения — дословно в вердикт-отчёт S4b.

## Task 4. Дока + статус

- [ ] roadmap.md (строка S4b: кандидаты E34/E35 готовы, дерево
  решений), TODO.md (обновление записи про E33/S4b).
- [ ] Commit + push dsevosty.

## Контингенси

- E34-вис: резерв (а) закрыт (TermSrc токсичен сам по себе) — план
  закрытия S4a в roadmap + возврат к (б)/патчу рендера TerminalDxe.
- E35-чисто при E34-чисто: E33-вис = NVRAM-грязь (D) → S4c:
  пересборка E33-цепочки + CMOS-clear на живом хосте... нет:
  E33-повтор = [SerialIo, TermSrc, glue-v3] с чистым NVRAM —
  собрать E36 при необходимости (файлы готовы, гейт s4a живой).
- Никаких новых байт-патчей TermSrc до дискриминации.
