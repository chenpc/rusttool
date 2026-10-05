#!/usr/bin/env bash
# Differential test for sha512sum.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=sha512sum
SYS=/usr/bin/sha512sum
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh
. /home/chenpc/git/rusttool/qemu-test/harness/sums-cases.sh
summary
