# Отчёт: NP-P Task 6 — pack E36/E37 (goto-дискриминатор + регистрация страницы)

> 2026-09-09, ветка `setup-new-page`, HEAD `7f1ac12` (гейты np1/np2).
> Дуга «Setup New Page», спека
> `docs/superpowers/specs/2026-09-07-setup-new-page-design.md` (§6 —
> лестница кандидатов). Финальная ступень NP-P: два кандидата собраны
> живым движком через CLI (транскрипт §1 — команды идентичны гейту
> `np_assemble`), валидированы независимо (§2), готовы к прошивке
> владельцем (NP-E). Прошивка и вердикт — только владелец; этот отчёт —
> хендафф.

## 1. Кандидаты

| Параметр | E36 «goto-only» | E37 «регистрация страницы» |
|---|---|---|
| Постоянная копия | `~/E36/E36-candidate-df9c38f0.bin` | `~/E37/E37-candidate-0c4d536e.bin` |
| /tmp-копия (до ребута) | `/tmp/setup-np/E36-candidate.bin` | `/tmp/setup-np/E37-candidate.bin` |
| Размер | 16 777 216 Б (= базе) | 16 777 216 Б (= базе) |
| sha256 | `df9c38f0b2c3650cfde50508a8c9a40f8ea925fb0380f896caa00b33d298289d` | `0c4d536e5c7bdbc4fd653eb416bb590ab333aec067482daa979f7b55a8bcec7c` |
| База | `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, sha256 `7e9d25443d5955e990f5f24cd1e7288d82e6655eb9c0f9f8c1ec5851de6a1c5d` | та же |
| Семантическая разница E36↔E37 | — | ровно одна: `hii page add` (регистрация формы 10101 в $SPF) |

Общий состав (обоих): LIVE + FV1-хвост E35 (SerialIo-донор 97C81E5D +
TerminalDxe 9E863906 + glue v3 1EF3A7C2) + вопросы S3 q512/q513 (форма
10019, varstore 1) + форма 10101 с varstore 2 + REF q528 «UEFIPatcher
Setup» в конец формы 10019. E37 дополнительно: страница формы 10101 в
таблице страниц $SPF (слот 188, count 189). Порядок сбора — гейт
`np_assemble`: triple → q512/q513 → form add → [page add] → ref add.

### 1.1 Стенд

- Движок `target/debug/engine`, CLI `target/debug/uefi-cli`,
  перестроены `cargo build -q -p uefi-engine -p uefi-cli` из HEAD
  `7f1ac12`. Сокет `UEFIPATCHER_SOCK=/tmp/setup-np/engine.sock`,
  данные `UEFIPATCHER_DATA=/tmp/setup-np`, сокет удалён перед стартом.
- Гейты np1/np2 прогнаны живьём на этом же HEAD перед сборкой: ok
  (§2).

### 1.2 Транскрипт сборки E36 (дословно)

```
$ export UEFIPATCHER_SOCK=/tmp/setup-np/engine.sock UEFIPATCHER_DATA=/tmp/setup-np
$ rm -f "$UEFIPATCHER_SOCK"; target/debug/engine &   # engine.log
$ uefi-cli session init --force
  2d49689f-691a-4a62-9cfa-98019fcb2d5c   caaf0952-789b-40bb-b1f3-086a95ecd5c2
$ uefi-cli image open --mode write refs/fw/HNX99TF_200525_original_E5C88C6F.bin
  9a3b482a-6ce6-48ef-ae30-9f5913bc2bf7   HNX99TF_200525_original_E5C88C6F.bin
$ uefi-cli node list | grep -E '^(3 |/3/215) '        # FV1 = 216 файлов, хвост:
  3  Volume type=65 … off=8978432 size=5046272        # FV1 @0x890000
  3/215  File(DXE driver) … guid=A0327FE0-… off=11814784 size=127892
  3/28   File(DXE driver) … guid=899407D7-… off=9246416 size=25889   # слот Setup
  3/208  File(Freeform)  … guid=FE612B72-… off=11126536 size=49884  # слот SetupData
$ uefi-cli node insert 3/215 --file …/serial/SerialIoAmiDxe.ffs --mode after
  3/215        # новый узел 3/216, size=7675
$ uefi-cli node insert 3/216 --file …/serial/TerminalDxe.ffs --mode after
  3/216        # новый узел 3/217, size=65596
$ uefi-cli node insert 3/217 --file …/serial/SerialConsoleGlueV3.ffs --mode after
  3/217        # новый узел 3/218, size=41072
$ uefi-cli hii question add 3/28/1/0#10019 --file …/serial/s3_questions.json
  question_id  spf_record_offset  string_id  name
  0x200  0x77164  749–755  Baud Rate (+опции/хелп)
  0x201  0x771F4  756–761  Terminal Type (+опции/хелп)
  engine.log: hii questions added count=2
$ uefi-cli hii form add --target 3/28/1/0 --file …/serial/np_form.json
  inserted_form_ids  10101
  string_id 762 UefiPatcherSetup; 763 UEFIPatcher Serial Settings;
  764 Serial Console; 765 Serial console output enable (demo);
  766 Disabled; 767 Enabled; 768 Verbose Boot;
  769 Verbose boot messages (demo); 770 Off; 771 On
$ uefi-cli hii question add 3/28/1/0#10019 --file …/serial/np_ref.json
  question_id  spf_record_offset  string_id  name
  0x210  0x0  772  UEFIPatcher Setup
  0x210  0x0  773  UEFIPatcher serial console settings
  engine.log: hii questions added count=0 refs=1   # REF = БЕЗ $SPF-записи
$ uefi-cli image save /tmp/setup-np/E36-candidate.bin
  ok           # 16 777 216 Б
$ sha256sum /tmp/setup-np/E36-candidate.bin
  df9c38f0b2c3650cfde50508a8c9a40f8ea925fb0380f896caa00b33d298289d
# НЕМЕДЛЕННО, до любых других шагов (прецедент E30/E31):
$ mkdir -p ~/E36 && cp /tmp/setup-np/E36-candidate.bin ~/E36/E36-candidate-df9c38f0.bin
$ sha256sum ~/E36/E36-candidate-df9c38f0.bin
  df9c38f0b2c3650cfde50508a8c9a40f8ea925fb0380f896caa00b33d298289d   # идентичен
```

### 1.3 Транскрипт сборки E37 (только отличия от E36)

```
$ uefi-cli session init --force     # 7b41c574… / новая сессия, образ заново
$ uefi-cli image open --mode write refs/fw/HNX99TF_200525_original_E5C88C6F.bin
  38429776-bd26-40d1-b9a5-fc3b55e9e4d2
$ … тройная вставка + s3_questions + form add — те же команды, тот же вывод …
# page add МЕЖДУ form add и ref add; np_page.json (в /tmp, не в git):
#   {"form_id":10101,"title":"UEFIPatcher Serial Settings"}
$ uefi-cli hii page add 3/28/1/0#10019 --file /tmp/setup-np/np_page.json
  form_id  slot  page_offset  title_string_id
  10101    188   488076       772             # 488076 = 0x7728C = container-end
$ uefi-cli hii question add 3/28/1/0#10019 --file …/serial/np_ref.json
  0x210  0x0  773  UefIPatcher Setup          # строки сдвинулись на +1: титул
  0x210  0x0  774  UEFIPatcher serial console settings   # страницы занял 772
$ uefi-cli image save /tmp/setup-np/E37-candidate.bin
  ok
$ sha256sum /tmp/setup-np/E37-candidate.bin
  0c4d536e5c7bdbc4fd653eb416bb590ab333aec067482daa979f7b55a8bcec7c
$ mkdir -p ~/E37 && cp … ~/E37/E37-candidate-0c4d536e.bin; sha256sum — идентичен
$ pkill -x engine      # движок остановлен, сокет удалён
```

### 1.4 Состав FV1-хвоста (вставка, узлы 3/216–3/218, state 0xF8)

| Офсет | Модуль | GUID | Размер, Б |
|---|---|---|---|
| 0xB63B78 | SerialIo (AMI-донор rd450x) | 97C81E5D-8FA0-486A-AAEA-0EFDF090FE4F | 7 675 |
| 0xB65978 | TerminalDxe (= S1) | 9E863906-A40F-4875-977F-5B93FF237FC6 | 65 596 |
| 0xB759B8 | SerialConsoleGlue v3 | 1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43 | 41 072 |

Стартовая цепочка = E32/E35 (0xB63B18/0xB65918/0xB75958) **+96** — см.
сдвиг ниже. Конец вставки 0xB7FA28 (= fv_audit used), далее
FFS-terminator. free_tail 2 082 028 → 1 980 504 Б (запас ≈17×
потребности).

### 1.5 ВАЖНО: сдвиг FV1 (Δ=96, 187/216 сток-файлов) — риск прошивки

Слот Setup-файла имеет 7 Б слэка; правки Setup (form add + ref add,
рост PE32-resource) не влезают в слот, а FFS не имеет экстентов →
билдер сдвигает все файлы FV1 после Setup на константу (by-design,
инвариант зон np1/np2, план Task 5 / спека §10):

- файлы 3/0–3/28 (включая Setup @0x8D16D0) — на месте (29 шт.);
- файлы 3/29–3/215 (187 шт., включая SetupData @0xA9C708→0xA9C768) —
  сдвиг +96 (0x60), порядок/GUID/тела сохранены побайтово;
- вставленная тройка — также +96 относительно позиции E35-эры.

Это первая дуга, двигающая сток-файлы FV1 (E30–E35 правили только
слоты и хвост). Гейты np1/np2 проверяют сдвиг детерминированным
(константный Δ, 8-выравнивание, тела идентичны); независимый дифф
(§2.3) подтверждает. Интерпретатор DXE-BDS читает FV1 по цепочке
заголовков от начала тома — сдвиг внутри цепочки прозрачен для
диспетчера; теоретический остаточный риск (записанные где-либо в
NVRAM/других томах абсолютные офсеты внутрь FV1) гейтом не закрыт —
именно поэтому откат (§4) обязателен, а E36 — первый бут.

### 1.6 Правки Setup/SetupData (цель `3/28/1/0` / `#10019`, PE32-resource канал)

| Правка | E36 | E37 |
|---|---|---|
| q512/q513 (S3, форма 10019, varstore 1 0x5D/0x5E) | да | да |
| форма **10101** «UEFIPatcher Serial Settings» (formset 7B59104A, varstore **2** `UefiPatcherSetup` A9E7D5C2, size 0x10) + q600/q601 one_of @0x0/0x1, строки 762–771 | да | да |
| REF **q528** (0x210) «UEFIPatcher Setup» в конец формы 10019, FormId@+13 = 10101, БЕЗ $SPF-записи (25/31 сток-goto живут так) | да | да |
| страница $SPF: слот **188** @0x354, count **188→189**, скелет на конце контейнера (0x20: marker/u18 от родителя slot 8, fid 10101, title-id 772, seq 188, B=8, cnt 0) | нет | **да** |
| слот Setup 899407D7 @0x8D16D0 | 25 889→25 992 (+103) | 25 889→25 986 (+97) |
| слот SetupData FE612B72 @0xA9C708(+96) | 49 884 (0) | 49 884 (0; рост контейнера +0x20 поглощён пересжатием) |

Рост Setup ±100 Б = 7 Б слэка слота + 96 Б сдвига последующих файлов.

## 2. Инварианты и независимая валидация

### 2.1 Движковые гейты (живой прогон, HEAD 7f1ac12)

`cargo test -p uefi-engine --test real_image -- --ignored
real_image_ops_insert_serial_np{1,2}` — **2/2 ok** (10.0 с):

- `np fv1 layout: 187/216 stock files shifted by constant delta 96`
  (оба гейта);
- np1 (E36): `ref stage: delta=0x11 ref@0xa2b records=391 shifted=0
  container=0x7728c` — REF-сплайн 17 Б (REF 15 + END 2), 391 запись =
  389 live + q512/q513, контейнер $SPF не растёт, страница не
  регистрируется (page = None), count = 188;
- np2 (E37): `page add: slot=188 skeleton=container+0x7728c
  title-id=772 parent=slot8@0x77248`, count 188→189, контейнер
  0x7728c→0x772ac (+0x20), count/slot/length — единственные тронутые
  поля page-add; q-round-trip q600/q601 (one_of, vs 2, 0x0/0x1,
  дефолт 0), строки разрешаются дословно, форма 10101 одна/visible;
- save→open→save байт-стабилен (оба); негативный смоук
  (check_question_add q602 на форме 10101) до регистрации страницы —
  NotFound, после (E37) — ok: страница-регистрация реально резолвится
  префлайтом вопросов.

### 2.2 `hack/fv_audit.py` (сторона движка)

| Образ | FV1 files | FV1 used | FV1 free_tail | FV@0x800000 / FV@0xDA0000 |
|---|---|---|---|---|
| LIVE | 216 | 0x2D3B14 | 0x1FC4EC | 1 файл 0x40000 / 59 файлов 0x260000 |
| E36 | **219** | 0x2EFA28 | 0x1E05D8 | идентичны LIVE |
| E37 | **219** | 0x2EFA28 | 0x1E05D8 | идентичны LIVE |

Δ used = +0x1BF14 = 0x1BEB4 (span тройки, = E32/E35) + 0x60 (сдвиг
Δ=96); free_tail −0x1BF14. E36 и E37 на уровне карты томов
неразличимы (разница — внутри слотов).

### 2.3 Байтовый файл-уровневый дифф (python, `/tmp/setup-np/zone_diff.py`, не в git)

Для обоих кандидатов против LIVE (слоты по FFS-заголовкам, как в
гейте):

- **вне FV1 [0x890000, 0xD60000) — 0 различающихся байт**; заголовок
  тома FV1 — 0 байт;
- файлы: 216→219, порядок/GUID стока сохранены;
- старты: дельты ∈ {0, +96}, ровно одна Δ=96, сдвинуто **187/216**
  (индексы 29–215; Setup idx 28 и всё до него — на месте);
- остальные **214 сток-файлов побайтово идентичны** LIVE (включая
  сдвинутые — перенос дословный);
- слот Setup: E36 25 638 / E37 25 640 различающихся байт в общем
  спане (LZMA-пересжатие диффундирует по слоту — конфайнмент
  файловый, как в гейте); слот SetupData: E36 49 643 / E37 49 629;
- тройка: state 0xF8, 8-выравнивание, в LIVE под ними 0xFF.

### 2.4 Повторный ParseImage живым движком

`image open --mode read` обоих кандидатов: парсится, FV1 = 219 файлов.
Семантический смоук E37: q512 (5 опций), q600/q601 (one_of, varstore
2, дефолт 0), строки 762–774 разрешаются; `hii form list`: форма
10101 «UEFIPatcher Serial Settings» visible=true в формсете
7B59104A. Дискриминатор E36: титул страницы ОТСУТСТВУЕТ (строка 772 =
«UEFIPatcher Setup» — REF-промпт, второго «UEFIPatcher Serial
Settings» нет) — goto-only состав подтверждён. REF q528 не виден
`hii question info` — by-design: `is_question_op` не включает
IFR_REF_OP (проверка REF — побайтовая в гейте: qid@+6=528,
FormId@+13=10101, перед END формы).

## 3. Протокол приёмки

### 3.1 E36 «goto-only» — дерево исходов (спека §6, дословно)

Состав: E35-цепочка + форма 10101 + REF q528. **Без** записи страницы
в $SPF. Пункты: Setup → Advanced → Super IO Configuration → Serial
Port 1 Configuration — REF-пункт «UEFIPatcher Setup» в конце списка;
вход в него.

- **(a)** пункт рендерится, вход открывает страницу, вопросы работают
  → канал goto доказан, регистрация страницы не нужна для
  функциональности (только заголовок/косметика — E38 опционально);
- **(b)** пункт рендерится, вход — «стена»/пустая страница →
  goto-пункт живёт из IFR, страница — из $SPF → **E37**;
- **(c)** пункт не рендерится → нужна $SPF-запись goto-вопроса (тип —
  R1; если R1 молчит — итерация записи) → E36b → затем E37;
- **(d)** вис/ERROR → откат на E32, разбор статикой (прецедент E33).

Ожидание дуги (R-журнал): **(b)** — сток-сигнал 0/31+0/58 REF-целей
без страницы; E36 остаётся дешёвым дискриминатором, **E37 — основной
путь** (строгое подмножество: шить E36 не обязательно, E37 готов в
этом же пакете).

### 3.2 E37 «регистрация страницы» — приёмка

1. REF-пункт «UEFIPatcher Setup» рендерится на 10019 (если нет —
   исход (c) из E36, отдельная итерация).
2. Вход → **полный рендер подстраницы**: заголовок «UEFIPatcher
   Serial Settings» (title-id 772), вопросы «Serial Console
   [Disabled/Enabled]» и «Verbose Boot [Off/On]» + хелпы.
3. Смена Serial Console → Enabled → Save & Exit → ребут → значение
   сохраняется (varstore `UefiPatcherSetup`); Load Optimized Defaults
   → оба вопроса возвращаются к 0.
4. Регресс serial-дуги: Baud Rate/Terminal Type на 10019 работают
   как в E35; консоль COM1 жива.
5. Аномалия — дословно в вердикт (вис Setup, ERROR, пустая страница).

### 3.3 Контингенси

- Вис/нет POST после E36/E37 — рефлеш E32 (§4), разбор статикой;
  риск сдвига FV1 (§1.5) — первоочередная гипотеза при (d).
- Валидация TSE длин/регионов (zero-scan vs count): слот+бамп
  валидны в обеих семантиках (R-журнал §1), остаточный риск → исход
  E37-(вис/аномалия) → откат на E32.

## 4. Ветка откката (обязательна)

1. `~/E32/E32-candidate-8def2850.bin`, sha256
   `8def285059d29acbfbabe4c83709bc8bace496e380d8252e9f27fa00568f82cc`
   — доказанное поведение E32 (serial-консоль + q512/q513, без
   новой страницы): серийная консоль жива, POST стабилен.
2. Базовый образ `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`,
   sha256
   `7e9d25443d5955e990f5f24cd1e7288d82e6655eb9c0f9f8c1ec5851de6a1c5d`
   — заводское состояние.

## 5. Инструкция прошивки владельцу (прецедент E31-pack §5)

1. Кандидаты уже в постоянных местах ДО любых ребутов (вне git,
   образы не коммитятся): `~/E36/E36-candidate-df9c38f0.bin`,
   `~/E37/E37-candidate-0c4d536e.bin`; sha256 сверен после
   копирования. /tmp-копии ребутов не переживают.
2. Сверить перед прошивкой: размер 16 777 216 Б; sha256 = `df9c38f0…`
   (E36) / `0c4d536e…` (E37). Mismatch — не прошивать.
3. Прошить инструментом по прецеденту E26–E35 (тем же способом, каким
   прошивались образцы дуги; E35 этим инструментом прошит
   `031df12f…`).
4. Порядок: рекомендуется **E37 как основной путь** (R-сигнал
   0/31+0/58); E36 — если нужна чистая дискриминация goto-рендера
   (исходы §3.1). Допустим и порядок E36→E37 (два флеша).
5. До включения: терминал 115200 8N1 без flow control открыт на
   COM1 (picocom `-b 115200`); бут-путь UEFI (не CSM). NVRAM-сброс
   после прошивки НЕ нужен (прецедент E30–E32); при аномалиях
   поведения консоли — прецедент S4b: CMOS/NVRAM-clear джампером
   перед повторным бутом (накопительные ConOut/ConIn).
6. Приёмка по §3 (E36 — дерево a–d, E37 — пункты 1–5); все наблюдения
   — дословно в вердикт-отчёт NP-E (прецедент E30/E31/E34).

## 6. Вердикт

Вердикт — за владельцем (флеш = действие владельца; этот отчёт —
хендафф). Закрытие NP-E — аддендум спеки §2 (карта каналов: строки
«goto-пункт», «рендер goto-цели», «заголовок страницы-цели») +
roadmap по вердикту. Наработка движка в репозитории: op `hii question
add` с refs (goto), op `hii page add` ($SPF-регистрация), гейты
np1/np2, фикстуры np_form/np_ref (коммиты Tasks 1–5 ветки
`setup-new-page`).

## 7. Аддендум (2026-09-09): вердикт негатив → пересборка на glue v2

Вердикт владельца на оба кандидата этого пакета (E36 `df9c38f0…`,
E37 `0c4d536e…`): **вис на входе в Setup**,
`ERROR: Class:3000000; Subclass:50000; Operation: D` — сигнатура E34.
Разбор и решения: `docs/reports/2026-09-09-setup-np-e36-e37-verdict.md`
(корневая причина-кандидат: кандидаты §1 стояли на **glue v3**
(E35-цепочка, никогда не прошивалась) вместо планной v2 — дефект
сборки гейта np, внесён переиспользованием хелперов s4c).

Пересобраны (живой движок, флоу идентичен §1.2/§1.3, отличие —
`SerialConsoleGlue.ffs` (v2) вместо V3; той же длины, раскладка FV1
идентична: 216→219, used 0x2efa28, Δ=96, 187 сдвинуто, вне FV1 — 0
байт; re-parse ок):

| кандидат | файл | sha256 |
|---|---|---|
| **E36v2** (goto-only) | `~/E36/E36v2-candidate-b1e38008.bin` | `b1e3800858e6ecfe58dc64d179b48a9bcc8896458ea6e39ea4b70a8602cbb302` |
| **E37v2** (+ страница) | `~/E37/E37v2-candidate-8e1b476c.bin` | `8e1b476cc9644ad74eb391bd85f4d101c1164fb28688d7c0e74cade843aa1f26` |

Гейты np1/np2 переведены на v2-тройку и зелёные (коммит ветки).
Прошивка — по §5 (приоритет E37v2); E36/E37 из §1 считаются
**закрытыми негативом** (не прошивать). Откат прежний: E32/base.

## 8. Аддендум (2026-09-09, раунд 3): демо-varstore → свободный id 31

Раунд 2 (E36v2/E37v2) — тоже вис; корень: **дубликат varstore id 2**
(сток объявляет ids 1..20; probe-инвентарь — вердикт-отчёт §7.2).
Раунд 3 = раунд 2 с единственной правкой: демо-varstore **id 31**
(31+ свободны), всё прочее (форма 10101, вопросы, REF q0x210,
страница E37, v2-цепочка) без изменений.

| кандидат | файл | sha256 | FV1-сдвиг |
|---|---|---|---|
| **E36v3** (goto-only) | `~/E36/E36v3-candidate-08f9b3df.bin` | `08f9b3df53eeceb766fcef35ca95298f448165a3a4ed6bb029e9c0f1ee54c1c4` | Δ=104 |
| **E37v3** (+ страница) | `~/E37/E37v3-candidate-d5b60d7d.bin` | `d5b60d7d10dca2edf77daec29371eb6cb0960c490daea265386d37ce28e4ff62` | Δ=96 |

(Δ различается из-за LZMA-пересжатия при равной семантике; в рамках
каждого кандидата Δ константен — проверено файл-уровневым диффом.)
Валидация: вне FV1 — 0 байт, сток порядок/GUID/тела сохранены,
fv_audit 216→219, re-parse живым движком ок; гейты np1/np2 зелёные
(36/36 ignored). E36/E36v2/E37/E37v2 (§1, §7) закрыты негативом —
не прошивать. Прошивка — E37v3 приоритетно; откат прежний.

## 9. Аддендум (2026-09-09, раунд 4): storage-free дискриминатор

Раунд 3 (E36v3/E37v3) — вис после ClearScreen (вердикт §8). Раунд 4 =
состав-эксперимент (движок/гейты не менялись): форма 10101 без
varstore/вопросов (title + 6 IFR_TEXT с неотжимаемым паддингом,
`tests/data/serial/np_form_v4.json`), REF q0x210 и страница E37
прежние, сдвиг FV1 гарантирован паддингом (Δ=392, 187/216 файлов,
вне FV1 — 0 байт, re-parse ок):

| кандидат | файл | sha256 |
|---|---|---|
| **E36v4** (goto-only, storage-free) | `~/E36/E36v4-candidate-24adcd1b.bin` | `24adcd1bc2488aa49b2b95ff2a66c17be2aacdd98ff8bbbe8c8272a6096463e1` |
| **E37v4** (+ страница) | `~/E37/E37v4-candidate-e097a186.bin` | `e097a18653bfad03e494bffd07005d7b6b9b800ec47458527575f7845f114a6a` |

E36–E37 (v1–v3) закрыты негативом — не прошивать. Прошивка — E37v4
приоритетно (дерево интерпретации — вердикт-отчёт §8.3). Откат
прежний: E32 `8def2850…` / E5C88C6F.

## 13. Аддендум (2026-09-10, раунд 8): REF без собственного END — E36v7/E37v7

Раунд 7 (E36v6/E37v6) — оба висят (вердикт §13): сентинел voff не
единственный; побайтовый дельта-дамп нашёл корень — `build_ref_ops`
эмитил [REF + END], сплайн ставил их перед сток form-END → лишний END
дисбалансировал скоуп-стек IFR-пакета (краш TSE на Entering Setup;
у E36v6 дополнительно Operation A на буте — REF на цель без $SPF).
Фиксы: `f3c3f6e` (план, rule-11) + `c78e11e` (код + инварианты роста
15 / scope-balance). Кандидаты пересобраны (флоу §1.2/§1.3, форма из
`np_form_e16p.json`):

| кандидат | файл | sha256 | FV1-дельта |
|---|---|---|---|
| **E36v7** (REF без END, без страницы) | `~/E36/E36v7-candidate-2d1e7305.bin` | `2d1e7305ae105c859a7882bad70f4d8816e2789bbbb6463f88ca04266d96b88e` | Δ=232 |
| **E37v7** (+ $SPF-страница) | `~/E37/E37v7-candidate-d4a0913e.bin` | `d4a0913e1de31fa1e45be19d1a013220201477b0865a356daba17e17ed3201ae` | Δ=240 |

Валидация: REF@0xa04, len 15, vsid 0 / voff 0xFFFF / flags 0, target
0x2775; сразу за REF — сток form-END; scope balance формс-пакета =
стоку; вне FV1 — 0 байт; 187/216 сток-файлов сдвинуты константно;
re-parse живым движком ок; гейты np1/np2 зелёные (36/36 ignored).

E36/E37 v1–v6, E16p-серия закрыты негативом — не прошивать. Протокол
прежний: розетка из сети перед каждым образом (батарейка вынута);
порядок **E37v7 → E36v7** (E37 — канонический состав дуги); при
сомнении — E32-контроль. Откат: E32 `8def2850…` / база E5C88C6F.

Интерпретация: оба ботят → приёмка NP-E (искать пункт
«UEFIPatcher Setup» в Serial Port 1 Configuration, вход, поведение
Enter; различие E37v7-vs-E36v7 закрывает «нужна ли страница для
рендера goto»); E37v7 ботит, E36v7 виснет → goto-рендер без страницы
падает (исход (b) спеки §6) — канон E37v7; оба висят → дамп контекста
глубже (список отличий от стока уже почти исчерпан).

## 14. Аддендум (2026-09-10, раунд 9): E37v8 — страница с вопросами

Раунд 8 — первый положительный вердикт дуги (вердикт §14): оба
кандидата ботят, goto-пункт рендерится, вход работает, страница для
рендера goto не нужна. Раунд 9 = финальная ступень: полный состав
гейта np2, собранный исправленным движком (форма из
`np_form.json` — varstore 31 + q600/q601 one_of — вместо
storage-free e16p):

| кандидат | файл | sha256 |
|---|---|---|
| **E37v8** (вопросы + страница + REF) | `~/E37/E37v8-candidate-a61909b6.bin` | `a61909b67c5c0f16194d253d075aa728a566f1d54c105cd5baf2296d0ce1f9b3` |

Валидация: полный гейт-прогон np_assemble(true) зелёный (REF
len 15 / voff 0xFFFF / сток-END за REF / scope-balance = стоку;
$SPF 188→189, скелет слот 188; q600/q601 резолвятся question_info;
вне FV1 — 0 байт; re-parse ок).

Протокол (один флеш): розетка из сети → E37v8 → розетка → старт.
Приёмка: (1) пункт «UEFIPatcher Setup» → вход: страница «UEFIPatcher
Serial Settings» с ДВУМЯ вопросами Serial Console (Disabled/Enabled)
и Verbose Boot (Off/On); (2) переключить оба, F10/save; (3) ребут —
значения живы? (NVRAM-переменная UefiPatcherSetup впервые живьём);
(4) серийный вывод по-прежнему есть. Аномалии — дословно.
Откат: E37v7 `d4a0913e…` (живой goto) / E32 / база.
