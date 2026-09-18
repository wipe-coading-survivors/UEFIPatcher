#!/usr/bin/env python3
"""SOL-риг: fifo -> pty -> ipmitool sol activate -> лог (hack-инструмент, не движок).

Прямой `ipmitool sol activate < fifo > log` падает на tcgetattr (stdin не
TTY) — мост даёт ipmitool pty, кидает клавиши из fifo в консоль и пишет
выход в лог. Обвязка сессии 450x (rd450x).

Запуск (креды не в репо: source ../IPMI-rd450x.txt — переменные IP/L/P):
    source ../IPMI-rd450x.txt && export IP L P
    python3 hack/solrig.py /tmp/solin.fifo /tmp/sol.log &

Клавиши в консоль (см. AGENTS.md «Работа с IPMI/SOL»):
    echo -ne '\\x13' > /tmp/solin.fifo          # Ctrl+S (0x13)
    echo -ne '\\x1b1' > /tmp/solin.fifo         # F1  = ESC+1 (вход в Setup по промпту Press <F1>)
    echo -ne '\\x1b9' > /tmp/solin.fifo         # F9  = ESC+9 (Optimal Defaults)
    echo -ne '\\x1b0' > /tmp/solin.fifo         # F10 = ESC+0 (Save & Exit)
    echo -ne '\\r'    > /tmp/solin.fifo         # Enter (диалоги подтверждения)

Грабли (проверено вживую, §O6):
- power cycle рвёт SOL-сессию («SOL session closed by BMC») — перезапустить
  мост; если BMC держит зомби-payload («SOL payload already active on
  another session» при живом/мёртвом ipmitool) — `pkill -x ipmitool`,
  затем `ipmitool ... sol deactivate`; не отпустило — `mc reset warm`
  (BMC перезапускается ~1 мин, хост не трогается).
- fifo перед запуском: mkfifo; мост сам держит fifo открытым (O_WRONLY).
- лог пишется сырыми байтами консоли — смотреть `cat -v`.
"""
import os
import pty
import select
import subprocess
import sys

fifo, logfile = sys.argv[1], sys.argv[2]
open("/tmp/solrig.pid", "w").write(str(os.getpid()))
env = dict(os.environ)
lf = open(logfile, "ab", buffering=0)
master, slave = pty.openpty()
child = subprocess.Popen(
    ["ipmitool", "-I", "lanplus", "-H", env["IP"], "-U", env["L"], "-P", env["P"], "sol", "activate"],
    stdin=slave, stdout=slave, stderr=slave, close_fds=True)
os.close(slave)
fifo_fd = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
hold = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
try:
    while child.poll() is None:
        r, _, _ = select.select([master, fifo_fd], [], [], 1.0)
        if master in r:
            try:
                data = os.read(master, 65536)
                if data:
                    lf.write(data)
            except OSError:
                break
        if fifo_fd in r:
            try:
                data = os.read(fifo_fd, 4096)
                if data:
                    os.write(master, data)
            except BlockingIOError:
                pass
finally:
    for fd in (fifo_fd, hold, master):
        try:
            os.close(fd)
        except OSError:
            pass
    if child.poll() is None:
        child.terminate()
