# Спека: мини-цикл `tui-live` — ленивая сессия, скролл нижней зоны Forms, read-back текущего значения

Дата: 2026-09-30. Статус: проект (до вердикта владельца).

Закрывает TODO: 3444 (автосессия TUI), 3586 (остаток: низ панели Form без скролла), 3686 (read-back текущего значения — superseded `seed_value` nvar-op + закрывается `HiiGetValue`).

## Контекст

Живое использование TUI без CLI подсвечило три болячки:

1. TUI никогда не зовёт `SessionCreate` и не пишет `.uefipatcher`; `:open`/`:upload`/`:import`/`:artifacts`/`:switch` падают с «no session», пока сессию не создал `uefi-cli`. Воркараунд владельца — руками делать `uefi-cli session init` перед TUI.
2. Нижняя зона панели Form (детали выбранного вопроса: диапазон/опции/Value/IFR default) — `Paragraph` без скролла; длинные one_of-списки обрезаются по cap (осознанное решение спеки three-zone 2026-09-19, §3: «хвост сверх cap обрезается — принято»). Список вопросов при этом скроллится — TODO:3586 частично устарел.
3. Read-back текущего значения вопроса (TODO:3686) в основном закрыт циклом nvar-op (`seed_value`/`seed_option` в `QuestionInfo`, рендер `Value (NVAR seed)`), но: (а) `seed_lookup` (hii/mod.rs:630) не фильтрует за-барьерные копии StdDefaults, а `set_value` пишет только в доступные → на mixed-образах показанный seed может не совпадать с записанным байтом; (б) точечного RPC «прочитать текущее значение» нет — CLI/WebUI доступа к read-back не имеют.

## §1 Ленивая сессия в TUI (A)

### Поведение

Новый хелпер `ensure_session(client) -> Result<(), String>` в `crates/uefi-tui/src/commands.rs`:

- `client.state.session_id == None` → `SessionCreate(name="")` → заполнить `client.state.{session_id, token}` → `state::write_state` (atomic, uefi-common). Персист state-файла — только в этот момент; существующий state с живой сессией не перезаписывается.
- `session_id` есть, но сессия мертва (TTL/GC/destroy) → движок теперь явно валидирует сессию в `open_from_bytes`/`artifact_import` и возвращает `NOT_FOUND "session not found"` (выяснено при планировании: раньше FK-ошибка sqlite приходила как opaque `internal`) → TUI пересоздаёт сессию один раз по этому сообщению и повторяет исходный вызов. Сетевые/транспортные ошибки (`RPC_INTERNAL`, unavailable) пересозданием не маскируются — исходная ошибка возвращается как есть.
- Анти-цикл: не более одного пересоздания на команду.

### Точки вызова

`ensure_session` перед исходным запросом в шести местах: `:open` (commands.rs:622), `:upload` (:666), `:import` (:785), `:artifacts` (:801), `reopen` (:2193), `:switch` (:940).

`refresh_registry` (:1970) сознательно НЕ получает ensure_session: startup-путь (`restore_session` → `refresh_registry`) не должен создавать сессию на старте (решение «лениво, при первой нужде»). Без сессии registry-refresh остаётся тихим статусом «registry: no session» — как сегодня; командные пути, ensure-нув сессию, в него уже попадают с ней.

### `:switch`: персист active_image_id

После успешного `image_nodes_list` в `:switch` — `state::write_state(&client.state)` с обновлённым `active_image_id` (CLI в этом CWD видит активный образ). Ошибка записи — warn в status_msg, команда остаётся успешной (переключение уже применено; принцип «no partial states» не нарушен — это кэш-файл, не данные образа).

### Аддендум живого гейта (2026-10-01): персист active на open/upload/reopen/close

Находка владельца: TUI-only поток (два `:open`) оставлял state без `active_image_id` → CLI `NO_ACTIVE_IMAGE`, рестарт TUI терял активный образ. Причина — паритет-разрыв: CLI `image open`/`upload`/`switch`/`close`-активного персистят active (commands/image.rs), а TUI персистил только при создании сессии и `:switch`. Решение: `:open`/`:upload`/`reopen` персистят `active_image_id` сразу после установки (best-effort warn, как `:switch`); `:close` закрывающего активный образ — персистит очистку. Статус-бар показывает полный uuid (конвенция R1) вместо 8-символьного обрубка.

### Тесты

- Unit/integration (mock): старт без state → `:open` работает, `.uefipatcher` создан; существующий живой state → `SessionCreate` не вызывается; мёртвая сессия (mock отвечает NOT_FOUND) → ре-создание + retry, state перезаписан; транспортная ошибка → исходная ошибка, без создания.
- `:switch` пишет active_image_id (best-effort warn на ошибке записи).

## §2 Скролл нижней зоны Forms (B)

### Состояние

`FormsData` (`crates/uefi-tui/src/app.rs`): `bottom_cursor: usize`, `bottom_viewport: u16` — по образцу `questions_state`/`questions_viewport`. Сброс `bottom_cursor = 0` при смене выбранного вопроса (`forms_question_cursor_down/up`) и при `refresh_forms`/`reload_forms`.

### Рендер

`ui/forms.rs`: нижняя зона — окно строк `panel.bottom[bottom_cursor..bottom_cursor + inner_h]` вместо цельного `Paragraph::new(bottom.join("\n"))`. При переполнении — индикатор `[a..b/N]` в титуле зоны (диапазон видимых строк из общего числа). Tiny-terminal (высота ниже cap-логики §3 спеки three-zone) — прежняя обрезка краем.

### Клавиши

`J`/`K` (Shift+j/k, через `AppEvent::Key('J')`/`Key('K')` — свободны, проверено; Ctrl+J/Ctrl+K заняты фокусом, но Ctrl-модификатор матчится в `map_key` раньше и отдельным событием — с заглавными не пересекаются): построчный скролл с клампом; действуют в обоих фокусах List/Details (низ виден всегда). Hint-бар и help-экран Forms-вида дополняются.

### Аддендум к спеке three-zone

§3 «хвост сверх cap обрезается — принято» заменяется на: «хвост сверх доступной высоты скроллится J/K со сбросом при смене вопроса; индикатор `[a..b/N]`; tiny-terminal — обрезка краем».

### Тесты

Пейджер: скролл/кламп/сброс при смене вопроса; индикатор `[a..b/N]` при переполнении; отсутствие индикатора когда влезает; tiny-terminal guard (регресс `bottom_cap_keeps_questions_visible`).

## §3 Паритет seed + HiiGetValue (C)

### C1. Барьер-паритет seed_lookup (engine)

`seed_lookup` (crates/uefi-engine/src/hii/mod.rs:630) читает первую попавшуюся StdDefaults-запись без барьер-фильтра, а `set_value` (`collect_std_defaults_hits`, :950) — только доступные копии. Фикс: чтение seed видит ту же запись, куда писал бы set_value — переиспользовать селектор доступных хитов (общий хелпер либо прямой вызов `collect_std_defaults_hits`). Доступных копий нет → `seed_value`/`seed_option` = None, UI показывает «—» (контракт nvar-op §4 сохраняется). Видимое изменение: на mixed-образах seed может стать None там, где раньше показывался байт за-барьерной копии.

### C2. Прото

`crates/uefi-proto/proto/engine.proto`, после `HiiSetValue` (:41):

```proto
rpc HiiGetValue(HiiGetValueRequest) returns (HiiGetValueResponse);

message HiiGetValueRequest {
  string image_id = 1;
  string item_id = 2;
}

message HiiGetValueResponse {
  optional uint64 value = 1;
  optional string option = 2;
  optional string store_path = 3;
  optional uint32 var_offset = 4;
  optional uint32 width = 5;
}
```

Стора вообще нет → все поля None (не ошибка). Доступный стор есть, но записи с именем+размером варстора нет → `ValueOpUnsupported` (диагностический паритет с set_value — уточнение финального ревью, аддендум 2026-09-30). Порядок ошибок: InvalidItemId → NotFound (чек-лист AGENTS.md).

### C3. Engine-хендлер

`hii_get_value` (rpc/server.rs, по образцу `HiiQuestionInfo`): question-таргет → varstore name/size + `var_offset`/`width` → доступный StdDefaults-хит → LE-чтение `[off..off+width]` → `value`; `option` — совпадающая one_of-опция; `store_path` — path хита; `width`. Числовые сужения — try-конверсия с `invalid_argument`; bounds — общий хелпер `package_bounds`.

### C4. CLI

`uefi-cli hii question value <TARGET> <QUESTION_ID>` — печать `value`, `option`, источника (`store_path @ var_offset, width`), «—» при отсутствии записи.

### C5. TUI

Нижняя зона панели Form: строка `Value (NVAR seed): …` заменяется на `Current: 5 (Enabled) @ 1/13/2/1` из ленивого `HiiGetValue` (кэш и инвалидация — вместе с `refresh_question_info_if_needed`, commands.rs:2593; сброс при set-value/reload — по образцу существующего). Суффиксы `= v`/`≠` в списке вопросов остаются из `QuestionSummary` (без дополнительного RPC).

### C6. WebUI

Gateway — без кода: generic bridge `POST /api/v1/rpc/HiiGetValue` подхватит новый метод из descriptor pool. WebUI: регенерация proto-типов (`webui/src/lib/proto/engine.ts`); `SetValueDialog.svelte` получает meta-строку `Current: N (Option)` рядом с kind/store/offset/width (ленивый `getValue` рядом с существующим `questionInfo`, api.ts:176). Колонку в `QuestionTable` не добавляем.

### C7. Моки

Оба `mock_server.rs` (uefi-tui :56/:509, uefi-cli :26/:353) получают `hii_get_value` стаб (значение из существующей фикстуры вопроса; вариант «записи нет» — все поля None). WebUI msw-хендлер — если page-тест SetValueDialog потребует.

## Гейты

- `cargo test --all`; `cargo clippy --all --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `cd webui && npm run check` + webui-тесты.
- Живой гейт владельца (rd450x/HNX99TF): (1) свежий CWD без state → TUI `:open` без `uefi-cli session init`; (2) длинный one_of → J/K-скролл + `[a..b/N]`; (3) `Current` до/после set-value; (4) `:switch` → CLI видит active_image_id.

## За рамками

- Живые NVRAM-значения платформы (post-boot) — out-of-scope, контракт nvar-op.
- Универсальный read-примитив по стору (`{store_path, var_offset, width}` без вопроса).
- Колонка Current в QuestionTable WebUI.
- Gateway/WebUI-обвязки `HiiFormAdd`/`HiiFormsetAdd` (TODO:543) и прочие отложенные TUI-миноры (TODO:3700 и др.).

## Вердикт

TBD после живого гейта владельца.
