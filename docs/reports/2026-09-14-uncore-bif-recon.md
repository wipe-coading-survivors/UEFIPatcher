# UncoreInitPeim бифуркации — разведка цикла A (розетта-первый)

Спека `docs/superpowers/specs/2026-09-14-bif-driver-recon-design.md`;
план `docs/superpowers/plans/2026-09-14-bif-recon.md`. Прошивки —
только владелец; таблица MMR — `2026-09-14-uncore-bif-mmr.json`.

## R0. Извлечение (2026-09-14)

| Билд | Путь в дереве | GUID | Размер | Сигнатура |
|---|---|---|---|---|
| натив (450x — копия) | `12/47` | D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 | FFS 799090; PE32 798880 | MZ |
| HNX99TF | `6/45` | D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 | FFS 606322; PE32 606112 | MZ |
| 超微450 | `10/43` (дубль `11/43`) | D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 | FFS 628434; PE32 628224 | MZ |

Извлечённые PE32-образы (PEIM-секция `*/…/1`, `--body-only`, без
заголовка секции; в git не попадают — `refs` есть symlink наружу):

| Файл | Байт | SHA-256 |
|---|---|---|
| `refs/extracted/bif/IIO-pei32-native.pei` | 798880 | `b2a5fdef7f2751a74781c8c41a6e3afbbd7cb2da6d4d44041200fab3e9e6d8eb` |
| `refs/extracted/bif/IIO-pei32-hnx.pei` | 606112 | `ae71b5cbd482bae83832f8458b3a61a872f5beb0ac389737a3aab578c45bcb0b` |
| `refs/extracted/bif/IIO-pei32-sm.pei` | 628224 | `c18369ece8823dbce95820091dae8bf5bd274d85ebada9ead77a81013ffecf52` |

Контроль-розетта: `refs/fw/IIO-pei32.bin` (798880, PE32, строки
«Invalid IOUx Bifurcation»/«busUncore»/«busIio», сага §5).

Находки R0 (уточняют спеку/план):

1. **GUID один и тот же во всех трёх билдах** — каноническая запись
   `D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3` (так показывает `node list`);
   побайтово в FFS-заголовке лежит `A48B1CD7-F24A-0D4D-B1BA-F2409F0C20D3`
   — тот самый префикс `A48B1CD7` из спеки (порядок байт первого поля LE).
2. **Розетта = натив.** `IIO-pei32-native.pei` побайтово совпадает с
   `refs/fw/IIO-pei32.bin` (SHA-256 `b2a5fdef…`, 798880). Розетта — не
   «четвёртый билд», а тот же нативный образ; калибровка стенда (Task 2)
   на розетте автоматически валидна для натива, дифф Task 5 фактически
   розетта↔HNX.
3. **Тройка размеров из саги — это размеры FFS-файлов**: 606322 = HNX,
   628434 = 超微450; 627410 среди трёх билдов не встречен (натив FFS =
   799090) — видимо, четвёртый/ранний билд. Маппинг закрыт.
4. **超微450 содержит две идентичные копии** PEIM (FV `10/43` и `11/43`,
   off 14933360 / 15981936, один GUID и размер); извлечено из `10/43/1`.
5. **Строковые якоря есть только в нативе/розетте** («Invalid IOUx
   Bifurcation», «busUncore», «busIio», ASCII); в HNX и 超微450 их нет
   ни в ASCII, ни в UTF-16 — дифф Task 5 по HNX/SM нельзя строить на
   строковых якорях, только структурный (xref по адресам/константам).
6. Сигнатуры всех трёх + розетты — `MZ` (PE32), расхождений нет: дифф
   Task 5 упрощён.

## R1. Калибровка дизасм-стенда

Стенд: `hack/disasm.sh` (podman-remote → `uefipatcher-edk2-builder:latest`
c `python3-capstone`, capstone 5.0.6) → `/hack/uncore_disasm.py`
(PE32/TE-парс, `info`/`strings`/`xref`/`dis`/`mmio`); пути контейнерные
(`/refs/…`). Калибровка на розетте `refs/fw/IIO-pei32.bin` (≡ натив, R0):

```
$ hack/disasm.sh info /refs/fw/IIO-pei32.bin
/refs/fw/IIO-pei32.bin: PE32 machine=I386 subsystem=11 entry=0xffe90ae8 base=0xffe64b08
  .text    va=0xffe64d28   vsize=0xbfa48 raw=0x220+0xbfa60
  .reloc   va=0xfff24788   vsize=0x3404 raw=0xbfc80+0x3420

$ hack/disasm.sh strings /refs/fw/IIO-pei32.bin 8 | grep -Ei 'IOU|bif|uncore|iio'
0xffe7c51d busUncore: 0x%02X 0x%02X 0x%02X 0x%02X
0xffe7c549 busIio: 0x%02X 0x%02X 0x%02X 0x%02X
0xffe90a7c ERROR: Invalid IOUx Bifurcation =%x

$ hack/disasm.sh xref /refs/fw/IIO-pei32.bin 'Invalid IOUx Bifurcation'
; target 'Invalid IOUx Bifurcation' va=0xffe90a83
hit  file=0x32fdf site~0xffe97ae7 width=4 ref=0xffe90a7c -7
```

Строки несут префиксы, код ссылается на начало полной строки («ERROR: »
— сдвиг -7; busUncore/busIio — префикс «\n», сдвиг -1: hit file=0x7439d
site~0xffed8ea5 и file=0x74371 site~0xffed8e79), поэтому `xref` сканит
окно `va-8..va`. База подтверждена 5947 HIGHLOW-фиксапами .reloc (все
значения 0xFFxxxxxx). Калибровка = локализация сразу в нативе.

Три PEIM'а Task 1 — все PE32 (magic 0x10B), machine=I386, subsystem=11,
секции .text/.reloc, SectionAlignment==FileAlignment==0x220:

| Билд | base | entry | .text va / vsize / raw | .reloc va / raw |
|---|---|---|---|---|
| натив | 0xffe64b08 | 0xffe90ae8 | 0xffe64d28 / 0xbfa48 / 0x220+0xbfa60 | 0xfff24788 / 0xbfc80+0x3420 |
| HNX99TF | 0xffdcff40 | 0xffde9510 | 0xffdd0160 / 0x92788 / 0x220+0x927a0 | 0xffe62900 / 0x929c0+0x15e0 |
| 超微450 | 0xfff3de10 | 0xfff57520 | 0xfff3e030 / 0x97ca8 / 0x220+0x97cc0 | 0xfffd5cf0 / 0x97ee0+0x1720 |

## R2. Purley-карта / дифф native↔HNX / таблица MMR

### R2a. Purley-референс (edk2-platforms) — карта чтения для дизасма Task 5

Клон: `refs/edk2-platforms`, shallow. Отклонение от брифа: **master
больше не содержит Purley** — платформа удалена 2026-03-03 (коммит
`6e821d5` «Platform/Intel/PurleyOpenBoardPkg: Remove Purley Platform»),
в т.ч. нет и `PurleySiliconBinPkg`; клон переключён на
tag `202603-before-platform-removals` (`09608a9`) — последнее состояние
до удалений. Все цитаты ниже — с этого тега (fallback с raw-URL не
понадобился).

Архитектурная граница открытого/закрытого подтверждена: открытый код
(`Platform/Intel/PurleyOpenBoardPkg` + `Silicon/Intel/PurleyRefreshSiliconPkg`)
готовит **политику** (`IIO_GLOBALS.SetupData`), программирование железа —
закрытые FV из `PurleySiliconBinPkg`, которого в дереве нет — он лишь
подключён DSC-ом: `refs/edk2-platforms/Platform/Intel/PurleyOpenBoardPkg/BoardMtOlympus/OpenBoardPkg.dsc:18`
(`DEFINE PLATFORM_SI_BIN_PACKAGE = PurleySiliconBinPkg`) и FV-инклюды
`OpenBoardPkg.dsc:148-150` (FvTempMemorySilicon / FvPreMemorySilicon /
FvPostMemorySilicon). Grantley-аналог нашего PEIM'а `D71C8BA4-…` живёт
именно в такой закрытой части.

**(a) Структура бифуркации.** Единица таблицы уровня платы:

```c
typedef struct {            // refs/edk2-platforms/Platform/Intel/PurleyOpenBoardPkg/Include/IioBifurcationSlotTable.h:20
  UINT8 Socket;
  UINT8 IouNumber;
  UINT8 Bifurcation;
} IIO_BIFURCATION_ENTRY;    // :24
```

Кодировка значений (`Bifurcation` / `ConfigIOUx`):

```c
#define IIO_BIFURCATE_AUTO      0xFF  // refs/edk2-platforms/Silicon/Intel/PurleyRefreshSiliconPkg/Library/BaseMemoryCoreLib/Chip/Skx/Include/Iio/IioRegs.h:89
// Ports 1D-1A, 2D-2A, 3D-3A                                    // :91
#define IIO_BIFURCATE_x4x4x4x4  0    // :93
#define IIO_BIFURCATE_x4x4xxx8  1    // :94
#define IIO_BIFURCATE_xxx8x4x4  2    // :95
#define IIO_BIFURCATE_xxx8xxx8  3    // :96
#define IIO_BIFURCATE_xxxxxx16  4    // :97
#define IIO_BIFURCATE_xxxxxxxx  0xF  // :98 — все порты IOU выключены
```

Хранилище в политике — per-IOU скаляры на сокет, setup-переменная
`L"SocketIioConfig"` (`Silicon/Intel/PurleyRefreshSiliconPkg/Include/Guid/SocketIioVariable.h:15`):

```c
UINT8 ConfigIOU0[MAX_SOCKET]; // 00-x4x4x4x4, …, 04-x16 (P5p6p7p8)  // SocketIioVariable.h:46
UINT8 ConfigIOU1[MAX_SOCKET]; // (P9p10p11p12)                     // :47
UINT8 ConfigIOU2[MAX_SOCKET]; // (P1p2p3p4)                        // :48
UINT8 ConfigMCP0[MAX_SOCKET]; // 04-x16 (p13)                      // :49
UINT8 ConfigMCP1[MAX_SOCKET]; // 04-x16 (p14)                      // :50
```

Таблица платы (MtOlympus; TiogaPass —
`BoardTiogaPass/Library/BoardInitLib/IioBifur.c:35-47`, тот же формат):

```c
IIO_BIFURCATION_ENTRY   mIioBifurcationTable[] =   // refs/edk2-platforms/Platform/Intel/PurleyOpenBoardPkg/BoardMtOlympus/Library/BoardInitLib/IioBifur.c:34
{
  { Iio_Socket0, Iio_Iou0, IIO_BIFURCATE_xxxxxx16 },  //Slot3: skt0/Iou0 Port1A x16     // :36
  { Iio_Socket0, Iio_Iou1, IIO_BIFURCATE_xxxxxx16 },  //PCH uplink x16                 // :37
  { Iio_Socket0, Iio_Iou2, IIO_BIFURCATE_x4x4x4x4 },  //Slot1/Slot2 (x8 slots)         // :38
  { Iio_Socket0, Iio_Mcp0, IIO_BIFURCATE_xxxxxx16 },  //MCP x16                        // :39
  { Iio_Socket0, Iio_Mcp1, IIO_BIFURCATE_xxxxxx16 },  //MCP x16                        // :40
  { Iio_Socket1, Iio_Iou0, IIO_BIFURCATE_xxx8xxx8 },  //Slot4 x16 Port1A/1B, 1C/1D     // :41
  { Iio_Socket1, Iio_Iou1, IIO_BIFURCATE_xxx8x4x4 },  //OCulink x8 + M.2 x4x4          // :42
  { Iio_Socket1, Iio_Iou2, IIO_BIFURCATE_xxxxxx16 },  //Slot5 x16                      // :43
  { Iio_Socket1, Iio_Mcp0, IIO_BIFURCATE_xxxxxx16 },  //MCP                             // :44
  { Iio_Socket1, Iio_Mcp1, IIO_BIFURCATE_xxxxxx16 },  //MCP                             // :45
};                                                                                      // :46
```

Лейн-мап/индексация портов: `IioRegs.h:100-137` — `PORT_1A_INDEX=1 …
PORT_5D_INDEX=20`, `SOCKET_x_INDEX` кратен 21 (порт 0 = DMI), per-port
PCI dev/func = 0x00/0x01/0x02/0x03 внутри IOU (`IioRegs.h:139-149`).

**(b) Порядок инициализации IIO по открытым файлам.**

PEI pre-mem, сторона платы (`BoardMtOlympus/Library/BoardInitLib/PeiMtOlympusInitPreMemLib.c`):

1. загрузка setup-дефолтов из PCD, в т.ч. `PcdSocketIioConfigData` →
   `SOCKET_IIO_CONFIGURATION` (`:455`);
2. **GPIO раньше бифуркации** — `PlatformInitGpios()` (`:478`), детект
   райзеров/слотов;
3. публикация борд-таблиц через PCD — `IioPortBifurcationConfig()`
   (`:382-397`, вызов `:482`): после GPIO, до `EarlyPlatformPchInit`
   (`:488`);
4. (закрытый FvPreMemorySilicon) силиконный uncore-PEIM зовёт колбэк
   `SystemIioPortBifurcationInit` из SystemBoardPpi и сам программирует
   железо.

Колбэк политики (`Policy/SystemBoard/SystemBoardPei.c:111-141`),
декларированный порядок в комментарии `:123-126` (defaults → overrides →
hide), фактический:

1. `SystemIioPortBifurcationInitCommon` (`:128`) — обнуление
   `PEXPHIDE`/`HidePEXPMenu` всех портов (`:83-86`), извлечение
   PCD-таблиц (`:94-97`) — **порт-енаблы (сброс)**;
2. `SetBifurcations` (`:130` → `SystemBoardCommon.c:17-62`) — **бифуркация**:
   борд-значение пишется в `ConfigIOUx[Socket]` ТОЛЬКО если setup-значение
   == `IIO_BIFURCATE_AUTO`, т.е. приоритет **setup > борд-таблица**;
3. `ConfigSlots` (`:131` → `SystemBoardCommon.c:107`) — слот-капабилитис,
   `PEXPHIDE`/`HidePEXPMenu` из таблицы слотов (per-port enables);
4. `OverrideConfigSlots` (`:132` → `SystemBoardCommon.c:246`) —
   динамические оверрайды (GPIO/райзеры);
5. `SystemHideIioPortsCommon` (`:136-140` → `SystemBoardCommon.c:613-625`)
   — **lane-мап → hide**: для каждого `SocketPresent[]`
   (`IioPlatformData.h:199`) по IOU0→IOU1→IOU2→MCP0→MCP1 (`:619-623`)
   `CalculatePEXPHideFromIouBif` (`:341-469`) выводит `PEXPHIDE` портов
   A-D из значения бифуркации (полный switch по 6 значениям, `:380-441`);
   `IIO_BIFURCATE_xxxxxxxx` нормализуется в `x4x4x4x4` с комментарием —
   единственное упоминание имени целевого регистра в открытом коде:
   «Bifurcation_Control[2:0] in IOU Bifurcation Control (PCIE_IOU_BIF_CTRL)
   register should be 000b ~ 100b» (`:443-447`).

Результат — `IIO_GLOBALS` (`Chip/Skx/Include/Iio/IioPlatformData.h:291-294`,
`IIO_CONFIG SetupData` + `IIO_VAR IioVar`; лейн-мапы портов
`CurrentPXPMap/MaxPXPMap/LinkedPXPMap/…` и флаг `resetRequired` — в
`IIO_OUT_DATA`, `:266-283`). Дальше — закрытая часть: программирование
PCIE_IOU_BIF_CTRL/MMR, тренинг линков, ресет по `resetRequired`; в DXE
`IioUdsDataDxe.c:46-62` переносит `IIO_UDS` из GUID-HOB в протокол
`gEfiIioUdsProtocolGuid`.

**(c) Вердикт: что переносится на Grantley (E5 v3), что нет.**
Purley (Skylake-SP) — другое поколение: это карта для чтения дизасма,
НЕ блюпринт.

Переносится (концепты-гипотезы для проверки в Task 5):

- скалярная per-IOU кодировка бифуркации малым енумом (0..4 + 0xFF=AUTO);
  строка-якорь натива «Invalid IOUx Bifurcation =%x» (R1) указывает, что в
  Grantley-PEIM'е есть та же валидация значения — искать в дизасме
  сравнение диапазона и обработчик невалидного кода;
- приоритет setup > борд-таблица (семантика AUTO=«не задано»);
- порты, «съеденные» бифуркацией, прячутся (`PEXPHIDE` выводится из
  `ConfigIOUx`), а не выключаются независимо;
- паттерн «политика на плате / программирование в силикон-коде»: наш
  UncoreInitPeim — закрытая Grantley-часть; ожидаемая структура входа —
  setup-подобная конфигурация + борд-таблица;
- IOU-группировка и port-index arithmetic (база = сокет × портов/сокет).

НЕ переносится:

- **регистровые АДРЕСА** — Purley/Skylake-SP MMR и `PCIE_IOU_BIF_CTRL`
  адреса не имеют отношения к Grantley/Haswell-EP uncore; даже имя
  регистра взято из комментария, адреса в открытом коде Purley вообще
  отсутствуют. Адреса даст ТОЛЬКО дизасм Task 5;
- состав/нумерация IOU и MCP (Purley: IOU0/1/2 + MCP0/1, 21 порт/сокет;
  у Grantley другой расклад — проверять по дизасму, не переносить);
- bus-раскладка, DMI/uplink-топология, «порт 0 = DMI»;
- VMD/NTB/VPP/райзер-механика — фичи поколения Purley.

### R2b. Дифф native↔HNX / таблица MMR
(заполняет Task 5)

## B1. Пробник и сборка
(заполняет Task 6)

## B2. EPA-1 (read-only) — вердикт владельца
(заполняет Task 8)

## B3. EPA-2 (write-ladder) — вердикт владельца
(заполняет Task 9)

## G. Гейт выбора варианта
(заполняет Task 10)
