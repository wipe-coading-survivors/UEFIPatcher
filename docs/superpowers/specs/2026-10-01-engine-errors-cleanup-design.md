# Спека: зачистка `engine-errors-cleanup` — u24-splice guard, селекторы add_varstores, add_ref из безстраничной формы

Дата: 2026-10-01. Статус: проект (до вердикта владельца).

Формат аддендума (не мини-цикл): три точечные правки классов `write-path`/`errors` из TODO, без изменения RPC/proto/поведения за пределами описанных отказов. Источники: финальное ревью formset-ordinal-followups (§1, §2), ревью setup-new-page 2026-09-10 (§3).

## §1 splice_*_ops: u24-переполнение длины пакета (класс write-path)

`splice_varstore_ops`/`splice_question_ops` (crates/uefi-engine/src/hii/ifr.rs) пишут `plen + ops.len()` в три заголовочных байта без проверки: пакет с u24-длиной ≈0xFFFFFF после вставки получает молча обрезанную длину (silent-data). Триггер нереалистичен для живых IFR-пакетов (~16MB), но паттерн write-path и общий у обеих функций.

Фикс: проверка ДО вставки (никаких частичных состояний — mut-аргумент остаётся нетронутым при отказе); переполнение → `ValueOpUnsupported("IFR package length … exceeds u24 after splice")`.

Тест: пакет с заголовком u24=0xFFFFFF (малое тело) + ops → Err, пакет байт-идентичен исходимому. Для обеих функций.

## §2 add_varstores: унификация селекторов валидации и splice (класс errors)

Валидация/атрибуция берут пакет через `form_package_ranges(node).next()` — PE32-ветка сканирует ВСЕ resource-записи (`pe_resource_form_packages`), а resource-splice — `form_add::resource_forms_package` (первый forms-пакет только ПЕРВОЙ записи). На multi-entry PE без PACKAGE_FORMS в первой записи валидация разбирает чужой (не мутабельный) пакет второй записи: его коллизии/ошибки всплывают вместо истинной причины, затем splice падает `NotASetupItem`.

Фикс: единый мутационный селектор `mutation_forms_package(node)`: PE32 → первая resource-запись (единственный writable-канал, паритет `collect_forms` из formset-ordinal: writable по индексу записи), остальные каналы — `form_package_ranges().next()` как сегодня. Три места применения: pkg_before, post-splice verify-снимок, (splice уже использует тот же источник внутри `splice_varstore_ops_into_resource`). Вариант ошибки первой записи без форм — `NotASetupItem` (существующая семантика «не мутабельный setup-канал», теперь из одной точки и до разбора схем).

Тест: multi-entry PE (первая запись — strings-only blob, вторая — forms с коллизией varstore id) → `NotASetupItem`, а не InvalidSchema-коллизия чужого пакета; образ не мутирован. Контроль: однозаписный PE и RAW работают как раньше (существующие тести).

## §3 add_ref из формы без $SPF-страницы (класс errors)

`add_ref`/`check_ref_add` переиспользуют `plan_spf_append`, который требует $SPF-страницу родительской формы (`page_slot.ok_or(NotFound)`), хотя REF не добавляет ни страницу, ни запись — использует только `selected_records` (сдвиг ifr_offset'ов после вставки). REF из безстраничной формы → невнятный NotFound.

Фикс (вариант «лёгкий plan_spf_fixup» из TODO): `plan_spf_fixup(body, pkg) -> Result<Vec<usize>, HiiError>` — `container_start` (отсутствие $SPF → NotFound, как сегодня) + `select_resolving_records`; без требований страницы/записей/шаблонов. `apply_spf_ifr_fixup` принимает записи напрямую; `apply_spf_question` передаёт `&plan.selected_records`. Поведенческое расширение: REF из безстраничной формы теперь проходит (безстраничная родительская форма — осознанно допустима; консистентно с `add_ref_allows_dangling_destination_form`), $SPF с нулём записей вопросов — тоже (фиксапу нечего сдвигать).

Тест: форма 10020 (вторая в фикстуре `question_add_spf_body`, страницы имеет только 10019) → `add_ref` проходит, REF в форме, $SPF байт-идентичен (записей за insert point нет); `check_ref_add` — тоже.

## Гейты

`cargo test --all`; `cargo clippy --all --all-targets -- -D warnings`; `cargo fmt --all -- --check`.

## За рамками

- Отдельный вариант ошибки «forms package не в writable-записи» (сообщение вместо NotASetupItem) — при живом прецеденте.
- add_ref валидация цели REF (TODO «висячий GOTO») — отдельная тема.
- Живой гейт владельца не требуется: синтетические фикстуры покрывают оба канала; существующие real-гейты не затронуты.
