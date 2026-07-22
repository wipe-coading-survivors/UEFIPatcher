# UEFIPatcher — Тестирование (цикл 1: UEFI Engine)

## Стратегия тестирования

### Уровни тестирования
- **Unit-тесты**: парсер, builder, setup, target-поиск, storage — чистая логика над `FfsNode` и SQLite (in-memory).
- **Интеграционные тесты**: движок↔SQLite↔сессии (через реальные unix-сокет/токены), round-trip парсинг↔сборка.
- **E2E-тесты**: полный сценарий CLI-клиент → движок → образ → сохранение, сравнение с UEFITool 0.28.8.
- **Manual-тесты**: проверка на реальных BIOS-образах (из `../refs`), сверка с UEFITool.

### Целевые метрики
- Покрытие unit-тестами: ≥80% для крейта `uefi-engine` (parser/builder/setup/target), ≥60% для storage/rpc.
- Покрытие интеграционными тестами: все gRPC-методы `EngineService` (позитив + негативные сценарии).
- Round-trip: 100% тестовых образов — `parse → build` бинарно идентичен исходнику.

## Unit-тесты

### parser
- Тестируемый функционал: чтение UEFI-образа в дерево `FfsNode`, парсинг `Target`.
- Test Cases:
  - `parse_image_valid_minimal_volume` — минимальный FirmwareVolume → дерево с root
  - `parse_image_valid_ffs_with_sections` — FFS-файл с PE32/GUIDed-секциями → дерево с детьми
  - `parse_image_compressed_lzma` — LZMA-compressed-секция → распакованное поддерево
  - `parse_image_unknown_section_preserved_as_is` — неизвестный тип секции → body сохранён as-is, WARN в логе
  - `parse_image_invalid_header` — битый заголовок → `ParserError::InvalidHeader`
  - `parse_target_guid` — `5C60F367-A505-419A-859E-2A4FF6CA6FE5` → `Target::Guid`
  - `parse_target_path` — `0/2/207/1/0` → `Target::Path([0,2,207,1,0])`
  - `parse_target_guid_type` — `899407D7-...:0x10` → `Target::GuidType(_, 0x10, None)`
  - `parse_target_guid_type_index` — `899407D7-...:0x10:2` → `Target::GuidType(_, 0x10, Some(2))`
  - `parse_target_invalid` — `not-a-target` → `ParserError::InvalidTarget`

### builder
- Тестируемый функционал: сборка дерева в байты, выравнивание.
- Test Cases:
  - `build_round_trip_minimal_volume` — parse → build → байты идентичны исходнику
  - `build_round_trip_ffs_with_sections` — то же для FFS с секциями
  - `build_round_trip_compressed` — то же для сжатых секций (re-compress при rebuild)
  - `build_with_padding` — дерево с gap → padding-байты корректны (0xFF)
  - `build_after_removal` — узел помечен `marked_for_removal` → отсутствует в выводе, выравнивание пересчитано
  - `build_after_rebuild` — узел помечен `marked_for_rebuild` → пересобран, размер/offset обновлены

### setup
- Тестируемый функционал: парсинг IFR, `set_item_visibility`.
- Test Cases:
  - `ifr_parse_valid_form` — IFR с формой и пунктами → дерево форм
  - `set_visibility_hide_item` — `visible=false` → флаг `suppressed` установлен в IFR-байтах
  - `set_visibility_show_item` — `visible=true` → флаг `suppressed` снят
  - `set_visibility_invalid_item` — item_id не пункт Setup → `SetupError::NotASetupItem`

### session
- Тестируемый функционал: создание, touch, удаление, TTL.
- Test Cases:
  - `create_session_returns_id_and_token` — UUID + непустой токен
  - `touch_session_updates_last_activity` — `last_activity` увеличилось
  - `destroy_session_removes_from_db` — сессия отсутствует в `list_sessions`
  - `gc_removes_expired_sessions` — сессия с `last_activity < now - TTL` удалена
  - `gc_keeps_active_sessions` — сессия в пределах TTL осталась

### storage
- Тестируемый функционал: CRUD sessions/artifacts, файлы артефактов.
- Test Cases:
  - `insert_and_get_session` — запись/чтение сессии
  - `delete_session_cascades_artifacts` — удаление сессии удаляет её artifacts-строки
  - `store_artifact_file_writes_bytes` — файл создан по пути `${DATA}/sessions/<id>/images/<image_id>.bin`
  - `list_expired_sessions` — возвращает только сессии с `last_activity < threshold`

### rpc/auth
- Тестируемый функционал: проверка токена из metadata.
- Test Cases:
  - `auth_valid_token` — `Authorization: Bearer <valid>` → запрос пропущен
  - `auth_invalid_token` — чужой токен → `UNAUTHENTICATED`
  - `auth_missing_token` — нет заголовка → `UNAUTHENTICATED`

## Интеграционные тесты

### gRPC EngineService (через unix-сокет)
- Тестируемый функционал: все методы `EngineService` end-to-end через unix-сокет.
- Test Cases:
  - `grpc_create_and_destroy_session` — CreateSession → DestroySession → ListSessions пусто
  - `grpc_open_image_read` — OpenImage(READ) → DumpTree → непустой текст
  - `grpc_open_image_write_copies_artifact` — OpenImage(WRITE) → артефакт в `${DATA}/sessions/<id>/images/`
  - `grpc_dump_tree_text_and_tsv` — оба формата возвращают корректный вывод
  - `grpc_list_items` — ListItems возвращает ненулевой список для реального образа
  - `grpc_find_item_by_guid` — FindItem(GUID) → item_id
  - `grpc_find_item_by_path` — FindItem(PATH) → item_id
  - `grpc_find_item_not_found` — FindItem(несуществующий GUID) → `NOT_FOUND`
  - `grpc_insert_into` — Insert(INTO) → элемент присутствует в DumpTree
  - `grpc_insert_before_after` — Insert(BEFORE/AFTER) → порядок в DumpTree корректен
  - `grpc_remove` — Remove → элемент отсутствует в DumpTree
  - `grpc_replace_full` — Replace(full) → новый элемент на месте старого
  - `grpc_replace_body_only` — Replace(body_only) → заголовок старый, тело новое
  - `grpc_rebuild` — Rebuild → пересобранный элемент корректен
  - `grpc_set_visibility` — SetSetupItemVisibility → DumpTree показывает изменение
  - `grpc_save_image_round_trip` — OpenImage → SaveImage → повторный OpenImage(WRITE) → DumpTree идентичен
  - `grpc_chained_edits_before_save` — несколько edit-методов → все видны в DumpTree до SaveImage
  - `grpc_unauthorized_request` — без токена → `UNAUTHENTICATED`

### Session GC (через unix-сокет)
- Тестируемый функционал: фоновый GC удаляет неактивные сессии.
- Test Cases:
  - `gc_deletes_inactive_session` — CreateSession, ждать > TTL (с мок-временем или малым TTL в тесте), ListSessions → пусто
  - `gc_deletes_artifact_files` — после GC файлы `${DATA}/sessions/<id>/` отсутствуют

## E2E-тесты

### Полные сценарии

- **Сценарий «Dump образа через CLI»**:
  1. Запустить `uefi-engine` (unix-сокет, тестовый `${DATA}`)
  2. `uefi-cli session create` → session_id, token
  3. `uefi-cli <session_id> open <test_image.bin> --mode READ`
  4. `uefi-cli <image_id> dump --format text` → вывод дерева
  5. Ожидаемый результат: вывод аналогичен `UEFIEdit <image> dump`

- **Сценарий «Round-trip save»**:
  1. `uefi-cli` open (WRITE), dump, save → `output.bin`
  2. Сравнить `output.bin` с исходником — бинарно идентичны
  3. Повторно open `output.bin`, dump — идентичен первому dump

- **Сценарий «Вставка + удаление + save»**:
  1. open (WRITE)
  2. insert TARGET file.ffs --mode before
  3. remove TARGET
  4. save → `output.bin`
  5. Сравнить dump `output.bin` с dump исходника — идентичны (вставка+удаление = no-op)

- **Сценарий «Скрытие пункта Setup»**:
  1. open (WRITE) образ с Setup-секцией
  2. find Setup-пункт по GUID
  3. set-visibility <item_id> --visible false
  4. save → `output.bin`
  5. Открыть `output.bin` в UEFITool → пункт скрыт

- **Сценарий «Сравнение с UEFITool 0.28.8»**:
  1. Для каждого тестового образа: `UEFIEdit <image> dump` vs `uefi-cli ... dump`
  2. Ожидаемый результат: деревья эквивалентны (с точностью до формата вывода)

## Тестовые данные

### Фикстуры
- `tests/fixtures/minimal_volume.bin` — минимальный FirmwareVolume (синтетический)
- `tests/fixtures/ffs_with_pe32.bin` — FFS с PE32-секцией (синтетический)
- `tests/fixtures/compressed_lzma.bin` — LZMA-compressed-секция (синтетический)
- `tests/fixtures/real_bios_*.bin` — реальные BIOS-образы (из `../refs` или предоставленные пользователем; в репозитории НЕ хранятся, путь указывается через переменную окружения теста)
- `tests/fixtures/sample.ffs` — отдельный FFS-файл для тестов вставки

### Моки и стабы
- `MockDb` (in-memory rusqlite) — для unit-тестов `session` без реальной БД
- `MockEngineServer` — заглушка `EngineService` для тестов `uefi-cli` без реального движка
- Стаб `time` — для тестов GC с управляемым временем (через `tokio::test` + mock time или малый TTL в тестах)

## CI/CD

### Пайплайн тестирования
- **При пуше**: `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test --lib` (unit)
- **При PR**: добавляются `cargo test --test '*'` (интеграционные) + E2E на синтетических фикстурах
- **При релизе**: E2E на реальных BIOS-образах (через podman-pod с секретами), сравнение с UEFITool 0.28.8

### Запуск тестов
- Все тесты: `cargo test --all`
- Unit-тесты: `cargo test --lib`
- Интеграционные тесты: `cargo test --test '*'`
- E2E-тесты: `cargo test --test e2e` (требует `UEFIPATCHER_DATA` для временного каталога; фикстуры реальных образов через `UEFIPATCHER_TEST_FIXTURES`)
- С покрытием: `cargo tarpaulin --all --out html`
- Контейнерные тесты: `podman build -t uefipatcher-engine -f docker/Dockerfile.engine && podman pod ...`