# Отчёт: S3 — E31-flashpack (кандидат для прошивки + протокол приёмки)

> 2026-09-06, ветка `serial-console`. Ступень S3 umbrella-спеки
> `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` §3.
> Гейт S3: serial-настройки видимы в Setup и смена baud меняет скорость —
> проверяется на железе (E31), вердикт за владельцем. Кандидат собран
> живым движком через CLI-транскрипт (§1), валидирован независимо от
> движковой сборки: сторонний `hack/fv_audit.py` + байтовый python-дифф
> + движковый real-image гейт 30/30 `--ignored` (§3). Состав = S2-тройка
> (E30-база) + НОВЫЕ два вопроса Setup; единственное отличие от
> E30-кандидата в хвосте FV1 — config-aware glue (+0x4000) и правки
> слотов Setup/SetupData.

## 1. Стенд

- Движок: `target/debug/engine`, CLI: `target/debug/uefi-cli`, оба
  перестроены `cargo build -q -p uefi-engine -p uefi-cli` из HEAD
  `serial-console` = `d042769` (Task 5: config-aware glue). Сокет:
  `UEFIPATCHER_SOCK=/tmp/serial-s3/engine.sock`, data = дефолт
  `~/.local/share/uefipatcher`.
- База: `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` (оригинал,
  sha256 ниже), режим `image open --mode write`.
- Артефакты: `crates/uefi-engine/tests/data/serial/` — SerialDxe
  (sha256 `d16b78ee…`) и TerminalDxe (`64003242…`) без изменений с S1;
  SerialConsoleGlue НОВЫЙ config-aware, sha256
  `58ee44c9db1936dedf497f410fffa7ad1b1cd076c0749795fbbaa4906f7a20d7`,
  41 072 Б (S1-версия была 24 688 Б, рост 0x4000); схема вопросов
  `s3_questions.json` (2 вопроса, q512/q513).
- Транскрипт сборки кандидата (Task 6 Step 1, дословно; вывод CLI-команд
  — как напечатано, движок логирует в engine.log):

```
$ export UEFIPATCHER_SOCK=/tmp/serial-s3/engine.sock
$ rm -f "$UEFIPATCHER_SOCK"
$ UEFIPATCHER_SOCK=... nohup target/debug/engine > /tmp/serial-s3/engine.log 2>&1 &
  INFO uefi_engine: Starting engine: sock=/tmp/serial-s3/engine.sock,
  data=/var/home/dsevosty/.local/share/uefipatcher, purge_artifacts=false
$ uefi-cli session init --force
  46d1c286-7e31-417c-b9e9-b992240d3f61   511a878b-7af7-4700-8963-3764651133fa
$ uefi-cli image open --mode write refs/fw/HNX99TF_200525_original_E5C88C6F.bin
  fd9d0564-9310-489f-8ce5-1e759219df81   HNX99TF_200525_original_E5C88C6F.bin
  engine.log: image parsed volumes=6 files=276; mode=Write size=16777216

$ uefi-cli node list | grep -E '^(3 |3/215) '
  3  Volume type=65 subtype=00 guid= off=8978432 size=5046272          # FV1 @0x890000, 216 файлов
  3/215  File(DXE driver) type=66 subtype=07 guid=A0327FE0-1FDA-4E5B-905D-B510C45A61D0 off=11814784 size=127892

$ uefi-cli node insert 3/215 --file crates/uefi-engine/tests/data/serial/SerialDxe.ffs --mode after
  3/215        # CLI печатает target; новый узел = 3/216
  engine.log: artifact inserted target=3/215 mode=After size=32848; image built size=16777216
$ uefi-cli node insert 3/216 --file crates/uefi-engine/tests/data/serial/TerminalDxe.ffs --mode after
  3/216        # новый узел = 3/217; size=65596
$ uefi-cli node insert 3/217 --file crates/uefi-engine/tests/data/serial/SerialConsoleGlue.ffs --mode after
  3/217        # новый узел = 3/218; size=41072 (новый config-aware glue)
  Хвост node list: 3/215 A0327FE0-… (127892), 3/216 9A5163E7-…
  (SerialDxe, 32848), 3/217 9E863906-… (TerminalDxe, 65596),
  3/218 1EF3A7C2-… (SerialConsoleGlue, 41072); у новых узлов off=0 —
  офсеты назначаются при build

$ uefi-cli hii question add 3/28/1/0#10019 --file crates/uefi-engine/tests/data/serial/s3_questions.json
  question_id  spf_record_offset  string_id  name
  0x200  0x77164  -      -
  0x200  0x77164  749    Baud Rate
  0x200  0x77164  750    Serial console port speed (applies on next boot)
  0x200  0x77164  751    115200
  0x200  0x77164  752    57600
  0x200  0x77164  753    38400
  0x200  0x77164  754    19200
  0x200  0x77164  755    9600
  0x201  0x771F4  -      -
  0x201  0x771F4  756    Terminal Type
  0x201  0x771F4  757    Serial console terminal type (applies on next boot)
  0x201  0x771F4  758    VT-UTF8
  0x201  0x771F4  759    VT-100+
  0x201  0x771F4  760    VT-100
  0x201  0x771F4  761    ANSI
  engine.log: hii questions added target=3/28/1/0#10019 count=2;
  image built size=16777216

$ uefi-cli image save /tmp/serial-s3/E31-candidate.bin
  ok           # 16 777 216 байт
$ sha256sum /tmp/serial-s3/E31-candidate.bin refs/fw/HNX99TF_200525_original_E5C88C6F.bin
  d30cca1c998e56af86db45ed44d3a9f2cacd09709307984e9fb912799b66c34c  E31-candidate.bin
  7e9d25443d5955e990f5f24cd1e7288d82e6655eb9c0f9f8c1ec5851de6a1c5d  HNX99TF_…_E5C88C6F.bin
$ pkill -x engine      # engine stopped

# НЕМЕДЛЕННО, до любых шагов, способных рискнуть кандидатом (прецедент E30):
$ mkdir -p ~/E31 && cp /tmp/serial-s3/E31-candidate.bin ~/E31/E31-candidate-d30cca1c.bin
$ sha256sum ~/E31/E31-candidate-d30cca1c.bin
  d30cca1c998e56af86db45ed44d3a9f2cacd09709307984e9fb912799b66c34c   # идентичен
```

- Пост-сборочный карты-проход (валидация §3, отдельная read-only
  сессия движка на базе — образ не менялся):

```
$ rm -f "$UEFIPATCHER_SOCK"; nohup target/debug/engine > /tmp/serial-s3/engine2.log 2>&1 &
$ uefi-cli session init --force
  89d4175a-e954-4ce0-ac46-0b8c4588bb33   150c012d-df42-43b2-90b3-760a70f93cf2
$ uefi-cli image open --mode read refs/fw/HNX99TF_200525_original_E5C88C6F.bin
  233e2b00-6ad8-4a0d-8f85-a95737383be1   HNX99TF_200525_original_E5C88C6F.bin
$ uefi-cli node list | grep -E '^(3/(2[5-9]|3[0-2])) '
  3/28  File(DXE driver) … guid=899407D7-99FE-43D8-9A21-79EC328CAC21 off=9246416 size=25889
  …                                            # слот Setup = 0x8D16D0
$ uefi-cli node list | grep FE612B72
  3/208  File(Freeform) … guid=FE612B72-203C-47B1-8560-A66D946EB371 off=11126536 size=49884
                                               # слот SetupData = 0xA9C708
$ pkill -x engine      # engine stopped
```

## 2. Артефакт

| Параметр | Значение |
|---|---|
| Кандидат | `/tmp/serial-s3/E31-candidate.bin` |
| Постоянная копия | `~/E31/E31-candidate-d30cca1c.bin` (создана до всего прочего, sha256 сверен) |
| Размер | 16 777 216 Б (16 МиБ, = базе) |
| sha256 (кандидат) | `d30cca1c998e56af86db45ed44d3a9f2cacd09709307984e9fb912799b66c34c` |
| sha256 (база) | `7e9d25443d5955e990f5f24cd1e7288d82e6655eb9c0f9f8c1ec5851de6a1c5d` |
| Целевой том | FV1 (DXE) @0x890000, erase polarity 1 |
| Старт вставки | 0xB63B18 — конец последнего файла базы (0xB63B14), выровненный до 8 |
| Цепочка FV1 | 216 → 219 файлов (узлы 3/216–3/218 после 3/215 A0327FE0) |

Состав FV1-вставки (span 139 520 Б = 0x22100, порядок диспетчеризации
SerialDxe → TerminalDxe → SerialConsoleGlue):

| Офсет | Модуль | GUID | Размер, Б |
|---|---|---|---|
| 0xB63B18 | SerialDxe | 9A5163E7-5C29-453F-825C-837A46A81E15 | 32 848 |
| 0xB6BB68 | TerminalDxe | 9E863906-A40F-4875-977F-5B93FF237FC6 | 65 596 |
| — | 4-Б 0xFF-прокладка (выравнивание: 65 596 ≡ 4 mod 8) | — | 4 |
| 0xB7BBA8 | SerialConsoleGlue (новый config-aware, sha256 `58ee44c9…`) | 1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43 | 41 072 |

Конец вставки **0xB85C18** (у E30 был 0xB81C18 — glue вырос 0x4000) —
далее FFS-terminator (свободный хвост). Запас: free_tail 2 082 028 →
1 942 504 Б (≈13,9× потребности).

Состав Setup-правок (цель `3/28/1/0#10019` — форма Serial Port 1
Configuration, файл Setup 899407D7, varstore id 1 = существующая семья
560BF58A, новых varstore не вводилось):

| Вопрос | IFR | varstore | Опции (значение) | $SPF-запись | Строки |
|---|---|---|---|---|---|
| Baud Rate | q512 (0x200), one_of, width 1 | id 1, offset 93 (0x5D) | 115200 (0, default optimized) / 57600 (1) / 38400 (2) / 19200 (3) / 9600 (4) | @0x77164 | 749–755 |
| Terminal Type | q513 (0x201), one_of, width 1 | id 1, offset 94 (0x5E) | VT-UTF8 (0, default optimized) / VT-100+ (1) / VT-100 (2) / ANSI (3) | @0x771F4 | 756–761 |

Потребители: config-aware glue читает `Setup[0x5D/0x5E]` (baud/type)
+ `PNP0501_0_NV[0]` (enable — существующий вопрос q34 «Serial Port»,
ветка enable досталась бесплатно). Хелпы у обоих вопросов («applies on
next boot»).

## 3. Инварианты

- **Движковый real-image гейт** `real_image_ops_insert_serial_s3`
  (`crates/uefi-engine/tests/real_image.rs`, `#[ignore]`, реальный
  образ) — прогнан живьём на HEAD `d042769`: ok. Полный `--ignored`
  набор real_image — **30/30 ok** (9.55 c), включая s3-гейт с
  инвариантами: зоны изменений = S2-окно + слот Setup + слот SetupData,
  вне — 0 байт; 389+2 $SPF-записей; selective ifr-fixup — 32
  Setup-формсетных записи скомпенсированы, 357 foreign байт-идентичны;
  page count неизменен; счётчик правила `(0x0001<<16)|(1+max_low+i)`;
  s14/s30 записи несут appended help/prompt id; строки разрешаются
  дословно; optimized-default = 0 у обоих.
- **Независимая валидация** `hack/fv_audit.py` (сторона движка): diff
  карт FV база/кандидат — ТОЛЬКО строка FV @0x890000: files 216→219
  (+3), used 0x2D3B14→0x2F5C18 (**+139 524**), free_tail 0x1FC4EC→
  0x1DA3E8 (−139 524). FV @0x800000 и @0xDA0000 идентичны. +139 524 =
  E30-дельта 123 140 + 16 384 (рост glue 24 688→41 072 = 0x4000);
  внутри: span 139 520 + 4 Б измерительного сдвига (fv_audit меряет
  `used` до невыровненного конца последнего файла базы — тот же
  эффект, что в E30).
- **Байтовый авторитет** (python-дифф база↔кандидат, слоты по
  FFS-заголовкам как в гейте): **211 301** различающийся байт, ВСЕ
  внутри трёх зон — S2-окно [0xB63B18, 0xB85C18): 136 027; слот Setup
  [0x8D16D0, 0x8D7BF1) (файл 3/28, 25 889 Б): 25 631; слот SetupData
  [0xA9C708, 0xAA89E4) (файл 3/208 FE612B72, 49 884 Б): 49 643;
  **вне зон — 0 байт**. Все изменения S2-окна лежат над 0xFF-хвостом
  оригинала. Header-checksum каждого вставленного валиден (сумма с
  обнулёнными [0x11]/[0x17] = 0x00), state-байты вставленных 0xF8
  (polarity 1). Слоты Setup/SetupData перезаписаны почти целиком — их
  HII-тела в LZMA-сжатых секциях, правка IFR/строк/$SPF + refixup
  диффундирует по слоту после пересжатия (конфайнмент файловый, как
  определяет гейт).
- **Регресс**: `cargo test --all` — 583 passed / 0 failed / 30 ignored;
  `cargo clippy --all -- -D warnings` и `cargo fmt --all -- --check` —
  чисто. (Env-примечание: integration-тесты uefi-gateway гоняют
  reqwest на 127.0.0.1 и ломаются 503, если в окружении выставлен
  `http_proxy` — гейты запускать с зачищенными proxy-переменными;
  код не при чём, без прокси 3/3 зелёные.)

## 4. Протокол E31 (приёмка на железе)

**Предусловия** (наследуются из E30, отчёт S2 §4–5): UEFI-бут (диск
буст-приоритетом грузится по UEFI-пути, НЕ CSM/legacy — легаси-бут
даст ложный негатив); NVRAM-сброс после прошивки НЕ нужен; терминал
115200 8N1 без flow control, открыт до подачи питания (picocom
`-b 115200`). Первый бут после прошивки: если `Setup[0x5D/0x5E]`/`
PNP0501_0_NV[0]` ещё не материализованы (timing GetVariable в DXE до
StdDefaults), glue берёт fallback 115200/VT-UTF8/enable — поведение
= E30, пользовательских отличий нет.

Пункты протокола:

- **(a) Дефолт-бут** — консоль на COM1 115200 8N1, поведение = E30
  (никаких регрессий: POST/баннер, Setup-рендер, OS-вывод).
- **(b)** Setup → Advanced → Super IO Configuration → Serial Port 1
  Configuration: видны «Serial Port [Enabled]», «Change Settings»,
  НОВЫЕ «Baud Rate [115200]» и «Terminal Type [VT-UTF8]» + хелпы.
- **(c)** Baud Rate → 57600 → Save & Exit → ребут → весь вывод и
  Setup-навигация на 57600 (терминал перенастроить!).
- **(d)** Возврат 115200 → ребут → 115200.
- **(e)** Terminal Type → VT-100 → ребут → экран рендерится в
  VT-100-эмуляции.
- **(f)** Serial Port → Disabled → ребут → UEFI-консоли на COM1 нет
  (OS-вывод после загрузки ОС остаётся — Linux программирует UART сам).
- **(g)** Load Optimized Defaults → Baud/Terminal возвращаются к
  [115200]/[VT-UTF8].

**Примечание к (e) — маркеры SC-S1 при смене Terminal Type.** При
Terminal Type ≠ VT-UTF8 молчание in-driver маркерной строки glue
(первый маркер SC-S1) — ОЖИДАЕМО: in-glue ConnectController создаёт
VT-UTF8-ребёнка TerminalDxe, который не может смэтчиться с
не-VT-UTF8 `mTerminalGuid`; маркер 2 появляется только после того,
как BDS подключит типизированный терминал. Отсутствие маркера НЕ
ДОЛЖНО читаться как «glue не исполнялся». (К моменту E31 маркеры и
так не наблюдаемы — гипотезы вердикта E30: ClearScreen Setup-рендера
и stale TextOut-child; функциональный рендер — сильнее маркеров.)

**Контингенси E31-(b) — вопросы не рендерятся.** В $SPF смешанные
конвенции page-list: 475 записей pool-block-style против 4736
direct-record; базовая страница формы 10019 — pool-style, наш клон
аппендит direct-указатели (доминирующая конвенция контейнера). Если
пункты (b) не видны — карта каналов E29 (запись s14 + string-pool
control + counter/type) — итерация полей записи движком, БЕЗ
прошивки-перебора: единый диагностический образ с печатью статуса
glue в COM при enable=1. 0xB2-quirk из E30 (вход в настройки из
загрузчика NixOS) — отдельный открытый вопрос, НЕ гейт S3.

**Критерий успеха ступени** (гейт спеки §3-S3): пункты serial-настроек
видимы в Setup, смена baud меняет скорость — т.е. (b) + (c).

## 5. Инструкция прошивки владельцу

1. Кандидат уже перенесён из `/tmp` в постоянное место ДО любых
   ребутов (артефакт вне git — образы не коммитятся, спека §5):
   `~/E31/E31-candidate-d30cca1c.bin`, sha256 сверен после копирования.
   `/tmp`-копия (`/tmp/serial-s3/E31-candidate.bin`) ребутов не
   переживает — работать с `~/E31`-копией.
2. Сверить перед прошивкой: размер 16 777 216 Б, sha256 =
   `d30cca1c998e56af86db45ed44d3a9f2cacd09709307984e9fb912799b66c34c`.
   Mismatch — не прошивать.
3. Прошить инструментом по прецеденту E26–E30 (тот же способ, каким
   прошивались образцы дуги; E30 этим же инструментом прошит
   `09f5e897…`).
4. Ветка отката (обязательна): при отсутствии POST после прошивки —
   рефлеш E30-кандидата `09f5e897e04817c9c409ba7991b5903a370b600a15e3f6bfa9a0eed4c53647f3`
   (S2-поведение: консоль есть, Setup-вопросов нет) или базового
   образа E5C88C6F sha256
   `7e9d25443d5955e990f5f24cd1e7288d82e6655eb9c0f9f8c1ec5851de6a1c5d`.
   Правки не append-only (слоты Setup/SetupData пересжаты), но
   проверены гейтом + двумя независимыми диффами.
5. NVRAM-сброс/defaults после прошивки НЕ требуется (E30-прецедент;
   glue сам создаёт Con*-переменные при NOT_FOUND).
6. До включения машины терминал открыт и подключен, UART-адаптер на
   COM1; убедиться в UEFI-пути бута; провести приёмку по §4.
   Результат — в вердикт E31.

## 6. Вердикт E31 и открытое

Вердикт — за владельцем (флеш = действие владельца; этот отчёт —
хендафф). Закрытие S3 — отдельным аддендумом спеки §7 + roadmap по
вердикту (прецедент E30). Открытое на момент пакета: (1) первая
живая аддитивная вставка $SPF — геометрия доказана статикой и
гейтом, дискриминатор = протокол (b) + карта каналов E29 для
итерации; (2) 0xB2-quirk E30 — парковка в TODO сохраняется; (3)
timing GetVariable в DXE — fallback = дефолты, поведение = E30.

Наработка движка (в репозитории, коммиты Tasks 1–5 ветки
`serial-console`): op `hii question add` (engine+RPC+CLI),
real-image гейт `real_image_ops_insert_serial_s3`, selective
ifr-fixup, config-aware glue-артефакт `58ee44c9…`.
