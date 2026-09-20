# Отчёт цикла nvar-op (2026-09-20)

Спека: docs/superpowers/specs/2026-09-20-nvar-op-design.md.
План: docs/superpowers/plans/2026-09-20-nvar-op.md.

## Что сделано

- Ядро `crate::nvar`: next-link, ext-header геометрия (отказ правки),
  resolve_guid (хвост-формула UEFITool nvramparser.cpp:209), flatten
  вложенных сторов (≤4), find_var имя+GUID с отказом на неоднозначность.
- `nvar list/set` (движок → RPC → CLI): согласованный флип всех копий за
  барьером компрессии v5; метка «NVRAM store» + Node.is_nvar + флоппи-глиф.
- TUI: трёхсекционная панель Details (мета/переменные/hex), цветные глифы
  вопросов + seed-значения (≠ при seed≠IFR-default).
- Seed-join: QuestionInfo.seed_value/seed_option,
  QuestionSummary.seed_value/ifr_default.

## Гейты (все зелёные)

- 226D2IL3.30/.50: листинг 14 переменных, Setup/EC87D643 @0x500088 1217b,
  Setup/01239999 204/206b (3.30/3.50); GUID-стор 0x60, free 0x10 —
  геометрия вложенного стора, по которому резолвятся GUID строк (сводку
  инструмент печатает по верхнему уровню блоба: guid store 16B, free
  0x1F7D9/0x1F7D7 для .30/.50); bake — дифф ровно 0x500089+0x5004FD,
  round-trip байт-точен.
- C275D4I3.20: Setup 226b / IntelSetup 608b / ServerSetup 456b.
- HNX99TF: nvar set ≡ set_value побайтово (обе копии, LZMA-дифф E14);
  отказ на неоднозначном Setup (EC87D643… vs 80E1202E…); seed-join 4G;
  регресс set_value 22/22 зелёный.
- cargo test --all / clippy --all -- -D warnings / fmt --check — зелёные.

## Известные ограничения

- При RPC-ошибке nvar_list TUI-панель NVAR остаётся стылой до следующего
  нажатия (транзиентно, самолечится; паттерн `let _ =` общий для
  refresh-хуков).

## Артефакты

| файл | sha256 |
|------|--------|
| refs/amibcp/asr1-330-sol4g-on.bin | 6e795fbda561e77a5853885b48fc4f3d6876d17c40e3049ed774928660070439 |
| refs/amibcp/asr1-350-sol4g-on.bin | a2302ac0d53c382a292dbf9414c96a51a2d9f4658effdb611bf22278d2c4512b |

SOL=1 (Setup[1]), Above 4G=1 (Setup[1141]). Прошивка asr1 — отдельный шаг
по согласованию (host policy); после прошивки — сброс NVRAM/Load Defaults
для пересева дефолтов.
