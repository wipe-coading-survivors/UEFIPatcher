# HOWTO: открытие скрытых страниц/вопросов Setup

Статус: **аппаратно-валидировано** (Huananzhi X99-TF: скрытая страница PCI +
Above 4G Decoding, значения сохраняются в NVRAM; отчёт
`docs/reports/2026-09-02-hw-validation-hnx99tf.md`, эталон E12).

Скрытность в AMI Aptio бывает двух видов, механизмы разные:

| Механизм | Где живёт | Инструмент |
|---|---|---|
| IFR-гейты (SUPPRESS_IF / GRAYOUT_IF) | IFR-байты форм Setup-модуля | `hii form/question gates` + `unlock` |
| hide-маркер формсета | stride-таблица PE-тела AMITSE | `tse report` + `tse unhide` |

## 1. IFR-гейты

### Разведка

```bash
uefi-cli image open образ.bin --mode write
uefi-cli hii form list                 # форма с visible=false — кандидат
uefi-cli hii form gates <target>#<form_id>          # что гейтит ФОРМУ
uefi-cli hii question list <target>#<form_id>       # вопросы формы
uefi-cli hii question gates <target>#<form_id>:<qid>
```

Грамматика item_id: `TARGET#FORM_ID[:QID]`, qid — dec или 0x-hex. TARGET
копируется из колонки form_id вывода `hii form list` (напр.
`899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0`).

`gates` печатает объемлющие скоупы и декодированные выражения — без правок.
Типовая вендорская схема двухслойная: always-true suppress на REF→форму в
родительском меню (`UINT64(1)==UINT64(1)`) + персональные grayout вопросов
(`EQ_ID_VAL(0x9A, 1)`, где 0x9A — невидимый NUMERIC-мастер-переключатель).

### Unlock

```bash
uefi-cli hii form unlock <target>#<form_id>
uefi-cli hii question unlock <target>#<form_id>:<qid>
uefi-cli image save out.bin
```

Классы флипов (проверены на железе, длины IFR/PE неизменны):

- `EqConst(1,1)` (always-true suppress) → литерал `1→2` — условие всегда ложно;
- `EqIdVal(qid, 1)` (grayout) → значение `1→0xFFFF` — недостижимо для 1-байтовой
  переменной.

Операция отказывает с диагностикой на нераспознанных классах (составные
AND/OR-гейты, TRUE-константы — не флипаются). Идемпотентна: уже вскрытые
гейты скипаются с warn, no-op не пишет на диск.

### Живой пример (HNX99TF, страница PCI Subsystem Settings)

```bash
uefi-cli hii form unlock 899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0#10029
# хаб-REF в Advanced: 1==1 -> 1==2
uefi-cli hii question unlock 899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0#10029:54
uefi-cli hii question unlock 899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0#10029:55
# ... каждый вопрос с EqIdVal-гейтом (54..60)
uefi-cli image save unlocked.bin
```

Итоговый дифф против оригинала — ровно 3 байта в IFR (побайтовая верификация
в real-image тесте `real_image_hii_unlock_matches_e12`).

## 2. Hide-маркер AMITSE (формсет скрыт из корневого меню целиком)

Если формсет есть в `hii form list`, но вкладки нет в корневом баре Setup —
проверьте stride-таблицы AMITSE-модуля:

```bash
uefi-cli image list                    # взять image_id
uefi-cli tse report <image_id>         # инвентарь stride-блоков и $SPF
uefi-cli tse unhide <image_id> <formset-guid> <root-form-id>
```

Механизм: в PE-теле AMITSE (B1DA0ADF-…) несколько stride-таблиц записей
`{GUID, form-id}` шагом 0x20; одна из них — hide-таблица (потребитель —
фаза-2 TSE). `tse unhide` зануляет запись — вкладка формсета появляется в
корневом баре.

Живой пример (RD450x, вкладка IntelRCSetup):

```bash
uefi-cli tse report 3f2a...            # увидеть блоки: hide-таблица @0x1b40
uefi-cli tse unhide 3f2a... EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 1
```

Контракт безопасности: hide-таблицей считается блок, где **все** записи —
корневые формы формсетов (минимальный form_id); при неоднозначности команда
отказывает — экспертный режим `--block-offset <pe-offset>` (зануление не того
блока даёт обратный эффект — вкладка пропадает).

Статус: живой гейт v3 (2026-09-22) прошит — вкладка IntelRCSetup появилась;
движковая операция — байт-паритет с v3. Спека:
`docs/superpowers/specs/2026-10-02-tse-unhide-design.md`.

## 3. Грабли

- **Двухслойность**: unlock формы не трогает гейты вопросов внутри неё (и
  наоборот) — смотрите `gates` на обоих уровнях.
- **REF-гейт живёт в родительской форме**: `hii form gates` на самой странице
  его не покажет; хаб-REF в родителе (Advanced) — частое место.
- **Suppress по рантайм-переменной**: если условие вычисляется по значению
  NVRAM-переменной (не литералу), unlock литералов может вскрыть страницу
  лишь частично — вопросы появятся после смены самой переменной в Setup
  (прецедент E24/E25: страница раскрылась полностью после перевода всех
  suppress-констант в «никогда»).
- **Составные гейты** (AND/OR из нескольких условий) не флипаются — это
  осознанный отказ (класс, валидированный на железе, — только одиночные
  EqConst/EqIdVal).
- После unlock имеет смысл сразу запечь нужные значения:
  [asrock-baked-defaults.md](asrock-baked-defaults.md).
