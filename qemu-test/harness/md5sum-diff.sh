#!/usr/bin/env bash
# Differential test for md5sum.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=md5sum
SYS=/usr/bin/md5sum
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh
. /home/chenpc/git/rusttool/qemu-test/harness/sums-cases.sh
summary
