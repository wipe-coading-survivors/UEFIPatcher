# Отчёт: S1 — сборочная инфраструктура edk2 (SerialDxe + TerminalDxe + Glue)

> 2026-09-06, ветка `fix/cycle6-reimplent`. Ступень S1 umbrella-спеки
> `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` §3.
> Гейт S1: воспроизводимая сборка в контейнере; .ffs-артефакты
> закоммичены как тест-данные. Числа — факты Tasks 2–5; размеры и
> sha256 перемерены по коммиту `840fd25` (артефакты идентичны сборке
> Task 4 `/tmp/serial-s1-r1`); SerialConsoleGlue перемерен заново после
> гейтинга маркера 1 на статус ConOut-append (fix-волна финального
> ревью; двойная сборка `/tmp/serial-s1-final{1,2}` — REPRODUCIBLE).

## 1. Стенд

- `docker/edk2-builder.containerfile` — fedora:44 на базе
  `runtime-base.containerfile` + gcc/make/python3/libuuid-devel/git/
  **nasm** (nasm добавлен по факту: GCC X64 BaseLib собирает .nasm —
  rule-11 Task 2).
- `docker/edk2/build_serial.sh` — эпизодичная сборка: git-archive копия
  `refs/edk2`@`bcd1687` в /work (чекаут не трогается), BaseTools
  APPLICATIONS='GenFfs GenFv GenSec GenFw', `build -t GCC` — tools_def
  3.06 удалил тулчейн GCC5, план исходно закладывал GCC5, факт: GCC —
  RELEASE X64.
- `PACKAGES_PATH=<edk2>:/<docker/edk2>` — свой пакет
  `UefiPatcherSerialPkg` (DSC/FDF) вне дерева edk2; SerialDxe и
  TerminalDxe — стоковые MdeModulePkg.

## 2. PCD (факт; S0 §5.2)

Все 19 токенов — `[PcdsFixedAtBuild]` DSC, значения дословно из S0 §5.2;
gEfiPcdProtocolGuid в рантайме не читается (встречается только в DEPEX —
гейт диспетчеризации).

| токен | значение | носитель |
|---|---|---|
| PcdUartDefaultBaudRate | 115200 | SerialDxe (SerialIo-профиль порта) |
| PcdUartDefaultDataBits | 8 | SerialDxe |
| PcdUartDefaultParity | 1 (NoParity) | SerialDxe |
| PcdUartDefaultStopBits | 1 (OneStopBit) | SerialDxe |
| PcdUartDefaultReceiveFifoDepth | 1 | SerialDxe |
| PcdSerialRegisterBase | 0x3F8 | BaseSerialPortLib16550 → SerialDxe |
| PcdSerialUseMmio | FALSE | 〃 |
| PcdSerialBaudRate | 115200 | 〃 |
| PcdSerialLineControl | 0x03 (= 8N1) | 〃 |
| PcdSerialFifoControl | 0x07 | 〃 |
| PcdSerialClockRate | 1843200 | 〃 |
| PcdSerialRegisterStride | 1 | 〃 |
| PcdSerialRegisterAccessWidth | 8 | 〃 |
| PcdSerialUseHardwareFlowControl | FALSE | 〃 |
| PcdSerialDetectCable | FALSE | 〃 |
| PcdSerialExtendedTxFifoSize | 64 | 〃 |
| PcdSerialPciDeviceInfo | {0xFF} | 〃 |
| PcdErrorCodeSetVariable | 0x03058002 | TerminalDxe (REPORT_STATUS_CODE-путь при неудаче SetVariable), без правки |
| **PcdDefaultTerminalType** | **3 (VT-UTF8)** | TerminalDxe — единственная правка |

## 3. Артефакты

Закоммичены в `crates/uefi-engine/tests/data/serial/` (коммит `840fd25`;
SerialConsoleGlue.ffs обновлён fix-волной финального ревью — гейтинг
маркера 1; SerialDxe/TerminalDxe байт-идентичны `840fd25`):

| файл | FFS GUID | размер, Б | sha256 | UI |
|---|---|---|---|---|
| SerialDxe.ffs | 9A5163E7-5C29-453F-825C-837A46A81E15 | 32848 | `d16b78eeb2550fa48d593c2b9ac13ddc988cccdec006558665cee8934fc963c2` | SerialDxe |
| TerminalDxe.ffs | 9E863906-A40F-4875-977F-5B93FF237FC6 | 65596 | `640032425f9e491acc8874dbfdc6b9fae28f24d9ea82badc71b5bf27558e868f` | TerminalDxe |
| SerialConsoleGlue.ffs | 1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43 | 24688 | `fa086e47fbc36d2bf9f08ff64ceb0ea284b805481d80201f0173e0606ec79823` | SerialConsoleGlue |

DEPEX (по секциям 0x13 артефактов): SerialDxe — `gEfiPcdProtocolGuid`;
glue — `gEfiSerialIoProtocolGuid AND gEfiPcdProtocolGuid` (билдер слил
[Depex] INF с авто-depex DxePcdLib); TerminalDxe — без depex. Промежуточный
`SERIAL_CONSOLE_FV.Fv` стендовой сборки (123 256 Б) не коммитится —
содержит те же три файла.

## 4. Инварианты и воспроизводимость

- `hack/edk2_build_check.py`: type=0x07, machine=0x8664 (AMD64),
  subsystem=11, header-checksum=0 (обнулены File 0x11 и State 0x17,
  сохранённый 0x10 участвует в сумме — rule-11 Task 4), size24==len —
  3/3; UI-секции = именам файлов.
- Двойная сборка (каждая — свежий контейнер + свежий git-archive):
  все три .ffs `reproducible (identical)`, итог `REPRODUCIBLE`, exit 0.
  Ключ FvForceReproducible в edk2@bcd1687 не существует (rule-11
  Task 2) — воспроизводимость доказана двойной сборкой, не FV-флагом;
  GenFw обнуляет COFF TimeDateStamp.
- Движок: `crates/uefi-engine/tests/serial_ffs.rs` — хелперы
  ffs.rs/parser/file.rs согласованы с выходом GenFfs (гейт Task 5).

## 5. Glue (рулинг R-S1.1/R-S1.2)

- Реализован вариант (а): мини-DXE append `ConOut`/`ConIn`/`ErrOut` до
  BDS + ранний ConnectController + два COM-маркера; двойная консоль
  (VGA+COM) сохраняется.
- Маркеры (строки-контракт E30/S2, дословно):
  `SC-S1 glue: serial console attached (ConOut updated)` — сразу после
  ConnectController в DXE-диспетчеризации; гейтится на
  `ConOutStatus == EFI_SUCCESS` (статус append-а `ConOut` captured,
  ConIn/ErrOut остаются fire-and-forget) — маркер не напечатается, если
  SetVariable по `ConOut` фактически не прошёл (AMI VarCheck /
  attribute / store), фикс fix-волны финального ревью;
  `SC-S1 glue: ReadyToBoot` — в событии ReadyToBoot.
  Нет маркеров → диспетчеризация/DEPEX/UART-init; только первый →
  терминал отключили по ходу бута (кандидат CsmDxe, §8.5-1); оба без
  boot-текста → маршрутизация ConOut в AMI BDS — включаем (б).
- (б) `gST->ConOut` swap — отложен как контингенси E30 (одно правка-
  место в существующем ReadyToBoot-колбэке); (в) полный
  ConPlatform+ConSplitter — отклонён (два ConSplitter дерут gST->ConOut
  с AMI-сплиттером).
- R-S1.2: допущение порядка диспетчеризации (SerialDxe → TerminalDxe →
  Glue, обход FV по порядку + DEPEX glue) + обе защиты — если
  терминальный child не найден, append идёт по сконструированному пути
  (devpath SerialIo-контроллера + vendor-узел VT-UTF8), а ReadyToBoot
  перепроверяет child (к этому моменту BDS сделал ConnectAll). Допущение
  и защиты переадресованы мини-pre-check S2 (§8.5-2).

## 6. Входы S2

- Вставка: SerialDxe → TerminalDxe → Glue, append в хвост FV1 @0x890000
  (первый слот 0xB63B18, free_tail 2 082 028 Б, S0 §8.1).
- E30-атрибуция по маркерам — таблица решений §5 выше (R-S1.1).
- Суммарный объём вставки: 32 848 + 65 596 + 24 688 = 123 132 Б +
  одна 4-Б выравнивающая прокладка (FFS-файлы в FV 8-выровнены;
  65 596 ≡ 4 mod 8, остальные два размера и старт слота 0xB63B18
  кратны 8) = **123 136 Б ≈ 123,1 КБ**. Запас free_tail ≈ 16,9×.
  Кросс-чек: стендовая SERIAL_CONSOLE_FV.Fv = 123 256 Б = 123 136 +
  120 Б заголовка FV.
- Замечание Task 4 (вход планирования S2): FDF-правило формы
  `PE32 PE32 |.efi` кладёт PE-образ каждого модуля дважды (SerialDxe
  16 388 Б ×2, TerminalDxe 32 772 ×2, glue 12 292 ×2 — избыточно
  61 452 Б). Гейтам S1 безвредно (инварианты и blessed-размеры включают
  удвоение), но для S2 выбор: принять ~123,1 КБ (запас ~16,9×) или
  предварительно вычистить FDF-правило (~61,7 КБ, ~33,7×) — решение за
  планированием S2.
