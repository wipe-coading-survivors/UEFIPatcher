# Вердикт: S4a / E33 — Терминальный консьюмер TermSrc (полный AMI)

> 2026-09-07, ветка `serial-console`. Прошивка E33 (sha256 `969aa67f…`)
> — владелец. Наблюдения владельца дословно:

- serial: `ERROR: Class:3000000; Subclass:50000; Operation: D` —
  единственный вывод в серийной консоли.
- VGA: при входе в BIOS Setup машина виснет на `Entering Setup...`.

## 1. Гейты приёмки

| Гейт | Статус |
|------|--------|
| (a) консоль жива с первого бута | **НЕ ПРОЙДЕН** — serial глухая; единственная строка — RAW-статускод (см. §2) |
| (b) цвет ANSI-SGR | не проверяем — Setup не входит |
| (c) q512-baud | не проверяем — Setup не входит |

Худший исход из предусмотренных (§11-6) не наступил (CMOS-клин
исключён, ОС-загрузка не проверялась), но Setup недоступен на обеих
консолях → **откат на E32** (`~/E32/E32-candidate-8def2850.bin`) как
первый шаг восстановления управляемости машины.

## 2. Что установлено разбором (статика, 2026-09-07)

1. **ERROR-строка — не TermSrc-вывод.** Формат `ERROR: Class:…;
   Subclass:…; Operation:…` — AMI StatusCode-принтер
   (MdesStatusCodeDxe-путь), пишущий в UART **напрямую, минуя
   SimpleTextOut**. Значение по PI (PiStatusCode.h):
   Class 0x03000000 = EFI_SOFTWARE, Subclass 0x00050000 =
   **EFI_SOFTWARE_DXE_BS_DRIVER**, Operation 0x0D — кода с таким
   смещением в edk2-наборе DXE_BS_DRIVER нет (0x1000-специфичные
   0x00–0x09) → AMI-специфичная операция, точный источник не
   идентифицирован. Вывод: статус-код ошибки драйвера фазы BDS;
   **serial-консоль (SimpleTextOut-путь) не работала ни секунды**.
2. **Контингенси (ii) опровергнуто**: `SioSerialPortsLocationVar` и
   `SerialPortsEnabledVar` читаются ТОЛЬКО в init-функции @0x1E58
   (там же создаются с нулевым содержимым; ссылки на их имена —
   единственные точки). Start @0x21F8 их не читает: нулевые значения
   НЕ глушат привязку. Массивы @0x8E78/@0x8E80 — внутренний
   dedup-реестр стартованных портов (init = 0xFF/нули), не NVRAM.
3. **Supported @0x206C**: OpenProtocol(SerialIo, BY_DRIVER) c
   корректной обработкой ACCESS_DENIED/уже-открыт; RemainingDevicePath
   = NULL → SUCCESS; если задан — первый узел обязан быть
   MESSAGING/MSG_VENDOR_DP **и GUID ∈ {VT100, VT100+, VT-UTF8,
   PcAnsi}** (сравнения @0x2186–0x21DD со всеми четырьмя) — PcAnsi
   принимается, фильтр не отсекает дефолтный тег TermSrc.
4. **Start создаёт полный чайлд**: DevicePath
   (InstallMultiple @0x2786, GUID 09576E91) → затем
   SimpleTextIn + SimpleTextInEx + AMI-протокол 0ADFB62D +
   SimpleTextOut (InstallMultiple @0x2A02); SetAttributes(SerialIo)
   из mode-билдера @0x2815; ReadKeyStroke @0x4938 — неблокирующая
   форма (FIFO-парсер 0x4C30, без цикла ожидания).

## 3. Гипотезы виса (дискриминатор — S4b)

Машина доходит до TSE (VGA-POST, строка «Entering Setup...»
печатается), затем глобальный вис на обеих консолях. Единственная
семантическая переменная против E32 — TermSrc, поэтому виновник в его
интеграции; порядок правдоподобия:

- **(A) Вис на первом выводе через TermSrc.SimpleTextOut** (Reset /
  OutputString / SetAttribute → NULL-deref на не-открывшемся SerialIo
  или не-прочитанном конфиге) → CPU-исключение без обработчика =
  полный вис TSE; ERROR-статускод мог предшествовать. Совпадает с
  «висит и VGA»: TSE пишет в ConOut-агрегат, первый serial-вызов
  роняет всю машину.
- **(B) TermSrc.Start упал** (частичный чайлд: DevicePath без
  SimpleTextOut) → glue v3 search-all промахнулась → fallback добавил
  в ConOut/ConIn осиротевший VT-UTF8-путь (клеймить некому —
  TerminalDxe удалён) → TSE/ConSplitter виснет при консоль-инициализации
  на мёртвом пути. ERROR DXE_BS_DRIVER = репорт упавшего Start.
- **(C) Таблетка AMI-стека**: TermSrc-чайлд несёт AMI-протокол
  0ADFB62D и SimpleTextInEx-таблицу, рассчитанные на AMI
  ConSplitter/TSE-среду; LIVE-гибрид (наш ConSplitter + донорский
  SerialIo) может расходиться в ожиданиях (WaitForKey-notify из
  SerialIo-колбэков и т.п.).

Дискриминаторы следующего цикла (S4b, план отдельно):
1. Додизасм SimpleTextOut-таблицы TermSrc (Reset/OutputString/
   SetAttribute) — NULL-риски по контексту; трассировка пути
   ERROR-статускода.
2. Кандидат-дискриминатор E34: `SerialIo + TermSrc` **без glue**
   (чайлд создаётся BDS ConnectAll сам) — если Setup входит и не
   висит, виновник — glue-цепочка/ConOut-агрегат (B); если висит —
   TermSrc сам (A/C).
3. При (A): байт-патчи TermSrc нецелесообразны до понимания точки
   исключения; при (C): резерв (а) закрывается как
   неработоспособный в гибриде, возврат к пути (б) SerialIoSioDxe +
   TerminalDxe + цвет через патч TerminalConOutSetAttribute-таблицы.

## 4. Статус дуги

- E30/E31/E32 живы (доказаны железом); E33 — **откат**. Дуга serial
  возвращается на E32-базу (рабочая консоль, baud, COM-переезд).
- Цель «цвет» не достигнута; путь TermSrc-свапа заблокирован до
  S4b-дискриминации.
- Резервы: SerialIoSioDxe (путь б'), Terminal 7A08CB98, полный
  патч-вариант §11-2 (19 Б) — актуальность переоценить после S4b.
