# UEFIPatcher — Тестирование (цикл 1: UEFI Engine)

## Стратегия тестирования

### Уровни тестирования
- **Unit-тесты**: types, ffs, parser, builder, ops, setup, storage, session — чистая логика над `FfsNode` и SQLite (tempfile).
- **Интеграционные тесты**: движок↔SQLite↔сессии (через unix-сокет), round-trip парсинг↔сборка.
- **E2E-тесты**: engine binary → uefi-cli → образ → сохранение, сравнение с UEFITool.

### TDD порядок (module-first rule)
1. Объявить `pub mod X;` в `lib.rs`/`mod.rs` + создать файл с тестами
2. `cargo test` — падает (функции не реализованы)
3. Реализация
4. `cargo test` — проходит
5. Commit

## Unit-тесты

### types (uguid)
- `guid_display_uppercase` — `guid_to_upper_string(&g)` возвращает UPPERCASE
- `guid_from_str_roundtrip` — `Guid::from_str("...")` → `guid_to_upper_string` → та же строка
- `guid_invalid` — `Guid::try_parse("not-a-guid")` → Err

### ffs (checksums, wrapping arithmetic)
- `checksum8_known_vector` — сумма байт + checksum = 0 (round-trip)
- `checksum16_known_vector` — аналогично для u16
- `uint24_roundtrip` — `size_to_uint24` → `uint24_to_u32` = исходное значение
- `large_section_threshold` — корректное определение large section (header[0..2] == 0xFF)
- `ffs_file_size_parsing` — small (24б) и large (32б) заголовки

### parser
- `parse_minimal_volume` — синтетический FirmwareVolume → FfsNode
- `parse_ffs_minimal` — FFS-файл с RAW-секцией
- `parse_raw_section` / `parse_multiple_sections` — секции
- `parse_target_guid` / `parse_target_path` / `parse_target_guid_type` / `parse_target_guid_type_index`
- `guid_from_bytes_safe` — не падает при коротком буфере (возвращает Err)

### builder
- `round_trip_volume` — parse → build → байты идентичны
- `round_trip_ffs_with_sections`
- `build_after_removal` — помеченный узел отсутствует
- `build_after_rebuild` — пересобранный узел корректен

### ops
- `rebuild_marks_node` — `rebuild()` устанавливает `Action::Rebuild`
- `insert_into` / `remove` / `replace_full` / `replace_body_only`

### storage (SQLite, tempfile)
- `insert_and_get_session_with_name` — сессия с `name` полем
- `delete_session_metadata_keeps_no_trace` — cascade удаление artifacts
- `list_artifacts_for_session` — список артефактов
- `list_expired` — сессии с истекшим TTL

### session (--purge-artifacts)
- `create_session_with_name` — сессия создаётся с именем
- `destroy_without_purge_keeps_files` — при `purge_files=false` файлы артефактов СОХРАНЯЮТСЯ
- `destroy_with_purge_removes_files` — при `purge_files=true` файлы удаляются

## Интеграционные тесты (gRPC через unix-сокет)

- `grpc_create_and_destroy_session` — CreateSession(name) → ListSessions → DestroySession
- `grpc_open_image_read` — OpenImage(READ) → DumpTree → непустой текст
- `grpc_extract_artifact` — ExtractArtifact(target) → artifact_id непустой
- `grpc_export_artifact` — ExportArtifact(artifact_id, output_path) → файл существует
- `grpc_import_artifact` — ImportArtifact(file_path) → artifact_id
- `grpc_list_artifacts` — ListArtifacts → список с метаданными
- `grpc_insert_from_artifact` — ExtractArtifact → Insert(artifact_id) → элемент в DumpTree
- `grpc_replace_from_artifact` — ImportArtifact → Replace(artifact_id)
- `grpc_insert_from_file` — Insert(ffs_path) → элемент в DumpTree
- `grpc_remove` / `grpc_replace_body_only` / `grpc_rebuild`
- `grpc_set_visibility`
- `grpc_save_image_round_trip` — OpenImage → SaveImage → повторный open → идентичен
- `grpc_unauthorized_request` — без токена → UNAUTHENTICATED

### Session GC
- `gc_removes_metadata_keeps_files` — при `purge_artifacts=false` GC удаляет metadata, файлы остаются
- `gc_purges_all` — при `purge_artifacts=true` GC удаляет metadata + файлы

## E2E-тесты (engine binary + uefi-cli)

- **Round-trip**: `engine --data-dir /tmp/test` → `uefi-cli open` → `dump` → `save` → сравнить
- **Artifact flow**: `extract TARGET` → `export ARTIFACT_ID out.bin` → `import out.bin` → `insert --from-artifact`
- **Setup visibility**: `set-visibility ITEM --hidden` → `save` → пункт скрыт
- **Сравнение с UEFITool**: dump через uefi-cli vs UEFIEdit

## Тестовые данные

- Синтетические фикстуры создаются в тестах (не хранятся в репо)
- Реальные BIOS-образы: через `UEFIPATCHER_TEST_FIXTURES` env var

## CI/CD

```bash
cargo test --all                    # все тесты
cargo clippy --all -- -D warnings   # lint
cargo fmt --all -- --check          # форматирование
```

Контейнерная сборка:
```bash
podman build -t uefipatcher-engine -f docker/engine.containerfile
```
