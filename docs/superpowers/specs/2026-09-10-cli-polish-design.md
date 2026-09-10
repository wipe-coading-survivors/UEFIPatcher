# Спека мини-цикла cli-polish (пачка 3: UX и тест-гигиена uefi-cli)

## 1. Цель

Закрыть открытые CLI-находки TODO (ревизия CLI 2026-08-10, ревизия u1–u5 §13,
финальное ревью setup-new-page): help-строки, валидация `image switch`,
content-ассерты stdout, дедуп мока, покрытие defaults-ветки принтера и
человекочительные NotFound-сообщения HII. Пачка не меняет протокол, топологию
команд и семантику операций — только UX вывода, ошибки и тесты.

## 2. Состав пачки и пересмотр диагнозов TODO

| # | Пункт TODO | Диагноз на 2026-09-10 |
|---|-----------|----------------------|
| 1 | `about:` на всех подкомандах | Открыт: 4 about из ~39 вариантов (`main.rs`) |
| 2 | `image switch` не валидирует образ | Открыт: `commands/image.rs:28` пишет state без RPC |
| 3 | `client.rs image_status` `.info.unwrap()` | **Уже закрыт** `4d1d473`: `client.rs:141` `ok_or_else` — в цикле только закрыть строку TODO |
| 4 | Нет теста на ArgGroup exclusivity | Открыт |
| 5 | cli_integration/e2e проверяют только exit-code | Частично: 18/13 упоминаний stdout-ассертов, инвентаризация в §3.6 |
| 6 | TSV `hii form gates` без content-ассертов | Открыт |
| 7 | TSV `hii question info` без content-ассертов | Открыт |
| 8 | Дубль ~40-строчного QuestionInfo-литерала в моке | Открыт: `mock_question()` не существует |
| 9 | defaults-ветка принтера question info не покрыта | Открыт: tests-mod в `output.rs:519` есть, кейса нет |
| 10 | about `page add` читается как валидация | Открыт, объединён с п.1 |
| 11 | Несуществующий item_id → голый `RPC_NOT_FOUND` | Пере-диагностировано: `HiiError::NotFound` печатает «not found» без цели — см. §3.4 |

Пункты 12–13 сокета/тафтологичного теста engine — вне пачки (решение владельца
при выборе скоупа).

## 3. Решения (дизайн)

### 3.1 Help-строки (пункты 1 + 10)

`#[command(about = "...")]` на все варианты `Cmd`/`SessionCmd`/`ImageCmd`/
`NodeCmd`/`ArtifactCmd`/`HiiCmd`/`HiiFormCmd`/`HiiFormSetCmd`/`HiiQuestionCmd`/
`HiiPageCmd`/`HiiStringCmd` (~39). Стиль существующих четырёх: английский,
императив, нижний регистр, без точки:

- `hii question info` — «show question value map (varstore/offset/width/options)»
  (эталон плотности: глагол + суть + скобки-детали).

`page add` перефразировать: текущее «register a form as a page in the $SPF
page table» читается как валидация IFR. Новый текст: «add a form to the $SPF
page table (no IFR validation; create the form first)».

`#[arg(help = ...)]` — только там, где имя не самодостаточно: `--mode`
(insert: into/before/after), `--body-only`, source-группа `--file/--artifact`
(«read new body from file» / «read new body from imported artifact»). Не на
каждый аргумент — самодостаточные (`--name`, `--limit`, `--filter`) не трогаем.

Снапшот-тест полного `--help` НЕ делаем: дублирует clap-рендер, хрупок к
переформатированию. Проверка — ручной прогон `--help` на каждом уровне.

### 3.2 `image switch` валидация (пункт 2)

`commands/image.rs::switch`: state → `Client::connect` → `images_list` →
если `image_id` отсутствует в списке — `AppError::new(ErrKind::RpcNotFound,
"image {id} not found on server; see 'image list'")`, state НЕ пишем;
иначе — пишем `active_image_id` как сейчас. Пустой список сервера — та же
ошибка (нет образа — переключать не на что).

### 3.3 ArgGroup exclusivity (пункт 4)

Тесты в cli_integration:
- `node insert <target> --file X --artifact Y` → exit 1 (clap), stderr
  содержит «cannot be used with»;
- `node replace` — зеркально;
- ни `--file`, ни `--artifact` → runtime-ошибка движка/клиента. Текущий текст
  проверяем; если не говорит «specify exactly one of --file/--artifact»,
  улучшаем (client-side pre-check в `commands/node.rs` или понятный текст
  ошибки — по месту, без нового флага).

### 3.4 NotFound-сообщения HII (пункт 11)

Проблема: `HiiError::NotFound` печатает «not found», CLI выводит
`error: not found (RPC_NOT_FOUND)` — цель потери. Код оставляем
(gRPC-семантика корректна), чиним сообщение на RPC-границе: обработчики
`rpc/server.rs` знают item_id/target из запроса.

Механика: обёртка `hii_error_status_ctx(e, ctx)` — для
`HiiError::NotFound` → `Status::not_found(format!("{ctx} not found"))`,
остальные варианты — как в `hii_error_status`. Применяется в обработчиках,
чей запрос несёт идентификатор цели (item_id/target): gates, unlock,
question_info, set_value, question_add, page_add, form_add, form_hijack.
Формсет-операции и прочие обработчики без цели в запросе остаются на
`hii_error_status`.
73 места конструирования `HiiError::NotFound` в engine НЕ трогаем
(payload в вариант — вне пропорций пачки). Отдельный RPC-код для HII-таргетов
НЕ вводим: tonic Status не различит его без proto-изменений.

### 3.5 `mock_question()` и defaults-ветка принтера (пункты 8 + 9)

- `tests/mock_server.rs`: `fn mock_question() -> QuestionInfo` с непустыми
  `defaults` (одна DefaultEntry — закрывает п.9 заодно) + хотя бы одна
  option; оба мока (question_info, set_value) строятся от неё.
- `output.rs` tests-mod: кейс `print_question_info` с defaults — text-формат
  содержит строку дефолта вида `default = {value} (id {default_id}, type
  {type})`. TSV-ветка принтера defaults НЕ печатает (только options) —
  ассерт только text-формата. Точный вид строки — из текущей реализации
  принтера (RED по отсутствию покрытия, не по изменению вывода).

### 3.6 Content-ассерты stdout (пункты 5–7)

Инвентаризация: 21 `print_*` в `output.rs`; чек-таблица «print-fn → есть ли
content-ассерт в cli_integration/e2e» фиксируется в плане. Правила:

- стратегия — стабильные подстроки (id, имена колонок, ключевые поля), не
  полные снапшоты stdout;
- TSV `hii form gates` — точный заголовок колонок + одна полная data-строка
  (значения из mock_question/фикстур);
- TSV `hii question info` — точный заголовок + строка дефолта;
- каждая print-fn получает хотя бы один content-ассерт (json или tsv —
  где дешевле стабильность);
- ассерты ставятся на существующие инвокации; исключение — print-fn без
  единой инвокации в тестах (`form_hijack`, `question_add`, `page_add`,
  `extract`): для них добавляются инвокации в существующие мок-флоу.
  Новых мок-хендлеров НЕ заводится — все ответы уже есть в `MockEngine`
  (mock_server.rs:223/361/376/144).

## 4. Не-цели

- Протокол/RPC-коды/топология команд — без изменений (кроме текстов
  сообщений NotFound на RPC-границе).
- Payload в `HiiError::NotFound` / отдельный код HII-таргета.
- Help-снапшоты, i18n, man-страницы, shell-completion.
- Сокет-дефолт `/run` и тафтологичный engine-тест (вне пачки, TODO).
- ImageUpload / docker-кейс (отдельная фича).
- TUI/WebUI (клиентские обёртки — свои пункты TODO).

## 5. Тесты и гейты

- `cargo test -p uefi-engine` — новый тест ctx-обёртки NotFound (TDD).
- `cargo test -p uefi-cli` — unit output.rs + cli_integration (mock-сервер,
  без живого образа) + e2e.
- `cargo clippy -p uefi-cli -p uefi-engine -- -D warnings`; `cargo fmt`.
- Ручная проверка: `uefi-cli --help`, `<noun> --help` на каждом уровне —
  после T2.
- Реальные образы не требуются: всё на моках/фикстурах.

## 6. Потребители и миграция

Потребители вывода CLI — человек и скрипты владельца. Изменения вывода:
только добавление help-строк и обогащение NotFound-сообщений (было
«error: not found (RPC_NOT_FOUND)», станет «error: 0#99 not found
(RPC_NOT_FOUND)»). Exit-коды и коды ошибок не меняются — скрипты,
грепающие `RPC_NOT_FOUND`, не ломаются.

## 7. Риски

- **Формулировки about субъективны** — стиль зафиксирован эталоном
  «show question value map…»; ревьюер проверяет единообразие, не вкусы.
- **Content-ассерты хрупнут к косметике вывода** — стратегия подстрок +
  точные строки только для TSV (машиночитаемый формат, меняем редко).
- **Разрастание инвентаризации §3.6** — если таблица покажет >10 непокрытых
  print-fn, в плане разбиваем T5 на два (по типам вывода), скоуп не режем.

## 8. Процесс

Ветка `fix/cli-polish`, спека → план (writing-plans) → исполнение
суб-агентами (subagent-driven-development, свежий исполнитель на задачу,
ревьюер на задачу, финальное ревью ветки). Коммиты по правилам AGENTS.md
(правило 11: дефекты плана — docs-коммит до реализации). По завершении —
закрыть пункты TODO 1–11 (п.3 — «Закрыто: 4d1d473 ещё до пачки, отмечено
здесь») и влить PR в master.
