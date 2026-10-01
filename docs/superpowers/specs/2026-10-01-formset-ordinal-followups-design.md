# Спека мини-цикла formset-ordinal-followups: формсет-скоуп varstore-семантики + фоллоу-апы ревью

Дата: 2026-10-01 · Статус: дизайн одобрен владельцем, ждёт плана
Первоисточник: TODO.md «Отложенное финального ревью formset-ordinal
(2026-10-01)» — 4 записи. Родитель: спека formset-ordinal
(`2026-10-01-formset-ordinal-design.md`); смежные: varstore-contract,
formset-unlock (§3 U3), hii-read-truth (§5, read/write-сплит рангов).

Решения владельца: (1) скоупинг и чтения, и write-валидации; (2) W3
(`question add` envelope: проверки + вставка деклараций) — целиком;
(3) подход A (scoped-хелперы поверх существующих проходов).

## 1. Проблема

Четыре отложенных пункта финального ревью `feat/formset-ordinal`:

1. `hii varstore list TARGET[#n]` игнорирует ординал: `list_varstores`
   (hii/mod.rs) парсит `#n` и отбрасывает; `values::varstore_map` собирает
   декларации со всего пакета — формсеты слиты. Справка CLI обещает выбор
   формсета. Пиннящий тест «карта не зависит от formset-ординала»
   фиксирует неверную премиссу §1 спеки formset-ordinal.
2. PE с дублирующимися resource-записями (одинаковые off,len): writable
   в collect_forms сравнивается по значению (`ranges.first()`), дубль
   матчится у обеих записей (hii/forms.rs:116-131). Патология, живых
   прецедентов нет.
3. Нет явной фикстуры «вторая resource-запись → ordinal None» — покрытие
   только косвенное.
4. `print_forms` (CLI): TSV-обвязка колонки `n` без прямого теста
   (unit бьёт только `ordinal_cell`).

При исследовании write-пути найден пятый, производный от (1):

5. `question add` envelope-поток формсет-слеп: `check_question_add`/
   `add_varstores` валидируют varstore-id по **всем** form-пакетам
   (`form_package_ranges`), а `add_varstores` вставляет декларации в
   пролог **первого** формсета writable-пакета (`splice_varstore_ops` без
   индекса), независимо от формсета целевой формы. На multi-formset
   пакете декларации молча уезжают в чужой формсет. `form add` при этом
   формсет-осведомлён (`formset_insert_points`) — но его
   `validate_form_varstores` тоже проверяет по всему пакету: коллизия id
   с декларацией соседнего формсета ложно отвергает мутацию.

Грамматики не меняются: `hii varstore list`/`hii form add` — `TARGET#n`
(ординал формсета), `question add` — `TARGET#FORM` (form_id; формсет
выводится из формы). W2 (`hii form import` CLI: list → plan → form add)
отдельного кода не требует — становится консистентным через (1) и
скоуп `validate_form_varstores`.

## 2. Решение: подход A — scoped-хелперы

| Примитив | Контракт |
|---|---|
| `ifr::formset_spans(pkg) -> Option<Vec<Range<usize>>>` | Спаны всех FORM_SET (конец заголовка i-го FORM_SET_OP → следующий FORM_SET_OP / конец пакета). Гейт `is_form_package`; malformed → None. Единый источник границ read/write (HII-чек-лист §6). |
| `values::varstore_map_formset(pkg, idx) -> Option<Vec<VarStoreMap>>` | Разбор ops как у `varstore_map`, записываются только ops внутри спана idx (offset живёт локально, `VarStoreMap` не меняется). None = out-of-range/malformed. `varstore_map` остаётся для read-потребителей (form_export и др.). |
| `ifr::locate_form_attribution(pkg, form_id) -> Option<(usize, span)>` | Первый формсет, содержащий форму, + её спан. Первый-match — текущее поведение `locate_form`. |
| `splice_varstore_ops(package, ops, formset_idx)` | Вставка перед первой формой i-го формсета (форм у формсета нет — NotFound, как сегодняшний formless-контракт), с обновлением u24-длины. Сигнатура меняется: потребитель один — `add_varstores` (+ тесты). В атрибутированном потоке ветка недостижима: формсет содержит целевую форму. |

### 2.1 Потребители

- **`list_varstores`**: `#n` больше не отбрасывается — гейт
  `ifr::formset_at(pkg, n)` (out-of-range → NotFound, зеркало `add_form`),
  карта = `varstore_map_formset(pkg, n)`. Пиннящий тест заменяется на
  scoped-ожидания; rustdoc-контракт переписывается.
- **`validate_form_varstores`** (form_add): принимает `formset_idx` из
  `add_form`; коллизии и ссылки — в формсете n. Текст «already exists in
  the formset» становится честным.
- **`resolve_question_target`**: `QuestionTarget.formset_idx` (атрибуция
  через `locate_form_attribution`); `check_question_add` (петля по всем
  рангам → scoped-карта), `check_question_slots` (`declared_size` из
  scoped-карты).
- **`add_varstores`**: атрибуция формсета целевой формы; форма обязана
  существовать (bogus TARGET#FORM → NotFound — ужесточение, раньше
  вставка молча шла в первый формсет). Валидация scoped; splice в пролог
  формсета-владельца (RAW и resource-ветки получают idx).

## 3. collect_forms и CLI (пункты 2-4)

- **writable-детект**: `enumerate()` по рангам, writable = `idx == 0`;
  диапазоны с уже слитым `(off, len)` пропускаются. Дубль-запись не
  рождает ни ложный ordinal, ни дублирующиеся строки. Выравнено с
  `resource_forms_package` (первая запись, без fallback).
- **Фикстура**: `synth_hii_pe` получает вариант на N resource-записей
  (сейчас зашита одна). Тесты: две различные записи → формсеты второй
  получают `None`; две идентичные (два directory-entry на один offset) →
  один набор строк.
- **`print_forms`**: TSV-сборка (заголовок + хвост строки) извлекается в
  чистые функции рядом с `ordinal_cell`; прямые unit-тесты заголовка с
  `n` и строк `#1`/`—`.

## 4. Тесты-инварианты

1. `list_varstores` на 2-формсетной фикстуре: #0 ≠ #1 (id/guid
   разделены), #2 → NotFound; мусорный `#x` → NotFound (есть).
2. `form add TARGET#1` со схемой, чей varstore-id коллидирует с
   декларацией формсета #0, — проходит, декларация встаёт в #1.
3. `question add` envelope на multi-formset: целевая форма во втором
   формсете → декларации в прологе второго формсета, spf-сдвиг корректен.
4. `add_varstores` с несуществующей формой → NotFound.
5. Round-trip инвариант formset-ordinal (list→add) не регрессирует.
6. Обрезанный/мусорный пакет: warn+stop-семантика списков не меняется
   (точное ожидание, не `len() <= 1`).

## 5. Ошибки

Новых кодов нет. NotFound расширяется: out-of-range `#n` у
`list_varstores` (сегодня — карта со всего пакета для любого n) и
bogus-форма у `add_varstores` (сегодня — молчаливая вставка в первый
формсет). Оба — честные ужесточения, пинняются тестами.

## 6. Объём и связи

Оценка: план ~7 задач (примитивы+list → form add → question-add
атрибуция+проверки → add_varstores splice → collect_forms+фикстура →
CLI-форматтер → TODO-закрытие+гейты). Опирается на formset-ordinal
(`parse_form_package_sets`, ordinal-каналы), varstore-contract,
formset-unlock (splice_varstore_ops). TUI/WebUI/RPC-прото не
трогаются: поверхностей varstore-add нет, грамматика та же, поля те же.
Закрывает все 4 записи TODO-секции «Отложенное финального ревью
formset-ordinal».

## 7. Отложенное

- `form_export::fill_varstores` scoping (read-путь, lossy-контракт) —
  при живом кейсе кросс-формсетных коллизий id в экспорте.
- Прямая адресация `varstore add TARGET#n` без формы — потребитель один
  (question-add envelope), отдельная грамматика не вводится.
- anchor-селектор `TARGET@form:id` — наследовано из formset-ordinal.
