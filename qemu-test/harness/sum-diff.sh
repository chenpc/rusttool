#!/usr/bin/env bash
# Differential test for sum.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=sum
SYS=/usr/bin/sum
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh
. /home/chenpc/git/rusttool/qemu-test/harness/sums-cases.sh
summary
