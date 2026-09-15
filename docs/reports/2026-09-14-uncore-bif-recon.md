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

## B3. EPA-2 (write-ladder) — вердикт владельца
(заполняет Task 9)

## G. Гейт выбора варианта
(заполняет Task 10)
