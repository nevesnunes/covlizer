#!/usr/bin/env python3

import gdb
import json

gdb.execute("set pagination off")
gdb.execute("set style enabled off")
gdb.execute("set startup-with-shell off")

gdb.execute("catch syscall")
gdb.execute("r --benchmarks=readseq")

bts = dict()
while gdb.selected_inferior().pid != 0:
    is_sqlite3_bt = False
    a = []
    depth = 0
    frame = gdb.newest_frame()
    while frame:
        if "sqlite3" in frame.name():
            is_sqlite3_bt = True
        a.append(frame.name())
        depth += 1
        frame = frame.older()

    if is_sqlite3_bt:
        k = tuple(a)
        if k not in bts:
            bts[k] = 0
        bts[k] += 1

    gdb.execute("c")

with open("bts.gdb.json", "w") as f:
    f.write(json.dumps({"\x1f".join(k): v for k, v in bts.items()}))

gdb.execute("q")
