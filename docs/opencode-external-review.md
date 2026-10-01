# Внешнее ревью планов через opencode-субагента — перенос в другой проект

Рецепт переносит практику «вторая линия обороны»: внешняя модель ревьюит план после селф-ревью
(блокирующий гейт) и выполненный план перед закрытием цикла (финальный блокирующий гейт).
Первисточник — UEFIPatcher: агент `.opencode/agents/plan-reviewer.md` на Kimi K3, гейты в
`AGENTS.md` секция «Внешнее ревью (Kimi K3)».

Места, отмеченные `<PLACEHOLDER>`, настраиваются под проект-приёмник.

## 1. Провайдер

- Нужен opencode с настроенным провайдером модели-ревьюера: `opencode auth login`
  (здесь — `kimi-code-plan-global`).
- Проверка: `opencode models | grep <provider>` → должен показать `<provider/model-id>`
  (здесь: `kimi-code-plan-global/k3`).

## 2. .gitignore

Если репо игнорирует `.opencode/` целиком — раздели, чтобы агенты коммитились:

```gitignore
# .opencode is local, except shared agents
.opencode/*
!.opencode/agents/
```

Важно: файл в `.opencode/agents/` = живой агент. Шаблоны и примеры туда НЕ класть —
opencode загрузит их как агентов.

## 3. Агент `.opencode/agents/plan-reviewer.md`

```markdown
---
description: External reviewer (<MODEL>) — reviews implementation plans after self-review (mode plan-review) and executed plans as final cycle gate (mode final-review). Read-only.
mode: subagent
model: <provider/model-id>
temperature: 0.1
permission:
  edit: deny
  bash:
    "*": deny
    "git diff*": allow
    "git log*": allow
    "git show*": allow
---

You are an external reviewer for the <PROJECT> repository. You never modify anything — review only. The dispatch prompt names one of two modes below.

First, read AGENTS.md at the repo root: it defines the conventions your review enforces.

## Mode: plan-review

Input: path to a plan plus brief cycle goal context.

Read the plan fully, then verify it against the actual codebase — do not trust the plan's description of the code, check the files yourself:

- Every file, module, function, type or API the plan references exists, or is created by an earlier step of the same plan.
- <PROJECT-SPECIFIC CHECKS: порядок задач (TDD), регистрация модулей, разрешённые библиотеки/крейты, дисциплина коммитов — перенести сюда главное из своего AGENTS.md>
- Each step is testable, has clear completion criteria, and does not silently merge unrelated work.
- No hidden assumptions about data, environment, or unfinished future tasks.

## Mode: final-review

Input: plan path plus what was done (branch and/or commit range; inspect via `git log` / `git diff`).

- Every Task/Step is done, or explicitly deferred with a documented reason; a `docs: fix ...` plan-defect commit is the legitimate path for plan errors found during implementation.
- Claimed gates (tests/lints) are plausible from the code and commit history; you do not need to re-run long builds.
- No tasks silently skipped, no extra undocumented changes in the diff, no fixups hidden inside implementation commits.

## Output format

1. Verdict, one line: `APPROVE` or `FIX-FIRST`.
2. Findings: numbered list; each item has severity (`blocker` / `major` / `minor`), location (plan `Task N Step M` or `file:line`), and what exactly is wrong.
3. If `FIX-FIRST`: the minimal set of changes required to reach `APPROVE`.

Be specific and skeptical. An empty findings list is a valid result. Do not restate the plan and do not give style opinions beyond AGENTS.md conventions. Respond in the language of the dispatch prompt.
```

## 4. Секция в AGENTS.md проекта-приёмника

```markdown
### Внешнее ревью (<MODEL>)

Агент `plan-reviewer` (`.opencode/agents/plan-reviewer.md`, модель `<provider/model-id>`, read-only) — вторая линия обороны после селф-ревью. Запуск — `task` с `subagent_type: plan-reviewer`, режим (`plan-review` / `final-review`) и входы указываются в промпте диспетчеризации.

1. **Ревью плана — блокирующий гейт после селф-ревью.** После написания плана и селф-ревью — диспетчеризовать `plan-reviewer` в режиме `plan-review`: путь к плану + краткий контекст цели цикла.
2. **Замечания правятся до старта реализации** отдельным коммитом `docs: fix <cycle> plan review findings (<суть>)`. Существенные правки (порядок задач, архитектура, объём) — повторное ревью; мелкие — без.
3. **Финальное ревью выполненного плана — блокирующий гейт закрытия цикла.** После завершения всех задач и прохода гейтов — `plan-reviewer` в режиме `final-review`: план + что сделано (ветка/диапазон коммитов). Расхождения устраняются до объявления цикла завершённым.
4. Внешнее ревью **не заменяет** селф-ревью и командные гейты — только дополняет их. Вердикт и замечания показывать пользователю целиком.
```

## 5. Smoke-тест

```bash
opencode run "@plan-reviewer Smoke test of the review pipeline. Skip all reading and verification. Reply with exactly one line: PIPELINE-OK"
```

Новая сессия подхватывает агента автоматически. Вызов из сессии: `task` с
`subagent_type: plan-reviewer` (из промпта основного агента) или `@plan-reviewer` (вручную).
