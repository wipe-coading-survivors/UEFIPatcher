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

**Локализация (Task 5).** Валидатор найден по строке-якорю: функция
`0xffe97ad7` (файл `0x32fcf`) — `cmp bl, 0xff` → «ERROR: Invalid IOUx
Bifurcation =%x», далее switch по IOU 0/1/2 × енум 1..4 → флаги hide
per-port (аналог Purley `CalculatePEXPHideFromIouBif`). Её вызывающий —
борд-инит `0xffe96cdc`: per-IOU байты полиси в конфиге по смещениям
`0x248/0x24c/0x250` (+`0x249/0x24d/0x251` для сокета 1; колоночный
`ConfigIOUx[MAX_SOCKET]` — как в Purley `SocketIioVariable.h`), значение
«AUTO» = 0xFF подменяется борд-константой. Отладочные строки натива
расшифровывают маппинг: `0xffe8ff14 «IIO=%d, IOU0=%d.»` печатает
`[cfg+0x248]`, `0xffe8ff28 «IOU1»` — `[0x24c]`, `0xffe8ff00 «IOU2»` —
`[0x250]`. Применение полиси — функция `0xffe90e48` (файл `0x2c340`,
вызов из `0xffe91b41` в фазе `0xffea7f3f`): RMW16 по трём регистрам
`BDF+0x190` расширенного конфиг-пространства ECAM.

**Енум подтверждён кодом** (не только Purley-картой): статические
lane-map таблицы натива — x8-под-таблица 2×2 на `0xffe755d8` (файл
`0x10ad0`): запись 0 = `{04,04}`, 1 = `{08,00}`; x16-таблица 5×4 на
`0xffe755dc` (файл `0x10ad4`): запись 0 = `{04,04,04,04}` (x4x4x4x4),
4 = `{16,00,00,00}` (x16). HNX идентичен: x8 на `0xffde0ae8` (файл
`0x10ba8`), x16 на `0xffde0aec` (файл `0x10bac`). Регистр: bits[2:0] =
енум, bit3 — код выставляет всегда (`or al, 8`).

**Порт-таблица** (статическая, `0xffe70e68` натив / `0xffddc258` HNX,
байт-в-байт совпадают): 11 записей × 8 байт (dev, fun, …): `[0]`=0/0,
`[1]`=1/0 (**IOU2**), `[3]`=**2/0 (Port 2A = IOU0)**, `[7]`=3/0 (**IOU1**);
funcs 1-3 внутри dev 2/3 = порты 2B/2C/2D, 3B/3C/3D. RMW-троица в
порядке кода: IOU2 → IOU0 → IOU1; RMW#2 (IOU0) пропускается только при
CPUID 0x5066 = Broadwell-**DE** (проверка `[cfg+0xcf2]` = CPUID>>4,
`0xffe9100d`); E5-2600 v4 = 0x406F → выполняется.

**Доступ = MMIO, не CF8**: смещение 0x190 ≥ 0x100. Адрес =
`[cfg+0xd78]` (натив; HNX — `[cfg+0xed8]`, прибавляется inline) +
`(bus<<20)|(dev<<15)|(fun<<12)|0x190`; база ECAM читается в рантайме из
PCI `0:5:0 + 0x90` & `0xFC000000` (`0xffe9ff62-0xffe9ff94`), ожидание
0xE0000000 (Grantley-дефолт). Сторы воркеров: натив — воркер `0xffe9649b`,
стор `0xffe964cf mov word ptr [esi], ax`; HNX — воркер `0xffdeeaef`, стор
`0xffdeeb04 mov word ptr [eax], cx`. Допущения анализа (EPA-1 проверит
телеметрией): ECAM-база = 0xE0000000 и **bus IIO сокета 0 = 0** (обе
величины код читает в рантайме: база из PCI, bus — из per-socket поля
`[cfg+0xd1c]`, статически не разрешимы).

| # | назначение | access | native (r450x231) | hnx (azteccity043) | sequence |
|---|---|---|---|---|---|
| 1 | IOU2/Port1 Bifurcation, `BDF(bus,1,0)+0x190` → **0xE0008190** | mmio32 RMW (код: RMW16) | полиси 1 (x4x4): `or 9` | тот же RMW, полиси из `[cfg+0x250]` | контекст (не в JSON) |
| 2 | **IOU0/Port2 Bifurcation, `BDF(bus,2,0)+0x190` → 0xE0010190** | mmio32 RMW (код: RMW16) | полиси 4 (x16): `or 0xC`, файл `0x2c535-0x2c57a` | тот же RMW (файл `0x1990e-0x19956`), known-good x4x4x4x4: полиси 0 → `or 8` | 1 |
| 3 | IOU1/Port3 Bifurcation, `BDF(bus,3,0)+0x190` → 0xE0018190 | mmio32 RMW (код: RMW16) | полиси 4 (x16): `or 0xC` | тот же RMW, полиси из `[cfg+0x24c]` | контекст (не в JSON) |

RMW-паттерн (HNX, VA→файл): read16 `0xffde984e`; `and cx, 0xFFF8`
`0xffde985d`; `mov al, [esp+0x1e]` (=`[cfg+sock+0x248]`) `0xffde9865`;
`or al, 8` `0xffde9869`; `or cx, ax` `0xffde986f`; addr = dev
`[cfg+0xf1b]`(=2)/fun `[cfg+0xf1c]`(=0) + 0x190 + ECAM `[cfg+0xed8]`
`0xffde988a`; write16 `call 0xffdeeaef` `0xffde9896`. Для x4x4x4x4:
`(val & 0xFFF8) | (0|8)` → в 32-битной записи `and_mask 0xFFFFFFF8`,
`or_value 0x00000008` (старшая половина регистра сохраняется).

**Дифф-вердикт.** Натив (RD450x `r450x231`, строки
`d:\aliang\rd450\t1\r450x231\GrantleySocketPkg\Library\IioEarlyInitialize\*.c`)
захардкодил политику **x16 (значение 4) для IOU0/Port 2** в борд-таблице:
шаблон конфига сеет 0xFF (AUTO) в `[cfg+0x248]` (файл `0xC600`), борд-инит
подменяет AUTO→константу. Полный список записей IOU0-сокета-0 (`+0x248`) —
**12 инструкций** (вход варианта 2; первая в коде — **файл-оффсет
`0x3228A`**, VA `0xffe96d92`, `mov byte ptr [edi + 0x248], 4`; «первая в
коде» ≠ гарантированно исполняемая — какая борд-ветка жива на машине
владельца статически неразрешима, switch по runtime board-id
`[cfg+0xd13]`; патч варианта 2 должен покрывать все 12 или доказать
достижимость ветки):

| # | файл | VA | значение |
|---|---|---|---|
| 1 | 0x3228A | 0xffe96d92 | 4 |
| 2 | 0x32317 | 0xffe96e1f | 4 |
| 3 | 0x32492 | 0xffe96f9a | 3 |
| 4 | 0x32633 | 0xffe9713b | 3 |
| 5 | 0x32665 | 0xffe9716d | 2 (райзер) |
| 6 | 0x326DA | 0xffe971e2 | 4 (райзер) |
| 7 | 0x328E8 | 0xffe973f0 | 4 |
| 8 | 0x32AAE | 0xffe975b6 | 3 |
| 9 | 0x32C2C | 0xffe97734 | 4 (GPIO-флаг) |
| 10 | 0x32C2C+0xE | 0xffe97742 | 1 (GPIO-флаг, альтернатива #9) |
| 11 | 0x32D51 | 0xffe97859 | 4 |
| 12 | 0x32DA4 | 0xffe978ac | 4 |

**Ручка IntelSetup+0x531 (IOU0) этим PEIM не читается вовсе** — ни одной
записи переменной/HOB в поля 0x248-0x251 нет. Для **IOU0/+0x248**
(исчерпывающе, все 12 выше) значения — только imm {1,2,3,4}, нуля (x4x4x4x4)
нет. В семействе полей 0x248-0x251 регистровые (вычислимые) записи есть —
ни одна не из setup: `0xffe9702d [0x251]=dl` (dl=1, маркер сокета),
`0xffe97074 [0x249]=al` (al∈{3,4} из флага), `0xffe97848 [0x250]=al`
(al∈{0,1} из MMIO-страпа: read32 `[cfg+0xd20]<<20|0xf308c`, `and 0x7ff8`,
`neg/sbb/inc`). «Сокет 0 не получает 0» верно только для IOU0/+0x248:
IOU1-сокета-0 получает 0 на `0xffe97297` (`mov [esi+0x24c], 0`), IOU2
сокета 0 может получить 0 на `0xffe97848` (страп), `0xffe972ea` пишет 0 в
`+0x251` = IOU2 **сокета 1**, `0xffe973c7` — 0 в `+0x249` = IOU0 сокета 1.
Полный дифф-вердикт: полиси локализована, вход варианта 2 валиден (патч
imm→0 по списку 12 site-ов либо доказательство живой ветки).

**HNX-соответствие и поправка.** HNX (`azteccity043`) структурно
идентичен: тот же RMW-тройник, та же порт-таблица, тот же енум, те же
смещения полей 0x248-0x251, шаблон тоже сеет 0xFF, борд-константы HNX
для сокета-0 IOU0 ∈ {1,3,4} — т.е. живой x4x4x4x4 на HNX-машине
сеется вне этого PEIM (платформенный код HNX прокидывает setup в полиси
до/вместо борд-таблицы — в нативной сборке этого слоя нет). Для replay
это не влияет: регистр, RMW-паттерн и known-good значение x4x4x4x4
(`полиси 0 | bit3` = 0x8) извлечены из HNX-кода напрямую.

**Fallback-статус по R-A.4:** не требуется — полиси локализована
(дивергирующие ветки найдены, вход варианта 2 зафиксирован — 12 site-ов
выше). Таблица MMR
(`2026-09-14-uncore-bif-mmr.json`, `gen_mmr_table.py --check` →
`ok: 1 entries`): одна replay-запись `IOU0-Port2-bifurcation`
(0xE0010190, RMW 0xFFFFFFF8/0x00000008); IOU1/IOU2 сознательно НЕ
включены (их полиси борд-зависимы — replay с угаданным значением менял
бы чужие порты; адреса зафиксированы в notes и отчёте для EPA-1).
Lock-регистров отдельными записями нет; bit3 (0x8) — всегда
выставляемый кодом бит (кандидат enable/commit) — только в notes,
пробник его не снимает.

**UART-база для Task 8 (допущение):** ожидание COM1 `0x3F8` (из
R0-заголовков/ServerSetup-дефолтов Grantley); статически в этом PEIM не
подтверждается — отметить как допущение, EPA-1 проверит телеметрией
(читающий пробник до write-фазы на MMIO-адресах вне таблицы). Итоговый
набор допущений R2b, ожидающих проверки EPA-1: ECAM-база = 0xE0000000,
bus IIO сокета 0 = 0, UART COM1 = 0x3F8.

## B1. Пробник и сборка

Пакет `docker/edk2/UefiPatcherBifPkg` (DSC/FDF/INF/C) + двухрежимный
сборщик `docker/edk2/build_bif_epa.sh` (хост: печёт `EpaConfig.h` из
MMR-JSON через `hack/gen_mmr_table.py`, затем контейнер
`uefipatcher-edk2-builder:latest` собирает edk2 RELEASE/X64/GCC) +
валидатор `hack/edk2_bif_check.py` (GUID/тип/checksum/PE32-инварианты,
двухсборочное сравнение с маскированием COFF TimeDateStamp).

Read-режим (EPA_MODE_WRITE=0), таблица R2b (1 запись), fix-round 1
(коммит d207e30 план; EpaSetStage → EFI_STATUS, статус-гейт маркера
WRITTEN перед ресетом — провал SetVariable = NO reset, защита от
ресет-лупа при полном NVRAM-сторе; IDLE-пути остаются без гейта —
безопасно: стадия остаётся WRITTEN, пост-ресет отчёт повторится без
записи):

- артефакт `/tmp/bif/epa-build-1/BifEpaProbe.ffs` (Task 7 копирует в
  репозиторий): sha256
  `1e4e2c5b4c2781bafa09fe59315931c6a0fa3f140af0cf9db0d6aef5cd3a3710`,
  32868 байт, FILE_GUID `B7E4A2C1-58D6-4E3F-B9A2-7C1D0E6F5A84`.
  После статус-гейта sha256/размер НЕ изменились: в read-режиме вся
  write-ветка (лестница, маркер, ресет, новый гейт) — мёртвый код при
  константе `EPA_MODE_WRITE==0`, RELEASE вы dead-strips её (в PE32
  только 4 read-строки телеметрии). Гейт живёт в write-режиме:
  контрольная write-сборка (только как evidence, не артефакт EPA-2)
  содержит строку гейта, sha256
  `974a570da90b5f543cc75283f618e54025b73fab3d79006162f2eb5ea354459f`;
- `EPA_ENTRY_COUNT 1` (IOU0-Port2-bifurcation, 0xE0010190,
  and 0xFFFFFFF8, or 0x00000008);
- воспроизводимость: две независимые сборки `/tmp/bif/epa-build-1`,
  `/tmp/bif/epa-build-2` — побайтово идентичны (совпадающий sha256),
  `python3 hack/edk2_bif_check.py /tmp/bif/epa-build-1 /tmp/bif/epa-build-2`
  → `ok: 1 artifact(s), reproducible`.

**B1-фикс (2026-09-15, после вердикта EPA-1 — зависание DXE).**
Форензика зависания указала на **задвоенную PE32-секцию** в нашем FFS
(два одинаковых PE32 по 16388: тела `0x40`/`0x4044`) — так строит
GenFds нашей версии по FDF-правилу формы `PE32 PE32 |.efi` (тот же
дефект во ВСЕх артефактах серийного цикла: SerialDxe/TerminalDxe/
SerialConsoleGlue — прошивкой не проверялись). Ни один файл родного
DXE-FV так не выглядит. Фикс — каноничная форма правила из OvmfPkg
(`PE32 PE32 $(INF_OUTPUT)/$(MODULE_NAME).efi`): ровно одна PE32,
файл **16480** байт (было 32868), sha256
`fb40777df76d0c9cdc8d6d69d65d61559d68618f2450e8ca1716c67c033eee98`;
две независимые сборки воспроизводимы, валидатор и `bif_ffs` зелёные,
тест-данные репозитория обновлены. Причинность «двойной PE32 →
зависание» проверяется следующей прошивкой (epa1b, §B2).

Отклонения от брифа Task 6 (5 дефектов брифа, исправлены минимально;
подробности — `.superpowers/sdd/2026-09-14-bif-recon/task-6-report.md`):
контейнерная ветка `build_bif_epa.sh` читала `${1:?}` после
arg-парсинг-цикла (shift съел позиционный аргумент); DSC ссылался на
несуществующий `DxeMemoryAllocationLib`; DSC не хватало `PciLib`/
`PciCf8Lib` (требуются `BaseSerialPortLib16550`→`BasePciLibCf8`); всё
семейство `PcdSerial*` в DSC было под `gEfiMdePkgTokenSpaceGuid`
(правильно `gEfiMdeModulePkgTokenSpaceGuid`, как в рабочем
`UefiPatcherSerial.dsc`), и `SerialPortWrite` вызывался с
`CONST UINT8*` (прототип — `UINT8*`, -Werror).

## B2. EPA-1 (read-only) — вердикт владельца

### Артефакт

| параметр | значение |
|---|---|
| образ EPA-1 | `refs/amibcp/450x-kopiya-bif-epa1.bin` (refs вне git; файл у владельца) |
| база | `refs/amibcp/450x — копия.bin`, sha256 `76ec3d3a4744da39c2b9c5ac23031fe2f9edad61518e6b76de9669e765cecc5f`, 16777216 байт |
| sha256 образа | `dc5efa42fcb48619ae97e8a1a0ccb98ed399f54e5c87a66a8608876bf6404b68` |
| размер образа | 16777216 байт (== база, 16 МиБ) |
| вставка | DXE-FV (том `8`), хвостовой файл `8/242` — последний `File(DXE driver)` тома, GUID `9A4713C2-EF56-4FC4-AB91-270825BFD142`, `--mode after` |
| вставленный артефакт | `BifEpaProbe.ffs`, sha256 `1e4e2c5b4c2781bafa09fe59315931c6a0fa3f140af0cf9db0d6aef5cd3a3710`, 32868 байт; воспроизведён побайтово третьей независимой сборкой (Task 8 Step 1: `build_bif_epa.sh /tmp/bif/epa1 --epa-mode=read`, валидатор + `bif_ffs` зелёные, `crates/uefi-engine/tests/data/bif/BifEpaProbe.ffs` не изменился) |
| конфиг из `EpaConfig.h` | режим read (`EPA_MODE_WRITE 0`), `EPA_ENTRY_COUNT 1` (`IOU0-Port2-bifurcation`, MMIO32 `0xE0010190`, RMW `0xFFFFFFF8`/`0x00000008`), reset=warm, pause 3000 мс |

Дифф-контроль (cmp база↔образ): единственный непрерывный спан
32868 байт @ смещение 0xBDA368 (0-based 12428136 — выровненный конец
файла `8/242` внутри DXE-FV); изменено 31936 байт, остальные 932 байта
спана — 434 коротких (1–4 байта) разбросанных участка 0xFF внутри
PE32-области артефакта (диапазон 4174–24791 файла артефакта), в которых
артефакт совпал с 0xFF-филлером свободного места FV (обе стороны 0xFF);
сам артефакт заканчивается UI-секцией (последние байты
…`B\x00i\x00f\x00E\x00P\x00r\x00o\x00b\x00e\x00\x00` — UTF-16
«BifEpaProbe») и по спану от базы отличается; вне спана образ побайтово
равен базе. Единственное отличие
вставленной области от артефакта — байт `State` FFS-заголовка
(0x07 → 0xF8): билдер нормализовал состояние файла под erase-polarity=1
тома (FV-атрибуты 0x0004FEFF, бит 0x800) — без этого DXE Core считал бы
файл невалидным и не диспетчеризовал бы драйвер. engine.log:
`artifact inserted target=8/242 mode=After source=/tmp/bif/epa1/BifEpaProbe.ffs size=32868`,
`image built size=16777216`; парсер после вставки: files 311 → 312.

### Инструкция владельцу

> 1. Прошить **только** `refs/amibcp/450x-kopiya-bif-epa1.bin` (sha256
>    `dc5efa42…04b68`) через TMM, как обычно. Никакие другие образы
>    этого цикла не прошивать.
> 2. При загрузке машины смотреть SOL (serial-over-LAN консоль). Ожидать
>    в DXE-фазе строки:
>    - `BIF-EPA: probe mode=read entries=1 reset=warm`
>    - первый бут печатает ОБЕ строки подряд: `BIF-EPA: prev stage: none (…)`
>      (NVRAM-переменной ещё нет) и сразу за ней `BIF-EPA: prev stage=0` —
>      это ожидаемо, «лишней» строки нет; на последующих read-бутах — только
>      `BIF-EPA: prev stage=0`
>    - `BIF-EPA: read [0] IOU0-Port2-bifurcation raw=0x<8 hex-цифр>` —
>      записать значение дословно.
> 3. Телеметрия значений — **только UART**: NVRAM-переменная `BifEpaStage`
>    хранит лишь маркер стадии (для one-shot логики), сами raw-значения в
>    NVRAM не попадают. Если строки «пролетели» — перезагрузиться и
>    подсмотреть ещё раз (read-режим безопасен: ничего не пишет, не
>    ресетит, бут не блокирует).
> 4. Интерпретация raw: младшие 3 бита — код полиси бифуркации IOU0/Port2
>    (по R2a/R2b: 0=x4x4x4x4, 1=x4x4xxx8, 2=xxx8x4x4, 3=xxx8xxx8,
>    4=xxxxxx16, 0xF=порты выключены; known-good x4x4x4x4 = `0x8`
>    = «полиси 0 | bit3»). Значение `raw=0xFFFFFFFF` или `raw=0x00000000`
>    — сигнал о проблеме адресного допущения (п. 5), записать и
>    сообщить как есть.
> 5. EPA-1 проверяет три допущения R2b: ECAM-база `0xE0000000`, bus IIO
>    сокета 0 = 0, UART COM1 = `0x3F8`. `0xFFFFFFFF`/`0x00000000` в raw —
>    проблема допущения ECAM/bus (устройство не декодирует адрес);
>    живой бут без `BIF-EPA:`-строк — проблема допущения UART (0x3F8).
> 6. **Молчание UART = STOP-условие**: без телеметрии EPA-2 не прошивать;
>    возврат к Task 5 за уточнением UART-базы (правило-11).
> 7. Вернуть: наличие/отсутствие каждой строки + все raw-значения
>    (фото или текст SOL-лога достаточно).

### Вердикт

**Промежуточный (2026-09-15, sol.log 833 строки — прошивка выполнена, разбор продолжается).**

- Прошивка: PASS (TMM, `450x-kopiya-bif-epa1.bin` sha256 `dc5efa42…`).
- Бут: два прохода MRC (ретрейн + warm reset — норма после прошивки),
  PEI завершён (`PeimMemoryQpiInit END`), DXE-фаза достигнута.
- Телеметрия пробника: **в захваченном окне молчание** — ни баннера
  `BIF-EPA:`, ни `prev stage`, ни raw-строки. Захват обрывается на
  самой ранней DXE-строке (`ERROR: Class:3000000; Subclass:50000;
  Operation: E`); никакого другого DXE-вывода (в т.ч. Setup-зеркала)
  в захвате нет — окно кончилось до BDS.
- Подтверждение допущения R2b из лога: RC QPI-вывод `busIio: 0x00` —
  bus IIO сокета 0 = 0 (ECAM-адрес `0xE0010190` валиден по шине).
  `Legacy Serial Debug Enabled` — PEI-дебаг идёт через legacy-UART
  (тот же класс порта 0x3F8, куда пишет пробник).
- Контроль вставки по образу (после прошивки): файл
  `0xBDA368..0xBE23CB`, далее 2 МБ чистого 0xFF до конца DXE-FV,
  чексумма заголовка валидна — структурной порчи FV нет.
- Архитектурный контроль: нативный DxeCore (`D6A2CB7F-…`, тело
  127776 Б) и DXE-драйверы — PE machine `0x8664` (X64); наш
  BifEpaProbe тоже X64 — гипотеза «IA32-DXE не грузит PE64» снята.
- Открытый вопрос (блокирует решение STOP): судьба бута после
  ERROR-строки. Ветки: (а) захват оборван раньше диспетчера пробника
  → нужен полный повторный захват с power-on до ОС; (б) зависание в
  раннем DXE (в т.ч. возможный вклад вставки — при всём порядке
  структуры) → TMM-recovery на базу и разбор; (в) пробник исполнился,
  но вывод не доходит до SOL в DXE-фазе (mux/маршрут) → STOP по
  UART-допущению, возврат к R2/Task 5. EPA-2 не собирается до
  разрешения.

ждёт владельца: полный лог бута (power-on → ОС) и состояние машины
после ERROR-строки

**Продолжение (2026-09-15, ответ владельца: машина воспроизводимо
висит на ERROR-строке; на обычной загрузке после неё идут ещё пара
строк и POST).** Диагноз: бут останавливается в самом раннем DXE,
до какого-либо вывода; ERROR-строка — нормальная строка обычного
бута. Дифференциальный разбор офлайн:

1. Контент-правки DXE-FV на натив-семействе бутятся (ds1 = AMIBCP-мод
   этой же семьи, years in service; v6) → гипотеза «хэш/верификация
   всего FV» снята. Новый элемент — именно **новый файл в цепочке
   FFS**.
2. Побайтовый аудит вставленного FFS вскрыл аномалию, отсутствующую
   у всех 242 родных файлов: **две идентичные PE32-секции** (дефект
   FDF-правила `|.efi` на нашей версии GenFds; см. B1-фикс). DXE-ядро
   (AMI) спотыкается на файле до всякого вывода — механизм совместим
   с наблюдением (висим до баннера пробника).
3. X64-гипотеза снята (DxeCore и драйверы натива — 0x8664).

**План следующей прошивки — epa1b** (одна переменная: чиненый FFS,
тот же спот, тот же read-режим): владелец сначала восстанавливает
базу (TMM, `450x — копия.bin`), затем шьёт
`refs/amibcp/450x-kopiya-bif-epa1b.bin`, sha256
`4541796661a2908e27cea7740e16c1d1143ec59e4919e7f8b80730c6178e9e67`
(вставка 16480 байт @ 0xBDA368 после 8/242, read-режим, маркеры те
же). Исходы: бут до ОС + `BIF-EPA:`-телеметрия → причинность
подтверждена, продолжаем на EPA-2; бут до ОС без телеметрии →
вставка нейтральна, проблема UART-маршрута (STOP → R2); снова
зависание → дело не в секциях, а в самом факте нового файла →
гейт G с вариантом 3/4 и разбором FV-защит.

**Вердикт EPA-1b (2026-09-15, sol-epa1b.log).** База восстановлена и
бутится (recovery ✓, машина здорова). epa1b — **висит там же
байт-в-байт**: «ERROR: Class:3000000; Subclass:50000; Operation: E»
после `PeimMemoryQpiInit END`, два зависания (бут + ресет владельца),
`BIF-EPA:`-строк ноль; владелец погасил машину через BMC
(`ipmitool power off`). Вывод: **двойной PE32 не был причиной**
(фикс ничего не изменил) — контент-гипотеза dxe-загрузки ослаблена
до нуля, остаётся «новый файл в цепочке FV».

Оффлайн-форензика (без прошивок):

- опрос родного FV8 (243 файла): attributes 0x00 у 241 (как у
  нас), первая секция DXE_DEPEX у 120, GUID-defined/сжатые у 47 —
  форма нашего файла ТИПИЧНА, аномалий формы не осталось;
- depex идентифицирован и корректен: PUSH gEfiVariableWriteArchProtocolGuid
  AND PUSH gEfiPcdProtocolGuid (MdePkg.dec:1200/1339; вторая — от
  DxePcdLib, норма для DXE-драйверов), END, без мусора;
- контент-правки DXE-FV на этой семье бутятся (ds1/v6 — правки
  внутри тел файлов), т.е. whole-FV хэш-гипотеза мертва: если
  реагирует — то именно на структуру цепочки (новый заголовок файла
  в «свободном» пространстве).

**Эксперимент PAD-проба (следующая прошивка, одна переменная —
сам факт нового файла).** `refs/amibcp/450x-kopiya-bif-padtest.bin`,
sha256 `f58b790d2cf8f306b625fb6a92cc33622e70373d8a0fcd868fc8592d9ea8e4e6`:
база + PAD-файл по образцу родных падов (GUID all-FF, type 0xF0,
attr 0x00, state 0xF8, чексумма по PI-правилу; в FV8 родные пады
есть — 2 шт.). Физически меняет лишь 8 байт заголовка (тело пада и
GUID = 0xFF стёртого флеша). Никакого кода. Исходы:
- **бутится** → цепочка терпит новые файлы → виноват контент именно
  драйверного файла → следующий шаг: контент-бисекция (depex-меньше/
  сжатая обёртка/пустой PE32);
- **висит** → intolerance цепочки → insert-путь на нативе мёртв,
  цикл B уходит на вариант 2 (патч UncoreInitPeim in-place, цепочка
  не меняется) + движку нужна не insert-, а replace-_capability;
  гейт G фиксирует это железно.

**Вердикт PAD-пробы (2026-09-15 ~03:00).** `450x-kopiya-bif-padtest.bin`
(f58b790d…) — **БУТИТСЯ**: сетап доступен, ОС грузится. Цепочка FV
новые файлы терпит ⇒ виноват контент драйверного файла.

**Форензика PE-стиля (после PAD).** Сравнение заголовков PE32:

| Поле | родные (AcpiVTD/DxeCore, все 243) | наш GCC-артефакт |
|---|---|---|
| Characteristics | 0x2022 (IMAGE_FILE_DLL — MSVC-сборка) | 0x002E (GenFw) |
| SectionAlignment | 0x20 | 0x1000 |
| FileAlignment | 0x20 | 0x1000 |
| SizeOfHeaders | 0x240/0x280 | 0x1000 |

Причина — тулчейн: `-t GCC` линкует с `-z common-page-size=0x1000`
(tools_def.template:868-874), GenFw честно переносит выравнивание.
Все серийные артефакты цикла S1 — та же болезнь (0x1000/0x2E,
железом не проверялись). ERROR-строку печатает PEI-модуль
(`AmiTpm20PlatformPei` 12/34 — владелец формата по смещению
0xE5B404), т.е. зависание — на самом стыке PEI→DXE, при первой
загрузке нашего PE старым AMI-загрузчиком.

**Фикс: EPA-1c.** Тулчейн `-t GCCNOLTO` (`-Wl,-n -z
common-page-size=0x40`, tools_def.template:622) + пост-патч
Characteristics 0x2E→0x2022 (2 байта, вне чексумм; код в
build_bif_epa.sh). Итог: chars=0x2022, SecAlign=0x40,
SizeOfHdr=0x240 — натив-стиль. Артефакт 15776 Б, sha256
`fa6c455f79d910556d6458d82968d3c62da50858d74c36d3f03dbeec5f566a46`,
воспроизводим ×2, валидатор и `bif_ffs` зелёные, тест-данные
обновлены. Образ `refs/amibcp/450x-kopiya-bif-epa1c.bin`, sha256
`75f7e89e40ec29622745c1c06249d5dc6c8cc7a2f9ffb372f8bc9455747aaf1e`
(вставка 15776 Б @ 0xBDA368 после 8/242, read-режим). Исходы: бут +
`BIF-EPA:` → телеметрия получена, цикл продолжает на EPA-2; висит →
PE-стиль не при чём → контент-бисекция (depex/PCD/сжатие).

**Вердикт EPA-1c (2026-09-15 ~03:30, screen.log epa1c).** Натив-стиль
PE не помог: **висит там же** (VGA не мигает, 5 минут ожидания,
два бута; владелец подтвердил по VGA — зависание реальное, не
SOL-маршрут). PE-стиль снят с подозрения.

**Финальный структурный аудит (все 207 драйвер-файлов FV8).**
Раскладки: `depex+GUIDED` ×118, `FV+GUIDED` ×56, `GUIDED` ×27 …
**Ни одного файла с сырой (несжатой) PE32-секцией.** Depex-формы
наши — норм (29 нативов с 36-байтовым, как наш). Единственная
оставшаяся аномалия — **сырая PE32-секция**: у всех нативов
полезная нагрузка внутри GUIDED `EE4E5898-…` (LZMA_CUSTOM_DECOMPRESS,
DataOffset 0x18, Attr 1, dict 16 МиБ, явный распакованный размер).

**EPA-1d (следующая прошивка): нативная раскладка.** FFS =
`[depex][GUIDED LZMA{PE32, UI}]` — точная форма родных файлов.
PE внутри — чиненый (GCCNOLTO + DLL-бит) из EPA-1c; LZMA-стрим —
как у нативов (dict 16 МиБ, размер в заголовке прописан явно —
python-lzma по умолчанию пишет unknown-size 0xFF…FF, что EDK2-
декомпрессор на железе не принял бы; поймано тестом bif_ffs).
Артефакт 7200 Б, sha256
`3c258bfec6c8bc10fda36f07b896e6f35997de8929b25b9ab3a8ecc784a54512`,
воспроизводим ×2 (валидатор обучен вложенной раскладке), движок
распарсил обёртку (вложенные PE32+UI видны). Образ
`refs/amibcp/450x-kopiya-bif-epa1d.bin`, sha256
`241ce2d8caa7e31388807e77af6c5ec1800d0960515eba237de08e38f0e06462`
(вставка 7200 Б @ 0xBDA368 после 8/242, read-режим). Исходы: бут +
телеметрия → EPA-2; висит → последняя структурная гипотеза мертва,
далее контент-бисекция (файл без depex / копия родного драйвера).

**Вердикт EPA-1d (2026-09-15, SOL владельца).** Нативная раскладка
FFS не помогла: **висит там же** — `PeimMemoryQpiInit END` →
`ERROR: Class:3000000; Subclass:50000; Operation: E`, `BIF-EPA:`-строк
ноль. Четвёртое подряд зависание (1: двойной PE32; 1b: одиночный;
1c: натив-стиль PE; 1d: нативная GUIDED-LZMA-раскладка) при живом
PAD-контроле и живой базе между прошивками. Итог цепочки B2:
**вставка нового драйверного файла в FV этого BIOS мертва как
класс** — все структурные переменные исчерпаны, контент-бисекция
(ещё 2+ прошивки) обслуживала бы вставку-ради-вставки. Владелец
принял решение на месте: **пивот на вариант 2** (in-place патч
UncoreInitPeim — правки внутри тел существующих файлов на этой
семье бутятся: ds1/v6), write-ladder (EPA-2) отложен (план
f88460d, Task V2). Допущения R2b (ECAM 0xE0000000, bus 0) остались
непроверенными телеметрией — для варианта 2 они не нужны: патч
меняет полиси ДО воркера, адрес считает сам родной код.

## B3. EPA-2 (write-ladder) — вердикт владельца
(заполняет Task 9; **статус: ОТЛОЖЕН** — маршрут вставки мёртв,
см. §B2/B4 и план f88460d; write-механика сборщика сохранена для
цикла B на случай оживления вставки)

## B4. Вариант 2 — in-place патч UncoreInitPeim (patch1)

**Механика (контейнерные факты).** PEIM
`D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3` (type 0x06) лежит в FV
`0xE200000` (len 0x1E0000) **несжатым**: FFS @ `0xE64A68` size
0xC3172, секции `0x1B/0x10/0x15/0x14`, PE32-тело побайтово @ image
`0xE64B08` (798880 Б, **одна копия в образе**). FFS `attrs=0x00` →
ATTRIB_CHECKSUM clear, file-checksum байт 0xAA — **не
пересчитывается**; заголовок не трогается. VA = `0xFF000000 +
image-offset` (соответствует §R2b: файл 0x3228A ↔ VA 0xffe96d92).

**Патч.** 12 сайтов §R2b (все пишут байт полиси IOU0/сокет-0
`[cfg+0x248]` — Port 2, из board-веток; другие порты/IOU не
затрагиваются): последний байт imm {1,2,3,4} → `0` (x4x4x4x4,
`и or 8` воркера даёт known-good 0x8 из HNX). Длина инструкций не
меняется (кодировщики: `c6 87/86 48 02 00 00 imm` ×4,
`c6 00 imm` ×7, `c6 01 imm` ×2 — включая GPIO-пару
0x32C2C/0x32C3A как альтернативные ветки). Скрипт
`hack/uncore_bif_patch.py` (verify → patch → self-check:
ровно 12 отличий от базы, всё вне сайтов байт-в-байт, размер
неизменен; `--check` для повторной верификации).

**Корректность значения 0:** нативный код сам пишет 0 в полиси
IOU1-сокета-0 (`mov [esi+0x24c], 0`, 0xffe97297) — валидатор
(`cmp bl, 0xff` → ошибка только на AUTO) и lane-таблицы
(запись 0 = `{04,04,04,04}`) этой же прошировки значение 0
обрабатывают штатно.

**Верификация.** `cmp -l` база↔patch1: ровно 12 байт, все
`{1,2,3,4}→0`; re-disasm (capstone, контейнер) всех 12 сайтов в
патченном образе: `mov byte ptr [edi+0x248], 0` / `[eax]` /
`[ecx]` — imm=0 везде.

**Артефакт.** `refs/amibcp/450x-kopiya-bif-patch1.bin` (локально,
не в git), 16777216 Б, sha256
`7b40d7251747b687fc1561bd6684226fc8772880cade7a3819896b54d8c6587c`
(база `76ec3d3a…` + 12 байт). Сайты (image):

| # | image | VA | imm |
|---|---|---|---|
| 1 | 0xE96D92 | 0xffe96d92 | 4→0 |
| 2 | 0xE96E1F | 0xffe96e1f | 4→0 |
| 3 | 0xE96F9A | 0xffe96f9a | 3→0 |
| 4 | 0xE9713B | 0xffe9713b | 3→0 |
| 5 | 0xE9716D | 0xffe9716d | 2→0 |
| 6 | 0xE971E2 | 0xffe971e2 | 4→0 |
| 7 | 0xE973F0 | 0xffe973f0 | 4→0 |
| 8 | 0xE975B6 | 0xffe975b6 | 3→0 |
| 9 | 0xE97734 | 0xffe97734 | 4→0 |
| 10 | 0xE97742 | 0xffe97742 | 1→0 |
| 11 | 0xE97859 | 0xffe97859 | 4→0 |
| 12 | 0xE978AC | 0xffe978ac | 4→0 |

**Дерево исходов (гейт владельца).** Флеш patch1 → в ОС
`lspci -nn | grep -i -e 02: -e nvme` / `lspci -vv` Port 2:
- **бут + порты 2A/2B/2C/2D видны (или карта в Port 2 режется до
  x4)** → полиси применилась, цель цикла A достигнута на варианте
  2 → §G;
- **бут, эффект ноль** → сид переопределяется (NVRAM-слой вне
  PEIM): снести CMOS и повторить; если снова ноль — вариант 3
  (кросс-свап PEIM из HNX, у которого фабричный x4x4x4x4);
- **зависание** (неожиданно: класс контентных правок в телах,
  бутивший на этой семье) → защитный механизм именно PEI-FV/PEIM →
  §G с разбором.

**patch2 «все порты сокета 0» (эксперимент владельца, §B4-расширение;
шить после вердикта patch1, далеко идущих выводов не делаем).**
Сканер `hack/uncore_policy_scan.py` (линейный sweep .text с lea-трекингом;
калибровка — 12 сайтов IOU0 = §R2b побайтно) дал полные списки
imm-записей полей: **IOU1/[0x24c] (порт 3) — 17** (16 ненулевых
+ нативный 0 на 0xffe97297), **IOU2/[0x250] (порт 1) — 7** (все
imm=1 = x4x4x8). Все записи под сторожем `cmp [field],0xFF; jne` —
подмена AUTO борд-константой, значит нули по всем сайтам гарантируют
итоговый 0 по любой ветке. Не патчатся: поля сокета-1 (0x249/0x24d/
0x251; вкл. два сайта IOU1-s1 в хвосте .text @ VA 0xfff208xx — если
стоит второй проц, добавить отдельным шагом), соседние не-полиси
поля 0x24a/0x24b/0x24e/0x24f (значения 3/4, семантика не
классифицирована), страп-ветка IOU2 VA 0xffe97848 (`mov
[esi+0x250], al`, al∈{0,1} из MMIO-страпа, без AUTO-сторожа — порт 1
может остаться x4x4x4x4/x4x4x8 по страпу, байтом не лечится).
`uncore_bif_patch.py --ports=all`: 35 сайтов (12 IOU0 + 16 IOU1 +
7 IOU2) imm→0. Образ `refs/amibcp/450x-kopiya-bif-patch2-allports.bin`
(локально), sha256
`2f7f137eceae1c86b809a11e037724ca6e0487ee1d9886cfeba8f4900865d438`,
cmp с базой = 35 байт. Сканер по патченному PEIM: в полях
0x248/0x24c/0x250 ненулевых imm не осталось (40 нулей = 35 патченных
+ 5 нативных; 29 оставшихся ненулевых — сокет-1/не-полиси поля).
Вердикт в ОС смотреть по всем трём портам: `lspci` (порты 1/2/3,
появление xxA/xxB/xxC/xxD) + `lspci -vv | grep -i lnk` (ширины).

**Вердикт patch1 (2026-09-15, владелец, первая реплика гейта).**
**БУТ ДО ОС** — первый неблокирующий флеш после цепочки EPA: in-place
класс на этой машине подтверждён. Топология из ОС:
`00:02.0-[03]`→Optane (x2, 8GT/s), `00:03.0-[04]`→SM2263EN,
`00:01.0-[01-02]`→I350×2; у dev 2 видна только функция 0; владелец
дополнительно менял рейзеры местами — картина та же. Бифуркационный
вердикт **пока не определён**: показанное совместимо и с
«применилось» (пустые 2B/2C/2D скрыты hide-флагами), и с «не
применилось» (grep владельца ушёл в пустоту из-за опечатки BDF:
`-s 02:0.0` = несуществующее 02:00.0; корневой порт = `00:02.0`).
Живость PEI-FV 0xE200000 подтверждена по строкам бут-лога:
`PeimMemoryQpiInit` @ 0xE75605, `Invalid IOUx` @ 0xE90A83,
`IIO=%d, IOU0=%d.` @ 0xE8FF14 — всё в патченом регионе, мёртвая
копия исключена (второй побайтной копии PEIM в образе нет).
Определительный вердикт — чтение самого регистра воркера из Linux
(sysfs/ECAM читает смещения >0xFF): `setpci -s 00:02.0 0x190.W`:
`…08` = x4x4x4x4 применилось; `…0C` = x16, не применилось. Контроли:
`setpci -s 00:03.0 0x190.W` — ожидаем `0C` (порт 3 не патчен),
`setpci -s 00:01.0 0x190.W` — `09` (или `08` по страпу). Маппинг
подтверждён владельческим lspci: IOU0/dev2 = порт с Optane —
хардкоды били в правильный порт. Примечания гейта: (1) форма
IntelRCSetup — patch1 Setup не трогал (12 байт в PEIM, другой FV);
владелец видел её на unlock-сборках (v5/v9, IFR-правки) — вернуть
слоем поверх можно по запросу; (2) patch2 НЕ шить до setpci-вердикта:
если полиси не применилось на патченом порту, superset тоже не
применится — флеш беречём.

**Разгадка (оффлайн-форензика после setpci-вердикта, 2026-09-15).**
Живые регистры: `00:02.0+0x190 = 0x0004`, `00:03.0 = 0x0004`,
`00:01.0 = 0x0001` — чистый енум (4=x16, 1=x4x4x8) **без бита 3**,
который воркер R2b ставит всегда (`or al, 8`) ⇒ финальный писатель —
не откартированный в R2b RMW (или была позднейшая перезапись).
Полиси «4» дошло до кремния при наших нулях ⇒ источник другой, и он
найден: **PEIM читает UEFI-переменную `IntelSetup`** (строка
L"IntelSetup" UTF-16 @ файл 0x2BFB8 внутри PEIM; GUID
`EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9`). Цепочка кода: хелпер
`0xffe97ec2` = PEI ReadVariable(This, L"IntelSetup", GUID, &0x1670,
buffer); единственный вызов — из функции `0xffe96b20`
(полиси-загрузчик):
- статус ≥ 0 (переменная есть) → `memcpy(cfg, var+база, 0xC7D)` —
  накрывает поля 0x248-0x251; маппинг по R2b-ручке:
  **[cfg+0x248] = IntelSetup+0x531 (IOU0/Port 2)**, IOU1 = +0x535,
  IOU2 = +0x539 (сокет-1: 0x532/0x536/0x53A) — точные адреса
  подтвердить живым дампом (триплет 04/04/01);
- статус < 0 (переменной нет) → шаблон AUTO=0xFF → срабатывают наши
  12 сайтов (подмена AUTO→0).
На живой машине переменная существует и держит явное значение ⇒
AUTO-сторож наших сайтов не срабатывает — **патч структурно обойдён
переменной**. Утверждение R2b «ручка IntelSetup+0x531 этим PEIM не
читается вовсе» — ошибочно (пропущен xref хелпера 0xffe97ec2; fix
истории — здесь). NVRAM-стор образа (FV 0x800000, NV-DATA) этой
переменной не содержит — живая IntelSetup создана бут-тайм посевом.
Открытая вторичная загадка: кто пишет финальное значение без бита 3
(кандидаты: другой accessor из четырёх `or eax,0x190`-сайтов
0xffe9218f-0xffe92ea0 / RC-слой) — на маршрут фикса не влияет.
**Следующий шаг — БЕЗ перепрошивки:** прочитать живую переменную из
Linux (`/sys/firmware/efivars/IntelSetup-EC87D643-…`, данные с +4
байта атрибутов): ожидание триплета `04 04 01` в районе 0x531/0x535/
0x539 (файловые +4). Подтверждение = маршрут фикса: записать `00` в
эти байты живой переменной (efivars/UEFI-runtime) — цель достигается
без нового флеша; patch1 остаётся AUTO-фолбэком. Противоречие с v6
(«натив игнорирует ручку из рантайма») снять по факту: что и куда
тогда писали (переменная/офсет).

**Живое чтение 2026-09-15 (владелец): efivars переменной IntelSetup
НЕТ** (`ls … | grep -i 'intel\|setup'` → только SetUpdateCountVar +
SetupMode). Вывод: бут-тайм переменная до рантайма не доживает
(PEI-времянка либо удаляется до BDS — гигиена скрытого мира);
ветка «прочитать живую» закрыта. Трансформация шага: **СОЗДАТЬ
постоянную переменную** (NV+BS+RT) с телом шаблона IntelSetup
0x1670 и сидом 0x531-0x534=00×4 — `refs/amibcp/intelsetup-var-bif0.bin`,
sha256 `740b44a7d90e7de8b95c91ae4bb1e751bf056f9ff4e7d220449caf96fbb9b259`
(источник — RAW-шаблон patch1, биф-зона перезаджена с фабричной
`04 03 ff ff`; живой триплет 04/04/01 = фабричные константы шаблона
— косвенно сеятель копирует фабрику, а не StdDefaults-сид, чем
объясняется и v9: сид шаблона до переменной не дошёл). Гейт:
если бут-сеятель уважает существующую переменную — полиси 0
придёт нативно, без единого флеша (и обратимо: rm переменной =
сток); перезапишет своей фабрикой — маршрут остаётся за patch1.
Контроль после ребута: `setpci -s 00:02.0 0x190.W` (08 = применилось)
+ чтение файла переменной (офсет 4+0x531 = 0x535: 00 = не
перезаписана, 04 = сеятель перезаписал).

**Гейт создания 2026-09-15 (владелец): EPERM на IntelSetup при
свободном создании постороннего имени** — `BifTest-<random GUID>`
(8 байт) создался без вопросов; `IntelSetup-EC87D643-…` отклонён
firmware-ным VarCheck (защита Setup-семейства от записи из ОС;
SB-allowlist ядра исключён тестом). OS-маршрут записи закрыт.
Третье подтверждение статуса имени (PEIM читает / VarCheck пасти /
efivars прячет). Кандидаты далее — оба in-place: (1) патч
сеятеля — найти звонящего `SetVariable(L"IntelSetup")` и источник
его констант 04/04/01 (не шаблон и не стор — доказано v9), флип
04→00; (2) NV-запись IntelSetup в NV-DATA-регионе образа — PEI
GetVariable не блокируется, работает если UncoreInitPeim читает
раньше перезаписи сеятеля. Оба требуют дизасм-поиска сеятеля —
он же закроет вторичную загадку писателя без бита 3.

**Матрица VarCheck (2026-09-15, владелец):** BifTest2@EC87D643
(5Б) ✓ создана — неймспейс записываем; IntelSetup@random-GUID
(5Б) ✓ — имя под чужим GUID свободно; IntelSetup@EC87D643
(5748Б) EPERM; BifTest3@random (5748Б) **EIO** — иная ошибка ⇒
персональная запись VarCheck ровно на пару `IntelSetup`@`EC87D643`
(не размер: большое тело на случайном GUID = EIO = прошивочный
лимит ~5.7КБ). Сеятель пишет до установки политики. OS-маршрут
закрыт окончательно; кандидаты: патч сеятеля / NV-запись в
стор образа. Осторожность: неудачные create оставили 0-байтные
фантом-записи в efivarfs (IntelSetup@EC87D643, BifTest3) — перед
ребутом проверить фантомность (`cat` → ENOENT), иначе пустая
настоящая переменная = непредсказуемый cfg у PEIM.

**Разворот 2026-09-15 (clean-фаза + пост-ребут):** EPERM-запись
**фактически ПРОШЛА** — `cat` прочитал полные 5748 байт
(IntelSetup@EC87D643 с сидом жила в сторе до ручного удаления;
ошибка bash — вторичный post-write вызов). Delete защищённой
пары прошёл; после единственного ребута переменной нет (тот бут
прошёл БЕЗ неё — PEIM наш сид не видел, setpci не изменился:
01.0=0001, 02.0=0004, 03.0=0004). Загадка: после ребута
 firmware создала `IntelRCSetup-12345678-…` (наш dummy-GUID,
 имя никем не создавалось; наши переменные к тому буту удалены) —
след обработчика, дамп/удаление pending. NOTE: владелец
переставил рейзеры местами — теперь Port 2 (02.0) = SM2263-x1,
Port 3 (03.0) = рейзер с оптанами; решающий маркер = setpci,
не счёт дисков. **Следующий шаг (контролируемый):** повторить
запись (ошибку игнорировать), сверить тело sha256
`740b44a7…b259` (`tail -c 5744 | sha256sum`), НЕ удалять, ребут,
snap: `setpci -s 00:02.0 0x190.W` → **0008 = полиси 0 пришло
нативно без флеша**; + обратное чтение байтов 4+0x531 файла
переменной (00 = сеятель не тронул / 04 = перезаписал — прямое
свидетельство для охоты на сеятеля).

**Переинтерпретация матрицы 2026-09-15 (post-rerun):** паттерн
ошибок коррелирует с ЧИСЛОМ write(), а не с именем/GUID/размером —
все одиночные write (BifTest/BifTest2/C, 5–8Б) создались; все
двойные `(printf attrs; cat body)` (IntelSetup@EC87D643 5748Б
EPERM, BifTest3 5748Б EIO, реран EINVAL) провалились — первый
write создаёт пустышку, второй модифицирует защищённое/конфликт.
«Персональная защита пары» и «лимит размера» — вероятно, артефакты
двух-write механики (тело с чужим sha при реране = кэш инода;
после ребута переменной нет — ни один большой write до стора не
дошёл; читка «5748 байт» первого раза = тоже кэш, не стор).
Протокол следующей попытки: ОДИН write (cat готового файла
attrs+payload) → сверка sha → без удаления → ребут → setpci.
**Разгадка IntelRCSetup-загадки:** содержимое = `07 00 0000 43`
ровно наш C-тест, тот же GUID ⇒ бут-обработчик ПЕРЕИМЕНОВАЛ
IntelSetup→IntelRCSetup (GUID/данные сохранив). Отпечаток
конвейера: вторая строка для дизасм-охоты — компонент с
L"IntelSetup" + L"IntelRCSetup" = сеятель/процессор.

**Финальная модель efivars-эпопеи (2026-09-15, 15:2x):** одиночный
write(2) тоже EPERM ⇒ защита пары настоящая, двухwrite-теория
снята. Читка «5748 байт» во ВСЕХ попытках (включая clean-фазу и
оба рерана, sha-tail стабильно `6b14c3fb…` через два разных
бута) — это НЕ наш сид и НЕ кэш ядра, а **тело живой постоянной
скрытой переменной**: `IntelSetup@EC87D643` существует во флеше,
читается прямым GetVariable по имени, но **не отдаётся
GetNextVariableName** — потому все `ls efivars` саги были пустыми
(«скрытый мир» = UEFI-переменные, скрытые от перечисления).
Наши записи ни разу не долетали; «удалена» у rm — молчаливый
отказ. OS-маршрут записи закрыт окончательно; открыт канал
**чтения живого конфига** (фантомный inode с прямым именем):
дамп тела = эталон сеятеля для дизасма; ожидание в теле:
0x531/0x535/0x539 = 04/04/01 (живое подтверждение маппинга
cfg+0x248↔0x531).

**Живой дамп получен (2026-09-15 12:31):** `refs/amibcp/
intelsetup-var-live.bin`, sha256 `40739afff36d02daf5b638f4eefa
4e3679e1e5b58be573941f9237571db5150d`, 5748 = attrs `00 00 00
00` + тело 5744. Триплет подтверждён живьём: 0x531/0x535/0x539 =
04/04/01 (маппинг cfg+0x248↔0x531 закрыт). Атрибуты 0 = это
**NVAR-запись, а не стандартная переменная** — оттого нет в
перечислении и оттого SetVariable мимо. Дифф тело-vs-шаблон
образа: **20 байт в 10 зонах** (0x69, 0xb68, 0xc4d, 0xf41, 0xf60,
0x1162, 0x11f5, 0x1591, 0x1666…), биф-зона идентична фабрике.
Интерпретация: источник сеятеля ≠ StdDefaults-шаблон (объясняет
провал v9); 20 байт = либо runtime-дописывание по ходу бута
(кандидат в «вторичную загадку» писателя), либо собственные
константы. Приоритетный кандидат фикса теперь: **NVAR-запись
IntelSetup с bif=00 в сторе образа** (patch3, чистые данные без
патча кода — VarCheck сидит на SetVariable, не на boot-time
чтении); альтернатива — патч констант сеятеля. Решение за
дизасм-стендом (строки L"IntelSetup" + L"IntelRCSetup", живое
тело + дифф как отпечатки).

## G. Гейт выбора варианта
(заполняет Task 10 по исходам B4/V2; ожидаемый вердикт — вариант 2)

**Разметка NVAR-стора в образе + v10 (2026-09-15, 16:2x).** IPMI
(lanplus) и ssh хоста проверены, SOL: enabled. Разбор FV@0x800000
(NVRAM FV, len 0x40000, «NVAR»-цепочка записей; формат записи:
магия `NVAR` + uint16 полного размера + 3 байта (счётчик записи,
`ffffff` = не дописывалась) + attrs-байт (0x82/0x83/0x88/0x02) +
guid-индекс + имя`\0` + тело):

- `0x800060` контейнер `StdDefaults` (0x1b8d), внутри вложенные
  дефолт-записи: `Setup`(0xa5), `PlatformLang`, `Timeout`,
  `AMITSESetup`(0x68), **`IntelSetup`@0x8001b7** (тело с 0x8001cd =
  тот самый «RAW-шаблон» из более ранних разделов) и др.;
- далее runtime-цепочка: `BootOrder`/`Boot####`/`MonotonicCounter`/
  `PchInit`/`ServerSetup`/`Setup`@0x80213c(g=7)/
  **`IntelSetup`@0x8021e1(g=7, счётчик 1d1800)**/`AMITSESetup`/…
  плюс **12 «призраков» 0x167a** (стёртые старые копии IntelSetup,
  attrs 0x88, без имени); стор занимает 0x800060–0x8189ff, дальше
  0xFF до маркера SetUpdateCountVar@0x83fe80.
  Guid-индекс 7 = EC87D643 (у `Setup` и `IntelSetup` общий).

Развязка «вторичной загадки»: дифф **runtime-тело@0x8021f7 vs живой
дамп = ровно 12 байт** (0xc4d, 0xc4e, 0xc50, 0xc54, 0xf41–0xf43,
0xf60, 0xf65, 0x1162, 0x1591, 0x1596) — и все 12 = ручки владельца
(выключенные ядра/порты, gen1, memfreq). «Сеятель ≠ шаблон» из
предыдущего абзаца снимается: живой стор = runtime-снапшот образа +
сохранёнки; никаких посторонних констант не нужно. Дефолт-копия в
v9 уже несла bif=00×4 (set-value цикла v4 писал и в контейнер
StdDefaults — потому F9 показывал x4x4x4x4), но PEI-потребитель
читает **runtime-запись** (0x8021e1, bif=04) — вот механика провала
v9: доставка в дефолты ≠ доставка в переменную. Призраки исторически
носят 03/04 — нуля не было ни разу ⇒ пишущий путь bif-зоны через
setup-save не ходил никогда (guard на записи, не на чтении PEIM).
TMM пишет стор образа (F9 после прошивки v9 показал наш 0 из
зашитого контейнера, а не заводской 04 из старого стора).

**patch3/v10 собран (чистые данные, 1 байт):**
`refs/amibcp/450x-native-v10-var-bif0.bin`, база v9, offset
`0x802728` (= 0x8021e1 + 22 + 0x531): `04→00`, sha256 `57f960480c
2ed440b882697f1b936f66ab8fa8353372f01e2a46bf7652ff6465`.
Runtime +0x531=00 (x4x4x4x4), +0x532=03 сохранён (минимальный
дифф до живого состояния); дефолты 00×4 (унаследовано из v9).
Чек-сумм в записи нет (доказано прошивкой: v9 менял тело дефолт-
контейнера и был принят). Ожидание: UncoreInitPeim находит
переменную (exists-path), memcpy var+0x531→cfg+0x248 = 00.
Вердикт после прошивки: `setpci -s 00:02.0 0x190.w` — `0008` =
сага закрыта; `0004` = guard ниже cfg (дизасм-охота потребителя
cfg+0x248). Проверка доставки: живой дамп (фантомный inode) —
если +0x531=00 при `0004`, доставка работает, guard вниз по
потоку; если 04 — стор переписывается на буте.

**v10 прошит и бутнут (2026-09-15, 18:17, железо).** TMM-флеш,
SOL-захват бута (лог /tmp/sol-v10-boot.log, QPI/MRC-дампы на
Debug=1 — IIO-бифуркация трейсом не печатается). Доставка стора
доказана независимо трижды: Gen1-cap Port 2A слетел (Optane
LnkSta 8GT/s, было 2.5), выключенные ядра вернулись (nproc=12 на
E5-2620v3), setupChanged=1 + Clearing MRC NVRAM в трейсе.
Регистр: `00:02.0 0x190 = 0004` — бифуркация НЕ применена,
Optane один, 00:02.1 нет. Фантомное чтение на recovery-ядре
умерло (EINVAL до VarCheck даже с attrs 07) — прямого чтения
переменной нет; различитель перенесён на показ меню Setup свежего
бута (меню читает переменную, без F9). Итог: стор доставлен, запись
IntelSetup с bif=00 была в железе; либо её переписал сеятель из
констант PEIM до/вместо чтения, либо консьюмер ниже cfg+0x248
игнорирует bif.

**v11 собран (patch3+patch1-композит):** `refs/amibcp/
450x-native-v11-var-auto-bif0.bin`, дифф vs v9 = 13 байт
(`0x802728` runtime var bif 04→00 + 12 AUTO-констант сеятеля
`0xe96d98..0xe978ae` 04/03/02/01→00 — те же сайты, что kopiya
patch1, в native байт-в-байт совпали), sha256 `26e3317e1462e15d
ce58bbd515ec75c4fabd2e09f3eec3ecc0caf56a8408c9b`. patch2-only
сайты (width-override, +23 байта) сознательно НЕ внесены.
v11 закрывает обе живые гипотезы: перепись переменной из констант
(var←AUTO) и безусловный посев cfg из AUTO; единственный
переживающий v11 сценарий = чистый downstream-guard (тогда
дизасм-цель = писатель 0x190/потребитель cfg+0x248).

**v11 прошит и бутнут (2026-09-15, 19:12, железо) — различитель сработал.**
SOL-сессия с fifo-stdin: F1-спам в окне POST вошёл в Aptio Setup Utility,
навигация Advanced → Intel RC Setup → IIO Configuration → IIO0
Configuration. Показ свежего бута (без F9): **IOU0=[x16], IOU1=[x16],
IOU2=[x8]** — переменная несёт фабричные 04/04/01, наш 00 не выжил.
При этом в v11 нулевые: runtime-запись (доставлена — доказано v10),
обе копии StdDefaults (FV0 raw + FV2 LZMA 0xafb520 — распаковано
и проверено: зона = 00), все 12 AUTO-констант PEIM. Операционная
поправка владельца (2026-09-15): IFR не перечитывается каждый
ребут (ручки ядер/gen1 жили через ребуты на v9) — пересеив
**однократный, пост-флешовый** (NVRAM признан невалидным после
TMM-записи; тот же механизм снёс runtime-настройки ещё в 超微-v2).
Кандидат источника однократного пересеива = manufacturing-дефолты.

**v12 собран (IFR-дефолт-флаг):** последний нетронутый источник 04
= опция `4="x16"` вопроса `#118:0x243` с flags 0x30
(DEFAULT|MANUFACTURING). IFR живёт в GUIDED-LZMA секции 0x918588
FFS abbce13d (несжатый PE32 0x9728a; раскладка r-efi: ONE_OF =
prompt,help,qid,varstore,offset; ONE_OF_OPTION = str,flags,type,
value — длина 7 при width-1). Патч: флаг 0x30 перенесён с value=4
на value=0 «x4x4x4x4»; рекомпресс lc0/lp0/pb1 (props 0x2D),
слот-фит: стрим 0x14963 + паддинг 0x4CE = слот 0x14E31 — размеры
секции/FFS/чек-суммы не менялись; поле размера заголовка
переписано на 0x9728a (python по умолчанию пишет 0xFF…8 — EDK2
GetInfo сломался бы). Артефакт: `refs/amibcp/
450x-native-v12-ifr-default-bif0.bin`, sha256 `f821745ef62e0b51e4a
77165417b6d7fbdafd419c47c47c288bf4f99c550ad4b`. Верификация
движком: `value = 0 "x4x4x4x4" (flags 0x30)` ✓. Итоговая карта
v12: все 5 известных источников 04 = 00 (runtime-запись, StdDefaults
×2, AUTO-константы ×12, IFR-флаг). Если пост-флешовый пересеив
читает IFR — переменная родится с 00 → PEI memcpy → cfg+0x248=00 →
`setpci 0008` = сага закрыта. Если меню после v12 снова [x16] —
источник глубже (DXE-переименователь/призраки стора) → чистый
дизасм. Если меню [x4x4x4x4] + регистр 0004 — консьюмер ниже cfg
игнорирует bif → дизасм-цель = писатель 0x190. Побочная находка
владельца: TMM/AST2400 деградирует после 2-3 прошивок — лечится
`ipmitool mc reset warm`.

**v12 на железе + ручной сейв-тест — ВЕРДИКТ ЦИКЛА (2026-09-15,
20:15).** v12 (все 5 источников 04 = 00): регистр 0004; меню
свежего бута — IOU0=[x16] ⇒ пост-флешовый пересеив читает НЕ IFR-
флаг тоже (источник глубже: DXE-шаблон/призраки — теперь второсте
пенно, см. ниже). Следующий зонд — ручной тест пишущего пути в
меню (SOL: навигация стрелками; F-клавиши >F4 на этой консоли НЕ
работают — ESC[xx~ съедается как ESC; сейв через вкладку Save &
Exit): IOU0 выставлен плюсом в [x4x4x4x4], «Save Changes and
Exit», ребут. Результат: **меню следующего бута показывает
IOU0=[x4x4x4x4]** — сейв ДОВЁЛ 00 до переменной и она пережила
ребут; регистр на этом буте — **0004** (x16, один 02.0).
⇒ **Переменная доставляет 00 в PEI, а консьюмер ширины её
игнорирует: guard — ниже по потоку. Доказано на железе.**
РЕТРАКЦИЯ: «гвард на записи bif-зоны» (из §B4-анализа призраков)
— НЕВЕРЕН: менюшный сейв пишет 0x531 нормально; 04 в живом дампе
v9-эры объясняется порядком флешей (каждый TMM-флеш сбрасывает
переменную фабрикой ДО дампа), не гвардом записи.
Итоговая карта цикла A: посев побеждён полностью (5/5 источников
обнулены + рабочий менюшный путь), транспорт переменной чист,
регистр не движется ⇒ последний враг = код, программирующий ширину
порта (писатель 0x190): читает НЕ cfg+0x248. Дизасм-цель финальной
фазы: в нативном PEI-цепочке найти программирование IIO width/
регистра 0x190 (xref константы, последовательность ресетов);
розетта-дифф с 超微-сборкой (там бифуркация работает на этом же
железе; UncoreInitPeim 606KB vs 628KB — другой билд RC) — сравнить
их width-пути. Открытая второстепенная загадка: источник
пост-флешового пересеива фабрикой (не IFR/не StdDefaults/не наша
запись/не AUTO) — кандидат DXE-процессор Setup с L"IntelSetup"
(9 модулей-кандидатов), но для практической цели не нужен: нужное
значение ставится через меню и живёт.

**Розетта-трио UncoreInitPeim (2026-09-15, вечер).** Три билда,
один и тот же GUID-модуль D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3,
все три — несжатая PE32 прямо в FV (распаковка не нужна):
натив @0xe64b08 (0xc30a0), 超微450 @0xe3de0e (0x99600), HNX
@0xdcff40 (0x93fa0). Скан по imm32 0x190: натив 12 хитов, 超微 9,
HNX 9. Главная находка: **HNX и 超微 — практически один билд**
(контексты всех сайтов совпадают байт-в-байт, сдвиг ~0x1d0);
натив — единственный отличившийся (12 vs 9 хитов, другие регистры
в мотивах: `0b c1` vs `0b c2`, сдвиг структуры cfg: чтения
`[esi+0xda4]` у рабочих vs `[esi+0xda3]` у натива — поля структуры
RC между билдами сместились). Интуиция владельца о единстве
бифуркации через тик/ток подтверждена бинарно: IIO-логика = один
кодес Intel RC у трёх вендоров. Ключевые мотивы: 4×ECAM-сайт
`…shl eax,0xc; or eax,0x190; …call <mmio-helper>` (по одному на
порт-стек) + `mov edi,0x190` + `mov eax,0x190` у jmp-таблицы.
Дизасм-срез рабочего хелпера (超微 0x1ada8): читает cfg-таблицы
0xd1d+port / 0xdab+idx*8 / 0xda4..0xda7+idx*8 (IIO-стек/порт
маппинг), собирает ECAM-адрес, политика приходит АРГУМЕНТОМ —
точка решения (откуда аргумент и где натив его подменяет) =
вышестоящий код, которого касаться надо с кросс-билдовым
вниманием к смещениям структур. Это масштаб дизасм-стенда
(как цикл A); вставка/драйвер-маршрут по-прежнему заблокирован
DXE-зависанием на новых экзешниках (§B2: 4 структурных варианта,
PAD-контроль бутится) — replace-capability движка остаётся
условием оживления драйверного плана-Б.

**ДИЗАСП-ФИНАЛ: ГВАРД НАЙДЕН (2026-09-15, вечер-ночь).** Продолжение
розетты (см. выше): путь записи бифуркации идентичен в обоих
билдах попарно — хелпер ECAM-0x190 (натив 0x2d640 / 超微 0x1ada8,
по 3 звонаря), «полиси»-хелпер (0x2c76e / 0x19efc) байт-в-байт
(кроме сдвига структуры 0xe63/0xe64), читатели cfg+0x248 (9 пар),
писатели поля: натив после v11 пишет только 0, 超微 пишет КОНСТАНТУ
4 (!) — поле значения не определяет. Зонд живых значений: ручка
«Serial Debug Message Level» (IntelSetup@0x2d6, 0=Disable/1=
Minimum/2=Normal/3=Maximum) выставлена в **Normal** через меню
(SOL; сейв живёт) — POST-трейс открыл новые принты, включая
**таблицу присутствия портов**: только `0|0|0|0|0 - Present`,
остальные 7 строк (вкл. Port 2) — **«Not Present»**. ⇒ Ветка
программирования 0x190 для Port 2 СКИПАЕТСЯ (je на флаге в звониле
хелпера), регистр остаётся в дефолте 0x0004 = x16 — объясняет все
нулевые посевы v10-v12. Легенда «вендоровский детект райзера»
подтверждена буквально. Код: presence-функция 0x7f8cf (натив;
~190 звонарей!), структуры `cfg+0x23840+stack*0x8357` (гейт) и
`cfg+0x2bb2d` (порт-таблица, шаг 0x23), глубокий запрос 0x84f51
(SMBus-стек: 0x84e2c→0x7fa73…); принтер таблицы ≈0x7ff30 (строки
файл 0x1a664/0x1a668). Единственный производитель «Not Present» =
`mov [rbp-8], 0xffffff00` @PE 0x7f985 (обе под-ветви глубокого
пути сходятся в него; ранний выход и ветка 0x7f98e всегда
Present). **v13 = v12 + 3 байта** (image 0xee4491-93: имм
`00 ff ff ff`→`00 00 00 00`): запрос присутствия больше не может
ответить «отсутствует» — все звонари получают Present; та же
инструкция есть в 超微 (0x5e3a0) — там их детект проходит честно.
Артефакт: `refs/amibcp/450x-native-v13-present-bif0.bin`, sha256
`76c81c054e71b9487d41aa0de1ff01164d884e41406767c42a934a94dec0cc
ed`. Риски: форс-Present может разбудить инициализацию фантомных
портов (таймауты/зависание) — откат = TMM-рекавери на v12. Проба
после прошивки: пост-флешовый пересеив вернёт Debug Level в
Minimum — выставить Normal снова через меню (механика отработана),
вердикт-бут: таблица присутствия (ожидаем все Present) + впервые
должны выстрелить принты `"IIO=%d, IOU0=%d.\n"` (файл 0x2b40c) у
точки решения + `setpci -s 00:02.0 0x190.w`.

## I. v13 — НЕ сработал; сага «присутствия» была ложной. НАСТОЯЩИЙ ЗАМОК НАЙДЕН (2026-09-15 ночь)

### I1. Вердикт v13 (прошит, меню: IOU0=x4x4x4x4, Debug сперва Normal, затем Maximum)

Регистр остался `0x0004`; набор RC-принтов байт-в-байт совпал с
v12-бутом (диф уникальных строк пуст, кроме одного не относящегося
к делу принта MRC). Таблица присутствия не изменилась. Затем Debug=
**Maximum** (меню, Save&Reset) — и на Maximum цепь точки решения
тоже молчит: `"IIO=…, IOUx=…"` = 0, `"PcieLinkTrainingInit…"` = 0,
`"Socket[%x] is socketValid"` = 0. ⇒ Проблема не в уровне принтов:
**вся цепь программирования LCTRL не исполняется вообще.**

### I2. РЕТРАКЦИЯ: «таблица присутствия портов» = DIMM-скан

Заголовок той таблицы — `Socket | Channel | DIMM | Bus Segment |
SMBUS Address`: это **скан SPD-шин DIMM** (строки `0|0|0|0|0 -
Present` = «DIMM0 канала 0 populated»), а не порты PCIe. Вывод
прошлой сессии «Port 2 Not Present → скип 0x190» — ошибка чтения
таблицы. Соответственно v13 патчил не гейт: функция PE 0x7f8cf —
**обёртка SMBus read/write** (гейт-байт `cfg+0x23840` проверяется
внутри, «0xffffff00» — код ошибки SMBus-WRITE, принтер таблицы
≈0x7ff30 живёт в функции скана райзера). Патч v13 менял код ошибки
write-операций — поведение бута не меняет. Призраков больше нет:
сага «вендоровский райзер-детект» в этой версии НЕ подтверждена.

### I3. Настоящая цепь программирования 0x190 (статика + Maximum-молчание)

Звено-стартер: завершение QPI-init (PE 0x34d4a) → печать
`"QPI Init completed! Reset Requested: %x"` (живой трейс: значения
0 и 2 по проходам) → при `[cfg+0x229a0]==0` вызов **F_43397
«IIO Early Link Training»** (строка-маркер `"IIO Early Link
Training Starting..."` PE 0x13a6c — в живом трейсе НИ РАЗУ) →
цикл сокетов → F_2c24a → F_2c279 (LCTRL-дефолты `cfg+0xe63+…←4`,
AUTO-таблицы; принты `"IIO=%d, IOU2/IOU0/IOU1=%d."` PE 0x2b3f8+,
каждый печатает содержимое cfg+0x250/0x248/0x24c) → F_2d014 →
**F_2c340** (`"PcieLinkTrainingInit at device scanning."` PE
0x2b370; полиси-хелпер 0x2c76e; гейт `cmp [esp+0x15],0` = «номер
порта на программирование»; построение ECAM-адреса с `edi=0x190`
@PE 0x2c457+). Полиси-хелпер читает селектор порта
`cfg+0xca9+iio*0xb+port`; селектор==0 или вычисленное==дефолт →
порт скипается. Таблица селекторов (44 байта = 4 сокета × 11)
копируется из переменной: F_32018 → LocateProtocol(2ab86ef5-…)
→ GetVariable(**L"IntelSetup", size 0x1670**) во фрейм → memcpy
cfg[0..0xc7d] ← var (тут живёт и bif 0x531→cfg+0x248) →
селекторы: `var+0x1596..0x15c1 → cfg+0xc7d..0xca8` (табл. A) и
`var+0x15c2..0x15ed → cfg+0xca9..0xcd4` (табл. B, читаемая
хелпером). Фабричная строка iio0 таблицы B: `02 02 02 02 02 00
00 00 00 00 00` — порт 2 = селектор 02 ≠ 0: **данные не блокируют,
блокируется исполнение цепи целиком.** При неудачном GetVariable
(esi<0) — ветка memset-дефолтов (селекторы гаснут); призраки
NVAR (0x167a × 12 = следы пересевов) показывают, что пересев
пишет полный размер 0x1670 — размер живой переменной корректен.

### I4. ЗАМОК: OEM-гейт Lenovo «Haswell-EP + фабричный ноль»

Единственный живой писатель мастер-флага `cfg+0x2273b` (вне
запертой цепи) — блок PE 0xbc4a4:

```
cmp byte [edi+0x160b], 0    ; edi=фрейм переменной: var[0x160b]
je  →flag=0                 ; ==0 → ВЫКЛ (фабрика = 00!)
cmp byte [esi+0x229ac], 0   ; поколение CPU по CPUID
je  →flag=0
flag=1                      ; оба условия → IIO Early Link Training ON
```

`cfg+0x229ac` пишет CPUID-классификатор PE 0x341cb+: **0x306F
(Haswell-EP) → 0**; 0x406F/0x5066 (Broadwell) → 2; прочие → 1.
Итог: на E5 v3 (наш E5-2620 v3) флаг поколения = 0 → ранняя
IIO-тренировка (и ЗАПИСЬ LCTRL 0x190 — бифуркация) **выключена
ядом Lenovo**, на v4 — включена. Второе условие — var[0x160b]:
во ВСЕХ копиях натива (runtime/StdDefaults/призраки) = 00; в
референсе 超微 на том же месте 01 (сдвиг лейаута +0xF, прямая
сравнимость ограничена). Ключевое: **в 超微-PE гейта 0x160b/0x229ac
НЕТ ВООБЩЕ** (поиск `cmp [edi+0x160b]` и `0x229ac`-гейта — 0
попаданий; строк «IIO Early Link Training» в sm тоже нет —
каркас другой версии RC, гейт — OEM-вставка Lenovo). Смежные
писатели флага неконфликтны: 0xba704 (без прямых звонков — мёртв),
0xbe041 — читатель. Писатель селекторов 0x31f9b и обе копии
`mov edi,0x190` (натив 0x2c450 / sm 0x19c03) парно идентичны
по структуре (сдвиг структур 0xe63/0xe64).

### I5. v14 = 4 байта NOP обоих `je` в гейте

`refs/amibcp/450x-native-v14-earlytrain-bif0.bin` = v13 + PE
0xbc4ab `74 12`→`90 90` и PE 0xbc4b4 `74 09`→`90 90`
(image 0xf20fb3/0xf20fbc). Флаг 0x2273b становится безусловно 1
(как в sm), цепь стартует после `"QPI Init completed! Reset
Requested: 0"`. Ожидания вердикт-бута: впервые `"IIO Early Link
Training Starting..."`, `"Socket[0] is socketValid = 1"`, принты
`"IIO=%d, IOUx=%d."` (значения cfg+0x248 должны показать 0 =
x4x4x4x4 из сохранённой переменной), и запись 0x190 ≠ 0004.
sha256 `58b74498af9b4b937317540bec17305a1203f08ac2436c21ad819d
61f8ad8452`. Риск: путь на Haswell-EP Lenovo не исполняла никогда
(свой гейт); у sibling-билда (HNX≈超微) эквивалентная цепь на
Haswell-платформах работает — умеренный. Откат = TMM на v13.

### I6. Развязка: var[0x160b] — вопрос Setup «IIO PCIe Link on phase»

Обход вопросов RC-формсета (CLI `hii question info`, 4066 строк в
`/tmp/rc-question-offsets.txt`) нашёл владельца смещения:
**form #5 «IIO», qid 0x20c, one_of «IIO PCIe Link on phase»**:
`1 = "Before memory chipset init"` / `0 = "Post chipset init"`
(дефолт, flags 0x30). Семантика замка окончательна:

```
ранняя IIO-тренировка (PEI, пишет LCTRL 0x190) ВКЛЮЧЕНА ⇔
  «IIO PCIe Link on phase» = Before memory chipset init  И
  CPU = Broadwell (флаг поколения ≠ 0)
```

На Haswell-EP с фабричным «Post chipset init» ни один путь этого
билда LCTRL не программирует — что и наблюдалось все 13 версий.
Пути владельца:
- **A. v14** (4 байта NOP, готов): гейт обезврежен, Haswell
  остаётся, CPU-свап не нужен;
- **B. Broadwell-EP CPU + меню**: «IIO PCIe Link on phase» →
  «Before memory chipset init» (+ IOU0=x4x4x4x4 уже сохранён) —
  чисто вендорская конфигурация, прошивка не трогается. Требует
  наличия E5 v4; сразу после свапа ручку всё равно ставить в меню
  (фабричный дефолт — «Post», и на BDW с «Post» гейт тоже закрыт).
  Замечание: v14 и B совместимы (NOP безусловен).

Для справки: в 超微-переменной на этом месте 01 (≈Before) —
согласуется с «у них работает».

## J. v14 — ВЕРДИКТ: ЗАМОК ВЗЛОМАН, ЦЕПЬ РАБОТАЕТ (2026-09-15, ночь)

### J1. Механика — всё сработало с первого раза

Прошит v14 (TMM, владелец). После пересева меню вернуло IOU0=x4x4x4x4
и Debug=Normal (SOL-риг). Вердикт-бут, впервые за всю сагу:

```
QPI Init completed! Reset Requested: 0
IIO Early Link Training Starting...          ← v13: этой строки не было НИ РАЗУ
Socket[0] is socketValid = 1
PcieLinkTrainingInit at device scanning...
IIO=0, IOU2=1.
IIO=0, IOU0=0.                               ← cfg+0x248 = 0 = x4x4x4x4 из меню!
IIO=0, IOU1=4.
IIO Early Link Training Completed!
DumpIioPcieLinkStatus():
  D[1]:F[0]  Link up as x04 Gen2   (I350 NIC, port 1)
  D[2]:F[0]  Link up as x02 Gen3   ← Port 2, первая половина
  D[2]:F[1]  Link up as x02 Gen3   ← Port 2, ВТОРАЯ половина — бифуркация!
  D[3]:F[0]  Link up as x01 Gen3
```

`setpci -s 00:02.0 0x190.w`: **0004 → 0000** — LCTRL впервые
программируется RC (битовая кодировка сплита; легенда «0008=x4x4x4x4»
не подтвердилась буквально — кодировка иная, регистр ПИШЕТСЯ, что и
требовалось). 4 байта NOP (PE 0xbc4ab/0xbc4b4) сняли OEM-гейт Lenovo.

### J2. Топология раскрылась — вопрос сместился в физику

- `lspci -t`: `00:02.0-[03]--03:00.0` (Optane) и `00:03.0-[04]--04:00.0`
  (SM2263) — **устройства на РАЗНЫх портах** (2 и 3), не на одном.
- LNKSTA: Optane x2@Gen3 (его LNKCAP=x2 — потолок самого устройства,
  полная своя скорость); root port 2 = x2; SM2263 = x1 (downgraded,
  LNKCAP=x4); root port 3 = x1.
- `dmidecode -t slot`: **SLOT3 «In Use» = 0000:03:00.0 = Optane**
  (x8-слот на port 2); SLOT4 «Available», bus 0000:ff:00.0 (порт не
  назначен — возможно, занят M.2); SLOT2 In Use = 01:00.0 (I350).
  SM2263 не принадлежит ни одному слоту → вероятнее всего это
  onboard M.2 (x1-распиновка), и его x1 — НЕ регрессия v14.

### J3. Меню-эксперимент с «Override Max Link Width» (безрезультатно, но показательно)

На пер-портовых экранах (Socket 0 PcieD02F0 - Port 2A и D03F0 -
Port 3A) вопрос «Override Max Link Width» (var 0x15c9/0x15cd; опции
Auto/x1/x2/x4/x8/x16, семантика = карта полиси-хелпера {1→1,2→2,3→4,
4→8,5→16}) выставлен в x4 на обоих портах, Save&Reset — ширины линков
не изменились (x2/x2/x1): фактические ширины ограничены устройствами/
распиновкой (Optane=x2, M.2=x1), а не селекторами. Побочно: пункты
2B/2C/2D (формы 87-89) в меню скрыты — GOTO-список показывает только
Port 0/DMI, 1A, 2A, 3A.

### J4. Открытый вопрос — владельцу (физика, не прошивка)

Куда физически садятся два Optane? Если SLOT3 (port 2) + SLOT4
(port 3) — каждому свой x8-порт, бифуркация НЕ нужна вовсе (снять
M.2 SM2263 с port 3, если он конфликтует со SLOT4). Бифуркация
port 2 → x4x4 нужна только если оба Optane должны сидеть на ОДНОМ
порту/слоте. Прошивочная часть саги закрыта: замок найден и снят,
тренировка и запись LCTRL работают, ручки доезжают до железа.

## K. Уточнения владельца + настоящая цель (2026-09-15)

1. **SOL F-клавиши РАБОТАЮТ через ESC+цифру** (владелец): F10 =
   ESC+0, F11 = ESC+Shift+1, F7 = ESC+7 и т.п. — прежний вывод
   «кнопки выше F4 не работают» относился только к CSI-последовательностям
   (`ESC[xx~`), НЕ к ESC+цифре. Для будущих меню-сессий: сохранение =
   ESC+0 (F10) напрямую с любой формы.
2. **SM2263 воткнут в райзер, слот x1** (распиновка райзера, не
   onboard M.2 — dmidecode-гипотеза §J2 скорректирована). PCI-дерево
   при этом по-прежнему показывает его за **портом 3** (00:03.0→
   04:00.0, x1): значит вторая позиция райзера wired к соседнему
   порту, а НЕ к lanes 4-7 порта 2. «Link up as x02» на D2:F1 —
   линк-фантом на висячих лайнах (реального устройства за F1 нет).
3. **Настоящая цель саги — умножение слотов** (линий хватает, слотов
   нет): x16 под 100G Mellanox + **x4x4x4x4 под 4 флешки** + x8 под
   SAS-HBA + желательно GPU под LLM. Текущий райзер (1×M.2 на port2
   + 1×M.2-x1 на port3) для этого всё равно непригоден — нужен
   квадро-адаптер x4x4x4x4.
4. **Тест-дискриминатор (владелец): райзер с оптанами → в Хуанань
   (HNX, бифуркация там работает)**. Если на HNX с x4x4x4x4 оба
   Optane видны на одном порте — райзер бифуркационный, добивать
   надо 450x-сторону (enum sub-портов F1-F3); если разъедутся по
   портам/не появятся — райзер не тот (или умер от экспериментов —
   владелец допускает). Заодно глянуть dip-переключатели райзера
   (многие M.2-райзеры имеют режим x4x4/single-x8).
5. **Открытый софт-вопрос на 450x (после HNX-теста):** почему
   обученный D2:F1 не публикуется как 00:02.1 в PCI-дереве.
   Кандидат: пер-портовые вопросы «PCI-E Port [Auto/Enable/Disable]»
   (form 86 qid 0x365 @ var 0x734 для 2A; для 2B — форма 87) —
   Auto при нераспознанном райзере глушит sub-порт. Проблема: формы
   2B/2C/2D (87-89) СКРЫТЫ из GOTO-списка меню (видны только
   0/DMI, 1A, 2A, 3A) — добраться менюшно нельзя; потребуются либо
   v15 (ан-suppress GOTO/force Port Enable), либо патч
   порт-енум-пути в RC/DXE.

### K6. v15 собран: формы Port 2B/2C/2D раскрыты (движок, form-unlock)

Двигковая машина formset-unlock (валидирована на этом же железе,
REF3) применена к скрытым пер-портовым формам: GOTO на формы 87-89
(2B/2C/2D) в форме 118 давились suppress-ref'ами на безымянных
чекбоксах «порт присутствует» (var 0xc51/0xc52/0xc53, фабрично =01
→ подавлены). `hii form unlock` ×3: expr `qid==0x0001` → `qid==0xFFFF`
(pkg+0x5dcb/0x5de4/0x5dfd, 01 00→ff ff), LZMA-респек секции Setup
(85189 байт диффа образа, контент-дифф = 6 байт). Артефакт
`refs/amibcp/450x-native-v15-unhide-2bcd.bin` = v14 + 3 флипа,
sha256 `5ac03eeca72d3c6ca4478032b190bebdf049b35894899faf290665762
86214cb`. Вердикн повторно: gates '==0xFFFF' (flip '-'), формы
87/88/89 visible=true; патчи v14 (NOP) и v13 (imm) на месте.

План после прошивки v15 (TMM, владелец): пересев → вернуть IOU0=
x4x4x4x4 + Debug=Normal (SOL) → в IIO0 Configuration появятся
«Socket 0 PcieD02F1 - Port 2B» / 2C / 2D → на 2B (и при желании
2C/2D) «PCI-E Port»=Enable (var 0x735/0x736/0x737, опции
Auto/Enable/Disable; меню-сейв персистентен) → Save (ESC+0) →
вердикт: lspci 00:02.1+ и две NVMe на порту 2. Semantика «PCI-E
Port»: per-port enable-массив var+0x731+i — кандидат на то, что
заставляет DXE публиковать sub-порты после сплита.

## L. v15 в железе: 2B обучается, но функция скрыта; v16 = дефолты чекбоксов (2026-09-16 ночь)

Владелец прошил v15 (TMM) и случайно забутнул её же; рейзер-дискриминатор
проведён: **рабочий райзер с хуананя (2×PM983A) → в ленову = стандартный
результат** (одно устройство), **на хуанане оба оптана видны в BIOS сразу**
— райзер и носители исправны, бифуркацию душит именно ленова.

### L1. Живой прогон v15: трейнинг 2B есть, публикации нет

SOL-риг (fifo/`ESC[OP`-спам/ESC+0), набор: IOU0=x4x4x4x4 (пережил флеш),
Debug=Normal (флеш пересеял в Minimum — выставлено заново), **Port 2B
«PCI-E Port»=Enable** (var 0x735; в форме 2B хелп: «In auto mode the BIOS
will remove the EXP port if there is no device…»). Верезультат:

- POST впервые печатает фазу **«IIO Early Post Link Training»**:
  `Skt[0], D[2]:F[0] Link up as x04 Gen3!` и **`Skt[0], D[2]:F[1] Link up
  as x04 Gen3!`** (+ `D[1]:F[0] x04 Gen2`, `D[3]:F[0] x01 Gen3`) — это и
  есть «2 устройства gen3 по x4» из наблюдения владельца. Gen3Override
  перечисляет под-порты PORT=2(3),(4),(5),(6) = 2A-2D — RC знает сплит.
- Трейнинг устойчив: warm-ребут, cold power-cycle, OMLW=x4 на 2B —
  D[2]:F[1] x04 Gen3 на каждом буте.
- Но в OS **00:02.1 нет и в config space**: `setpci -s 00:02.1` → FFFF —
  функция скрыта на уровне IIO (аппаратный ход, не скип PciBus). За 2A
  виден PM983#1 (LnkSta 8GT/s x4), за 3A — SM2263.
- Меню-рычаги исчерпаны: Enable ✓, OMLW 2B=x4 ✓ (обе — на 2A/3A,
  опубликованных портах, стояли ещё с §J), «Hide Port?»=[no] — публикации
  нет. Статус-строки формы 2B («Link Did Not Train», «ERROR: Not
  Available») читают enum-данные RC, а не LTSSM — «Did Not Train» при
  живом x04 Gen3 = порт для энума не существует.

### L2. Кто скрывает: clr_hdrmfd = 0b111 + безымянные чекбоксы

- `00:05.0` (UBOX/IIO-мод, 8086:2f28) `hdrtypectrl+0x80 = 0x00000007` —
  **clr_hdrmfd для Device#1/2/3 все взведены** («only function#0
  visible»), см. E5 v4 Vol.2 §6.6.17. Девайсы 1-3 официально
  однофункциональны — под-порты 2B/2C/2D скрыты сознательно, каждый бут.
- Рефайн семантики регистров (живые чтения): **0x190 = сырой per-IOU
  селектор бифуркации** (2A=0000↔IOU0=0=x4x4x4x4, 3A=0004↔IOU1=4,
  1A=0001↔IOU2=1 — совпадает с принтами `IIO=0, IOUx=`); публикация от
  него НЕ зависит (3A работает при дефолтном 0004). В policy-хелпере
  (F_2c76e) найдены **ECAM-записи lane-масок (0x3/0xc/0x30/0xc00/
  0x3000/0xc000) в offset 0x45C каждого под-порта по его функции** —
  т.е. в донорском флоу функции видимы ДО этого места; ленова скрывает
  раньше.
- Дефолт-сторы: **безымянные чекбоксы живут в самих формах 87/88/89**
  (последний вопрос после «Hide Port?», qid 0x3bf/0x3ed/0x41b ↔ var
  0xc51/0xc52/0xc53) — **навигационно недостижимы** (врап перескакивает,
  рендер пустой). OEM-дефолт 01 = «порта нет на плате» — ложь, опроверг
  нутая райзером. Кандидат на первопричину скрытия.
- UncoreInitPeim не читает 0xc51/0x735 литеральными офсетами — только
  через пер-портовые копии (selector-copy): статик-охота на пишущего
  DEVHIDE — отдельная сессия. Даташит: DEVHIDE («must hide all unused
  root ports… via the DEVHIDE register», Intel QPI reg space) в
  публичном E5 v4 Vol.2 только упоминается; определение — NDA EDS.

### L3. v16 = v15 + дефолты чекбоксов 0 (движок set-value)

`hii question set-value … 0` ×3 (qid 0x3bf/0x3ed/0x41b) — применено в
**оба** NVAR StdDefaults-стора (file CEF5B9A3 raw body + file 9221315B
section 0x19; store+0xdbe/dbf/dc0: 01→00). Артефакт
`refs/amibcp/450x-native-v16-presence0.bin`, sha256 `4ad67b7bf9f30686
c8ce19c5c86266f5d46e655af0865b36ab737483f14fd3fb`, дифф 636 байт
(LZMA-репак NVAR-файлов). Верифицировано: suppress-флипы v15 на месте
(==0xFFFF, pkg+0x5dc5/0x5dde/0x5df7), NOP-ы v14 на месте (image
0xf20fb3/0xf20fbc = 90 90).

### L4. План прошивки v16 + развилки

После TMM-прошивки (SOL): (1) F9 «Load Optimized Defaults» —应用 новых
дефолтов (или пересев применит сам; сверить: GOTO форм 2B/2C/2D видны и
без флипа v15 — чекбоксы теперь 0); (2) заново IOU0=x4x4x4x4, 2B
«PCI-E Port»=Enable, OMLW 2B=x4, Debug=Normal; (3) Save (ESC+0) →
вердикт `lspci`: 00:02.1 и второй PM983 на порту 2. Риск: низкий
(дефолты влияют только на пересев). Если чекбоксы не при чём —
развилки: (a) статик: найти пишущего DEVHIDE (кросс-дифф nat/sm PE,
охота на ECAM-записи в dev8/QPI-пространство); (b) **Broadwell-EP
владельца**: cfg+0x229ac → 2 — OEM-гейт поколения CPU проходит
нативно; если pass скрытия проверяет тот же флаг, замена CPU снимает
всё разом (v14/v15 патчи при этом безвредны).

### L5. v16 в железе: чекбоксы отвергнуты; дефолт IOU0 = x4x4x4x4 (2026-09-16 ~01:00)

Владелец прошил v16 (TMM). Флеш НЕ пересеял (первый бут печатал полный
трейс — значения сессии v15 жили), SOL-риг поднят, **F9 Load Optimized
Defaults** применён (диалог Yes), случайный ESC-выход из Setup → бут на
дефолтах. Вердикты:

- **A/B чекбоксов: отвергнут.** После F9 (дефолты чекбоксов теперь 0 в
  обоих сторах) — `00:02.1` всё ещё FFFF, `00:05.0+0x80 = 0x07`.
  Безымянные чекбоксы 0xc51-53 кормят только UI-suppress, не запись
  скрытия.
- **Хирургия registров из Linux:** `setpci -s 00:05.0 0x80.l=0`
  (clr_hdrmfd 0x07→0x00, читается обратно 0) + PCI rescan — **2.1 не
  появился**. hdrtypectrl = декорация заголовка; реальный гейт декода —
  тот самый NDA DEVHIDE («Intel QPI Configuration Register space»,
  dev8 QPI-агент, на этом одиночном сокете даже не виден в lspci).
- **Находка: фабричный дефолт IOU0 = x4x4x4x4.** На дефолтном буте
  `2A.0x190 = 0000` (= байт меню 0 = x4x4x4x4; запись 0x190 идёт при
  любом Debug level). Квад-сплит — ЗАДУМАННЫЙ дефолт платформы; OEM
  обошёлся двумя вещами: замок трейнинга (§I, снят v14) + скрытие
  функций. Дефолтный бут RC-молчит (Debug=Minimum) — тренинг 2B на
  дефолтном Auto не наблюдаем из лога, но на скрытие функций это не
  влияет (0x07 при любых значениях).
- Статик-быстроскан провалился: в PE нет hide-строк, нет литерала
  0x28080 (ECAM-адрес hdrtypectrl), все `mov $0x80,%edi`-сайты —
  value-OR/битовые флаги, не конфиг-офсеты. Пишущий DEVHIDE не в
  трассированных функциях UncoreInitPeim — нужен отдельный заход
  (кросс-дифф с超微-донором по всей прошивке, охота на ECAM-записи в
  dev8/QPI-пространство).

**Развилки (приоритет):** (1) **Broadwell-EP владельца** — дешёвый
дискриминатор: если pass скрытия проверяет cfg+0x229ac (поколение CPU,
как гейт трейнинга в §I), замена CPU снимает всё сразу; v16 в прошивке
безвреден на BDW (NOP-ы дублируют натуральный проход). Текущее
состояние переменных (дефолты, IOU0=x4x4x4x4) уже готово к тесту.
(2) Глубокий статик-RE DEVHIDE-писца по всему образу. Машина в
рабочем состоянии: обе NVMe в OS (PM983 на 2A за 00:02.0, SM2263 на
3A), ОС грузится.

### L6. v17 = «запечённые дефолты» (запрос владельца: F9 поднимает всё)

Идея владельца: не пересеивать руками после каждого флеша — испечь
рабочие значения в NVAR StdDefaults, чтобы F9 (или пост-флешовый
пересев) поднимал всё сразу. `set-value` по обоим сторам (файлы
CEF5B9A3 raw + 9221315B sec 0x19):

| вопрос | офсет | было→стало |
|---|---|---|
| IOU0 (IIO PCIe Port 2), form 118 qid 0x243 | IntelSetup+0x531 | 00 (уже 0 — фабрика) |
| Attempt Fast Boot / Fast Cold Boot, form 4 qid 0xb57/0xb58 | +0x10a8/9 | 00 / 02→00 |
| Serial Debug Message Level, form 9 qid 0x1b | +0x2d6 | 01→02 (Normal) |
| Port 2B «PCI-E Port», form 87 qid 0x393 | +0x735 | 00→01 (Enable) |
| Launch PXE OpROM, form 10063 qid 0x8b | Setup+0x79 | 02→00 (Do not launch) |
| Onboard NIC1/NIC2 ROM, qid 0x8c/0x8d | Setup+0x7a/7b | 01→00 ×2 |

«Quick Boot» в этом BIOS = «Attempt Fast Boot» (RC formset, form 4).
IOU0=0=x4x4x4x4 и Fast Boot=0 уже стояли в сторах — печка только
дожала остальное. Артефакт `refs/amibcp/450x-native-v17-baked-defaults
.bin`, sha256 `ffda1c9339729b3d7c0a0b479017c9299a8a51b336084ea20e4
0e295d1baa6a3`, дифф 904 байта (LZMA-репак), NOP-ы v14 и флипы v15
на месте. После TMM-флеша — один F9 → SOL без PXE-ромов («Press
Ctrl+S» уходит), Debug=Normal с первого POST, сплит+Enable в дефолте.
Дальше по плану §L5: BDW-EP свап-тест или глубокий RE DEVHIDE.

### L7. v17 в железе: флеш сам пересеивает печёные дефолты; база HSW снята (2026-09-16 ~02:00)

TMM-флеш v17 **сам пересеял StdDefaults в NVRAM** — F9 даже не
понадобился (диалог F9, к слову, в этой сессии не открылся — клавиши
съелись, но и не надо): первый же бут печатал RC-трейс = Debug=Normal
из печки. Проверено: Boot Agent/PXE-ROM исполнился только на буте
сразу после флеша (пересев случился в его ходе), на следующем —
чисто; `IIO=0, IOU0=0` в принтах; OS-база HSW: 00:02.0 only (2.1
скрыт — ожидаемо), PM983 (2A, x4 Gen3) + SM2263 (3A, x1) на месте,
`2A.0x190=0000`, `00:05.0+0x80=0x07`. **Машина готова к BDW-свапу:
после замены CPU — просто бут, все значения уже в NVRAM.**

## M. Broadwell-EP свап-тест: скрытие НЕ зависит от поколения CPU (2026-09-16 ~02:30)

Владелец заменил E5-2640 v3 → **E5-2640 v4** (BDW-EP, CPUID model 79 =
0x4F — та самая ветка `cfg+0x229ac→2`, на которую гейт трейнинга
реагировал «нормально»). v17 в прошивке, печёные дефолты в NVRAM.

Результат первого бута (память натренировалась, RC-трейс полный):
- D[2]:F[0] x04 Gen3 + **D[2]:F[1] x04 Gen3** — трейнинг сплита
  работает и на BDW (v14-NOP-ы безвредны, как и предсказано);
- root port сменил личину: «Xeon E7 v4/E5 v4/E3 v4/Xeon D PCI
  Express Root Port 2 (rev 01)» — BDW-идентификаторы;
- **но 00:02.1 по-прежнему FFFF, hdrtypectrl=0x07, 0x190=0000.**

**Вывод: pass скрытия функций НЕ проверяет поколение CPU — это
безусловный OEM-код** (или вход, который мы ещё не нашли; все
menu-вары, чекбоксы и CPU-gen исчерпаны). Осталась одна дорога:
статически найти и нейтрализовать запись скрытия в коде образа.
План: (a) кросс-дифф nat/sm UncoreInitPeim по всей площади (не только
трейнинг-цепь) + охота на ECAM-записи в dev5/dev8/QPI-пространство
(sm-PE без MZ — дизассемблировать как raw binary); (b) если писец не
в UncoreInitPeim — перебрать DXE-модули RC (Iio/Uncore DXE-обёртки)
по FFS-списку движка. Владельцу BDW рекомендовано оставить (E5-2640
v4 = +4 ядра, для саги эквивалентен).

### M2. Пост-свапная механика пересевов: ДВА источника дефолтов (2026-09-16 ~02:10)

Свап CPU (как любое изменение железа) вызывает пересев NVRAM из
**IFR-дефолтов вопросов** — а НЕ из NVAR StdDefaults-сторов, которые
мы пекли в v17. Наблюдение владельца «была заставка + мелькал intel
opt rom» = ровно это: Debug→Minimum, OpROM→Legacy, IOU0→x16 (IFR-
дефолт!). Итого два источника: (a) TMM-флеш/F9 → NVAR StdDefaults
(наши печёные значения, фабрика IOU0 там = 0 = x4x4x4x4); (b)
hardware-change reseed → IFR per-question defaults (IOU0 = x16!).
Для полной неуязвимости нужно патчить и IFR-дефолты (DEFAULT-флаги
one_of) — отдельная движковая операция, отложено.

Практика: меню-попап (Enter на one_of) — надёжный способ ставить
значения (список опций IOU0: x4x4x4x4, x4x4x8, x8x4x4, x8x8, x16,
Auto; '+' циклит в другом порядке). Полный пресет восстановлен
вручную: IOU0=x4x4x4x4, 2B Enable, OMLW 2B=x4, Debug=Normal.
Вердикт BDW с полным тюнингом = прежний: `IIO=0, IOU0=0`,
D[2]:F[1] x04 Gen3, **00:02.1 FFFF**, hdrtypectrl=0x07.

### M3. Debug=Maximum: принтов скрытия нет (тупик отладочного пути)

Поднял Debug до Maximum (+ бут) — 405KB трейса: ни одного
hide/DEVHIDE-принта; `IioUniphyDisable (per socket): 0 0 0 0`,
`OutIioUniphyDisable: 0,0,0,0` (PHY ничего не выключен — скрытие
чисто декодовое). RC-дебаг писца не называет — только статика.

## N. Статическая охота — публикатор найден, v18 ГОТОВ (2026-09-16)

Полная цепочка по дизассму nat-uncore.pe (VMA = file_off + 0xffe64b08;
полный дизассм /tmp/nat-uncore.asm, регенерация в §M3):

1. **F_918f8** (VMA 0xffe918f8): цикл 11 портов; BDF собирает из
   cfg[0xd1c+sock] (bus) + cfg[0xda3+8i]/cfg[0xda4+8i] (dev/fn);
   флаг публикации = cfg[0x2bc+sock*11+port]; зовёт F_9182d.
   Дефолтная BDF-таблица лежит в PE (VA 0xffe70e68, 0x58 байт,
   11×8) и содержит ВСЕ подфункции, вкл. (2,1)(2,2)(2,3) —
   полный список: DMI,(1,0),(1,1),(2,0),(2,1),(2,2),(2,3),(3,0)..(3,3).
2. **F_9182d** (VMA 0xffe9182d): ECAM `(bus<<20)|(dev<<15)|(fn<<12)`,
   офсет **0xA0** (или 0x1B0 при NTB-раскладке — выбор по F_97c98
   «REVCLASS == 0x060000, мост присутствует»); 16-бит RMW:
   `and 0xffaf` (сброс 0x50) + **OR 0x40 при флаге≠0**, запись, delay 2ms.
3. **Живое подтверждение** (setpci с хоста, BDW+v17): 00:01.0 /
   00:02.0 / 00:03.0 читают `0xa0.w = 0x0c40` — **бит 0x40 стоит у
   всех видимых** портов; 2B/2C/2D = FFFF. Поле 0xA2[9:4] у 2A = 4
   (x4 lanes — железо знает о бифуркации). 0x190: 1A=0001, 2A=0000,
   3A=0004 (=IOU0 x4x4x4x4). UBOX 0x80=7 (clr_hdrmfd — декорация),
   UBOX 0x304=0. Т.е. бит 0x40@RP+0xA0 = «функция опубликована».
4. **Источник флагов** cfg+0x2bc (0x2bc < 0xc7d): это префикс
   policy-структуры 0x1670 байт, которую UncoreInitPeim получает
   через LocatePpi(**2AB86EF5-ECB5-4134-B556-3854CA1FE1B4**)->method0
   (F_97ec2; второй GUID EC87D643…=ID под-policy). PPI ставит/форвардит
   **NVRAMPei** (FV12 file 12/45; дескриптор {flags=0x10, GUID,
   iface=0xffe62e80}; второй PPI 3CDC90C6-13FB-4A75-9E79-59E9DD78B9FA;
   method0=форвард к менеджеру, method2=broadcast-цепочка 0xffe63eac).
   RC-дефолт той же структуры — константа в PE (VA 0xffe70ec0,
   0xc7d байт): **+0x2bc = все 0x01 — «публиковать все 44 функции»**.
   Сокрытие 2B приходит из NVRAM-политики поверх RC-дефолта.
   Побайтовый паттерн живых флагов `01 01 00 01 00 00 00 01 00 00 00`
   в образе НЕ найден → флаги вычисляются при сборке NVRAM-записи
   (по дефолтной конфигурации IOU = x16 → только fn0), а не лежат
   статикой. Меню «PCI-E Port 2B=Enable» (v15) и чекбоксы наличия
   (v16) в этот расчёт не входят — потому и не помогали.
5. Лейн-механика вокруг: F_91c48 (IOU-селекторы 0x248/0x24c/0x250 →
   fn-таблицы VA 0xffe750f3/0xffe750f5/0xffe75105 → F_91b8b);
   F_90d81 (лейн-таблицы VA 0xffe755d8/0xffe755dc: IOU0 sel0 =
   {04,04,04,04} → cfg+0xe66+sock*11, 11 портов = DMI+1A/1B+2A..2D+3A..3D);
   F_96a0f (копирует дефолтную BDF-таблицу + живой PCI-проб vendor
   0x8086 → гейты cfg+0xe0b/e37); F_91ceb (конфигуратор порта,
   гейт на e0b/e37 — но **F_918f8 эти гейты НЕ проверяет**).
6. **Вывод**: единственный гейт публикации 2B — флаг в NVRAM-политике.
   Патч F_9182d на безусловный 0x40 = восстановление reference-семантики
   (RC-дефолт «публиковать все»), побочный эффект — публикуются и
   мёртвые по лейнам 1B/3B/3C/3D (пустые RP в lspci, косметика).

**v18 ГОТОВ: `refs/amibcp/450x-native-v18-publish-all.bin`** =
v17 + ровно 11 байт @image 0xe918aa (внутри 12/47/1 nat-uncore):
`0f b6 4d 08 f7 d9 1b c9 83 e1 40` (movzbl 0x8(%ebp),%ecx; neg; sbb;
and $0x40) → `b9 40 00 00 00 90 90 90 90 90 90` (mov $0x40,%ecx; nops).
sha256 `764b899e6f223f729bb04e2ac251c583626f0997b0661e4c8e3c7369701f
df85`. Линейдж v14 (NOPs 0xf20fb3/0xf20fbc) и v15/v17 проверен.
**Вердикт после TMM-прошивки**: `lspci | grep 00:02` → ждём
00:02.0-3; затем NVMe на райзере. Если 2.1 появился, но hdrtypectrl
врёт (multifunction) — clr_hdrmfd чистится с хоста (§L5), это
декорация. Если НЕ появился — бит 0x40@0xA0 не гейт, следующий
кандидат: DEVHIDE-писец в QPI-пространстве (dev8, NDA) — искать
писателей в см-диффе nat/sm.

## O. v18 в железе: publish недостаточен; вскрыт самозамок presence; v19 (2026-09-16)

### O1. Вердикт v18 — отрицательный, но информативный

Хозяин вшил v18 (TMM), F9 отработал (диалог жив — клавиши не съедены).
Первичный срез (ssh, BDW E5-2640 v4): **00:02.1 НЕ появился**; селектор
`2A.0x190 = 0000` → IOU0=x4x4x4x4 активен (пресет из переменной пережил
TMM+F9 — «F9 поднимает печёные» из v17); 2A x4 Gen3, PM983 за ним;
SM2263 на 3A; UBOX `00:05.0+0x80 = 0x07` по-прежнему.

Живые эксперименты (Bazzite-хост, `iomem=relaxed` в ostree-BLS
`/boot/loader/entries/ostree-*.conf` + ребут — grubby в immutable-ОС нет):

- **Прямой ECAM-проб /dev/mem** (MCFG: ECAM=0x80000000, bus 00-ff):
  2A fn0 = `6f048086` ✓ (контроль), 1.1/2.1/2.2/2.3/3.1 = **FFFFFFFF**
  → декод функций ≠0 у dev1/2/3 закрыт на уровне ECAM (master abort).
  NB: bulk-dd 4096 по ECAM читает FF (burst-чтения = невалидные
  конф-доступы) — только поштучные выровненные dword (python+mmap /dev/mem,
  `r+b` — MAP_SHARED на ro-fd даёт EPERM).
- **hdrmfd-чистка + MF + rescan**: 0x80:=0 (держится, SMM не переписывает),
  у 02.0 бит MF уже стоит (0e.b=0x81), `rescan` — пусто → hdrtypectrl
  не гейт декода (подтверждение §L5, теперь без лазейки «сканер не
  трогал sibling-функции»).
- **dev8/dev9/dev10 (QPI-агенты) из ECAM не читаются вовсе** (FF) —
  сами скрыты; даташит (Vol.2, SLTSTS-примечания ×2) DEVHIDE только
  упоминает: «BIOS must hide all unused RPs … via the DEVHIDE register
  in Intel QPI Configuration Register space», офсет NDA.
- **Приватные PEI-окна мертвы в рантайме**: литералы `0xb004xxx`
  (dev8+0x79/0x7c/0xb4/0xc0/0xd0/0x104/0x10c/0x180..0x1a8/0x300; окно C:
  `0xc004544`) в рантайме читают FF — это PCIEXBAR-времянки PEI-фаз,
  потом ECAM перепрограммирован на 0x80000000.
- **UBOX dev5.0 полная карта** (живой дамп 4KB): подтверждает
  идентификацию по даташит-таблице §6.6 — hdrtypectrl@0x80=7,
  mmcfg_base@0x90=0x80000000, genprotrange1@0xb0=0xfed10000,
  cipintrc@0x148=0x44444440 (прерывания, НЕ presence), vtuncerrptr@0x1b0;
  UBOX SM-блоки 0xfed10000/15000/18000 не декодируются. Гейта декода
  в UBOX-конфиге нет.
- CAPID2/3 (PCU bus1 dev30, SKU-фузы «PCIE_DISXPDEV») — RO_FW, не наш
  случай (порты тренируются).

### O2. Статика: полный разбор оркестратора + самозамок presence

Оркестратор F_91acc (вызывается из 0xffea7f12 рядом с F_91b1c):
`F_956e4(принт) → F_918f8(publish 0xA0) → F_95681 → F_95f19 → F_9196c →
F_956e4`; отдельной цепочкой F_91b1c: `F_91ceb → F_93ca8 → F_90e48`.
Дочитано всё, что не раскрыли в §N:

- **F_9182d перечитан**: офсет = 0xA0 + (0x110 если F_97c98≠0) →
  скрытому порту RMW идёт именно в 0xA0 (0x1B0 — NTB-раскладка для
  видимых). v18 целился верно.
- F_9450d: пишет 0x40 в RP+0x208 **всем 11 портам без гейтов** —
  XPUNCERRSTS (статус ошибок), не visibility.
- F_948c3/F_95f19: VT-d/interrupt/стековые регистры (0x4f0/0x5e4,
  dev6fn7+0x3c4/0x450, RP+0xB18), не visibility.
- **F_9196c** («писатель 0x160» из анкера №3): 11-портовый цикл по
  скрытым портам (скип при F_97c98≠0), тумблеры битов 0x40/0x100 в
  RP+0x160 по пер-функциональной политике cfg[0xc7d+fn-индекс]
  (fn+{1,3,7} по девайсу, значения 0/1/2). RP+0x160 = ERRCAP (AER),
  статический дефолт таблицы — все 00 → функция-нооп. Не гейт.
- **F_96a0f (пробер) расколот полностью**: memcpy дефолтной BDF-таблицы
  (VA 0xffe70e68, все 44 функции) в cfg+0xd1c/0xda3/0xda4, затем
  11-портовый цикл: read16 VID@ECAM → `cmp 0x8086` → miss ⇒
  `cfg[0xe37+sock*11+port]=0`; hit ⇒ `cfg[0xe0b+...]=1`. Флаги
  предустановлены 1 → присутствие = «VID читается».
- **САМОЗАМОК**: скрытая функция → зонд FFFF → e37/e0b=0 → F_91ceb
  (единственный пер-портовый конфигуратор: RP+0x39c/0x3f0 RMW,
  двойной гейт e0b∧e37) скипает порт навсегда. v18 publish этот замок
  не трогает. Потребители e0b/e37 в PE — ровно четыре (F_91ceb,
  F_937f2, F_93ca8, F_96a0f) — перечислены исчерпывающе.
- **Донорская развилка**: в sm-uncore VID-зонда НЕТ ни в одной
  кодировке (66 3d 86 80 / 3d 86 80 00 00 / 81 f9 … / b9 86 80 00 00
  / 68 86 80 00 00 — все пусто), маска публикатора `b9 af ff 00 00
  66 23 c1` — есть (sm@0x1a528). RC общий, presence-механика
  версионно другая; леновин пробер и держит замок.
- Писца DEVHIDE в nat-uncore НЕТ: все потребители policy-данных
  перечислены (0x2bc→только F_918f8; e0b/e37→4 сайта; 0xc7d→F_9196c),
  ни один не пишет dev8. В 23 карвленных PEI-модулях FV12 литералов
  0xb00xxxxx нет. Если v19 не хватит — писец в DXE (декомпресс FV8
  движком) или hide = «декод по умолчанию выключен, F_91ceb его
  включает».

### O3. v19 = v18 + форс presence (5 байт)

`hack/uncore_presence_force.py`: F_96a0f @image 0xe96a8a
`74 05 c6 03 00 eb 04` → `90 90 c6 03 01 90 90` — je/jmp вырезаны,
`movb $1,(%ebx)` (e37) и `movb $1,-0x2c(%ebx)` (e0b) выполняются
безусловно, сравнение VID остаётся (флаги больше не используются).
Артефакт **`refs/amibcp/450x-native-v19-presence-force.bin`**, sha256
`b9adce5038fe785fb658bb2ca3883d1ff938180c54e94c26eb5603a5aac351e2`,
дельта от v18 ровно 5 байт @ {0xe96a8a, 0xe96a8b, 0xe96a8e,
0xe96a8f, 0xe96a90}; publish v18 (@0xe918aa) на месте, --check
обоих скриптов зелёный. Риск: presence=1 и для лейн-мёртвых портов
→ лишние конфиг-записи/таймауты (те же 180ms-ожидания F_937f2/F_93ca8),
косметика.

**Вердикт после TMM v19**: `lspci | grep 00:02` → 00:02.0-.3;
setpci 2.1; NVMe на райзере. Развилки при неудаче: (a) писец DEVHIDE
в DXE — декомпресс FV8 движком, греп 0xb004xxx/паттернов RMW;
(b) декод-включатель внутри F_91ceb (0x39c/0x3f0) не срабатывает —
дизассмлить ветки F_96403/cfg+0x448; (c) SMM-переписывание. Хост
остаётся с `iomem=relaxed` (для живых проб) — убрать при желании:
удалить аргумент из обоих ostree-*.conf.

### O4. v19 в железе — отрицательный; DXE-писец найден: IioInit (2026-09-16, ночь)

Хозяин вшил v19 (TMM), флеш дефолты НЕ пересеил (OpROM жив, Quick Boot
нет) — ребут + явный F9 через SOL (вход в Setup по экранному промпту
**Ctrl+S**, не F1; F9=ESC+9 → Enter(Yes); F10=ESC+0 → Enter; дефолты
поднялись: селектор 2A.0x190=0000). Вердикт: **00:02.1 нет, сырой ECAM
2B/2C/2D/1B VID = FFFF** — presence-форс декод не открыл (замок был
реален, но F_91ceb — не декод-включатель; его «вечный скип» на входе
оказался «скип порта 0 (DMI)» — цикл 11 портов с хвостом-инкрементом
на ffe9212d).

**DXE-охота движком** (движок поднят: чистый UEFIPATCHER_DATA +
hand-made клиентский стейт .uefipatcher — token из sqlite движка,
т.к. `session init` тонет в RPC_INTERNAL после создания сессии;
артефакты FV8: 207 PE32-модулей в /tmp/dxe-scan). Паттерн-скан по
всем модулям (LE-паттерны! первый скан с 0x0b-суффиксом был мусором):

- **8/17 = IioInit** (GUID 63809859-F029-41C3-9F34-EEEB9EA787A5,
  x64) — единственный с литералом 0x28080 (ECAM-адрес UBOX
  hdrtypectrl). Функция @0xd2d0: per-socket цикл (cfg[0xd18]),
  per-port цикл 11: **гейт `cfg[0x8cb+sock*11+port]==0 → скип**
  (je @0xd3cf)**; внутри — маска-аккумулятор (бит на порт с флагом 0)
  → **запись байта в UBOX+0x80** (hdrmfd — та самая 0x07, «декорация»
  по §L5/O1), плюс записи dev4fn3+(0xb0+fn*4) и dev7fn3+(0x40+fn*4)
  |= {1,2,4,8} — CBDMA/CTLE-сантехника (dev6/7 = IIO MEM/PHY
  CTLE-пространство, даташит §6.12). Декод-гейта в функции нет.
- **Писатель cfg+0x8cb = 1: IioInit @0xe1ef** — только при
  `cfg[e37+sock*11+port]≠0` + cfg+0x448≠2 + вызовы 0x121c8/0x1624c≠1
  (e157-e1ff). Т.е. e37-presence из PEI течёт и в DXE… но v19 форс
  e37=1 декод не открыл ⇒ либо 0x121c8/0x1624c режут 2B, либо 0x8cb
  для 2B пишет не этот путь, а NVRAM-политика (PEI-мемкопы cfg+0x8cb,
  ~11 lea-сайтов ffe96db2-ffe977ad; статический RC-дефолт +0x8cb —
  **все 00**, инверсия к publish +0x2bc все 01).
- Второй потребитель: IioInit @0xbecf — инвертированный гейт
  (`0x8cb≠0 → скип`) + `cfg+0xe63 (лейны)≠0` → ветки вызовов
  0x13c04/… — кандидат «второго шанса» для скрытых-с-лайнами.

**Незакрытое:** регистр, держащий ECAM-декод сабфункций RP, всё ещё
не найден (UBOX-конфиг исчерпан, QPI dev8/PHY dev7 из рантайма
недостижимы, окна PEI мертвы). Кандидаты на v20: (a) NOP je@0xd3cf —
обрабатывать все порты в IioInit-сантехнике; (b) разобрать
0x121c8/0x1624c (что режет e1ef для 2B); (c) вызовы ветки 0xbecf;
(d) полный аудит IioInit (100KB) на другие ECAM-записы; (e) PciBus
DXE. Патч в сжатой секции FV8 — нужен write-режим движка
(репак GUIDed-LZMA, как в NVAR-циклах v15-v17).

### O5. v20 собран: ложный DL_Active расколот, доноры-дифф, write-режим движка уже работает (2026-09-16, вечер)

**Разбор (b) — резак найден.** `0x121c8` = предикат «IsDlActive»: собирает
ECAM(bus,dev,fn) из упаковки {b1=fn,b2=dev,b3=bus}; при dev∈{1,2,3}
сканирует соседние функции (VID≠FFFF + LNKSTA[0xA2] бит 13 → «занято»);
затем фолбэки-классы выбирают регистр (0x161ec: class 06:00:00 → 0x1B2;
0x1624c: class 06:80:00 → 0x1A2; дефолт 0xA2) и возвращает
`(WORD>>13)&1`. **Для скрытого порта ECAM читает 0xFFFF → (0xFFFF>>13)&1
= 1 → «линк активен» ложно** — writer @0xdff4 срезает установку
`cfg[0x8cb+sock*11+port]=1` (je @PE+0xe1ba) → d2d0-гейт (je @0xd3cf)
скипает сантехнику. `0x1624c` сам по себе скрытых не режет (класс FFFF ≠
06:80:00). Это зеркальный DXE-аналог PEI VID-зонда из §O.

**Разбор (c) — ветка 0xbecf = основной init-путь RP.** Содержащая функция
0xbd10: внешний цикл по ширинам r15b∈{1,2,4} × 11 портов; гейты порта:
presence cfg+0xe37≠0, найден сокет по bus (cfg+0xd1c), не host-bridge
(0x161ec), **0x8cb==0**, лейны cfg+0xe63≠0 → свитч ширины: x1 → 0x14180 +
RP+0x94 (бит 5 по политике cfg[0x990+sock]) + RP+0xB4 (бит12=1, бит13=0,
бит5 для dev0fn0) + 0x129d8 + 0x13420; x2 → 0x13420+0x12bc0+0x138fc;
x4 → 0x13c04 (COMMAND@0x4 биты PERRE/SERR#, RMW 0x92/0x9c/0xA4,
LNKSTA→cfg+0xebb(ширина)/0xee7(скорость)). Т.е. после v19 (presence) и F9
(лейны) этот путь **уже выполняется для 2B** — но все его записи идут в
ECAM самого скрытого 00:02.1; writer ставит 0x8cb=1 только пустым
не-06:80 портам, видимые с лейнами идут сюда же. DMI (host-bridge класс)
обрабатывается отдельным путём c0cd (RP+0xF0/0xE4/0xF8).

**Разбор (d) — аудит всех ECAM-записей IioInit** (хелперы 0x914c DWORD /
0x918c WORD / 0x91d0 BYTE, mmcfg@g cfg[0x168a0]): RP+{0x4,0x8,0x18-0x1A,
0x2C/0x2E,0x3D,0x40,0x44,0x46,0x50,0x61,0x62,0x91,0x92,0x9C,0xA0,0xA2,
0xA4,0xAA,0xB0,0xB4,0xB8,0xBC,0xC0-0xCC,0xE4,0xF0,0x116,0x11C,0x12C,
0x130,0x14C,0x160,0x18C,0x190,0x1B0,0x1B2,0x208-0x228,0x248,0x268,
0x288-0x294,0x300-0x320,0x39C,0x3CC,0x3F0-0x3F4,0x428-0x4C4,0x4E0,
0x4F0,0x504,0x600,0x858,0x870-0xB40}; UBOX(0x37xxxx)+{0x308-0x330,
0x3C4,0x400-0x40C,0x43C,0x450,0x524,0x5A8,0x608,0x638,0x640,0x648,
0x668,0x670,0x678}; dev5+{0x80,0xA4,0x180,0x1C0,0x800,0x808,0x82A};
dev3fn0+{0x18-0xD6}; dev5fn4+{0x4,0x2C,0x2E,0x40}; dev5fn2+{0xB0,0xB4};
CBDMA(0x87xxx)+{0x000,0x05C,0x0B0,0x0B4,0x0C0,0x0C4,0x0CC};
0xF3000/0xF3040, 0xFA000. **Главное: publish-бит RP+0xA0.6 в DXE не
пишется нигде** — все три писателя 0xA0 (0x9030/0xF634/0x12Fxx) ставят
бит 0x20 (retrain) в функциях ожидания трейнинга.

**Донор-дифф: IioInit не OEM-кастом.** Из X10DRH1_816.bin и 超微450.rom
извлечены донорские IioInit (python-экстрактор: FFS по GUID → GUIDed-LZMA
→ PE32; `refs/amibcp/iioinit-{x10drh,sm450}.pe` + native). Writer
доноров семантически идентичен (0x8cb/0x448/BDF-таблица 0xDA3/0xDA4 те
же; presence 0xF97, bus 0xE7C, второй флаг 0xAC4 — сдвиг версии
policy-структуры); хвост DL_Active так же хрупок (есть во всех трёх
сборках). Вывод: на доноре логика корректна только потому, что порты
ВИДИМЫ и LNKSTA читает реальные значения — пустые порты получают
0x8cb=1 и полный прумблинг.

**v20 = v19 + NOP je @IioInit-PE+0xE1BA (2 байта `74 45`→`90 90`)** —
writer перестаёт доверять ложному DL_Active скрытых портов; семантика
донора («нет функции = линк неактивен») восстановлена. Ожидаемые эффекты:
0x8cb=1/0x964=1 для 2B/2C/2D → d2d0-сантехника (UBOX+0x80 биты, CBDMA
dev+0xB0+fn*4, CTLE dev+0x40+fn*4). Сборка движком: `node replace
8/17/1/0 --artifact <PE> --body-only` + `image save` — **write-режим
движка (репак GUIDed-LZMA) уже был реализован** (builder::
build_recompressed_guided + compress_lzma_fit с бюджетом секции;
догадка анкера №4 «нужен write-режим» устарела — нужен был правильный
флоу: replace декомпрессированного ребёнка, не самой guided-секции).
Дельта v20←v19: 36006 байт, строго внутри payload секции
(0x8C6898-0x8CF5E8), заголовки FFS/секции/FV нетронуты; PE-дельта vs
native = ровно 2 байта (0xE1BA/0xE1BB), проверено lzma-rs (engine
extract) и python (roundtrip). Независимый путь воспроизведения:
`hack/iioinit_plumbing_patch.py` (python-lzma, alone-заголовок с точным
usize — python пишет -1, EDK2 требует точный; пэд нулями до исходного
размера секции). `refs/amibcp/450x-native-v20-iioinit-plumbing.bin`,
sha256 84bad780550b99dd0c0ea7c849c4c7ae6a946829588c25942d48401e6303d678.
ЖДЁТ TMM.

**Развилки вердикта.** Если 2B не появился — декод-гейт вне IioInit
(все его записи — собственное пространство порта + UBOX-декор, аудит
исчерпан): следующий шаг (e) PciBus DXE (паттерн-скан 207 модулей
/tmp/dxe-scan на литералы 0x28080/ECAM-механику) и SMM-модули; если
появился, но multifunction-бит врёт — clr_hdrmfd с хоста (§L5). Если
2B появился и NVMe на райзере виден — охота закрыта (SOL-вердикт как в
§N: `lspci | grep 00:02`).

### O6. v20/v20b в железе — оба DXE-вис; диагнозы найдены статикой, v20c готов (2026-09-16, ночь)

**v20 (NOP je @0xe1ba) — вис.** Два бута (мягкий ребут + жёсткий цикл):
POST-трейс обрывается после `PeimMemoryQpiInit END`, VGA чёрен, SOL
молчит (DXE тихий при Debug=Normal), ОС не поднимается. Диагноз
статикой: NOP срезал путь не только скрытых, но и **тренированных**
портов (2A с NVMe, 3A): те перестали попадать в ветку width-масок
@e201 → у них сработала запись 0x8cb=1/0x964=1 → ветка 0xbd10/becf
(«0x8cb==0 && lanes≠0») — их основной init-путь — не выполнилась вовсе
(RP+0x94/0xB4/COMMAND/линк-конфиг не запрограммированы) → зависание
downstream. На доноре trained-порты всегда идут через e201 (реальный
DL_Active=1) и 0x8cb не получают. d2d0 дочитан полностью — вечных
ожиданий нет (чистые записи), не он.

**v20b (пещера в хвосте 0x121c8) — тоже вис.** Диагноз: пещера легла в
файловый slack .text за **VirtualSize** (vsize=0x17748, пещера
@0x179C8 = 0x280+0x17748 — первый байт за границей). DXE PE-загрузчик
мапит секцию по VirtualSize → в рантайме пещеры нет, `jmp` уходит в
нулевую память → вис. Урок: **кодные пещеры — только внутри vsize или
с раздуванием vsize**.

**v20c = v19 + [VirtualSize .text 0x17748→0x17760 (=raw, ровно до
vaddr следующей секции 0x179E0, SizeOfImage не меняется)] + jmp@cave
(тот же, что v20b)** — 29 байт дельты PE, собран движком, roundtrip
сверен (lzma-rs + python). `refs/amibcp/450x-native-v20c-cave-mapped.bin`,
sha256 046b50779eec4f388c724a3ea634cd9fd00f9361cb80fef7762aa7a7ddd65e01,
`hack/iioinit_dlactive_cave_patch.py`. Семантика: trained — сток-путь
как у донора, скрытые — 0x8cb=1 (сантехника d2d0). ЖДЁТ TMM.

**Попутное:** SOL-риг переписан на pty-мост (`hack/solrig.py` —
ipmitool требует TTY, tcgetattr); BMC-зомби сессии лечатся `sol
deactivate` / `mc reset warm` (сработало вживую). Ориентир входа в
Setup — `Press <F1>` (F1=ESC+1), Ctrl+S-промпт = OptROM сетевух и после
F9 исчезает (поправка владельца). F9 после флеша обязателен —
самопересева печёных дефолтов флешем не наблюдается (наблюдение
владельца). AGENTS.md дополнен разделом «Работа с IPMI/SOL».

### O7. v20c в железе — СТАБИЛЬНЫЙ БУТ, но 2.1 не открылся: IioInit исчерпан (2026-09-16, ночь)

TMM-флеш v20c → жёсткий цикл → POST прошёл ДО КОНЦА (впервые с DXE-патчем:
OpROM сетевух исполнился, меню, ОС поднялась). SOL-процедура F9 (итог
итераций входа): **вход в Setup = спам ESC+1 (F1) каждые 0.3с через хвост
POST до появления «Aptio Setup Utility»** (подсказка хозяина; DEL и
одиночный Ctrl+S по таймингу не сработали — Ctrl+S в окно NIC-промпта
открывает меню сетевухи, выход ESC). Далее ESC+9 (F9) → «Load Optimized
Defaults? » → Enter → ESC+0 (F10) → «Save configuration and exit?» →
Enter → ребут.

**Вердикт (после F9, ОС):** `lspci 00:02` = только 00:02.0; сырой ECAM
(@0x80000000): 1A/2A = 0x8086, **1B/2B/2C/2D = 0xffff**; NVMe: PM983
(03:00.0) + SM2263 (04:00.0) на месте; `2A.0x190=0000` (x4x4x4x4 жив);
UBOX+0x80 (00:05.0+0x80) = 0x07 (не изменился). **Декод-гейт НЕ в
IioInit** — как предсказывала развилка §O5. Сантехника d2d0 скрытым
портам доставлена (0x8cb=1 по предикату-кейву), бут стабилен, машина в
строю — v20c можно оставить рабочим образом.

**Итог цикла §O5-§O7:** ложный DL_Active (FFFF→бит13=1) в 0x121c8
расколот и нейтрализован хвост-кейвом; writer/becf/d2d0/0xbd10
прочитаны целиком; доноры подтвердили сток-семантику. Прятание — не
в PEI-publication (v18), не в presence (v19), не в IioInit-DXE
сантехнике (v20c). **Следующий фронт по плану: (e) PciBus DXE**
(паттерн-скан 207 модулей /tmp/dxe-scan: ECAM-механика, литералы
0x28080/hdrtypectrl, multi-function enumeration) и SMM-модули; туда же
кандидат — потребитель cfg[0x112]/cfg[0x964] (гейты writer'а с
неизвестными значениями для 2B).

## O8. Фронт (e): DXE чист, шина 0xFF вскрыта, ТРЕТИЙ ЗАМОК cfg+0x448; v21 (2026-09-16, вечер)

### O8.1. DXE-фронт закрыт за один заход

Карта имён всех 207 PE32 из /tmp/dxe-scan (UI-секции tree19):
PciBus=8/117, PciRootBridge=8/41, PciDxeInit=8/42, IioSmm=8/150,
CpuCsrAccessSMM=8/148 и т.д. Паттерн-скан по всем: точные литералы
сантехники (0x28080/0x87000/0xf3000) + структурный `shl reg,0x14`
(ECAM bus-шифт) → **единственный модуль, трогающий IIO/UBOX ECAM —
IioInit (86 shl14)**. Хиты прочих — совпадения (0x87000 в сетевых
стеках, shl14 у PchInitDxe = PCH dev31 0xf8xxx, CpuCsrAccess = CSR-
хелперы). Вывод: **декод-гейта в DXE нет вообще** (IioInit исчерпан
v20c), SMM-модули IIO-ECAM тоже не пишут.

### O8.2. Живые тесты и археология реестров

- **UBOX+0x80 := 0x00 из Linux** (setpci, v20c-хост): запись
  принимается, lspci не меняется, восстановлено 0x07 → hdrtypectrl
  точно не гейт (подтверждение §L5/O1).
- **DID уточнение**: RP=6F04, UBOX=6F28 → платформа Broadwell-EP
  (E5 v4), не v3.
- **Полный ECAM-скан всех 256 шин × dev × fn** (mmap, поштучные
  выровненные dword): живые шины 0,1,3,4,6,7 + **0xFF**. Шина 0xFF =
  CPUBUSNO(1) «Processor Uncore Devices» (E7v4/E5v4 §1.1.2):
  dev11 = R3 QPI Link 0/1 (6F36/6F37/6F81) + QPI Debug (6F76),
  dev12-15 Cbo, dev16 = R2PCIe+Ubox, dev18 HA, dev19-23 IMC,
  dev30-31 PCU. Полный дамп 62 фн → /tmp/busff.json. Точных
  bitmap-паттернов скрытых {1B,2B,2C,2D,3B,3C,3D} (0x27c/0x488/
  per-dev 02/0e) в дампе нет; **dev8-10 (QPI-агенты) сами скрыты** —
  туда DEVHIDE и «спрятан» (chicken-egg для ECAM-доступа).
- **Даташиты скачаны** (CDRV2/Wayback): E5 v4 Vol.2 (док. 333810),
  E7 v4, Xeon D-1500. Подтверждения: скрытие = master abort (FFFF);
  hdrtypectrl@UBOX+0x80 clr_hdrmfd «Bit0=Dev1, Bit1=Dev2, Bit2=Dev3»
  (наше 0x07 = «все три девайса одиночно-функциональные», косметика
  при скрытых фн); таблицы 6-2..6-4 «Function Number of Active Root
  Ports based on Port Bifurcation»: при x4x4x4x4 активны фн 0-3 —
  железо создаёт функции, прячет их только DEVHIDE-решение RC.
  Сам DEVHIDE (офсет в QPI-конфиг-пространстве dev8-10) = NDA, в
  публичных Vol.2 только QPIMISCSTAT (dev8 fn0 +0xD4).
- QPI-термальная сантехника nat-uncore (RMW dev8+0xb4 биты 1/5,
  цикл 4× в dev8+0x544 с маской 0xff807fff, Tcontrol/лимиты
  90/100/185°C) — не наш писец. cfg+0xc8f/0xc90 = QPI-link presence
  (гейт CSR-обёрток F_f0699a/F_f06a0d), не IIO-порты.

### O8.3. ТРЕТИЙ ЗАМОК: cfg[0x448+port]==2

Триаж всех 11-портовых функций (`imul $0xb`) + хелперов
(F_93f17/30/62/76 → F_96465/9649b/96509/9653e = RP-регистровые
R/W; F_92148/F_9227c = R/W селектора RP+0x190 через BDF-таблицы
0xd1c/0xda3/0xda4/0xdaa):

- **F_91ceb** (конфигуратор, RP+0x39c/0x3f0 RMW — подтверждено
  `or $0x39c`/`or $0x3f0`): гейт-каскад = e0b (v19✓) → e37 (v19✓) →
  **`F_96403(...)==1 && cfg[0x448+sock*11+port]==2 → je ffe9212d`**
  (хвост цикла, скип порта). Третий замок v18/v19 НЕ тронут!
- F_9642c (предикат F_937f2/F_93ca8 — post-training цепочки
  F_91b4b→F_95681): cfg[0x448] 0→override-таблица, 1→true,
  **2→false** (порт выключен).
- Второй cmpb-сайт @F_94c5x (ffe94c65, вызывается из F_948c3).
- **RC-дефолт массива** (константа политики PE VA 0xffe70ec0):
  0x448 = все 00, 0x2bc = все 01 (reference-семантика «включить
  всё»). Живые значения строит мост переменных: **LoadHob (FV12
  12/16)** читает Setup/IntelSetup (GUID EC87D643-…) и референсит
  PPI 2AB86EF5; **NVRAMPei (12/45)** читает MfgDefaults/StdDefaults/
  Setup/IntelSetup/IpmiCmosClear. NVRAM-магазин = FV @0x800000
  (NVAR, file CEF5B9A3): переменной IntelSetup НЕТ (только Setup с
  данными и удалённый StdDefaults) → запись политики вычисляется
  при буте из полей AMI-Setup (совпадает с §N.4). Писца 0x448
  литеральным displacement в PE-модулях нет (скан всех FV12:
  только nat-uncore, 3 чтения) → пишется через вычисляемый
  указатель/структурный memset где-то в мосту.

### O8.4. v21 = v20c + 3 байта (трилока нейтрализована)

Проверка базы: **v20c уже содержит v18 (0xe918aa = b9 40…) и v19
(0xe96a8a = 9090 c6 03 01…)** — лейндж полный, v21 = три иммедиата
`cmp $2 → cmp $0xff` (сравнение никогда не истинно, скип-ветка
мертва):

| image | VMA | инструкция |
|-------|-----|------------|
| 0xe91d7c (imm@+6) | 0xffe91d76 | `cmpb $0x2,0x448(%edx)` — F_91ceb |
| 0xe94c6c (imm@+7) | 0xffe94c65 | `cmpb $0x2,0x448(%eax,%esi,1)` — F_94c5x |
| 0xe96460 (imm@+1) | 0xffe9645f | `cmp $0x2,%al` — F_9642c |

`hack/uncore_trilock_patch.py` (--check зелёный), артефакт
`refs/amibcp/450x-native-v21-trilock.bin`, sha256
`e840efd6b0e3df5c92b44bc734dbee9ff219942ac051610393c51a9bb73df119`,
дельта от v20c ровно 3 байта. Риск: конфигуратор прогонит и
политически-выключенные порты (переопределения применятся) — тот же
класс косметики, что у v18/v19. Гипотеза: если F_91ceb (или его
publish-соседи) — декод-включатель, 2B откроется; при неудаче
писец DEVHIDE в pre-mem MRC-части nat-uncore (адрес вычисляемый,
без литералов) — следующий фронт.

## O9. v21 в железе — отрицательный; три замка сняты, декод закрыт всё равно (2026-09-16, ночь)

Хозяин вшил v21 (TMM). Power cycle по BMC (мост умер ожидаемо,
перезапуск + `sol deactivate`; примечание: мягкий ребут из ОС SOL
переживает — экономит танцы). Бут: POST прошёл, ОС поднялась
(медленно: фоновый спам ESC+1 поверх plymouth устроил консольный
шторм 3.6MB — косметика; вывод: спам-луп гасить СРАЗУ после входа,
иначе F1 переоткрывает General Help после каждого Enter).

Вердикт-цикл: вход в Setup (спам ESC+1), General Help → Enter,
F9 (ESC+9 → «Load Optimized Defaults?» Yes → Enter), F10 (ESC+0 →
«Save configuration and exit?» Yes → Enter), ребут, ОС:

- `lspci`: только 00:01.0/00:02.0/00:03.0 (+ за ними PM983 03:00.0,
  SM2263 04:00.0 — оба NVMe живы);
- сырой ECAM: 1B/2B/2C/2D/3B = **FFFFFFFF** (контроль 1A/2A/3A ✓);
- UBOX+0x80 = 0x07; 2A.0x190 = 0 (x4x4x4x4-дефолт пережил F9 ✓);
- incidental: «Processor 2 Version: Not Present» — риг 1P (шина
  0xC0/sock1 никогда не релевантна).

**Итог v21:** третий замок (cfg[0x448]==2) реально срезан, но декод
не открылся → F_91ceb (RP+0x39c/0x3f0) — не декод-включатель. Полный
касад «publish (v18) + presence (v19) + policy-skip (v21)» исчерпан:
**после-памятная цепочка nat-uncore к декоду отношения не имеет.**
Писец DEVHIDE — в pre-mem MRC-части nat-uncore (адреса вычисляемые,
литералов нет — потому все литеральные сканы молчали), либо в ещё
более раннем коде SEC/temp-PCIEXBAR-инициализации.

**Следующий фронт:** (f) pre-mem MRC — трассировка всех ECAM/CSR-
записей ранней фазы (вход через точки, где RC программирует
CPUBUSNO/PCIEXBAR — там же рядом и DEVHIDE-программирование);
донор-дифф sm/nat по writer-множеству в этой зоне. Запасной вариант —
SMBus/PECI-вид (даташит E5v2: скрытые DEVHIDE-устройства видны из
SMBus/PECI-порта IIO) — живой дамп QPI-пространства с хоста.

v21 = новый стабильный рабочий образ (в строю, линейдж v17+v18+v19
+v20c-cave+trilock).
