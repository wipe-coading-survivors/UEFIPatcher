# HOWTO(в работе): PCIe-бифуркация Lenovo RD450x — конспект саги v2–v20c

Статус: **В РАБОТЕ** (цель пока не достигнута на нативном билде). Документ —
конспект, чтобы не искать заново; факты подкреплены прошивками и побайтовыми
диффами. Полные версии: `docs/reports/2026-09-14-450x-bifurcation-serial-saga.md`,
`docs/reports/2026-09-14-uncore-bif-recon.md`, TODO-раздел «450x».

Цель: разделить Port 2 (x16) на x4x4x4x4 под 4 NVMe на райзере.
Платформа: Lenovo RD450x (Purley/Grantley, AMI Aptio; CPU E5-2620 v3, позже
E5-2640 v4 Broadwell-EP).

## 1. Карта миров (главное, что надо помнить)

| Билд | Бифуркация посевом | Серийный дебаг | Комментарий |
|---|---|---|---|
| HNX99TF (десктоп; билд — «тихий» производный Lenovo-референса) | **ДА** (v2: PASS, Port 2 → 2A/2B, обе NVMe) | нет (код вырезан вендором) | наша референс-база посева |
| натив Lenovo («450x — копия»/ds1) | **НЕТ** (политика игнорирует ручку, v9/v12 доказано) | да (BMC/SOL) | объект саги v4+ |
| Supermicro 超微450 (комьюнити) | да (заводской посев 0x531–0x534=00×4) | молчал, чинится посевом (v8) | донор-эталон поведения |

**Рабочий рецепт посева** (класс HNX/超微): `hii form unlock` хаб-REF →
`hii question set-value` IIO0=x4x4x4x4 во все копии стора (на 450x: qid
0x248 «IOU0 (IIO PCIe Port 2)» @ IntelSetup+0x531; v2-дифф = 1 байт).
Подтверждено на железе дважды (v2, v7).

## 2. Что доказано про замок натива (цепочка v9–v20c)

1. **Ручка жива, доставка работает**: F9-пересев читает наши сиды (консольные
   ручки того же стора применяются), но IIO-консьюмер значение игнорирует —
   селективность потребителей (v9).
2. **Посев во все 5 источников** (FV0 raw-шаблон, FV2 LZMA-копия, runtime
   NVAR-запись IntelSetup, IFR default-флаг, NVRAM-гэп) — ручной SOL-сейв
   пишет 00 в переменную и она живёт через ребут, но регистр 0x190 остаётся
   0004 ⇒ guard ниже по потоку (v10–v12).
3. **Главный замок**: OEM-гейт Lenovo в UncoreInitPeim (PE 0xbc4a4): ранняя
   IIO-тренировка (единственный путь записи LCTRL 0x190) включена ⇔
   var[0x160b]≠0 **И** поколение CPU ≠ Haswell-EP. **v14 = 2×2 байта NOP**
   (`74 12`→`90 90`, `74 09`→`90 90` @ PE 0xbc4ab/0xbc4b4) — **замок взломан**:
   «IIO Early Link Training» исполнилась, Port 2 = две линии, 0x190 = 0000,
   пишет сам RC (2026-09-15, вердикт PASS).
4. **Осталась публикация функции 00:02.1** (конфиг-спейс скрыт, FFFF):
   - v15: unlock форм 87–89 («PCI-E Port» Enable) — пункты появились, порта нет;
   - v16: посев presence-чекбоксов — отвергнут чистым A/B;
   - v17: запечённые дефолты (Debug/2B Enable/OpROM) — не при чём;
   - v18: publish-патч RP+0xA0.6 безусловным `mov $0x40` (11 байт) — мимо;
   - v19: вырезание VID-зонда presence (5 байт) — мимо;
   - v20/v20b: NOP писателя IioInit / code-cave — **DXE-вис** (урок: пещера
     за VirtualSize .text = грузится в нулях; только внутри vsize или с
     раздувом vsize);
   - v20c: кейв-патч корректно (vsize 0x17748→0x17760) — POST/ОС стабильны,
     сантехника скрытым портам доставлена, **но 00:02.1 всё ещё FFFF** —
     декод-гейт НЕ в IioInit.
5. CPU-свап на Broadwell-EP (гипотеза CPU-gen гейта публикации) — скрытие не
   снял (безусловный OEM-код).

## 3. Открытые Lead'ы (продолжение охоты)

- PciBus DXE-модули (скан /tmp/dxe-scan на 0x28080/ECAM-литералы);
- SMM и потребители cfg[0x112]/cfg[0x964];
- живой дифф регистров UBOX с донора Supermicro (если плата доступна):
  `lspci -xxx -s 00:05.0` + `lspci | grep 02:` — локализует реальный
  DEVHIDE-офсет;
- физическая альтернатива: **рассадка по портам** (SLOT3=port2 + SLOT4=port3
  или onboard M.2) — бифуркация тогда вообще не нужна; вопрос владельца.

## 4. Движковые механики, отработанные сагой (переиспользуйте)

- карта «вопрос→varstore-офсет» всего RC-формсета регенерируется движком
  (`refs/amibcp/450x-rc-formset-question-offsets.txt`, 4k строк);
- посев в runtime-копию стора: `nvar set`/`hii question set-value` пишут все
  копии, включая сжатую FV2 (decompress→flip→repack→slot-fit);
- замена PE-модуля в сжатой секции: `node replace 8/17/1/0 --artifact <PE>
  --body-only` в write-режиме (GUIDed-LZMA репак) — v20c собран движком;
- byte-патчи PE: python-дубль `hack/uncore_publish_patch.py`,
  `hack/uncore_presence_force.py`, `hack/iioinit_dlactive_cave_patch.py`
  (roundtrip проверен, sha256 сходится);
- ECAM-пробы из ОС — поштучные dword (python mmap /dev/mem r+b;
  `iomem=relaxed`; bulk-чтения дают FF).

## 5. Артефакты (refs/amibcp/, gitignored)

| Артефакт | Состав |
|---|---|
| `450x-unlock-bif-port2-4x4x4x4.bin` (v1, устарел — не шить) | unlock + 1 байт |
| `450x-unlock-bif-4x4x4x4-all.bin` (v2, на базе HNX — **PASS**) | REF3 + посев 4 экз. Port 2 |
| `450x-native-unlock-bif-4x4x4x4-all.bin` (v4) | натив + REF3 + посев |
| `450x-native-full-v5.bin` (v5, POST-поток в цикле — осторожно) | + консольные сиды Maximum |
| `450x-native-v9-bif-seed.bin` (v9 ≡ v4) | без консольных сидов |
| `450x-native-v10-var-bif0.bin` … `v12` | runtime-запись / IFR-флаг |
| `450x-native-v14-earlytrain-bif0.bin` (**замок снят**) | 4 байта NOP гейта |
| `450x-native-v15-unhide-2bcd.bin` … `v20c` | серия публикации функции |

## 6. SOL-процедуры стенда

- движок: `UEFIPATCHER_SOCK=/tmp/uefipatcher.sock ./target/debug/engine` +
  `UEFIPATCHER_DATA=/tmp/uefipatcher-data`; SOL-мост `hack/solrig.py <fifo>
  <log>`; креды `../IPMI-rd450x.txt` (не печатать);
- вход в Setup: спам ESC+1 (F1) каждые 0.3 с хвостом POST до «Aptio Setup
  Utility»; F9=ESC+9, F10=ESC+0; Ctrl+S в окне NIC-промпта открывает меню
  сетевухи (выход ESC);
- зомби SOL: `sol deactivate`, упорные — `mc reset warm`; TMM деградирует
  после 2–3 прошивок — тоже `mc reset warm`;
- workflow смены железа: свап CPU → F9 (пересев из IFR-дефолтов).
