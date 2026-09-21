#!/usr/bin/env python3
"""Дифф двух снапшотов efivars (выход asr1-efivars-snap.sh): какие переменные
изменились и какие именно байты. Контент efivars = u32 attrs + данные; офсеты
печатаются по файлу и по данным (file-4).

Использование: hack/efivars-diff.py <dirA> <dirB> [max_diffs]
Пример: hack/efivars-diff.py refs/amibcp/asr1-map/s0-baseline refs/amibcp/asr1-map/s1-boot-timeout
"""
import pathlib
import sys


def load(d: pathlib.Path) -> dict[str, bytes]:
    e = d / "efivars"
    return {p.name: p.read_bytes() for p in e.iterdir() if p.is_file()}


def main() -> None:
    a, b = (pathlib.Path(x) for x in sys.argv[1:3])
    max_diffs = int(sys.argv[3]) if len(sys.argv) > 3 else 32
    va, vb = load(a), load(b)
    for name in sorted(va.keys() | vb.keys()):
        if name not in va:
            print(f"+ {name} ({len(vb[name])}B) new in {b.name}")
        elif name not in vb:
            print(f"- {name} ({len(va[name])}B) gone in {b.name}")
        elif va[name] != vb[name]:
            xa, xb = va[name], vb[name]
            diffs = [
                (i, xa[i] if i < len(xa) else None, xb[i] if i < len(xb) else None)
                for i in range(max(len(xa), len(xb)))
                if (xa[i] if i < len(xa) else None) != (xb[i] if i < len(xb) else None)
            ]
            print(f"~ {name}: {len(xa)}B -> {len(xb)}B, {len(diffs)} byte diffs")
            for i, old, new in diffs[:max_diffs]:
                print(
                    f"    file[0x{i:04X}] (data[{i - 4}]): "
                    f"{f'0x{old:02X}' if old is not None else '--'} -> {f'0x{new:02X}' if new is not None else '--'}"
                )
            if len(diffs) > max_diffs:
                print(f"    ... и ещё {len(diffs) - max_diffs}")


if __name__ == "__main__":
    main()
