#!/usr/bin/env python3

import json
import os
import re
import sys
import subprocess

FUNC_NAMES_BY_ADDR = dict()


def resolve_name(addr):
    if addr not in FUNC_NAMES_BY_ADDR:
        # LIEF doesn't parse function size...
        name = subprocess.check_output(
            [
                "addr2line",
                "-f",
                "-e",
                os.path.expanduser("~/opt/sqlite-bench/sqlite-bench"),
                addr,
            ]
        ).splitlines()[0]
        FUNC_NAMES_BY_ADDR[addr] = name.decode("utf-8")

    return FUNC_NAMES_BY_ADDR[addr]


if __name__ == "__main__":
    is_callstack = False
    is_first_frame = False
    addr_pattern = r"^\.+ +[0-9]+: +([0-9a-f]+)$"
    a = []
    bts = dict()
    line = sys.stdin.readline()
    while line:
        if "branch callstack:" in line:
            is_callstack = True
            is_first_frame = True
        elif is_callstack:
            addr_match = re.search(addr_pattern, line)
            if addr_match:
                resolved_name = resolve_name(hex(int(addr_match.group(1), 16)))
                if not ("??" in resolved_name and is_first_frame):
                    a.append(resolved_name)
            else:
                k = tuple(a)
                if k not in bts:
                    bts[k] = 0
                bts[k] += 1
                a = []
                is_callstack = False
            is_first_frame = False
        line = sys.stdin.readline()

    with open("bts.perf.json", "w") as f:
        f.write(json.dumps({"\x1f".join(k): v for k, v in bts.items()}))
