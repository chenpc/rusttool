#!/usr/bin/env bash
# Clean full-cycle proof: host unit tests, then pack, then parallel QEMU boots.
set -uo pipefail
REPO=/home/chenpc/git/rusttool
cd "$REPO" || exit 1

echo "############ 1. host unit tests: cargo test -p rev ############"
cargo test -p rev 2>&1 | tail -30
echo "############ 2. clean all build outputs ############"
chmod +x qemu-test/run-many.sh qemu-test/pack-initramfs.sh qemu-test/run-one.sh
rm -rf "$REPO/target" qemu-test/target qemu-test/tiny.cpio \
       qemu-test/guest-root/bin qemu-test/guest-root/tests
echo "removed: repo target/, qemu-test/target, tiny.cpio, guest-root/{bin,tests}"

echo "############ 3. pack-initramfs.sh ############"
( cd qemu-test && ./pack-initramfs.sh ) 2>&1 | tail -45

echo "############ 4. host test of the 3 pre-existing members + rev ############"
cargo test -p rev -p lscpu -p fallocate 2>&1 | tail -20

echo "############ 4b. pre-existing tools/dmesg breakage (NOT touched by this work) ############"
cargo build -p dmesg 2>&1 | grep -E '^error' | head -5

echo "############ 5. run-many.sh rev ############"
( cd qemu-test && ./run-many.sh rev ) 2>&1 | tail -40
echo "############ run-many exit: ${PIPESTATUS[0]:-?} ############"
