# UEFIPatcher

Инструментарий для исследования и модификации UEFI BIOS (семейство AMI Aptio V):
парсинг и байт-точная пересборка full-flash образов, работа с HII Setup-формами
(unlock скрытых страниц, запекание значений по умолчанию, добавление форм и
вопросов), AMI NVAR-сторы, setup-браузер AMITSE. Ключевые сценарии
аппаратно-валидированы на живых платформах: Huananzhi X99-TF, ASRock
E3C226D2I, Lenovo RD450x.

Прошивка подготовленного образа — всегда вручную (flashrom / BMC / внешний
программатор); см. [docs/howto/flashrom-write.md](docs/howto/flashrom-write.md).

## Компоненты

| Крейт | Назначение |
|---|---|
| `crates/uefi-engine` | движок: парсер/билдер образов, HII/NVAR/TSE-операции, RPC-сервер (unix-сокет) |
| `crates/uefi-proto` | контракт движка (protobuf, единственный источник `proto/engine.proto`) |
| `crates/uefi-common` | общий код: state-файл, форматирование, CLI-парсинг |
| `crates/uefi-cli` | CLI-клиент (`uefi-cli`) |
| `crates/uefi-tui` | терминальный UI (`uefi-tui`) |
| `crates/uefi-gateway` + `webui/` | HTTP-шлюз и WebUI (SvelteKit) |

## Возможности

- **Парсинг full-flash**: Intel flash-дескриптор и регионы (ME/$FPT), FV/FFS/секции,
  LZMA- и Tiano-сжатие (декомпрессия + слот-фит рекомпрессия), gap-aware
  байт-точный round-trip (проверен на 16 МиБ живых образах).
- **HII Setup**: листинг форм/вопросов/строк/varstore; карта гейтов
  (SUPPRESS_IF/GRAYOUT_IF) и операция unlock; запекание значений по умолчанию
  в NVAR `StdDefaults` (`set-value`); добавление формсетов/форм/вопросов/$SPF-страниц;
  экспорт/импорт форм в JSON; подмена страницы (`hijack`).
- **NVAR**: листинг и правка переменных AMI NVRAM-сторов (обе копии стора).
- **TSE**: инвентарь stride-таблиц AMITSE (`tse report`), снятие hide-маркера
  формсета (`tse unhide`).
- **Сессии/безопасность**: снапшоты образа, артефакты, TTL сессий, read/write
  режимы открытия образов.

## Сборка

```bash
cargo build                 # рабочее пространство Rust (edition 2024)
cd webui && npm install && npm run build   # WebUI (SvelteKit)
```

Контейнеры: `<component>.containerfile` в `docker/`, базовый образ
`registry.fedoraproject.org/fedora:44`, Rust-сборки наследуются от
`docker/rust-builder.containerfile`.

## Запуск

```bash
# движок (RPC over unix-сокет)
rm -f "$UEFIPATCHER_SOCK"   # осевший сокет прошлой сессии валит клиентов
./target/debug/engine

# CLI (в отдельном терминале)
./target/debug/uefi-cli session init --name work
./target/debug/uefi-cli image open <образ.bin> --mode write
```

Переменные окружения:

| Переменная | Назначение | По умолчанию |
|---|---|---|
| `UEFIPATCHER_DATA` | корень данных | `~/.local/share/uefipatcher` |
| `UEFIPATCHER_SOCK` | unix-сокет движка | `~/.local/state/uefipatcher/uefipatcher.sock` |
| `UEFIPATCHER_SESSION_TTL_SECS` | TTL сессии | 864000 (10 дней) |
| `UEFIPATCHER_GATEWAY_LISTEN` | адрес шлюза | 0.0.0.0:8080 |

## HOWTO

Пошаговые сценарии с реальными командами (все — на живых образах; статус
аппаратной валидации указан в каждом документе):

| Документ | Сценарий |
|---|---|
| [cli.md](docs/howto/cli.md) | сквозной workflow CLI: движок, сессии, образы, узлы, артефакты |
| [flashrom-write.md](docs/howto/flashrom-write.md) | прошивка подготовленного образа: flashrom, регионы, BMC |
| [unlock-hidden-forms.md](docs/howto/unlock-hidden-forms.md) | открытие скрытых страниц/вопросов: IFR-гейты + hide-маркер TSE |
| [setup-section-intelrc.md](docs/howto/setup-section-intelrc.md) | добавление своего раздела в Setup (паттерн E43) |
| [serial-console-huananzhi.md](docs/howto/serial-console-huananzhi.md) | серийная консоль на платформе, где её нет (донор-цепочка) |
| [asrock-baked-defaults.md](docs/howto/asrock-baked-defaults.md) | запекание дефолтов BIOS (StdDefaults/set-value) |
| [page-hijack.md](docs/howto/page-hijack.md) | свои тексты на существующей странице Setup |
| [bifurcation-rd450x.md](docs/howto/bifurcation-rd450x.md) | PCIe-бифуркация RD450x — **в работе** (конспект саги) |

## Проверка

```bash
cargo test --all
cargo clippy --all --all-targets -- -D warnings
cargo fmt --all -- --check
cd webui && npm run check
```

Детали тестирования — [TESTING.md](TESTING.md).

## Структура репозитория

- `docs/howto/` — пользовательские сценарии (см. выше)
- `docs/reports/` — отчёты реверс-сессий и аппаратной валидации (первоисточники HOWTO)
- `docs/superpowers/specs/`, `docs/superpowers/plans/` — дизайн-спеки и планы циклов
- `docs/analyzer-patterns.md` — справочник шаблонов для анализатора
- `roadmap.md`, `TODO.md` — планирование и отложенные правки
- `refs/` — внешние референсы (UEFITool fork, IFRExtractor-RS, тестовые образы; gitignored)
