#!/usr/bin/env bash
# Differential test for cksum.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=cksum
SYS=/usr/bin/cksum
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh
. /home/chenpc/git/rusttool/qemu-test/harness/sums-cases.sh
summary
