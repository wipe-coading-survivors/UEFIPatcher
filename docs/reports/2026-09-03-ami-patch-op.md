# Отчёт: мини-цикл «ami-patch-op» — patch_ami пишет в payload-секции (BLOCKER E17 закрыт)

> 2026-09-03, ветка `fix/cycle6-reimplent`. Спека:
> `docs/superpowers/specs/2026-09-03-ami-patch-op-design.md`, план:
> `docs/superpowers/plans/2026-09-03-ami-patch-op.md`. Закрывает TODO-пункт
> «E17 → баг: BLOCKER — patch_ami теряет записи при ребилде».
> Адресат — в т.ч. параллельная исследовательская сессия: §3–5 = что уже
> разведано про $SPF и что можно переиспользовать.

## 1. Что было (BLOCKER)

`patch_ami` редактировал **body ФАЙЛА** (`setupdata.body.extend_from_slice`,
`amitse.body.splice`) и ставил `mark_rebuild_to_root_by_path(&[vi, fi])`.
Но `build_file` при Rebuild собирает body **из children-секций** — правка
файла выбрасывалась. E17: файлы AMITSESetupData/AMITSE в собранном образе
байт-в-байт равны оригиналу. Unit-тесты этого не ловили: их фикстуры были
файлами **без детей** (`children=[]` → build_file берёт `node.body`).

## 2. Что сделано

Коммиты `61b9c31..7f3308a` (TDD, задача-за-задачей):

1. **Task 1 (`d27d455`)** — payload-finders: `find_payload_path` (DFS за
   leaf-секцией через обёртки, зеркало `walk_for_pe_resource_channel`) +
   предикаты `is_pfs_payload` (leaf-секция, в `body[..0x40]` сигнатура
   `b"$SPF"`) и `is_pe32_payload` (leaf subtype 0x10). `precheck_ami_modules`
   усилен: файл без payload → `AmiFilesNotFound`; кандидат за
   non-recompressable-обёрткой → `MutationBehindCompression`. Инвариант
   атомарности `c316da9` сохранён (precheck до мутаций строк). UI-fallback
   `find_ami_module` понимает HNX-имя `AMITSESetupData`.
2. **Task 2 (`82fe7e6`)** — `patch_ami` пишет в **payload-секции**: SDP-записи
   аппендятся в конец тела $SPF-секции, form-id пары splice'ятся в тело
   PE32-секции AMITSE (маркер = bytes[12..14] GUID формсета; не найден →
   конец тела); `mark_rebuild_to_root_by_path` — от пути секции. Rebuild
   каскад поднимается через LZMA-обёртку/файл/том. Фикстуры formset_add и
   rpc-тестов переведены на файлы с payload-секциями.
3. **Task 3 (`cd5455c`)** — синтетический LZMA slot-fit гейт: обёртки
   собираются `compress_lzma` (+нулевой паддинг слота), после build дифф
   confined к регионам AMI-файлов, записи видны в re-parse.
4. **Task 4 (`2b66f3c`)** — regression-пины атомарности formset_add: нет
   $SPF-секции / $SPF за Tiano → отказ ДО мутации строк (снапшот-ассерты).
5. **Task 5 (`7f3308a`)** — real-image гейт `real_image_hii_formset_add_ami_records_in_built_bytes`
   (HNX99TF): после `add_setup_formset` → `build_image` → SDP-запись
   (108 байт, layout `make_ami_record`) в хвосте $SPF-секции; form-id пара
   в PE32 AMITSE (splice @pe+0x2561 — маркер встретился в теле); flash-дифф
   confined ровно к трём Rebuild-файлам (Setup-модуль + оба AMI). 23/23
   `#[ignore]`-тестов зелёные; `cargo test --all`, clippy, fmt — чистые.

Формат записей и семантика полей НЕ менялись (см. §5).

## 3. Разведка $SPF-контейнера (HNX99TF, побайтово) — переиспользуйте

Живое дерево (движок, vi=3 = FV2):

| Файл | Секции |
|---|---|
| `FE612B72` AMITSESetupData @0xa9c708 (RAW, 0xC2DC) | GUIDed-LZMA (слот 0xC2C4) → **0x18** FREEFORM_SUBTYPE_GUID (body 0x77174 = $SPF-контейнер) + UI |
| `B1DA0ADF` AMITSE @0xa780a8 (DRIVER, 0x23890) | GUIDed-LZMA (слот 0x23878) → **PE32** (body 315264) + UI |

Контейнер `$SPF` (тело секции 0x18):

```
+0x00  GUID секции (= GUID файла FE612B72)
+0x10  сигнатура "$SPF" (24 53 50 46)
+0x14  u32 0x200, +0x18 u32 0x210 (первый блок? хедер 0x200)
+0x24  GUID EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 (varstore «Setup»)
+0x38  u32 0x48 (=72, размер записи!), +0x3c u32 0x60
+0x40  u32-таблица смещений (0x7dbc, 0x73f0c, 0x76614, 0x7663c, 0x770cc, 0x77164)
       далее ещё таблицы (страниц? @0x60..: 02 00 03 00 04 00; @0x70+: 0xbc, 0x358, 0x3a4, 0x444, …)
```

Живые **Table-3 записи = 72 байта** (0x48), найдены сканом (4G-вопрос
@body+0x99E8, qid 0x3B; соседние записи стридом 0x48):

```
+0  qid u32          +12 pageId u16 (0xffff = нет)   +16 access (0x09)
+20 help-id u32      +24 u32 (инкрементится на запись: 0x63,0x64,0x65…)
+28 ifr-offset u32 (абсолютное смещение ONE_OF в form-пакете — правило E9)
+36 8×0xff…f8        +44 01 00 01 00                  +48 prompt-id u32
+52 failsafe         +53 optimal                      +54..72 нули
```

Подтверждает TODO value-op v3 (та же раскладка, но теперь известен размер
записи и стрид). UEFI-Editor regex (`scripts.ts:100`) те же записи ищет
сканом по всему телу — блочную структуру не парсит.

## 4. Что осталось (следующий цикл — «$PFS-формат + TSE-видимость»)

1. **Writer записей не соответствует живому формату**: текущий
   `make_ami_record` = 108 байт, page@24/access@32/fs@104/opt@106 (старая
   интерпретация «hex-чаров», TODO value-op). Живой формат = 72 байта
   (§3). Поля, которых движок сейчас не знает: help-id/prompt-id/ifr-offset
   (+24-инкремент) — берутся из form-пакета/строк добавляемого формсета.
2. **Аппенд в хвост $SPF не регистрирует записи** в блоках/таблицах
   контейнера (u32-таблицы §3) — AMITSE их не увидит. Нужен парсер
   блочной структуры (первый блок начинается ~0x210; записи 0x48-стридом
   где-то в блоках) и регистрация по правилам контейнера. Вставка в
   середину сдвинет абсолютные смещения — урок E9.
3. **AMITSE form-id splice** — эвристика по 2 байтам GUID[12..14] (в
   PE32 нашлись ложные вхождения: splice попал @pe+0x2561, не в конец);
   семантика «где AMITSE хранит списки form-id формсетов» не разведана.
4. HW-валидация класса «TSE-видимая страница» (E18): вставка +
   зарегистрированные записи + REF-инъекция (после E16: без page-метаданных
   страницы невидимы и непроходимы).

## 5. Ограничения текущей реализации (осознанные)

- Слот LZMA: прирост декомпрессата сжимается с запасом; переполнение слота
  не молчит — секция растёт, файл растёт, дальнейшие файлы FV сдвигаются
  (наблюдалось на synthetic-фикстуре со слотом впритык; на HNX у Setup-модуля
  запас есть, у мелких модулей может не быть — target formset_add выбирать
  осознанно; при исчерпании FV → `SizeMismatch`).
- Реальный образ требует явного `target_ffs_guid` (Setup 899407D7): walker
  без target берёт первый PE32-with-strings — на HNX это мелкий модуль без
  запаса слота.
- PE checksum модулей не пересчитывается (унаследованное, TODO).
- `find_ami_module` сканирует только верхний уровень томов (nested-FV не
  спускается — наследие).
