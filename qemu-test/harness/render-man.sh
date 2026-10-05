#!/usr/bin/env bash
# Render the GNU coreutils man pages to plain text once, so each tool can be
# implemented straight from its documented behaviour.
set -u
cd /home/chenpc/git/rusttool/qemu-test
mkdir -p harness/man
for tool in cp mv touch tee mktemp readlink cut tr sort uniq; do
  if man --nh --nj "$tool" > "harness/man/$tool.txt" 2>"harness/man/$tool.err" && [ -s "harness/man/$tool.txt" ]; then
    printf '%-10s %5s lines\n' "$tool" "$(wc -l < "harness/man/$tool.txt")"
  else
    printf '%-10s NO MANPAGE (%s)\n' "$tool" "$(head -1 harness/man/$tool.err)"
  fi
done