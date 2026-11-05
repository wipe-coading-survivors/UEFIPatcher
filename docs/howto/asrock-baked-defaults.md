# HOWTO: запекание дефолтов BIOS (StdDefaults / set-value)

Статус: **аппаратно-валидировано** — ASRock E3C226D2I: полный пресет
(4G/SOL/EMS/Timeout) прошит, первый бут пересеял его сам, значения пережили
полное снятие питания. Тот же механизм (`StdDefaults`-посев) отдельно
валидирован на Huananzhi X99-TF (E14).
Первоисточники: `docs/reports/2026-09-21-asr1-226d2il-live-varmap.md`,
`docs/reports/2026-09-02-hw-validation-hnx99tf.md`.

Цель: BIOS **всегда** поднимается с нужными настройками — первый бут после
прошивки, сброс NVRAM, севшая/снятая батарейка. Значения по умолчанию при
пересеве берутся не из IFR, а из NVAR-стора `StdDefaults` внутри образа —
вот его и печём.

Побочный факт: правки «дефолтов» в AMIBCP (SDP/IFR-дефолты) на этих
платформах **инертны** для посева (проверено E13) — поэтому «AMIBCP правит,
а на плате не применяется». Наш путь — именно StdDefaults.

## 1. Найти вопрос и его карту

```bash
uefi-cli image open образ.bin --mode write
uefi-cli hii form list                              # форма по титулу меню
uefi-cli hii question list <target>#<form_id>       # вопросы формы
uefi-cli hii question info <target>#<form_id>:<qid> # varstore/off/width/опции
uefi-cli hii question value <target>#<form_id>:<qid>  # текущие байты (read-back)
```

`info` даёт допустимые значения (OneOf-опции / Numeric min-max) и varstore
тайпинг; `value` — что запечено сейчас.

## 2. (Опционально) маппинг полей живого BIOS

Если IFR непрозрачен (вендорские формы-призраки, безымянные вопросы) —
атрибутация байтов живыми пробами (метод сессии asr1):

1. полный слепок efivars: `hack/asr1-efivars-snap.sh <имя>` (не только Setup —
   `Timeout`, например, живёт в стандартной переменной);
2. одно изменение в BIOS за ребут (F10);
3. ОС поднялась → новый слепок → `hack/efivars-diff.py <s0> <sN>`;
4. откат значения и повторная проба для подтверждения (для SOL-тумблера —
   откат вслепую по ssh, возврат POST в лог = байт переключён верно).

Диффы-помехи: `MonotonicCounter*` тикает от любой активности,
`SetUpdateCountVar` +1 на каждый Setup-сейв, `HiiDB` — внутреннее состояние,
`ConIn/ConOut/ErrOut*` — производные списки консолей.

## 3. Испечь

```bash
uefi-cli hii question set-value <target>#<form_id>:<qid> <value>
```

Движок пишет значение **во все копии** `StdDefaults`-стора согласованно
(обычно их две: raw-файл FV0 + сжатая копия FV2 — вторая через
decompress→flip→repack→slot-fit), чексуммы пересчитываются, длина образа
не меняется. Read-back: `hii question value` показывает новое значение.

Низкоуровневая альтернатива (когда вопрос неизвестен, а известен
var/офсет — например из шага 2):

```bash
uefi-cli nvar list [--var StdDefaults]      # найти стор/переменную
uefi-cli nvar set Setup --offset 1141 --value 1          # байт внутри данных переменной
uefi-cli nvar set Timeout --guid 8BE4DF61-... --offset 0 --value 5 --width 2
```

## 4. Полный пример (ASRock E3C226D2I, прошитый пресет)

```bash
T=899407D7-99FE-43D8-9A21-79EC328CAC21:0x18:0     # Setup-модуль (0x18-канал!)
uefi-cli hii question set-value $T#1215:0x13E 1   # Above 4G Decoding = on
uefi-cli hii question set-value $T#1158:0x10D 1   # SOL = on
uefi-cli hii question set-value $T#1158:0x10B 0   # COM1 = off
uefi-cli hii question set-value $T#1158:0x10F 0   # EMS = off (вендор запёк 1!)
uefi-cli hii question set-value $T#1031:0x4   5   # Setup Prompt Timeout, сек
uefi-cli image save 226d2i-ds1.rom
```

Итог: дифф от стока — ровно 4 байта (Timeout лёг во вложенную копию
переменной). После прошивки первый бут пересеял пресет **сам** (без F9);
полное снятие питания — все значения на месте, POST в SOL с холодного старта.

## 5. Верификация

- до прошивки: `cmp`/дифф образа против оригинала — только ожидаемые байты в
  ожидаемых офсетах (обе копии стора); `hii question value` = испечённое;
- после прошивки: слепок efivars и дифф против ожидания (метод §2); меню BIOS
  показывает пресет; контрольный тест «снял питание → дефолты наши».

## 6. Грабли

- **Канал Setup-модуля**: на AMI-образах двух видов — `:0x10:` (PE-ресурсный)
  и `:0x18:` (FREEFORM_SUBTYPE_GUID, ASRock) — копируй target из колонки
  form_id `hii form list`, не угадывай.
- **Вендорский seed ≠ желаемому**: если вендор запёк в IFR seed=1 (как EMS на
  asr1), обязательна печать 0 в StdDefaults — иначе пересев поднимет 1.
- **Формы-призраки**: одинаковый prompt может быть у нескольких форм
  (служебные свалки AMI на сотни вопросов) — ориентируйтесь на титулованные
  живые формы, сверяйте офсет с живой пробой (§2).
- **IFR-дефолты/SDP optimal-failsafe** на посев не влияют (E13) — не тратьте
  на них время; Load-Defaults-семантика меню — отдельная тема.
- NVRAM-стор может отсутствовать в свежем образе вовсе (создаётся рантаймом
  при первом буте) — тогда печём единственную существующую копию, движок
  найдёт её сам; отказ `record-missing-in-store` = карта вопросов не совпала
  со стором, проверьте `hii question info`.
- Прошивка и активация пересева — [flashrom-write.md](flashrom-write.md).
