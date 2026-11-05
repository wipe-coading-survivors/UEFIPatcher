# HOWTO: сквозной workflow CLI

Базовые приёмы работы с `uefi-cli`: запуск движка, сессии, образы, дерево
узлов, артефакты. HII/NVAR/TSE-операции — в профильных HOWTO (см.
[указатель](../../README.md#howto)).

## 1. Движок

CLI — тонкий клиент; вся работа происходит в движке (RPC over unix-сокет).

```bash
# перед стартом удалить осевший сокет прошлой сессии
# (клиент на мёртвом сокете падает с transport error — это окружение, не код)
rm -f "${UEFIPATCHER_SOCK:-$HOME/.local/state/uefipatcher/uefipatcher.sock}"

./target/debug/engine          # или: UEFIPATCHER_SOCK=/tmp/x.sock ./target/debug/engine
```

Сокет для клиента: `--sock <path>` > `UEFIPATCHER_SOCK` > state-файл
`.uefipatcher` > дефолт. При `transport error (RPC_INTERNAL)` первым делом
проверить жив ли движок (`pgrep -x engine`) и совпадает ли сокет.

## 2. Сессии и образы

```bash
uefi-cli session init --name work --force   # создать сессию (+ state-файл)
uefi-cli session list
uefi-cli image open refs/fw/образ.bin --mode write --name my   # путь на ФС движка
uefi-cli image list                        # открытые образы
uefi-cli image status                      # активный образ (узлы/размер/режим)
uefi-cli image switch <image_id>
uefi-cli image save /tmp/out.bin           # выгрузить на ФС движка
uefi-cli image close
```

Правила:

- **`--mode write` обязателен для любых мутаций** (unlock/set-value/add/
  insert/replace/remove). Read-режим открывается по умолчанию и дешевле.
- `image open` читает файл **на стороне движка** — в docker-развертывании
  используйте TUI `:upload PATH` или WebUI (шлют байты напрямую).
- Каждая мутация write-through: движок пересобирает и валидирует образ
  (validate-before-persist) после каждой операции. Ошибка операции = байты
  на диске не тронуты.
- `image save` выгружает текущее состояние; исходный файл не перезаписывается.

## 3. Дерево узлов

Образ — дерево: регионы дескриптора → FV-тома → FFS-файлы → секции (сжатые
секции прозрачно распаковываются, у узлов сохраняются исходные слоты).

**Адресация TARGET** (стабильна между запусками, предпочтительна путям):

```
<ffs-guid>:<section-type>:<index>     # напр. 899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0
<tree-path>                           # напр. 3/28/1/0 (том/файл/секция/...)
```

Позиционные пути нестабильны на full-flash (gap-узлы сдвигают индексы) —
GUID-target предпочтителен.

```bash
uefi-cli node list                     # дерево активного образа
uefi-cli node search <substr>          # поиск по имени/GUID
uefi-cli node extract <target> [--body-only]   # тело узла → артефакт
uefi-cli node insert <target> --file f.ffs --mode after   # into|before|after
uefi-cli node replace <target> --file body.bin [--body-only]
uefi-cli node remove <target>
uefi-cli node rebuild <target>
```

Нюансы:

- `node insert` печатает id **таргета**, а не нового узла; путь нового узла =
  путь таргета + 1 по последнему индексу (при `--mode after`).
- Мутации **внутри сжатых секций** (LZMA/GUIDed) поддержаны: движок сам
  распаковывает при парсинге и рекомпрессирует при сборке (LZMA — со
  слот-фитом; Tiano-компрессия — только декомпрессия, мутации за Tiano-барьером read-only).
- extract через `--artifact` в insert/replace переиспользует сохранённые
  артефакты вместо файлов.

## 4. Артефакты

```bash
uefi-cli artifact list
uefi-cli artifact import <local-file>
uefi-cli artifact export <artifact-id> <server-path>
```

Артефакты живут в data-dir движка, привязаны к сессии, переживают рестарт
движка (TTL сессии).

## 5. Форматы вывода

Глобальный флаг `--format text|tsv|json` — на все подкоманды:

```bash
uefi-cli --format tsv hii form list     # колонки для скриптов
uefi-cli --format json hii form list | jq '.[] | select(.title=="Advanced")'
```

## 6. Типовой скелет сессии

```bash
uefi-cli session init --name exp1 --force
uefi-cli image open ~/bios/orig.bin --mode write --name base
# ...мутации (unlock/set-value/add — см. профильные HOWTO)...
uefi-cli image save ~/bios/patched.bin
uefi-cli image close
uefi-cli session destroy
```

Перед прошивкой — проверить дифф патченного против оригинала
(`cmp`/`radiff2`): правки должны лежать строго в ожидаемых слотах (примеры
байтовых диффов — в отчётах `docs/reports/`). Сама прошивка —
[flashrom-write.md](flashrom-write.md).

## 7. Грабли

- **Мёртвый сокет**: `rm -f` сокета перед стартом движка; `pkill -x engine`,
  не по подстроке.
- **Read-режим**: HII-мутации дают `NotWritable` — открыть образ с
  `--mode write`.
- **Гейты видимости**: форма есть в `hii form list`, но в меню Setup её не
  видно — она скрыта suppress-гейтом или hide-маркером TSE:
  [unlock-hidden-forms.md](unlock-hidden-forms.md).
- **`image save` пишет на ФС движка**, не клиента.
