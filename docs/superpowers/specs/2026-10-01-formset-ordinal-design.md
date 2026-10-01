# Спека мини-цикла formset-ordinal: discoverability дискриминатора `TARGET#<n>`

Дата: 2026-10-01 · Статус: дизайн одобрен владельцем («ок»), ждёт плана
Первоисточник: TODO.md «FormInfo: formset-порядковый номер не виден в `hii
form list`»; смежная запись TODO (TUI/WebUI-обёртки) закрыта ревизией —
поверхности исполнены раньше (e730dad TUI, 558073c WebUI W3).

## 1. Проблема

Мутация `hii form add TARGET[#n] FILE` (и `hii varstore list TARGET[#n]`)
адресует formset внутри секции числовым ординалом: `n` = индекс
IFR_FORM_SET_OP в блобе, который резолвит `add_form` (default 0). Список
форм (`hii form list` / TUI Forms / WebUI FormsView) ординал не показывает —
на multi-formset-пакетах пользователь угадывает `n` перебором NotFound'ов.

Дополнительно найден дефект атрибуции: `parse_form_package`
(hii/ifr.rs:545) агрегирует все формы пакета в один `FormSetInfo`, а
guid/title берёт от последнего IFR_FORM_SET_OP — на multi-formset-пакете
`hii form list` приписывает все формы чужому formset_guid. Цикл чинит и
атрибуцию, и discoverability.

Выбран подход A (варианты B «anchor-селектор TARGET@form:id» и C «оба»
отвергнуты владельцем): пробел в видимости, не в выразительности — новая
селектор-грамматика не вводим, `#n` остаётся как есть.

## 2. Каналы нумерации (read/write-split, HII-чек-лист §6)

`add_form` (hii/form_add.rs) резолвит `#n` ровно в двух каналах:

| Канал | Условие | Блоб для `#n` |
|---|---|---|
| bare | RAW-секция, `is_form_package(body)` | всё тело секции |
| resource | PE32, `resource_forms_package(body)` | первый HII-resource-entry → его package-list → первый PACKAGE_FORMS |

Всё прочее (FREEFORM_SUBTYPE_GUID-списки, PE32 bare-пакеты вне
resource-блоба, прочие subtype) → `NotASetupItem` — не мутабельно для
`form add`. `collect_forms` (hii/forms.rs) читает шире (все resource-блобы,
все PACKAGE_FORMS в списках, bare-пакеты PE32, FREEFORM-списки).

**Инвариант цикла:** ordinal в FormInfo нумерует только writable-канал —
формсет с ordinal=n обязан резолвиться `form add TARGET#n` в себя.
Формсеты вне writable-канала отображаются с `formset_ordinal = None`
(«—»): они видимы, но не адресуемы.

Внутри блоба ordinal = порядок IFR_FORM_SET_OP (0, 1, …) — семантика
существующего `locate_formset_insert_points(pkg, idx)`.

## 3. Решение по слоям

### 3.1 Engine

Пер-формсетный проход форм-пакета для нужд списка: на каждый
IFR_FORM_SET_OP — свой ordinal, guid, title, свои формы (до следующего
FORM_SET_OP / конца пакета). Новый проход; агрегирующий
`parse_form_package` не меняется — его потребители остаются при текущей
семантике «пакет целиком»: form_export, ref_tree, валидация form_add,
pe_resource.

`collect_forms`:

- RAW-секции (bare-канал): ordinal = индекс FORM_SET_OP в теле секции;
- PE32: ordinal только у формсетов из блоба `resource_forms_package`
  (первый resource-entry, первый PACKAGE_FORMS); формсеты остальных
  resource-блобов/списков и bare-пакетов → None;
- FREEFORM_SUBTYPE_GUID-списки → None (не writable-канал).

Формы вне любого FORM_SET_OP-скопа (мусорный хвост пакета): как сейчас —
не атрибуцируются и не показываются (behaviour сохранить, тестом зафиксировать).

### 3.2 Proto

`FormInfo`: `optional uint32 formset_ordinal = 6`. Именно optional: 0 —
валидный ordinal, дефолт-0 неотличим от «нет». Реген webui-типов
(`cd webui && npm run proto`).

### 3.3 CLI

- `print_forms` (output.rs): колонка `n` после form_id в Text и TSV
  (`—` для None), JSON — поле. Отдельная колонка обязательна: суффикс
  `#n` в form_id сломал бы copy-paste `TARGET#FORM_ID` для
  `hii question list` (справка main.rs:327 прямо говорит «copy TARGET
  from the form_id column»).
- Справки `hii form add` / `hii varstore list`: дописать грамматику
  `TARGET[#n]` и указание источника n («колонка n из `hii form list`»).

### 3.4 TUI

Forms view: нижняя зона деталей выбранной формы получает строку
`formset #n` (None → `formset —`) рядом с target-строкой — пользователь
читает готовый суффикс для `:hii form add`. Строка формы слева не
меняется: колонка в дереве тесна, а группировка идёт по formset_guid —
ordinal редкий кейс. FormKey (ключ кэшей) не расширяется — target уже
дисамбигуирует. Completion/хелп `:hii form add` не меняются (грамматика
та же).

### 3.5 WebUI

FormsTree (дерево форм, `webui/src/lib/components/FormsTree.svelte`): у
строки формы dim-суффикс `#n` при `formsetOrdinal != null` (после regen
типов). api.ts — без изменений (generic-мост, поле приезжает само).

## 4. Тесты-инварианты

1. Мульти-формсет bare-фикстура (2 FormSet в одном RAW-пакете): каждой
   форме — guid СВОЕГО формсета + ordinal 0/1 (регресс атрибуции).
2. Round-trip list→add: `form add TARGET#1` на фикстуре (1) вставляет
   форму в формсет с ordinal=1 (видно повторным листом).
3. Out-of-range `#2` → NotFound (существующие тести form_add сохранить).
4. PE32-фикстура: формсет внутри resource-блоба → ordinal 0..k; bare-пакет
   вне блоба / FREEFORM-список → None.
5. Обрезанный/мусорный пакет: warn+stop-семантика списка не меняется
   (точное ожидание, не `len() <= 1`).

## 5. Ошибки

Новых кодов нет: NotFound на out-of-range/malformed `#n` уже существует и
не меняется. Список — read-only, ошибок мутации не добавляет.

## 6. Объём и связи

Оценка: план ~6-8 задач (engine-проход+ordinal → proto+реген → CLI → TUI →
WebUI → инвариант-тесты/гейты). Опирается на циклы formset-unlock (add_form,
pre-check I1) и tui-forms-view (Forms view). Закрывает TODO-запись
«FormInfo: formset-порядковый номер не виден».

## Отложенное

- anchor-селектор `TARGET@form:id` — если ordinal окажется неудобен в
  живом использовании (не сейчас).
- `default_stores`-фидбек (R2) — вне цикла, ждёт живого use case.
