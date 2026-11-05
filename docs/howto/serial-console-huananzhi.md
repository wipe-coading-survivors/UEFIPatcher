# HOWTO: серийная консоль на платформе, где её нет (Huananzhi X99-TF)

Статус: **гейт S4 пройден на железе** (E32: POST + полный Setup по COM,
смена baud/порта из Setup действует; лестница S0–S4b, вердикты
`docs/reports/2026-09-06-serial-s4-e32-verdict.md` и далее). Спека:
`docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md`.

Кейс: Huananzhi X99-TF — обычная десктопная плата, но её BIOS-билд —
«тихий» производный серверного Lenovo-референса: серийный вывод вырезан
вендором, снаружи только COM-разъём. Решение — собрать консольный стек из
трёх DXE-модулей и вставить в свободный хвост FV1, а настройку повесить на
вопросы Setup.

## 1. Архитектура (итог лестницы E30–E34)

| Модуль | GUID | Источник | Роль |
|---|---|---|---|
| SerialIo (AMI) | 97C81E5D | **донор** из образа RD450x | продюсер EFI Serial Io; resource-aware — берёт базу из ресурсов SIO-чайлда, следует за переездом порта |
| TerminalDxe | 9E863906 | свой (edk2-пакет) | терминальный протокол поверх SerialIo |
| SerialConsoleGlue v2 | 1EF3A7C2 | свой (edk2-пакет) | привязка: SetAttributes → ConnectController → терминальный чайлд → append в ConOut/ConIn/ErrOut + нотификация на установку SerialIo |

Почему именно так (проверено на железе):

- **донор вместо своего SerialDxe**: свой SerialDxe с фиксированной базой
  0x3F8 после смены порта из Setup глухнет и клинит обе консоли (E31,
  лечение — CMOS-джампер); донорный AMI SerialIo переезжает за ресурсами
  (E32: переезд подтверждён — ОС видит `ttyS1 at I/O 0x2f8`);
- **не использовать TermSrc** (полный AMI-терминал): токсичен вне родного
  AMI-окружения — E34 вис при входе в Setup с ERROR-спамом DXE;
- glue v2 держит и immediate-путь, и нотификацию (донор — driver-model,
  стартует по BDS ConnectAll).

Blessed-артефакты трёх модулей лежат в
`crates/uefi-engine/tests/data/serial/` (`SerialIoAmiDxe.ffs`,
`TerminalDxe.ffs`, `SerialConsoleGlue.ffs`); для пересборки своих —
edk2-стенд UefiPatcherSerialPkg (отчёт S1). Донорский SerialIo изымается
из образа-донора движком: `node extract` по GUID.

## 2. Сборка E32-конфигурации

```bash
uefi-cli session init --name serial --force
uefi-cli image open refs/fw/HNX99TF_200525_original_E5C88C6F.bin --mode write

uefi-cli node list
# найти том FV1 (@0x890000, напр. path "3") и индекс последнего файла (напр. "3/215")

uefi-cli node insert 3/215 --file SerialIoAmiDxe.ffs      --mode after
uefi-cli node insert 3/216 --file TerminalDxe.ffs         --mode after
uefi-cli node insert 3/217 --file SerialConsoleGlue.ffs   --mode after

# вопросы настройки в форму Serial Port 1 Configuration (10019)
uefi-cli hii question add \
    899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0#10019 \
    --file s3_questions.json        # q512 Baud Rate, q513 Terminal Type (varstore 1 @93/94)

uefi-cli image save /tmp/serial/E32-candidate.bin
```

Вставка идёт строго в свободный хвост FV над 0xFF-padding (state 0xF8);
движок соблюдает выравнивание и пересчёт заголовков. Пример схемы вопросов —
`crates/uefi-engine/tests/data/serial/s3_questions.json` (см.
[setup-section-intelrc.md](setup-section-intelrc.md) про выбор
varstore/qid/офсетов).

## 3. Верификация образа (до прошивки)

```bash
# байтовый дифф: изменения строго в трёх зонах — окно вставки FV1,
# слот Setup (LZMA-рекомпрессия), слот SetupData; вне зон 0 байт
cmp -l base.bin E32-candidate.bin | wc -l    # ~186 427 изменённых байт (E32)
cmp -l base.bin E32-candidate.bin | head     # офсеты (1-based) — сверить с зонами

# независимый аудит FV: files 216→219, used += вставка, free_tail -= вставка
python3 hack/fv_audit.py <образ>
```

Real-image гейты движка прогоняют те же инварианты
(`real_image_ops_insert_serial_s4`, `tests/real_image.rs`).

## 4. Протокол приёмки (после прошивки)

1. **(a) Дефолты**: COM1 (0x3F8) 115200 8N1 — boot-вывод и полный Setup на
   серийной консоли с первой загрузки (если первая глухая из-за тайминга
   нотификации — вторая живая).
2. **(b) Переезд порта**: Serial Port 1 Configuration → Change Settings =
   «2F8, IRQ3» → Save → ребут: консоль на COM2 (0x2F8); в ОС
   `dmesg | grep ttyS` подтверждает переезд (`ttyS1 at I/O 0x2f8`).
3. **(c) Baud**: смена Baud Rate → первые строки (баннер) читаются на
   старой скорости, далее «мусор» = DXE-консоль переключилась — смотрящий
   терминал догоняет.
4. Terminal Type (VT-UTF8/VT-100+/VT-100/ANSI) — сохраняется; цветного
   вывода как на платформах с полным AMI-стеком нет (косметика TerminalDxe).

## 5. Грабли

- **Порядок и состав модулей**: единственный SerialIo-продюсер в образе
  (вендорского нет — вставляем донора; если в целевом образе SerialDxe уже
  есть — сначала удалить `node remove`, иначе два продюсера дерутся).
- **TermSrc не вставлять** (токсичен в гибриде, E34).
- Ранний баннер всегда 115200 — ранняя печать до применения настроек.
- Если консоль глухая на обоих портах и Setup недоступен — CMOS-джампер
  (E31-класс клина); лечится, образ не портится.
- Комбинируется с добавлением своей страницы настроек —
  [setup-section-intelrc.md](setup-section-intelrc.md) (E43 собран поверх
  E32-базы: страница «UEFIPatcher Setup» с вопросами Serial Console /
  Verbose Boot).
