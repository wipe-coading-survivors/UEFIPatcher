# Serial Console S1 — сборочная инфраструктура edk2 (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Воспроизводимый контейнерный стенд сборки edk2-тройки SerialDxe + TerminalDxe + SerialConsoleGlue (fixed PCD COM1 0x3F8/115200/8N1, terminal VT-UTF8), с валидацией .ffs-артефактов движком и коммитом их как тест-данных.

**Architecture:** Свой мини-пакет `UefiPatcherSerialPkg` (DSC+FDF) собирается против немодифицированного чекаута `refs/edk2` (голова `bcd1687`) через `PACKAGES_PATH`; сборка полностью эпизодична — контейнер копирует edk2 через `git archive` в `/work`, чекаут не загрязняется. Glue-драйвер реализует вариант (а) из §5.3-4 отчёта S0: append devpath в `ConOut/ConIn/ErrOut` + ConnectController + два COM-маркера для атрибуции E30. Артефакты проверяются python-скриптом (FFS/PE-инварианты + двойная сборка на побайтовую воспроизводимость) и интеграционным тестом движка.

**Tech Stack:** podman-remote (fedora:44, существующий `uefipatcher-runtime-base`), edk2 basetools + GCC (нативный gcc X64; tools_def 3.06 удалил GCC5), python3, Rust-крейт uefi-engine (существующие хелперы `ffs.rs`/`parser/file.rs`).

**Spec:** `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` (§3 ступень S1, §7 аддендум) + вход S0-разведки: `docs/reports/2026-09-05-serial-s0-recon.md` §5.2 (PCD-таблица), §5.3–5.4 (маршрут ConOut, ландшафт LIVE), §5.6 (список модулей), §8.2 (конфиг пары).

## Global Constraints

- **Никакой прошивки и никаких BIOS-образов в этой ступени.** Движок не запускается (сокет не нужен). Донорские образы не читаются (всё нужное уже в отчёте S0).
- **BIOS-образы не коммитятся**; `.ffs`-тест-данные коммитить можно (спека §3 S1) — кладём в `crates/uefi-engine/tests/data/serial/` (спека буквально требует `tests/data/`, не `tests/fixtures/`).
- **`refs/edk2` не модифицируется**: в контейнер монтируется `:ro`, сборка идёт в копии `/work/edk2` (git-archive). Никаких коммитов внутри `refs/edk2`.
- **push только на remote `dsevosty`**, один раз в Task 6: `git push dsevosty fix/cycle6-reimplent`. Работа в текущей ветке `fix/cycle6-reimplent`, новых веток не создаём.
- **Container runtime**: среда контейнерная (`/run/.containerenv`), podman-remote 5.8.2 доступен, базовые образы `localhost/uefipatcher-runtime-base:latest` и др. уже собраны (проверено при планировании).
- **Комментариев в коде нет** (кроме ссылок на референс `file:line`); docstring-шапка только у `hack/`-скриптов (по образцу `hack/fv_audit.py`).
- **Правило-11**: расхождение плана с реальностью — отдельный `docs:`-коммит с правкой ЭТОГО плана ДО реализации шага.
- После задач, меняющих Rust/тесты (Task 5): `cargo test -p uefi-engine`, `cargo clippy -p uefi-engine -- -D warnings`, `cargo fmt --all -- --check`.
- Один коммит на задачу, сообщение — из задачи. TDD где применимо (Task 5).

## Решения плана (закрывают развилки, оставленные S0)

### R-S1.1 Glue-вариант: (а) + маркеры; (б) — отложен; (в) — отклонён

Развилка §5.3-4 отчёта S0 / §8.2: **(а)** мини-DXE с append в `ConOut` до BDS, **(б)** прямое переназначение `gST->ConOut` в ReadyToBoot, **(в)** полный ConPlatform+ConSplitter.

**Решение: (а), усиленное ранним ConnectController и двумя COM-маркерами.**

Обоснование:
- Переменная `ConOut` — стандартная семантика; AMI-эквивалент ConPlatform в LIVE читает её (консольный стек AMI есть под своими GUID: сплиттер 628A497D, GraphicsConsole 43E7ABDD, отчёт §5.4). Append до BDS даёт AMI BDS шанс подключить терминал штатно — двойная консоль (VGA+COM) сохраняется.
- (в) конфликтует с AMI-сплиттером (два ConSplitter дерут `gST->ConOut`) и тяжёл — отклонён (§8.2).
- (б) гарантированно переживает любой BDS, но ворует `gST->ConOut` у AMI-сплиттера (потеря VGA-вывода) и срабатывает только после BDS. Отложен как **контингенси E30**: если E30 покажет «маркеры есть, boot-текста нет» — S2-итерация добавляет swap в существующий ReadyToBoot-колбэк glue (одно правка-место, код колбэка уже будет в дереве).
- Маркеры дают атрибуцию E30 независимо от исхода маршрутизации:
  - `SC-S1 glue: serial console attached (ConOut updated)` — печатается сразу после ConnectController в DXE-диспетчеризации (до BDS): пара жива, UART работает, порт инициализирован;
  - `SC-S1 glue: ReadyToBoot` — в событии ReadyToBoot: терминал жив и после BDS.
  - Нет маркеров вовсе → диспетчеризация/DEPEX/UART-init; маркер 1 есть, маркера 2 нет → терминал отключили по ходу бута (кандидат — CsmDxe §8.5-1); оба есть, boot-текста нет → маршрутизация ConOut сломана в AMI BDS → включаем (б).

### R-S1.2 Устойчивость glue к порядку диспетчеризации

Assumption: в S2 файлы вставляются в FV1 в порядке SerialDxe → TerminalDxe → Glue (хвостовые слоты). DXE-диспетчерчер обходит FV по порядку; glue с DEPEX `gEfiSerialIoProtocolGuid` стартует после SerialDxe; TerminalDxe (без [Depex]) к этому моменту уже зарегистрировал driver binding.

Защита без доп. механики: если терминальный child НЕ найден (TerminalDxe ещё не диспетчеризован или не забиндился), glue не падает — append в переменную идёт по **сконструированному** пути (devpath SerialIo-контроллера + vendor-узел VT-UTF8), а маркер 2 в ReadyToBoot перепроверяет child (к этому моменту BDS уже сделал ConnectAll). Допущение + обе защиты фиксируются в отчёте S1 (Task 6) и переадресуются мини-pre-check S2 (§8.5-2).

### R-S1.3 Файловая структура

| Файл | Ответственность |
|---|---|
| `docker/edk2-builder.containerfile` | Образ сборки: runtime-base + gcc/make/python3/libuuid-devel/git + маркер ENV |
| `docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.dsc` | Платформа: библиотечные инстансы, [PcdsFixedAtBuild] всей §5.2, [Components] |
| `docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.fdf` | FV SERIAL_CONSOLE_FV (3 INF) + [Rule.Common.<ModuleType>] PE32+UI |
| `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.inf` | Модуль-описание glue (DXE_DRIVER, DEPEX SerialIo) |
| `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.c` | Glue: append переменных + ConnectController + маркеры |
| `docker/edk2/build_serial.sh` | Двухрежимный: на хосте оборачивает podman-remote; в контейнере — git-архив-копия edk2 + BaseTools + build + выкладка .ffs |
| `hack/edk2_build_check.py` | Валидатор артефактов: FFS-заголовки/чексуммы, секции, PE machine/subsystem, воспроизводимость двух сборок |
| `crates/uefi-engine/tests/data/serial/*.ffs` | Коммитимые артефакты (тест-данные, спека §3 S1) |
| `crates/uefi-engine/tests/serial_ffs.rs` | Интеграционный тест движка над артефактами |
| `docs/reports/2026-09-06-serial-s1-build.md` | Отчёт ступени: таблица артефактов, sha256, PCD, воспроизводимость, входы S2 |

Фиксированные GUID (новые, вводятся этим планом):
- glue FILE_GUID: `1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43`
- PLATFORM_GUID DSC: `4C9E2B77-3D1A-4F60-9B2E-8A7D40C15E63`
- FvNameGuid: `6B4A19D8-2E75-4C31-B8F0-59D2837A90C4`

Граунд-траст констант проверок (снят с доноров при планировании, MNX @0x987C58/@0x98B0B8): FFS type `0x07` для DXE_DRIVER и UEFI_DRIVER; PE machine `0x8664`; subsystem `11` (EFI_BOOT_SERVICE_DRIVER) для обоих типов модулей; TimeDateStamp `0` (edk2-сборки репродуцируемые).

---

### Task 1: Образ edk2-builder

**Files:**
- Create: `docker/edk2-builder.containerfile`

**Interfaces:**
- Consumes: существующий образ `uefipatcher-runtime-base:latest` (наследование как у engine/gateway).
- Produces: образ `uefipatcher-edk2-builder:latest` с ENV `EDK2_BUILDER_CONTAINER=1` (маркер режима для `build_serial.sh` Task 2).

- [ ] **Step 1: Убедиться, что базовый образ есть**

Run: `podman-remote images | grep runtime-base`
Expected: строка `localhost/uefipatcher-runtime-base   latest   …`. Если нет — `podman-remote build -t uefipatcher-runtime-base:latest -f docker/runtime-base.containerfile docker/` и повторить проверку.

- [ ] **Step 2: Написать containerfile**

Содержимое `docker/edk2-builder.containerfile` (полностью):

```dockerfile
FROM uefipatcher-runtime-base
RUN dnf install -y git gcc make python3 libuuid-devel && dnf clean all
ENV EDK2_BUILDER_CONTAINER=1
WORKDIR /work
```

Примечание (rule-11, обнаружено при выполнении Task 2 Step 4): без `git` в образе контейнерный режим `build_serial.sh` падает на `git -C /src/edk2 archive …` (`git: command not found`); git нужен именно внутри контейнера, т.к. archive-копия делается в container-mode ветке скрипта.

- [ ] **Step 3: Собрать образ**

Run: `podman-remote build -t uefipatcher-edk2-builder:latest -f docker/edk2-builder.containerfile docker/`
Expected: `Writing manifest to image destination` (или COMMIT + ID), exit 0. Возможная проблема: dnf-сеть в контейнере — это первый образ с dnf после runtime-base; если таймауты, повторить (транзиентность registry).

- [ ] **Step 4: Smoke-тест toolchain в образе**

Run:
```bash
podman-remote run --rm uefipatcher-edk2-builder:latest bash -c 'git --version | head -1; gcc --version | head -1; make --version | head -1; python3 --version; echo EDK2_BUILDER_CONTAINER=$EDK2_BUILDER_CONTAINER'
```
Expected: строки версий git/gcc/make/python3 и `EDK2_BUILDER_CONTAINER=1`, exit 0.

- [ ] **Step 5: Commit**

```bash
git add docker/edk2-builder.containerfile
git commit -m "feat(s1): edk2-builder container image"
```

---

### Task 2: Пакет UefiPatcherSerialPkg + сборка SerialDxe/TerminalDxe в контейнере

**Files:**
- Create: `docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.dsc`
- Create: `docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.fdf`
- Create: `docker/edk2/build_serial.sh`

**Interfaces:**
- Consumes: образ Task 1; чекаут `refs/edk2` (голова `bcd1687`, чистый — проверен при планировании; если `git -C refs/edk2 status -s` не пуст — СТОП и rule-11, не «чинить» чекаут).
- Produces: CLI `bash docker/edk2/build_serial.sh [OUT_DIR]` (по умолчанию создаёт `/tmp/serial-s1-out.XXXXXX`), кладёт туда `SerialDxe.ffs`, `TerminalDxe.ffs` (в этой задаче; `SerialConsoleGlue.ffs` добавит Task 3) и `SERIAL_CONSOLE_FV.Fv`. Пакет `UefiPatcherSerialPkg` собирается с `PACKAGES_PATH=<edk2>:<docker/edk2>`.

- [ ] **Step 1: Написать DSC**

Содержимое `docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.dsc` (полностью; PCD-значения — дословно §5.2 отчёта S0, все кроме `PcdDefaultTerminalType|3` — дефолты, пиннинг для воспроизводимости и самодокументации):

```
[Defines]
  PLATFORM_NAME           = UefiPatcherSerial
  PLATFORM_GUID           = 4C9E2B77-3D1A-4F60-9B2E-8A7D40C15E63
  PLATFORM_VERSION        = 0.1
  DSC_SPECIFICATION       = 0x00010005
  OUTPUT_DIRECTORY        = Build/UefiPatcherSerial
  SUPPORTED_ARCHITECTURES = X64
  BUILD_TARGETS           = RELEASE
  SKUID_IDENTIFIER        = DEFAULT
  FLASH_DEFINITION        = UefiPatcherSerialPkg/UefiPatcherSerial.fdf

[LibraryClasses]
  BaseLib|MdePkg/Library/BaseLib/BaseLib.inf
  BaseMemoryLib|MdePkg/Library/BaseMemoryLib/BaseMemoryLib.inf
  DebugLib|MdePkg/Library/BaseDebugLibNull/BaseDebugLibNull.inf
  PrintLib|MdePkg/Library/BasePrintLib/BasePrintLib.inf
  PcdLib|MdePkg/Library/DxePcdLib/DxePcdLib.inf
  DevicePathLib|MdePkg/Library/UefiDevicePathLib/UefiDevicePathLib.inf
  MemoryAllocationLib|MdePkg/Library/UefiMemoryAllocationLib/UefiMemoryAllocationLib.inf
  UefiBootServicesTableLib|MdePkg/Library/UefiBootServicesTableLib/UefiBootServicesTableLib.inf
  UefiRuntimeServicesTableLib|MdePkg/Library/UefiRuntimeServicesTableLib/UefiRuntimeServicesTableLib.inf
  UefiDriverEntryPoint|MdePkg/Library/UefiDriverEntryPoint/UefiDriverEntryPoint.inf
  UefiLib|MdePkg/Library/UefiLib/UefiLib.inf
  IoLib|MdePkg/Library/BaseIoLibIntrinsic/BaseIoLibIntrinsic.inf
  PlatformHookLib|MdePkg/Library/BasePlatformHookLibNull/BasePlatformHookLibNull.inf
  PciLib|MdePkg/Library/BasePciLibCf8/BasePciLibCf8.inf
  ReportStatusCodeLib|MdePkg/Library/BaseReportStatusCodeLibNull/BaseReportStatusCodeLibNull.inf
  SerialPortLib|MdeModulePkg/Library/BaseSerialPortLib16550/BaseSerialPortLib16550.inf

[Components]
  MdeModulePkg/Universal/SerialDxe/SerialDxe.inf
  MdeModulePkg/Universal/Console/TerminalDxe/TerminalDxe.inf

[PcdsFixedAtBuild]
  gEfiMdePkgTokenSpaceGuid.PcdUartDefaultBaudRate|115200
  gEfiMdePkgTokenSpaceGuid.PcdUartDefaultDataBits|8
  gEfiMdePkgTokenSpaceGuid.PcdUartDefaultParity|1
  gEfiMdePkgTokenSpaceGuid.PcdUartDefaultStopBits|1
  gEfiMdePkgTokenSpaceGuid.PcdUartDefaultReceiveFifoDepth|1
  gEfiMdePkgTokenSpaceGuid.PcdDefaultTerminalType|3
  gEfiMdeModulePkgTokenSpaceGuid.PcdErrorCodeSetVariable|0x03058002
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialRegisterBase|0x3F8
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialUseMmio|FALSE
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialBaudRate|115200
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialLineControl|0x03
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialFifoControl|0x07
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialClockRate|1843200
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialRegisterStride|1
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialRegisterAccessWidth|8
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialUseHardwareFlowControl|FALSE
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialDetectCable|FALSE
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialExtendedTxFifoSize|64
  gEfiMdeModulePkgTokenSpaceGuid.PcdSerialPciDeviceInfo|{0xFF}
```

Примечание (rule-11, проверено по .dec чекаута refs/edk2@bcd1687): все `PcdSerial*` объявлены в `MdeModulePkg.dec` под `gEfiMdeModulePkgTokenSpaceGuid` (в т.ч. `PcdSerialRegisterBase/UseMmio/BaudRate/LineControl/FifoControl/ClockRate/RegisterStride/RegisterAccessWidth/UseHardwareFlowControl/DetectCable/ExtendedTxFifoSize/PciDeviceInfo`), а `PcdDefaultTerminalType` — в `MdePkg.dec` под `gEfiMdePkgTokenSpaceGuid`. Исходные префиксы плана не соответствовали декларациям (build упал бы с PCD not found); исправлены только префиксы token space, значения — по-прежнему дословно §5.2 отчёта S0.

Пояснение для исполнителя (не в файл): все PCD fixed-at-build — обращения `PcdGet*` в SerialDxe/TerminalDxe/16550 резолвятся в compile-time константы, gEfiPcdProtocolGuid в рантайме не трогается (важно для чужого PCD-пространства LIVE).

- [ ] **Step 2: Написать FDF**

Содержимое `docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.fdf` (полностью; в этой задаче два INF, glue добавит Task 3; релокации НЕ стрипаем — образец DxeCore чужой, грузит по любому адресу):

```
[FV.SERIAL_CONSOLE_FV]
FvNameGuid           = 6B4A19D8-2E75-4C31-B8F0-59D2837A90C4
INF MdeModulePkg/Universal/SerialDxe/SerialDxe.inf
INF MdeModulePkg/Universal/Console/TerminalDxe/TerminalDxe.inf

[Rule.Common.DXE_DRIVER]
  FILE DRIVER = $(NAMED_GUID) {
    PE32     PE32                    |.efi
    UI       STRING="$(MODULE_NAME)"
  }

[Rule.Common.UEFI_DRIVER]
  FILE DRIVER = $(NAMED_GUID) {
    PE32     PE32                    |.efi
    UI       STRING="$(MODULE_NAME)"
  }
```

Примечание (rule-11, дословная форма из исходного плана отвергнута парсером — `GenFds.FdfParser.Warning: expected [FD.] near line 2`):
- `FvForceReproducible = TRUE` — ключевого слова НЕ существует в edk2@bcd1687 (0 упоминаний в BaseTools/Source/Python и OvmfPkg); парсер обрывал FV-секцию на неизвестном слове. Удалено; воспроизводимость обеспечивается не FV-флагом, а проверкой двойной сборкой в Task 4 (SOURCE_DATE_EPOCH/TimeDateStamp уже учтены риском R-S1.4).
- Секция правил обязана быть `[Rule.<Arch>.<ModuleType>]` с телом `FILE <type> = $(NAMED_GUID) { <секции> }`; формы `SECTION PE32 = <путь>` в rule-грамматике нет (это грамматика FILE-стейтментов [FV]-секций), заголовок `DRIVER.SERIAL` без `FILE` невалиден. Каноническая форма: `PE32 PE32 |.efi` (файл берётся из OUTPUT модуля) + `UI STRING="$(MODULE_NAME)"`.
- Правил два: SerialDxe `MODULE_TYPE = DXE_DRIVER`, TerminalDxe `MODULE_TYPE = UEFI_DRIVER`; lookup-ключ GenFds — `RULE.COMMON.<MODULE_TYPE>` (FfsInfStatement.py).

- [ ] **Step 3: Написать build_serial.sh**

Содержимое `docker/edk2/build_serial.sh` (полностью; версия этой задачи — cp без glue):

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/../.." && pwd)"
PKG_DIR="$REPO_ROOT/docker/edk2"
EDK2_SRC="$REPO_ROOT/refs/edk2"
IMAGE="uefipatcher-edk2-builder:latest"

if [ "${EDK2_BUILDER_CONTAINER:-0}" != "1" ]; then
    OUT_DIR="${1:-$(mktemp -d /tmp/serial-s1-out.XXXXXX)}"
    mkdir -p "$OUT_DIR"
    podman-remote run --rm --security-opt label=disable \
        -v "$EDK2_SRC":/src/edk2:ro \
        -v "$PKG_DIR":/pkg:ro \
        -v "$OUT_DIR":/out \
        "$IMAGE" /pkg/build_serial.sh /out
    echo "artifacts: $OUT_DIR"
    ls -la "$OUT_DIR"
    exit 0
fi

OUT_DIR="${1:?usage: build_serial.sh <out-dir>}"
rm -rf /work/edk2
mkdir -p /work/edk2
git -C /src/edk2 archive --format=tar HEAD | tar -xf - -C /work/edk2
cd /work/edk2
make -C BaseTools -j"$(nproc)" APPLICATIONS='GenFfs GenFv GenSec GenFw'
export WORKSPACE="$PWD"
export PACKAGES_PATH="$PWD:/pkg"
set +u
. ./edksetup.sh BaseTools
set -u
build -p UefiPatcherSerialPkg/UefiPatcherSerial.dsc -a X64 -t GCC -b RELEASE -n "$(nproc)"
BUILD_ROOT="$WORKSPACE/Build/UefiPatcherSerial/RELEASE_GCC/X64"
cp "$BUILD_ROOT/MdeModulePkg/Universal/SerialDxe/SerialDxe/OUTPUT/SerialDxe.ffs" "$OUT_DIR/"
cp "$BUILD_ROOT/MdeModulePkg/Universal/Console/TerminalDxe/TerminalDxe/OUTPUT/TerminalDxe.ffs" "$OUT_DIR/"
cp "$WORKSPACE/Build/UefiPatcherSerial/RELEASE_GCC/FV/SERIAL_CONSOLE_FV.Fv" "$OUT_DIR/"
ls -la "$OUT_DIR"
```

Примечание к `-t GCC` / `RELEASE_GCC` (rule-11, обнаружено при выполнении Step 4): tools_def.txt 3.06 в edk2@bcd1687 УДАЛИЛ тулчейн `GCC5` (заголовок шаблона: «3.06 - Remove GCC48, GCC49 and GCC5»; доступны `GCC`, `GCCNOLTO`, `CLANGDWARF`, `CLANGPDB`). `-t GCC5` даёт `error 4000: Not available [GCC5] not defined`, поэтому тег — `GCC`, а каталог сборки — `Build/UefiPatcherSerial/RELEASE_GCC/`.

`chmod +x docker/edk2/build_serial.sh`.

Примечание к `. ./edksetup.sh BaseTools` (rule-11, обнаружено при выполнении Step 4): sourced-скрипт наследует позиционные параметры вызова, т.е. внутри edksetup.sh `$1` = `/out` (аргумент container-mode) — не совпадает ни с `BaseTools`, ни с `--reconfig` → edksetup печатает Usage и `return 1` → скрипт умирает под `set -e`. Явный аргумент `BaseTools` (edksetup принимает его как no-op для обратной совместимости) изолирует позиционные параметры (bash ≥5 восстанавливает `$@` после source).

Примечание к `set +u … set -u` вокруг source (rule-11, обнаружено при выполнении Step 4): опции шелла действуют и на sourced-код, а edksetup.sh/BaseEnv не рассчитаны на `set -u` — падают на unbound variable (`edksetup.sh:110: PYTHON_COMMAND: unbound variable`; следующим был бы `EDK_TOOLS_PATH`); BuildEnv вдобавок source-ит Conf-файлы с непредсказуемым содержимым. Гард `set +u`/`set -u` вокруг source закрывает всё сразу; после source строгий режим возвращается.

Примечание к `APPLICATIONS='GenFfs GenFv GenSec GenFw'`: чекаут edk2 БЕЗ submodules (brotli не инициализирован), полный `make -C BaseTools` падает на BrotliCompress; нашему сетапу нужны только эти четыре генератора (+ общая либа Common). Если `build` пожалуется на отсутствие ещё какого-то C-инструмента (сообщение вида `command not found: …/BaseTools/Source/C/bin/…`) — добавить его в список APPLICATIONS (rule-11, зафиксировать какой и почему).

- [ ] **Step 4: Запустить сборку**

Run: `bash docker/edk2/build_serial.sh /tmp/serial-s1-t2`
Expected: лог BaseTools (`Finished building BaseTools C Tools`), затем `build` завершается `Done` (return code 0), в конце `ls -la` показывает `SerialDxe.ffs`, `TerminalDxe.ffs`, `SERIAL_CONSOLE_FV.Fv` и строку `artifacts: /tmp/serial-s1-t2`. Ожидаемый порядок размеров: SerialDxe ~6–12 КБ, TerminalDxe ~35–60 КБ.

Типовые проблемы:
- permission denied на монтировании `/out` → проверить, что `/tmp/serial-s1-t2` создан тем же пользователем, что и сервис podman; при необходимости `chmod 777 /tmp/serial-s1-t2`;
- `podman-remote` не отвечает → `podman-remote info` (socket user-сессии).

- [ ] **Step 5: Проверить GUID/типы/PE-инварианты inline-питоном**

Run:
```bash
python3 - <<'EOF'
import struct, uuid
base = "/tmp/serial-s1-t2/"
for name, guid in [("SerialDxe.ffs", "9A5163E7-5C29-453F-825C-837A46A81E15"),
                   ("TerminalDxe.ffs", "9E863906-A40F-4875-977F-5B93FF237FC6")]:
    d = open(base + name, "rb").read()
    assert str(uuid.UUID(bytes_le=d[:16])).upper() == guid, name
    assert d[0x12] == 0x07, (name, hex(d[0x12]))
    size = d[0x14] | (d[0x15] << 8) | (d[0x16] << 16)
    assert size == len(d), (name, size, len(d))
    off, pe = 24, None
    while off + 4 <= len(d):
        ss = d[off] | (d[off+1] << 8) | (d[off+2] << 16)
        if d[off+3] == 0x10:
            pe = d[off+4:off+ss]
            break
        off += (ss + 3) & ~3
    assert pe is not None and pe[:2] == b"MZ", name
    lfa = struct.unpack_from("<I", pe, 0x3C)[0]
    assert pe[lfa:lfa+4] == b"PE\x00\x00", name
    assert struct.unpack_from("<H", pe, lfa + 4)[0] == 0x8664, name
    assert struct.unpack_from("<H", pe, lfa + 92)[0] == 11, name
    print(name, len(d), "OK")
EOF
```
Expected: `SerialDxe.ffs <N> OK` и `TerminalDxe.ffs <N> OK` (N = фактический размер). Записать N для отчёта Task 6.

- [ ] **Step 6: Commit**

```bash
git add docker/edk2/UefiPatcherSerialPkg/ docker/edk2/build_serial.sh
git commit -m "feat(s1): UefiPatcherSerialPkg dsc/fdf + container build (SerialDxe+TerminalDxe)"
```

---

### Task 3: Модуль SerialConsoleGlue

**Files:**
- Create: `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.inf`
- Create: `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.c`
- Modify: `docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.dsc` ([Components] +1 строка)
- Modify: `docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.fdf` ([FV] +1 INF)
- Modify: `docker/edk2/build_serial.sh` (+1 строка cp)

**Interfaces:**
- Consumes: пакет/DSC/FDF/скрипт Task 2; протокол SerialIo (продюсер — SerialDxe Task 2).
- Produces: `SerialConsoleGlue.ffs` (FILE_GUID `1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43`, FFS type 0x07, subsystem 11); COM-маркеры `SC-S1 glue: serial console attached (ConOut updated)` и `SC-S1 glue: ReadyToBoot` (строки-контракт для E30/S2 — дословно); переменные `ConOut/ConIn/ErrOut` получают инстанс devpath терминального child (или сконструированный controller-path + VT-UTF8 vendor-узел, R-S1.2).

- [ ] **Step 1: Написать INF**

Содержимое `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.inf` (полностью):

```
[Defines]
  INF_VERSION       = 0x00010005
  BASE_NAME         = SerialConsoleGlue
  FILE_GUID         = 1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43
  MODULE_TYPE       = DXE_DRIVER
  VERSION_STRING    = 0.1
  ENTRY_POINT       = SerialConsoleGlueEntry

[Sources]
  SerialConsoleGlue.c

[Packages]
  MdePkg/MdePkg.dec
  MdeModulePkg/MdeModulePkg.dec

[LibraryClasses]
  UefiDriverEntryPoint
  UefiBootServicesTableLib
  UefiRuntimeServicesTableLib
  DevicePathLib
  MemoryAllocationLib
  BaseMemoryLib
  DebugLib
  BaseLib

[Guids]
  gEfiGlobalVariableGuid
  gEfiVTUTF8Guid
  gEfiEventReadyToBootGuid

[Protocols]
  gEfiSerialIoProtocolGuid
  gEfiDevicePathProtocolGuid
  gEfiSimpleTextOutProtocolGuid

[Depex]
  gEfiSerialIoProtocolGuid
```

- [ ] **Step 2: Написать C**

Содержимое `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.c` (полностью; API-точки — Terminal.c:203/215-229/244-258 (тип из vendor-узла), Terminal.c:1084-1141 (паттерн append+SetVariable gEfiGlobalVariableGuid), BmConsole.c:418/482-489 (семантика UpdateConsoleVariable)):

```c
#include <Uefi.h>
#include <Guid/GlobalVariable.h>
#include <Guid/PcAnsi.h>
#include <Guid/EventGroup.h>
#include <Library/UefiDriverEntryPoint.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiRuntimeServicesTableLib.h>
#include <Library/DevicePathLib.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/BaseMemoryLib.h>
#include <Library/DebugLib.h>
#include <Protocol/SerialIo.h>
#include <Protocol/SimpleTextOut.h>
#include <Protocol/DevicePath.h>

#define GLUE_MARKER_BOOT   L"SC-S1 glue: serial console attached (ConOut updated)\r\n"
#define GLUE_MARKER_READY  L"SC-S1 glue: ReadyToBoot\r\n"

typedef struct {
  VENDOR_DEVICE_PATH       Vendor;
  EFI_DEVICE_PATH_PROTOCOL End;
} VT_UTF8_DEVICE_PATH;

STATIC VT_UTF8_DEVICE_PATH mVtUtf8Path = {
  {
    { MESSAGING_DEVICE_PATH, MSG_VENDOR_DP, { sizeof (VENDOR_DEVICE_PATH), 0 } },
    EFI_VT_UTF8_GUID
  },
  { END_DEVICE_PATH_TYPE, END_ENTIRE_DEVICE_PATH_SUBTYPE, { sizeof (EFI_DEVICE_PATH_PROTOCOL), 0 } }
};

STATIC EFI_EVENT mReadyToBootEvent = NULL;

STATIC
BOOLEAN
HasVtUtf8Node (
  IN CONST EFI_DEVICE_PATH_PROTOCOL  *DevicePath
  )
{
  CONST EFI_DEVICE_PATH_PROTOCOL *Node;

  if (DevicePath == NULL) {
    return FALSE;
  }
  for (Node = DevicePath; !IsDevicePathEnd (Node); Node = NextDevicePathNode (Node)) {
    if ((DevicePathType (Node) == MESSAGING_DEVICE_PATH) &&
        (DevicePathSubType (Node) == MSG_VENDOR_DP) &&
        CompareGuid (&((VENDOR_DEVICE_PATH *)Node)->Guid, &gEfiVTUTF8Guid)) {
      return TRUE;
    }
  }
  return FALSE;
}

STATIC
EFI_HANDLE
FindTerminalChild (
  VOID
  )
{
  EFI_STATUS                Status;
  EFI_HANDLE                *Handles;
  EFI_HANDLE                Child;
  EFI_DEVICE_PATH_PROTOCOL  *DevicePath;
  UINTN                     Count;
  UINTN                     Index;

  Handles = NULL;
  Status = gBS->LocateHandleBuffer (
                  ByProtocol,
                  &gEfiSimpleTextOutProtocolGuid,
                  NULL,
                  &Count,
                  &Handles
                  );
  if (EFI_ERROR (Status) || Handles == NULL) {
    return NULL;
  }
  Child = NULL;
  for (Index = 0; Index < Count; Index++) {
    DevicePath = NULL;
    Status = gBS->HandleProtocol (
                    Handles[Index],
                    &gEfiDevicePathProtocolGuid,
                    (VOID **)&DevicePath
                    );
    if (!EFI_ERROR (Status) && HasVtUtf8Node (DevicePath)) {
      Child = Handles[Index];
      break;
    }
  }
  FreePool (Handles);
  return Child;
}

STATIC
EFI_STATUS
AppendInstanceToVariable (
  IN CONST CHAR16                   *Name,
  IN CONST EFI_DEVICE_PATH_PROTOCOL *DevicePath
  )
{
  EFI_STATUS                Status;
  EFI_DEVICE_PATH_PROTOCOL  *Buffer;
  EFI_DEVICE_PATH_PROTOCOL  *Walk;
  EFI_DEVICE_PATH_PROTOCOL  *Instance;
  EFI_DEVICE_PATH_PROTOCOL  *Merged;
  EFI_DEVICE_PATH_PROTOCOL  *Tail;
  UINTN                     Size;
  UINTN                     InstanceSize;
  UINTN                     PathSize;
  BOOLEAN                   Found;

  PathSize = GetDevicePathSize (DevicePath);
  Size = 0;
  Status = gRT->GetVariable ((CHAR16 *)Name, &gEfiGlobalVariableGuid, NULL, &Size, NULL);
  if (Status == EFI_NOT_FOUND) {
    return gRT->SetVariable (
             (CHAR16 *)Name,
             &gEfiGlobalVariableGuid,
             EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS | EFI_VARIABLE_NON_VOLATILE,
             PathSize,
             (VOID *)DevicePath
             );
  }
  if (Status != EFI_BUFFER_TOO_SMALL) {
    return Status;
  }

  Buffer = AllocateZeroPool (Size);
  if (Buffer == NULL) {
    return EFI_OUT_OF_RESOURCES;
  }
  Status = gRT->GetVariable ((CHAR16 *)Name, &gEfiGlobalVariableGuid, NULL, &Size, Buffer);
  if (EFI_ERROR (Status)) {
    FreePool (Buffer);
    return Status;
  }

  Found = FALSE;
  Walk = DuplicateDevicePath (Buffer);
  if (Walk != NULL) {
    while (TRUE) {
      Instance = GetNextDevicePathInstance (&Walk, &InstanceSize);
      if (Instance == NULL) {
        break;
      }
      if ((InstanceSize == PathSize) && (CompareMem (Instance, DevicePath, PathSize) == 0)) {
        Found = TRUE;
      }
      FreePool (Instance);
      if (Found) {
        break;
      }
    }
    FreePool (Walk);
  }
  if (Found) {
    FreePool (Buffer);
    return EFI_SUCCESS;
  }

  Merged = AllocateZeroPool (Size + PathSize);
  if (Merged == NULL) {
    FreePool (Buffer);
    return EFI_OUT_OF_RESOURCES;
  }
  CopyMem (Merged, Buffer, Size);
  Tail = (EFI_DEVICE_PATH_PROTOCOL *)((UINT8 *)Merged + Size - sizeof (EFI_DEVICE_PATH_PROTOCOL));
  Tail->Type = END_DEVICE_PATH_TYPE;
  Tail->SubType = END_INSTANCE_DEVICE_PATH_SUBTYPE;
  SetDevicePathNodeLength (Tail, sizeof (EFI_DEVICE_PATH_PROTOCOL));
  CopyMem ((UINT8 *)Merged + Size, DevicePath, PathSize);
  Status = gRT->SetVariable (
           (CHAR16 *)Name,
           &gEfiGlobalVariableGuid,
           EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS | EFI_VARIABLE_NON_VOLATILE,
           Size + PathSize,
           Merged
           );
  FreePool (Merged);
  FreePool (Buffer);
  return Status;
}

STATIC
VOID
EFIAPI
OnReadyToBoot (
  IN EFI_EVENT  Event,
  IN VOID       *Context
  )
{
  EFI_HANDLE                    Child;
  EFI_SIMPLE_TEXT_OUT_PROTOCOL  *TextOut;

  Child = FindTerminalChild ();
  if (Child == NULL) {
    return;
  }
  TextOut = NULL;
  if (!EFI_ERROR (gBS->HandleProtocol (Child, &gEfiSimpleTextOutProtocolGuid, (VOID **)&TextOut)) &&
      (TextOut != NULL)) {
    (VOID)TextOut->OutputString (TextOut, GLUE_MARKER_READY);
  }
}

EFI_STATUS
EFIAPI
SerialConsoleGlueEntry (
  IN EFI_HANDLE        ImageHandle,
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  EFI_STATUS                    Status;
  EFI_HANDLE                    *Handles;
  EFI_HANDLE                    SerialHandle;
  EFI_HANDLE                    Child;
  EFI_DEVICE_PATH_PROTOCOL      *Path;
  EFI_DEVICE_PATH_PROTOCOL      *ConsolePath;
  EFI_SIMPLE_TEXT_OUT_PROTOCOL  *TextOut;
  UINTN                         Count;
  UINTN                         Index;

  Handles = NULL;
  Status = gBS->LocateHandleBuffer (
                  ByProtocol,
                  &gEfiSerialIoProtocolGuid,
                  NULL,
                  &Count,
                  &Handles
                  );
  if (EFI_ERROR (Status) || (Handles == NULL) || (Count == 0)) {
    return EFI_NOT_FOUND;
  }

  SerialHandle = Handles[0];
  Child = NULL;
  for (Index = 0; (Index < Count) && (Child == NULL); Index++) {
    (VOID)gBS->ConnectController (Handles[Index], NULL, NULL, FALSE);
    Child = FindTerminalChild ();
  }
  FreePool (Handles);

  Path = NULL;
  if (Child != NULL) {
    Status = gBS->HandleProtocol (Child, &gEfiDevicePathProtocolGuid, (VOID **)&Path);
  } else {
    Status = gBS->HandleProtocol (SerialHandle, &gEfiDevicePathProtocolGuid, (VOID **)&Path);
  }
  if (EFI_ERROR (Status) || (Path == NULL)) {
    return EFI_NOT_FOUND;
  }
  ConsolePath = DuplicateDevicePath (Path);
  if (ConsolePath == NULL) {
    return EFI_OUT_OF_RESOURCES;
  }
  if (Child == NULL) {
    Path = AppendDevicePathNode (ConsolePath, (EFI_DEVICE_PATH_PROTOCOL *)&mVtUtf8Path);
    FreePool (ConsolePath);
    if (Path == NULL) {
      return EFI_OUT_OF_RESOURCES;
    }
    ConsolePath = Path;
  }

  (VOID)AppendInstanceToVariable (L"ConOut", ConsolePath);
  (VOID)AppendInstanceToVariable (L"ConIn", ConsolePath);
  (VOID)AppendInstanceToVariable (L"ErrOut", ConsolePath);
  FreePool (ConsolePath);

  Child = FindTerminalChild ();
  if (Child != NULL) {
    TextOut = NULL;
    if (!EFI_ERROR (gBS->HandleProtocol (Child, &gEfiSimpleTextOutProtocolGuid, (VOID **)&TextOut)) &&
        (TextOut != NULL)) {
      (VOID)TextOut->OutputString (TextOut, GLUE_MARKER_BOOT);
    }
  }

  (VOID)gBS->CreateEventEx (
                  EVT_NOTIFY_SIGNAL,
                  TPL_CALLBACK,
                  OnReadyToBoot,
                  NULL,
                  &gEfiEventReadyToBootGuid,
                  &mReadyToBootEvent
                  );
  return EFI_SUCCESS;
}
```

- [ ] **Step 3: Включить glue в DSC/FDF/скрипт**

Три правки:

`UefiPatcherSerial.dsc`, секция [Components], после строки TerminalDxe:
```
  UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.inf
```

`UefiPatcherSerial.fdf`, [FV.SERIAL_CONSOLE_FV], после INF TerminalDxe:
```
INF UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.inf
```

`build_serial.sh`, после cp TerminalDxe:
```bash
cp "$BUILD_ROOT/UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue/OUTPUT/SerialConsoleGlue.ffs" "$OUT_DIR/"
```

- [ ] **Step 4: Собрать и проверить inline-питоном**

Run: `bash docker/edk2/build_serial.sh /tmp/serial-s1-t3`
Expected: тот же успех + `SerialConsoleGlue.ffs` (~5–12 КБ) в выводе `ls`.

Проверка (тот же inline-скрипт из Task 2 Step 5, дополнить строкой таблицы):
```python
("SerialConsoleGlue.ffs", "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43"),
```
Expected: три строки `<name> <N> OK`. Записать размер glue для отчёта.

- [ ] **Step 5: Commit**

```bash
git add docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/ docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.dsc docker/edk2/UefiPatcherSerialPkg/UefiPatcherSerial.fdf docker/edk2/build_serial.sh
git commit -m "feat(s1): SerialConsoleGlue dxe driver (ConOut/ConIn/ErrOut append + COM markers)"
```

---

### Task 4: Валидатор артефактов hack/edk2_build_check.py + доказательство воспроизводимости

**Files:**
- Create: `hack/edk2_build_check.py`

**Interfaces:**
- Consumes: каталог сборки из Task 3 (3 .ffs + Fv); хелпер `walk_fvs()` НЕ используется (файлы вне FV-контекста).
- Produces: CLI `python3 hack/edk2_build_check.py <dir> [<dir2>]`; exit 0 = все инварианты держатся (и при наличии dir2 — воспроизводимость), exit 1 + диагностика иначе. Используется Task 5 (гейт перед коммитом данных) и всеми S2+ пересборками.

- [ ] **Step 1: Написать скрипт**

Содержимое `hack/edk2_build_check.py` (полностью; docstring по образцу `hack/fv_audit.py`):

```python
#!/usr/bin/env python3
"""edk2 build artifacts check: FFS-инварианты + воспроизводимость сборки.

Стенд S1 (план docs/superpowers/plans/2026-09-06-serial-s1-edk2-build.md):
валидирует тройку SerialDxe/TerminalDxe/SerialConsoleGlue и серийный FV;
со вторым аргументом-каталогом сравнивает две сборки побайтово
(допустимое расхождение — только COFF TimeDateStamp). Не часть движка;
запуск вручную:

    python3 hack/edk2_build_check.py <out-dir> [<out-dir2>]
"""
import struct, sys, uuid

EXPECTED = {
    "SerialDxe.ffs": ("9A5163E7-5C29-453F-825C-837A46A81E15", 11),
    "TerminalDxe.ffs": ("9E863906-A40F-4875-977F-5B93FF237FC6", 11),
    "SerialConsoleGlue.ffs": ("1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43", 11),
}
FFS_TYPE_DRIVER = 0x07
SECTION_PE32 = 0x10
SECTION_UI = 0x15
PE_MACHINE_AMD64 = 0x8664

def u24(b, o):
    return b[o] | (b[o + 1] << 8) | (b[o + 2] << 16)

def walk_sections(d):
    off, end, pe32, ui = 24, len(d), None, None
    while off + 4 <= end:
        size, stype = u24(d, off), d[off + 3]
        if size < 4:
            break
        if stype == SECTION_PE32:
            pe32 = d[off + 4:off + size]
        elif stype == SECTION_UI:
            ui = d[off + 4:off + size].decode("utf-16-le", "ignore").rstrip("\x00")
        off += (size + 3) & ~3
    return pe32, ui

def pe_fields(pe):
    assert pe[:2] == b"MZ", "no MZ"
    lfa = struct.unpack_from("<I", pe, 0x3C)[0]
    assert pe[lfa:lfa + 4] == b"PE\x00\x00", "no PE signature"
    machine = struct.unpack_from("<H", pe, lfa + 4)[0]
    subsystem = struct.unpack_from("<H", pe, lfa + 92)[0]
    return machine, subsystem, lfa + 8

def check_ffs(path, expect_guid, expect_subsys):
    d = open(path, "rb").read()
    guid = str(uuid.UUID(bytes_le=d[:16])).upper()
    assert guid == expect_guid, f"{path}: guid {guid}"
    assert d[0x12] == FFS_TYPE_DRIVER, f"{path}: type {d[0x12]:#04x}"
    hdr = bytearray(d[:24])
    hdr[16] = hdr[17] = hdr[23] = 0
    assert sum(hdr) & 0xFF == 0, f"{path}: header checksum"
    if not (d[0x13] & 0x40):
        assert d[17] == 0xAA, f"{path}: file checksum fixed value"
    size = u24(d, 0x14)
    assert size == len(d), f"{path}: size24 {size} != {len(d)}"
    pe32, ui = walk_sections(d)
    assert pe32 is not None, f"{path}: no PE32 section"
    machine, subsystem, tds_off = pe_fields(pe32)
    assert machine == PE_MACHINE_AMD64, f"{path}: machine {machine:#06x}"
    assert subsystem == expect_subsys, f"{path}: subsystem {subsystem}"
    assert len(d) > 1024, f"{path}: implausibly small"
    return len(d), ui, pe32, tds_off

def main():
    if len(sys.argv) not in (2, 3):
        sys.exit("usage: edk2_build_check.py <out-dir> [<out-dir2>]")
    dir1 = sys.argv[1].rstrip("/")
    results = {}
    for name, (guid, subsys) in EXPECTED.items():
        size, ui, pe32, tds_off = check_ffs(f"{dir1}/{name}", guid, subsys)
        results[name] = (pe32, tds_off)
        print(f"{name}: {size} B  ui={ui!r}  machine=AMD64 subsystem={subsys}")
    fv = open(f"{dir1}/SERIAL_CONSOLE_FV.Fv", "rb").read(0x30)
    assert fv[0x28:0x2C] == b"_FVH", "FV signature"
    print(f"SERIAL_CONSOLE_FV.Fv: _FVH ok")
    if len(sys.argv) == 3:
        dir2 = sys.argv[2].rstrip("/")
        for name, (guid, subsys) in EXPECTED.items():
            b1 = open(f"{dir1}/{name}", "rb").read()
            b2 = open(f"{dir2}/{name}", "rb").read()
            if b1 == b2:
                print(f"{name}: reproducible (identical)")
                continue
            for b in (b1, b2):
                pe32, _ = walk_sections(b)
                machine, subsystem, tds_off = pe_fields(pe32)
                struct.pack_into("<I", b, tds_off, 0)
            if b1 == b2:
                print(f"{name}: reproducible (COFF TimeDateStamp only)")
            else:
                sys.exit(f"{name}: NOT reproducible beyond TimeDateStamp")
        print("REPRODUCIBLE")
    return 0

if __name__ == "__main__":
    sys.exit(main())
```

`chmod +x hack/edk2_build_check.py`.

- [ ] **Step 2: Прогнать на сборке Task 3**

Run: `python3 hack/edk2_build_check.py /tmp/serial-s1-t3`
Expected: три строки `<name>: <N> B  ui='SerialDxe'|'TerminalDxe'|'SerialConsoleGlue'  machine=AMD64 subsystem=11` + `SERIAL_CONSOLE_FV.Fv: _FVH ok`, exit 0. ui-имена соответствуют BASE_NAME модулей.

- [ ] **Step 3: Двойная сборка — доказательство воспроизводимости**

Run:
```bash
bash docker/edk2/build_serial.sh /tmp/serial-s1-r1 && bash docker/edk2/build_serial.sh /tmp/serial-s1-r2 && python3 hack/edk2_build_check.py /tmp/serial-s1-r1 /tmp/serial-s1-r2
```
Expected: для каждого файла `reproducible (identical)` (ожидание: GenFw обнуляет TimeDateStamp — как у доноров, §4.3 «репродуцируемые»); допустимый исход — `(COFF TimeDateStamp only)`; `NOT reproducible` = провал гейта S1 (разбираться, не коммитить). Финальная строка `REPRODUCIBLE`, exit 0. Результат (какой из двух исходов) записать для отчёта Task 6.

- [ ] **Step 4: Commit**

```bash
git add hack/edk2_build_check.py
git commit -m "feat(s1): hack/edk2_build_check.py (ffs/pe asserts + build reproducibility)"
```

---

### Task 5: Артефакты как тест-данные движка (TDD)

**Files:**
- Create: `crates/uefi-engine/tests/data/serial/SerialDxe.ffs` (копия из сборки)
- Create: `crates/uefi-engine/tests/data/serial/TerminalDxe.ffs` (копия)
- Create: `crates/uefi-engine/tests/data/serial/SerialConsoleGlue.ffs` (копия)
- Test: `crates/uefi-engine/tests/serial_ffs.rs`

**Interfaces:**
- Consumes: `uefi_engine::ffs::{calculate_checksum8, uint24_to_u32}` (ffs.rs:55, ffs.rs:71), `uefi_engine::parser::file::guid_from_bytes` (parser/file.rs:9); артефакты Task 4-валидной сборки.
- Produces: интеграционный тест `serial_ffs.rs` (имя файла-теста для cargo: `cargo test -p uefi-engine --test serial_ffs`); коммит тест-данных. Двусторонняя валидация: хелперы движка обязаны соглашаться с артефактами GenFfs.

- [ ] **Step 1: Написать падающий тест**

Содержимое `crates/uefi-engine/tests/serial_ffs.rs` (полностью):

```rust
use std::path::PathBuf;
use uefi_engine::ffs::{calculate_checksum8, uint24_to_u32};
use uefi_engine::parser::file::guid_from_bytes;

const FFS_TYPE_DRIVER: u8 = 0x07;
const SECTION_PE32: u8 = 0x10;
const PE_MACHINE_AMD64: u16 = 0x8664;
const PE_SUBSYSTEM_BOOT_DRIVER: u16 = 11;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/serial")
}

struct ParsedFfs {
    guid: String,
    ffs_type: u8,
    size24: u32,
    header_checksum_zero: bool,
    pe_machine: u16,
    pe_subsystem: u16,
}

fn parse_ffs(bytes: &[u8]) -> ParsedFfs {
    assert!(bytes.len() >= 24);
    let guid = guid_from_bytes(&bytes[..16]).unwrap();
    let mut hdr = bytes[..24].to_vec();
    hdr[16] = 0;
    hdr[17] = 0;
    hdr[23] = 0;
    let mut off = 24usize;
    let mut pe32: Option<&[u8]> = None;
    while off + 4 <= bytes.len() {
        let sec_size =
            uint24_to_u32([bytes[off], bytes[off + 1], bytes[off + 2]]) as usize;
        if sec_size < 4 {
            break;
        }
        if bytes[off + 3] == SECTION_PE32 {
            pe32 = Some(&bytes[off + 4..off + sec_size]);
            break;
        }
        off += (sec_size + 3) & !3;
    }
    let pe = pe32.expect("no PE32 section");
    assert_eq!(&pe[..2], b"MZ");
    let lfanew = u32::from_le_bytes(pe[0x3C..0x40].try_into().unwrap()) as usize;
    assert_eq!(&pe[lfanew..lfanew + 4], b"PE\x00\x00");
    let pe_machine = u16::from_le_bytes(pe[lfanew + 4..lfanew + 6].try_into().unwrap());
    let pe_subsystem =
        u16::from_le_bytes(pe[lfanew + 92..lfanew + 94].try_into().unwrap());
    ParsedFfs {
        guid: guid.to_string().to_ascii_uppercase(),
        ffs_type: bytes[0x12],
        size24: uint24_to_u32([bytes[0x14], bytes[0x15], bytes[0x16]]),
        header_checksum_zero: calculate_checksum8(&hdr) == 0,
        pe_machine,
        pe_subsystem,
    }
}

#[test]
fn serial_s1_artifacts_parse() {
    let cases = [
        ("SerialDxe.ffs", "9A5163E7-5C29-453F-825C-837A46A81E15"),
        ("TerminalDxe.ffs", "9E863906-A40F-4875-977F-5B93FF237FC6"),
        ("SerialConsoleGlue.ffs", "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43"),
    ];
    for (file, expect_guid) in cases {
        let path = data_dir().join(file);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let parsed = parse_ffs(&bytes);
        assert_eq!(parsed.guid, expect_guid, "{file}");
        assert_eq!(parsed.ffs_type, FFS_TYPE_DRIVER, "{file}");
        assert_eq!(parsed.size24 as usize, bytes.len(), "{file}");
        assert!(parsed.header_checksum_zero, "{file} header checksum");
        assert_eq!(parsed.pe_machine, PE_MACHINE_AMD64, "{file}");
        assert_eq!(parsed.pe_subsystem, PE_SUBSYSTEM_BOOT_DRIVER, "{file}");
    }
}
```

- [ ] **Step 2: Убедиться, что тест падает (данных ещё нет)**

Run: `cargo test -p uefi-engine --test serial_ffs`
Expected: FAIL — panic `read …/tests/data/serial/SerialDxe.ffs` (No such file or directory). Если упало иначе (компиляция) — исправить импорты до копирования данных.

- [ ] **Step 3: Положить артефакты**

```bash
mkdir -p crates/uefi-engine/tests/data/serial
cp /tmp/serial-s1-r1/SerialDxe.ffs /tmp/serial-s1-r1/TerminalDxe.ffs /tmp/serial-s1-r1/SerialConsoleGlue.ffs crates/uefi-engine/tests/data/serial/
```
(Источник — каталог первой воспроизводимой сборки Task 4; если r1 утерян, пересобрать `bash docker/edk2/build_serial.sh /tmp/serial-s1-r1`.)

- [ ] **Step 4: Убедиться, что тест проходит**

Run: `cargo test -p uefi-engine --test serial_ffs`
Expected: `test serial_s1_artifacts_parse ... ok`, 1 passed.

- [ ] **Step 5: Полный прогон crates + lint**

Run:
```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
```
Expected: все тесты зелёные (базовый уровень: 571 passed / 26 ignored на момент планирования + 1 новый), clippy 0 warnings, fmt чисто.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/tests/data/serial/ crates/uefi-engine/tests/serial_ffs.rs
git commit -m "test(s1): serial ffs build artifacts as uefi-engine test data"
```

---

### Task 6: Отчёт S1, аддендум спеки, roadmap, push

**Files:**
- Create: `docs/reports/2026-09-06-serial-s1-build.md`
- Modify: `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` (§7, добавить пункт в конец)
- Modify: `roadmap.md` (статус S1)

**Interfaces:**
- Consumes: фактические числа Tasks 2–5 (размеры, sha256, исход воспроизводимости, ui-имена).
- Produces: закрывающий документ ступени (гейт S1 = «воспроизводимая сборка в контейнере + артефакты в git»); вход writing-plans S2.

- [ ] **Step 1: Собрать sha256 артефактов**

Run: `sha256sum crates/uefi-engine/tests/data/serial/*.ffs`
Expected: три строки — записать в отчёт.

- [ ] **Step 2: Написать отчёт**

Структура `docs/reports/2026-09-06-serial-s1-build.md` (заполнить фактическими числами; заготовка):

```markdown
# Отчёт: S1 — сборочная инфраструктура edk2 (SerialDxe + TerminalDxe + Glue)

Статус: гейт S1 (спека §3): воспроизводимая сборка в контейнере;
.ffs-артефакты закоммичены как тест-данные.

## 1. Стенд
- docker/edk2-builder.containerfile (fedora:44/runtime-base + gcc/make/python3/libuuid-devel/git)
- docker/edk2/build_serial.sh: эпизодичная сборка, git-archive копия refs/edk2@bcd1687 в /work
  (чекаут не трогается), BaseTools APPLICATIONS='GenFfs GenFv GenSec GenFw', GCC X64 RELEASE
- PACKAGES_PATH=<edk2>:/<docker/edk2> — свой пакет UefiPatcherSerialPkg вне дерева edk2

## 2. PCD (факт; §5.2 S0)
[таблица: токен → значение → носитель (SerialDxe PcdUartDefault* / 16550 PcdSerial* /
TerminalDxe PcdDefaultTerminalType=3)] — все fixed-at-build, gEfiPcdProtocolGuid не читается

## 3. Артефакты
| файл | FFS GUID | размер | sha256 | UI |
[3 строки: SerialDxe / TerminalDxe / SerialConsoleGlue; + SERIAL_CONSOLE_FV.Fv]

## 4. Инварианты и воспроизводимость
- hack/edk2_build_check.py: type=0x07, machine=0x8664, subsystem=11, header-checksum=0,
  size24==len — 3/3
- двойная сборка: [identical | COFF TimeDateStamp only]
- движок: tests/serial_ffs.rs — хелперы ffs.rs/parser/file.rs согласованы с GenFfs

## 5. Glue (рулинг R-S1.1/R-S1.2)
- вариант (а): append ConOut/ConIn/ErrOut + ConnectController + маркеры
- маркеры: «SC-S1 glue: serial console attached (ConOut updated)» / «SC-S1 glue: ReadyToBoot»
- (б) gST->ConOut swap — контингенси E30; (в) отклонён (конфликт AMI-сплиттера)
- допущение порядка диспетчеризации + обе защиты (сконструированный путь; перепроверка в
  ReadyToBoot) — переадресовано мини-pre-check S2 (§8.5-2)

## 6. Входы S2
- вставка: SerialDxe → TerminalDxe → Glue, append в хвост FV1 @0x890000 (первый слот
  0xB63B18, free_tail 2 082 028 Б, S0 §8.1)
- E30-атрибуция по маркерам (§5 таблица решений)
- суммарный объём вставки: [сумма 3 FFS + выравнивания] Б
```

- [ ] **Step 3: Аддендум спеки**

В `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md`, конец §7, добавить пункт:

```markdown
- **S1 закрыта 2026-09-06**: отчёт `docs/reports/2026-09-06-serial-s1-build.md`.
  Стенд: `docker/edk2-builder.containerfile` + пакет `UefiPatcherSerialPkg` (DSC/FDF) +
  `docker/edk2/build_serial.sh` (эпизодичная сборка в контейнере, git-archive копия
  `refs/edk2`@`bcd1687`, чекаут не загрязняется). Артефакты SerialDxe + TerminalDxe +
  SerialConsoleGlue (glue = развилка (а): append ConOut/ConIn/ErrOut + маркеры SC-S1;
  (б) — контингенси E30) закоммичены в `crates/uefi-engine/tests/data/serial/` и
  валидируются `hack/edk2_build_check.py` + `tests/serial_ffs.rs`. PCD — все fixed-at-build
  по §5.2 (единственная правка PcdDefaultTerminalType=3). Воспроизводимость: [исход].
  Гейт S1 выполнен.
```

(`[исход]` заменить фактом из Task 4 Step 3.)

- [ ] **Step 4: Roadmap**

В `roadmap.md`: ступень S1 отметить закрытой со ссылкой на отчёт; текущей ступенью дуги становится S2 (по образцу формулировок закрытия S0).

- [ ] **Step 5: Commit**

```bash
git add docs/reports/2026-09-06-serial-s1-build.md docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md roadmap.md
git commit -m "docs(s1): build report + spec addendum + roadmap"
```

- [ ] **Step 6: Верификация дерева и push**

```bash
cargo test --all && cargo fmt --all -- --check && git status -s
git push dsevosty fix/cycle6-reimplent
```
Expected: тесты зелёные, fmt чисто, `git status -s` пуст (всё закоммичено), push принят удалым `dsevosty`.

---

## Самопроверка плана (выполнена при написании)

- **Покрытие спеки**: гейт S1 (воспроизводимая контейнерная сборка → Tasks 1–4; артефакты как тест-данные → Task 5; закрытие ступени → Task 6). PCD-конфиг §5.2 — Task 2 Step 1 (дословная таблица). Glue-развилка §8.2 — R-S1.1 (решение + реализация Task 3). Наследование «на базе runtime-base» — Task 1. `.ffs` в `crates/uefi-engine/tests/data/` — Task 5.
- **Заполнители**: код всех файлов дан полностью; отчёт Task 6 — измерительная заготовка с указанием, какие числа откуда берутся (S0-прецедент).
- **Консистентность имён**: `SerialConsoleGlue` / `UefiPatcherSerialPkg` / `build_serial.sh` / `edk2_build_check.py` / `serial_ffs.rs` / `tests/data/serial/` — едины по всему плану; GUID-константы совпадают в Tasks 2–6.
- **Известные риски** (каждый с путём лечения): синтаксис `SECTION UI` в FDF-правиле (Task 2 Step 2, rule-11); BaseTools APPLICATIONS-набор (Task 2 Step 3, rule-11); порядок диспетчеризации TerminalDxe vs glue (R-S1.2 — не блокер по построению, эмпирика E30); TimeDateStamp-вариативность (Task 4 Step 3 допускает оба исхода).
