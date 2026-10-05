#!/usr/bin/env bash
# Differential test for sha384sum.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=sha384sum
SYS=/usr/bin/sha384sum
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh
. /home/chenpc/git/rusttool/qemu-test/harness/sums-cases.sh
summary
