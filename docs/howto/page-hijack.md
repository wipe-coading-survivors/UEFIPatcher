# HOWTO: подмена текстов страницы Setup (hijack)

Статус: **аппаратно-валидировано** (HNX99TF, E24–E29; финальный движковый
артефакт E29 прошит, чеклист пройден полностью: свои prompt/help на чужом
вопросе, пережили reboot и Load Optimized Defaults). Первоисточник:
`docs/reports/2026-09-05-hijack-v2-e29.md`, спека
`docs/superpowers/specs/2026-09-04-hijack-v2-design.md`.

Кейс: страница вендора («PCI Subsystem Settings») должна остаться рабочей,
но отдельные вопросы получить наши тексты (prompt/help). Прямая правка IFR
string-id недостаточна — AMITSE рендерит тексты из нескольких источников, и
все их надо согласовать.

## 1. Карта каналов рендера (hardware-доказана, 6/6)

| Что видим | Источник | Правится |
|---|---|---|
| видимость страницы/вопроса | IFR suppress/grayout-гейты | `hii form/question unlock` |
| prompt вопроса | IFR prompt-id | hijack (string append + same-length замена id) |
| help вопроса | **обе копии id**: запись s14 в $SPF **и** пул-контрол u16 в $SPF-пуле | hijack (автоматически) |
| заголовок страницы / пункт меню | $SPF title-id@+0xE / goto prompt-id | НЕ меняется (сток остаётся) |
| опции вопроса | сток IFR one_of | не трогается |

Если править только IFR — help останется вендорским; если только $SPF —
prompt останется вендорским. `hijack` согласует обе стороны сам.

## 2. Команда

```
uefi-cli hii form hijack --target <TARGET#FORM_ID> --file hijack.json \
    [--setupdata-guid <GUID>]
```

Схема (только нужные вопросы):

```json
{
  "questions": [
    {
      "question_id": 59,
      "prompt": "UEFIPatcher probe (qid 59)",
      "help": "Our help text rendered by AMITSE"
    }
  ]
}
```

`--setupdata-guid` — GUID файла AMITSESetupData ($PFS/$SPF-контейнер);
на образах класса HNX99TF файл лежит за GUIDed-LZMA-обёрткой, которую
автодискавери не видит насквозь — передавайте явно (HNX:
`FE612B72-203C-47B1-8560-A66D946EB371`). Узнать GUID: `node list`/`node
search setupdata`, RAW-файл рядом с Setup-модулем.

## 3. Полный живой пример (E29, собран движком)

```bash
T=899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0
uefi-cli session init --name e29 --force
uefi-cli image open base.bin --mode write

# 1) вскрыть жертву (если страница/вопросы скрыты) — до hijack
uefi-cli hii form unlock $T#10029
uefi-cli hii question unlock $T#10029:54    # и каждый гейтнутый вопрос
# ... 54..60

# 2) hijack
uefi-cli hii form hijack --target $T#10029 \
    --file e29-schema.json \
    --setupdata-guid FE612B72-203C-47B1-8560-A66D946EB371

uefi-cli image save e29.bin
```

Отчёт операции печатает контракт: число аппенднутых строк (`string_ids`),
сколько гейтов вскрыто внутри hijack (`unlock_flips` — 0, если всё вскрыто
заранее: идемпотентность), `help_controls` и `help_records` (обе копии
хелп-id).

## 4. Верификация

- дифф образа confined в два слота: Setup-модуль и SetupData-файл; вне
  слотов 0 изменённых байт; длина образа неизменна;
- re-parse: строки с нашими текстами в `hii string list`, вопросы читаются;
- после прошивки: вопрос рендерит наш prompt, выделение — наш help;
  смена значения вопроса сохраняется через ребут (NVRAM не задета).

## 5. Грабли

- Правки **только IFR** (без $SPF-канала) оставляют help вендорским —
  используйте `hijack`, а не ручные правки строк.
- Заголовок страницы и пункт меню остаются вендорскими by design (каналы
  $SPF title/goto) — «переименовать страницу» отдельная задача.
- Жертва может быть сток-скрыта (suppress) — не повторяйте охоту E18–E23:
  сначала убедитесь по `gates`, что страница реально скрыта, и вскройте;
  «исчезновение страницы» после правок почти всегда = она и была скрыта
  вендором.
- `--setupdata-guid` обязателен на образах с $SPF за LZMA-обёрткой; без него
  операция откажет с внятной ошибкой.
