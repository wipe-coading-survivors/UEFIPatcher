# UncoreInitPeim бифуркации — разведка цикла A (розетта-первый)

Спека `docs/superpowers/specs/2026-09-14-bif-driver-recon-design.md`;
план `docs/superpowers/plans/2026-09-14-bif-recon.md`. Прошивки —
только владелец; таблица MMR — `2026-09-14-uncore-bif-mmr.json`.

## R0. Извлечение (2026-09-14)

| Билд | Путь в дереве | GUID | Размер | Сигнатура |
|---|---|---|---|---|
| натив (450x — копия) | `12/47` | D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 | FFS 799090; PE32 798880 | MZ |
| HNX99TF | `6/45` | D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 | FFS 606322; PE32 606112 | MZ |
| 超微450 | `10/43` (дубль `11/43`) | D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 | FFS 628434; PE32 628224 | MZ |

Извлечённые PE32-образы (PEIM-секция `*/…/1`, `--body-only`, без
заголовка секции; в git не попадают — `refs` есть symlink наружу):

| Файл | Байт | SHA-256 |
|---|---|---|
| `refs/extracted/bif/IIO-pei32-native.pei` | 798880 | `b2a5fdef7f2751a74781c8c41a6e3afbbd7cb2da6d4d44041200fab3e9e6d8eb` |
| `refs/extracted/bif/IIO-pei32-hnx.pei` | 606112 | `ae71b5cbd482bae83832f8458b3a61a872f5beb0ac389737a3aab578c45bcb0b` |
| `refs/extracted/bif/IIO-pei32-sm.pei` | 628224 | `c18369ece8823dbce95820091dae8bf5bd274d85ebada9ead77a81013ffecf52` |

Контроль-розетта: `refs/fw/IIO-pei32.bin` (798880, PE32, строки
«Invalid IOUx Bifurcation»/«busUncore»/«busIio», сага §5).

Находки R0 (уточняют спеку/план):

1. **GUID один и тот же во всех трёх билдах** — каноническая запись
   `D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3` (так показывает `node list`);
   побайтово в FFS-заголовке лежит `A48B1CD7-F24A-0D4D-B1BA-F2409F0C20D3`
   — тот самый префикс `A48B1CD7` из спеки (порядок байт первого поля LE).
2. **Розетта = натив.** `IIO-pei32-native.pei` побайтово совпадает с
   `refs/fw/IIO-pei32.bin` (SHA-256 `b2a5fdef…`, 798880). Розетта — не
   «четвёртый билд», а тот же нативный образ; калибровка стенда (Task 2)
   на розетте автоматически валидна для натива, дифф Task 5 фактически
   розетта↔HNX.
3. **Тройка размеров из саги — это размеры FFS-файлов**: 606322 = HNX,
   628434 = 超微450; 627410 среди трёх билдов не встречен (натив FFS =
   799090) — видимо, четвёртый/ранний билд. Маппинг закрыт.
4. **超微450 содержит две идентичные копии** PEIM (FV `10/43` и `11/43`,
   off 14933360 / 15981936, один GUID и размер); извлечено из `10/43/1`.
5. **Строковые якоря есть только в нативе/розетте** («Invalid IOUx
   Bifurcation», «busUncore», «busIio», ASCII); в HNX и 超微450 их нет
   ни в ASCII, ни в UTF-16 — дифф Task 5 по HNX/SM нельзя строить на
   строковых якорях, только структурный (xref по адресам/константам).
6. Сигнатуры всех трёх + розетты — `MZ` (PE32), расхождений нет: дифф
   Task 5 упрощён.

## R1. Калибровка дизасм-стенда

Стенд: `hack/disasm.sh` (podman-remote → `uefipatcher-edk2-builder:latest`
c `python3-capstone`, capstone 5.0.6) → `/hack/uncore_disasm.py`
(PE32/TE-парс, `info`/`strings`/`xref`/`dis`/`mmio`); пути контейнерные
(`/refs/…`). Калибровка на розетте `refs/fw/IIO-pei32.bin` (≡ натив, R0):

```
$ hack/disasm.sh info /refs/fw/IIO-pei32.bin
/refs/fw/IIO-pei32.bin: PE32 machine=I386 subsystem=11 entry=0xffe90ae8 base=0xffe64b08
  .text    va=0xffe64d28   vsize=0xbfa48 raw=0x220+0xbfa60
  .reloc   va=0xfff24788   vsize=0x3404 raw=0xbfc80+0x3420

$ hack/disasm.sh strings /refs/fw/IIO-pei32.bin 8 | grep -Ei 'IOU|bif|uncore|iio'
0xffe7c51d busUncore: 0x%02X 0x%02X 0x%02X 0x%02X
0xffe7c549 busIio: 0x%02X 0x%02X 0x%02X 0x%02X
0xffe90a7c ERROR: Invalid IOUx Bifurcation =%x

$ hack/disasm.sh xref /refs/fw/IIO-pei32.bin 'Invalid IOUx Bifurcation'
; target 'Invalid IOUx Bifurcation' va=0xffe90a83
hit  file=0x32fdf site~0xffe97ae7 width=4 ref=0xffe90a7c -7
```

Строки несут префиксы, код ссылается на начало полной строки («ERROR: »
— сдвиг -7; busUncore/busIio — префикс «\n», сдвиг -1: hit file=0x7439d
site~0xffed8ea5 и file=0x74371 site~0xffed8e79), поэтому `xref` сканит
окно `va-8..va`. База подтверждена 5947 HIGHLOW-фиксапами .reloc (все
значения 0xFFxxxxxx). Калибровка = локализация сразу в нативе.

Три PEIM'а Task 1 — все PE32 (magic 0x10B), machine=I386, subsystem=11,
секции .text/.reloc, SectionAlignment==FileAlignment==0x220:

| Билд | base | entry | .text va / vsize / raw | .reloc va / raw |
|---|---|---|---|---|
| натив | 0xffe64b08 | 0xffe90ae8 | 0xffe64d28 / 0xbfa48 / 0x220+0xbfa60 | 0xfff24788 / 0xbfc80+0x3420 |
| HNX99TF | 0xffdcff40 | 0xffde9510 | 0xffdd0160 / 0x92788 / 0x220+0x927a0 | 0xffe62900 / 0x929c0+0x15e0 |
| 超微450 | 0xfff3de10 | 0xfff57520 | 0xfff3e030 / 0x97ca8 / 0x220+0x97cc0 | 0xfffd5cf0 / 0x97ee0+0x1720 |

## R2. Purley-карта / дифф native↔HNX / таблица MMR
(заполняют Tasks 3, 5)

## B1. Пробник и сборка
(заполняет Task 6)

## B2. EPA-1 (read-only) — вердикт владельца
(заполняет Task 8)

## B3. EPA-2 (write-ladder) — вердикт владельца
(заполняет Task 9)

## G. Гейт выбора варианта
(заполняет Task 10)
