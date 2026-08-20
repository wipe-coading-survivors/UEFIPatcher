# План: добавление форм/формсетов (фазы A–C)

> Спека: `docs/superpowers/specs/2026-08-14-hii-form-addition-design.md`.
> Порядок: фазы A → B → C; внутри фазы — по задачам строго последовательно.
> Правила: TDD (тест → red → impl → green → commit), module-first, без
> комментариев в коде, после каждой задачи `cargo test -p uefi-engine`
> (для CLI-задач — `-p uefi-cli`) + `cargo clippy -p <crate> -- -D warnings`
> + `cargo fmt -p <crate> -- --check`.
> Фазы B/C исполняются после ревью результата фазы A (правки плана —
> отдельным docs-коммитом до реализации, по правилу 11 AGENTS.md).

## Фаза A — bare-канал + CLI

### Task 1: insert-цель = Volume-узел

1. Тест (`formset_add.rs` tests): образ gap-aware (первый ребёнок root —
   Padding, Volume вторым) → `add_setup_formset` (schema с одной формой)
   → `build_image` → в выводе есть новый FFS (по new_ffs_guid), длина
   образа сохранена. Red.
2. Реализация: в `add_setup_formset` заменить `Target::Path(vec![0])` на
   индекс volume, в котором найден string-пакет (`sp_vi` из
   `find_string_package_section`; фильтр по `target_ffs_guid` уже внутри
   поиска). Поправка 2026-08-20 (ревью фазы A): изначальная формулировка
   «первый `FfsType::Volume`-узел» отброшена — новый FFS ко-лоцируется
   с volume string-пакета, из которого берётся снапшот; Padding-узлы
   парсера без детей, поэтому цель всегда Volume.
3. Green; commit `fix(uefi-engine): formset add inserts into a Volume
   node, not raw child[0]`.

### Task 2: string-поиск через walker

1. Тесты (`string_pack.rs` tests): string-пакет внутри LZMA-обёртки
   (синтетика из фикстуры `lzma_guided_section*` или дерево) —
   `add_strings` находит; PE-resource-канал (`synth_hii_pe("HII", …)` в
   0x10-секции) → `StringPackageNotFound` (честный отказ фазы A).
2. Реализация: `find_string_package` — DFS по дереву (все leaf-секции
   всех subtype с `declared_len_sane` + `is_string_package` — валидный
   пакет по семантике §4.A2 спеки, спуск через обёртки); отдельная
   функция `string_package_section_path` (pub), `add_strings` использует
   путь внутренне — сигнатура не меняется: путь потребителям не нужен.
   Уточнение 2026-08-20: изначальная формулировка «add_strings
   возвращает путь» отброшена — возвращённый путь был бы мёртвым API
   (вызывающая сторона formset_add игнорирует его).
   Поправка 2026-08-20 (ревью фазы A): «`find_string_package_section`
   в formset_add остаётся bare» отброшено — formset_add использует
   ЕДИНЫЙ walker-поиск (`string_package_section_path` с ограничением
   `path.len()==3` — домен bare-канала), и ДО любой мутации. Два
   расходящихся поиска давали: (а) Err после мутации, когда walker
   находит пакет в обёртке, а bare-поиск — нет (частичная мутация
   образа в сессии, дубли строк при retry); (б) безошибочную порчу —
   walker мутирует DFS-ранний пакет в обёртке, bare возвращает прямую
   секцию, снапшот для нового FFS собирается без добавленных строк
   (битые string-id в IFR).
3. Green; commit `feat(uefi-engine): string_pack finds packages through
   wrappers (walker-based)`.

### Task 3: CLI `hii formset add`

1. Proto-RPC, server-handler и mock-ответ `hii_form_set_add` уже есть;
   client-обёртки в `client.rs` нет — добавить вместе с подкомандой.
   CLI: `hii formset add --file <schema.json> [--ffs <guid>]` (активный
   образ через `client.active_image()`, как у остальных hii-команд);
   вывод new_ffs_id + form ids (output.rs).
2. Тесты: unit-парсер аргументов; mock e2e (существующий
   `cli_integration`-паттерн).
3. Green; commit `feat(uefi-cli): hii formset add subcommand`.

## Фаза B — PE-resource writer

### Task 4: `try_grow_rsrc_tail`

1. Тесты (`pe_resource.rs`): `synth_hii_pe` → grow(64) → длина +64×k
   (file-align), SizeOfRawData/VirtualSize/SizeOfImage обновлены, старые
   resource-смещения читаются; не-PE и «.rsrc не последняя» → false,
   байты не тронуты.
2. Реализация через `object`-структуры (read) + точечные правки
   заголовков (write). Поправка 2026-08-20 (префлайт фазы B):
   `try_grow_rsrc_tail` дописывает `delta` нулевых байт в хвост файла и
   обновляет ТОЛЬКО заголовки (SizeOfRawData/VirtualSize по file-align,
   SizeOfImage при последней виртуальной секции); resource data entry
   Size НЕ трогает — точную новую длину записи знает только операция
   уровня блоба (Task 5/6), автоприращение «записи, кончающейся на
   старом конце секции», портит чужие entry на реальных PE, где HII не
   последняя. Утверждение «data-entry size» перенесено в Task 5.
3. Green; commit `feat(uefi-engine): pe_resource try_grow_rsrc_tail`.

### Task 5: `append_package_to_resource`

1. Тесты: synth PE + список (FORM+STRING+END) → append FORM-пакета →
   `parse_package_list` нового блоба видит 2 FORM; u32 total и data
   entry size пересчитаны; END остаётся последним; growth-путь и
   false-путь (нет 'HII').
2. Реализация по спеке §4.B2.
3. Green; commit `feat(uefi-engine): append form package to 'HII'
   resource list`.

### Task 6: string-append в resource-канале

1. Тесты: STRING-пакет в ресурсе → `add_strings_to_body` на срезе →
   длины пакета/списка/ресурса согласованы, старые id сохранены, новые
   id последовательны.
2. Реализация: цепочка смещений resource→list→package; переиспользовать
   `add_strings_to_body`. Поправка 2026-08-20 (префлайт фазы B): точка
   входа — `pub fn add_strings_to_resource(pe: &mut Vec<u8>, strings:
   &[String]) -> Option<HashMap<String, u16>>` в `string_pack.rs`
   (рост/пересчёт длин — общий helper из Task 5); image-level
   `add_strings` остаётся bare-only — фазо-A тест честного отказа
   `add_strings_refuses_pe_resource_channel` сохраняется, resource-ветку
   подключает Task 7 в formset_add.
3. Green; commit `feat(uefi-engine): append strings to resource-backed
   string package`.

### Task 7: wiring + acceptance

1. `add_setup_formset`: pre-check AMI-целей ДО любой мутации — обе
   находки через логику `find_ami_module` (pub(crate)-обёртка), Err
   `AmiFilesNotFound` до `add_strings`; закрывает TODO «частичная мутация
   при ошибке patch_ami» (поправка 2026-08-20 по префлайту фазы B, по
   решению пользователя). Ветка resource-канала (strings → Task 6,
   формсет → append Task 5, новый FFS не собирается);
   mark_rebuild-каскад на PE32-секцию; новый
   `HiiError::PeGrowthUnsupported` (Display по §5 спеки) — отказ роста
   из Tasks 5/6 маппится в него.
2. Real-image `#[ignore]` тест: HNX99TF — добавить формсет в список
   ресурса Platform-файла → build → длина сохранена, вне-FV1 байты
   идентичны, re-parse: формсет в `collect_forms`.
3. Green (вкл. регрессию round-trip); commit `feat(uefi-engine):
   add_setup_formset works on PE-resource layouts (HNX99TF acceptance)`.

## Фаза C — форма в существующий формсет

### Task 8: `insert_form_into_package`

1. Тесты: формсет-пакет (фикстура-подобная синтетика) → вставка формы +
   varstore → баланс END, `parse_form_package` видит новую форму,
   длина выросла ровно на вставку; несуществующий формсет (`#n` за
   пределами) → false.
2. Реализация по спеке §4.C1 (walker симметричен
   `find_form_suppress_scope`).
3. Green; commit `feat(uefi-engine): insert form into existing formset
   package`.

### Task 9: RPC + CLI

1. `HiiFormAdd` (proto: image_id, target, schema_json) → handler →
   engine-функция `hii::add_form` (строки Task 6 + вставка Task 8);
   маппинг ошибок через `hii_error_status` + `PeGrowthUnsupported` →
   `failed_precondition`.
2. CLI `hii form add --target <form_id> --file <schema.json>`.
3. Green; commit `feat(uefi-engine,uefi-cli): form add RPC + subcommand`.

### Task 10: acceptance + финал

1. Real-image `#[ignore]`: форма в Setup-формсет HNX99TF → build →
   re-parse: форма видна с титулом.
2. Полные гейты: `cargo test --all`, `cargo clippy --all -- -D warnings`
   + `-p uefi-engine --all-targets`, `cargo fmt --all -- --check`;
   полный real-image набор 15+ тестов зелёный.
3. Commit `feat(uefi-engine): HNX99TF form-add acceptance + docs`.

## Отложенное (не в этом цикле)

- EDK2 bare-конст-массивы: мутация канала (§2 не-цели спеки).
- TUI/WebUI обёртки над новыми RPC.
- Перенос `find_string_package_section` (formset_add.rs:301) на общий
  walker после Task 2 — выполнен досрочно по ревью фазы A: formset_add
  использует `string_package_section_path` с `path.len()==3`,
  `find_string_package_section` удалён.
