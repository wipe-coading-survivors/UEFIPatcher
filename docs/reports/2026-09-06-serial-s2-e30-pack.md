# Отчёт: S2 — E30-flashpack (кандидат для прошивки + протокол приёмки)

> 2026-09-06, ветка `fix/cycle6-reimplent`. Ступень S2 umbrella-спеки
> `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` §3.
> Гейт S2: UART-адаптер видит UEFI boot-вывод — проверяется на железе
> (E30), вердикт за владельцем. Кандидат собран движком (гейт-тест —
> коммит `5dc2ef7`, state-адаптация — `e88691a`), валидирован дважды
> независимо: движковый real-image тест + сторонний `hack/fv_audit.py`.
> **Прошивать ТОЛЬКО v1** sha256 `09f5e897…` — отвергнутая первая
> сборка v0-broken sha256 `a0ff90f3…` невидима для DXE-core (§3.1).

## 1. Стенд

- Движок: `target/debug/engine`, CLI: `target/debug/uefi-cli`, оба
  перестроены `cargo build -q -p uefi-engine -p uefi-cli` из коммита
  `e88691a` (fix round 1: адаптация state-байта вставляемых FFS к erase
  polarity тома). Сокет: `UEFIPATCHER_SOCK=/tmp/serial-s2/engine.sock`.
- База: `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` (оригинал,
  sha256 ниже), режим `image open --mode write` (значения CLI:
  `read|write`; «edit» брифа ≡ `write`).
- Вставляемые артефакты S1: `crates/uefi-engine/tests/data/serial/
  {SerialDxe,TerminalDxe,SerialConsoleGlue}.ffs` (коммит `840fd25`
  + fix-волна ревью S1).
- Транскрипт сборки кандидата (Task 2 Steps 1–3, дословно; вывод
  CLI-команд — id/`ok`, движок логирует в engine.log):

```
$ export UEFIPATCHER_SOCK=/tmp/serial-s2/engine.sock
$ rm -f "$UEFIPATCHER_SOCK"
$ UEFIPATCHER_SOCK=... nohup target/debug/engine > /tmp/serial-s2/engine.log 2>&1 &
  INFO uefi_engine: Starting engine: sock=/tmp/serial-s2/engine.sock, purge_artifacts=false
$ uefi-cli session init --force
  1bb0e6cf-18ae-4c07-bfd1-dce02e8ff4bd
$ uefi-cli image open --mode write refs/fw/HNX99TF_200525_original_E5C88C6F.bin
  6c0fdd7a-2e0c-4c1d-b223-e2e70819f90d   HNX99TF_200525_original_E5C88C6F.bin
  engine.log: image parsed volumes=6 files=276; mode=Write size=16777216

$ uefi-cli node list | grep -E '^(3|3/215) '
  3  Volume ... off=8978432 size=5046272          # FV1 @0x890000, 216 файлов
  3/215  File(DXE driver) guid=A0327FE0-1FDA-4E5B-905D-B510C45A61D0 off=11814784 size=127892

$ uefi-cli node insert 3/215 --file .../serial/SerialDxe.ffs --mode after
  3/215        # CLI печатает target (rpc/server.rs:418); новый узел = 3/216
  engine.log: artifact inserted target=3/215 size=32848; image built size=16777216
$ uefi-cli node insert 3/216 --file .../serial/TerminalDxe.ffs --mode after
  3/216        # новый узел = 3/217; size=65596
$ uefi-cli node insert 3/217 --file .../serial/SerialConsoleGlue.ffs --mode after
  3/217        # новый узел = 3/218; size=24688
  Хвост node list: 3/215 A0327FE0-…, 3/216 9A5163E7-… (SerialDxe),
  3/217 9E863906-… (TerminalDxe), 3/218 1EF3A7C2-… (SerialConsoleGlue)

$ uefi-cli image save /tmp/serial-s2/E30-candidate.bin
  ok           # 16 777 216 байт
$ sha256sum /tmp/serial-s2/E30-candidate.bin refs/fw/HNX99TF_200525_original_E5C88C6F.bin
  09f5e897e04817c9c409ba7991b5903a370b600a15e3f6bfa9a0eed4c53647f3  E30-candidate.bin
  7e9d25443d5955e990f5f24cd1e7288d82e6655eb9c0f9f8c1ec5851de6a1c5d  HNX99TF_…_E5C88C6F.bin
$ pkill -x engine      # engine stopped
```

## 2. Артефакт

| Параметр | Значение |
|---|---|
| Кандидат | `/tmp/serial-s2/E30-candidate.bin` |
| Размер | 16 777 216 Б (16 МиБ, = базе) |
| sha256 (кандидат) | `09f5e897e04817c9c409ba7991b5903a370b600a15e3f6bfa9a0eed4c53647f3` |
| sha256 (база) | `7e9d25443d5955e990f5f24cd1e7288d82e6655eb9c0f9f8c1ec5851de6a1c5d` |
| Целевой том | FV1 (DXE) @0x890000, erase polarity 1 (Attributes 0x4FEFF, бит 11) |
| Старт вставки | 0xB63B18 — конец последнего файла базы (0xB63B14), выровненный до 8 |
| Цепочка FV1 | 216 → 219 файлов (новые узлы 3/216–3/218 после 3/215 A0327FE0) |

Состав вставки (span 123 136 Б, порядок диспетчеризации
SerialDxe → TerminalDxe → SerialConsoleGlue):

| Офсет | Модуль | GUID | Размер, Б |
|---|---|---|---|
| 0xB63B18 | SerialDxe | 9A5163E7-5C29-453F-825C-837A46A81E15 | 32 848 |
| 0xB6BB68 | TerminalDxe | 9E863906-A40F-4875-977F-5B93FF237FC6 | 65 596 |
| — | 4-Б 0xFF-прокладка (выравнивание: 65 596 ≡ 4 mod 8) | — | 4 |
| 0xB7BBA8 | SerialConsoleGlue | 1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43 | 24 688 |

Конец вставки 0xB81C18 — далее FFS-terminator (свободный хвост).
Запас: free_tail 2 082 028 → 1 958 952 Б (≈15,9× потребности).

## 3. Инварианты

- **Движковый гейт** `real_image_ops_insert_serial_s2`
  (`crates/uefi-engine/tests/real_image.rs`, `#[ignore]`, реальный
  образ, коммиты `5dc2ef7` + `e88691a`) — все ассерты зелёные: первый
  изменённый байт диффа = 0xB63B18; все изменения лежат над 0xFF-хвостом
  оригинала; старты вставленных 8-выровнены; `header[23] == 0xF8` у
  всех трёх вставленных (валид при polarity 1); polarity-бит 11 FV1
  читается из rebuilt-образа; повторный rebuild байт-стабилен.
- **Независимая валидация** `hack/fv_audit.py` (стороны движка): diff
  карт FV оригинал/кандидат — ТОЛЬКО строка FV @0x890000: files
  216→219 (+3), used 0x2D3B14→0x2F1C18 (+123 140), free_tail
  0x1FC4EC→0x1DE3E8 (−123 140). ±123 140 = 123 136 полезной вставки +
  4 Б выравнивания (fv_audit меряет `used` до невыровненного конца
  последнего файла базы). FV @0x800000 и @0xDA0000 идентичны.
- **Байтовый авторитет** (cmp, python-подсчёт 0-based): 119 781
  различающийся байт, ВСЕ внутри [0xB63B18, 0xB81C18); вне окна — 0
  байт. Header-checksum каждого вставленного валиден (сумма с
  обнулёнными [0x11]/[0x17] = 0x00); state-байты соседних файлов
  rebuild не тронул (0xF8 → 0xF8).
- **Регресс крейта**: `cargo test -p uefi-engine` — 425 passed /
  27 ignored / 0 failed (ignored — real-image `#[ignore]`);
  `cargo clippy -p uefi-engine -- -D warnings` и `cargo fmt --all
  -- --check` — чисто.

### 3.1. Хронология v0-broken → v1 (правило прошивки)

Первая сборка (v0-broken, sha256
`a0ff90f326b6d013f7ab0b9aada6f6d3fc36cdab8c0eda787b35a3b2aaded410`,
сохранена как `/tmp/serial-s2/E30-candidate.v0-broken.bin`) имела
state-байт вставленных файлов 0x07 — GenFfs пишет polarity-0 форму
заголовка, а целевой FV1 имеет erase polarity **1**: при polarity 1
«валиден» = биты CONSTRUCTION|HEADER_VALID|DATA_VALID сброшены в
raw-байте, т.е. raw 0xF8; raw 0x07 инвертируется в
HEADER_INVALID|DELETED — PI-совместимый DXE core сочёл бы файлы
удалёнными и не диспетчеризовал их (консоль молчала бы при целом
остальном). Исправлено движковым фиксом `e88691a`: `ops::insert`
адаптирует 0x07→0xF8, когда enclosing-том имеет `empty_byte == 0xFF`
(header-checksum не пересчитывается — state исключён из суммы;
в polarity-0 томах вставка осталась verbatim — юнит-тесты
`insert_adapts_state_byte_to_erase_polarity`,
`insert_keeps_state_byte_verbatim_in_polarity0_volume`).

v1 отличается от v0-broken ровно **3 байтами**: `0xB63B2F`,
`0xB6BB7F`, `0xB7BBBF` (= старты файлов +23, байт state), каждый
0x07→0xF8. Ничего больше не менялось.

**Правило: прошивать ТОЛЬКО v1 `09f5e897…`; v0-broken `a0ff90f3…`
не прошивать ни при каких условиях.**

## 4. Протокол E30 (приёмка на железе)

**Предусловия:**

- **PC-A — бут UEFI-путём.** Диск буст-приоритетом грузится по
  UEFI-пути, НЕ через CSM/legacy: легаси-бут статически доказано
  отбирает порт — все DisconnectController-сайты SerialIo в LIVE
  CsmDxe принадлежат CSM-рантайму (LegacyBios-vtable + legacy-boot
  event, спека §8.1) → CSM-бут даст **ложный негатив** (маркеров и
  вывода не будет при рабочей вставке).
- **NVRAM-сброс после прошивки НЕ нужен** (PC-B, спека §8.2):
  фабричный NVRAM не содержит ConOut/ConIn/ErrOut (их создаёт BDS
  при первом буте), glue при NOT_FOUND создаёт переменные сам через
  `SetVariable` под штатным `gEfiGlobalVariableGuid` 8BE4DF61.
- **Терминал**: любой терминал 115200 8N1 без flow control;
  рекомендация `picocom /dev/ttyUSB0 -b 115200` (или minicom).
  Маркеры — чистый ASCII, VT-UTF8-специфика для приёма несущественна
  (полноценная VT-UTF8-графика — вопрос S3+).

**Ожидаемая последовательность на COM1** (маркеры — строки-контракт
S1 дословно):

1. `SC-S1 glue: serial console attached (ConOut updated)` — сразу
   после ConnectController в DXE-диспетчеризации (гейтится на
   EFI_SUCCESS append-а ConOut);
2. UEFI boot-вывод (POST/баннер);
3. `SC-S1 glue: ReadyToBoot` — событие ReadyToBoot.

**Таблица атрибуции наблюдений** (из S1 §5, рулинг R-S1.1):

| Наблюдение | Атрибуция | Действие |
|---|---|---|
| Нет ничего | Диспетчеризация/DEPEX/UART-init (файл не запущен или порт не поднят) | Проверить UEFI-бут (PC-A); затем смотреть контингенси S1 §5 |
| Только маркер 1 | Вставка/SerialIo/ConOut-append OK; терминал отключили по ходу бута (кандидат — CsmDxe при нечистом UEFI-буте); маршрут BDS под вопросом | Убедиться в UEFI-буте; контингенси S1 §5 |
| Маркеры 1+2 без boot-вывода | Консоль-энумерация AMITSE: TSE пересобирает ConOut по своей энумерации, наш экземпляр выпадает | Контингенси: из UEFI Shell `dmpstore ConOut`, сверка инстансов; резерв — вариант (б) `gST->ConOut` swap в ReadyToBoot-колбэке (одно правка-место) |
| Мусорный вывод | Скорость/формат линии | 115200 8N1 без flow control; VT-UTF8 vs ANSI на стороне терминала |

**Критерий успеха ступени** (гейт спеки §3-S2): UART-адаптер видит
UEFI boot-вывод. Маркеры без вывода гейт НЕ закрывают (см. таблицу).

## 5. Инструкция прошивки владельцу

1. Перенести `/tmp/serial-s2/E30-candidate.bin` на машину прошивки
   (артефакт вне git — образы не коммитятся, спека §5).
2. Сверить перед прошивкой: размер 16 777 216 Б, sha256 =
   `09f5e897e04817c9c409ba7991b5903a370b600a15e3f6bfa9a0eed4c53647f3`.
   **Мismatch — не прошивать**; особенно sha256 `a0ff90f3…` — это
   v0-broken (§3.1), его прошивка даст молчащую консоль.
3. Прошить инструментом по прецеденту E26–E29 (тот же способ, каким
   прошивались образцы дуги hijack).
4. NVRAM-сброс/defaults после прошивки НЕ требуется (§4, PC-B).
5. Подключить UART-адаптер к COM1, терминал 115200 8N1 без flow
   control, убедиться в UEFI-пути бута (PC-A), провести приёмку по §4.
   Результат (наблюдение из таблицы) — в вердикт E30.

## 6. Открытое

- Вердикт E30 — за владельцем; до него ступень S2 в статусе «в
  ожидании E30» (roadmap, аддендум спеки §7). Закрытие ступени —
  отдельным аддендумом по вердикту (следующая сессия).
- При негейте — контингенси по таблице §4 (вплоть до резерва (б)
  gST->ConOut swap — одно правка-место в существующем
  ReadyToBoot-колбэке glue).
- Наработка попутно (спека §4): real-image гейт `edit insert`
  (`real_image_ops_insert_serial_s2`) + state-адаптация в
  `ops::insert` — движковые, останутся в репозитории.
