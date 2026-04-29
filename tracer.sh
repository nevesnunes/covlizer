#!/bin/sh

set -eu

bin=${1:-sqlite-bench}
test -x "$bin"

probes=
funcnames=$(objdump -tC ./"$bin" | grep '\.text' | awk '/sqlite3/{print $NF}' | grep -v '\.' | grep -vE 'sqlite3_value|sqlite3_str|sqlite3_result' | sort -u | head -n100)
while read -r funcname; do
  probes=$probes'uprobe:'"$PWD/$bin"':"'"$funcname"'" { printf("%s\\n", ustack()); } '
done <<HEREDOC_INPUT
$funcnames
HEREDOC_INPUT

sudo bpftrace -e "$probes"
