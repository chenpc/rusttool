#!/usr/bin/env bash
# Boot the guest exactly once and run one payload from the 9p share.
#
#   usage: run-one.sh <test-id>
#
# <test-id> must exist as an executable at guest-root/tests/<test-id>, or as a
# shell script at guest-root/tests/<test-id>.sh (run by the guest's brush).
# The serial console goes to logs/<test-id>.log; the QEMU-TEST marker in that
# file is the verdict. Exit status: 0 PASS, 1 FAIL, 3 TIMEOUT, 2 usage error.
set -uo pipefail
cd "$(dirname "$(readlink -f "$0")")"

TIMEOUT_SEC=120
KERNEL=assets/vmlinuz-virt-6.18.52-0
INITRD=tiny.cpio
SHARE=guest-root

usage() {
  echo "usage: $(basename "$0") <test-id>" >&2
  exit 2
}

[ $# -eq 1 ] || usage
test_id=$1
case "$test_id" in
  "" | /* | *..*) echo "bad test id: $test_id" >&2; exit 2 ;;
esac

[ -f "$KERNEL" ] || { echo "missing $KERNEL" >&2; exit 2; }
[ -f "$INITRD" ] || { echo "missing $INITRD -- run ./pack-initramfs.sh first" >&2; exit 2; }
[ -x "$SHARE/tests/$test_id" ] || [ -f "$SHARE/tests/$test_id.sh" ] || {
  echo "missing payload $SHARE/tests/$test_id (or .sh)" >&2
  exit 2
}
[ -x "$SHARE/bin/brush" ] || { echo "missing $SHARE/bin/brush" >&2; exit 2; }

mkdir -p logs
log="logs/$test_id.log"
: > "$log"

qemu_cmd=(
  qemu-system-x86_64
  -machine pc
  -accel kvm
  -cpu host
  -m 256M
  -smp 1
  -kernel "$KERNEL"
  -initrd "$INITRD"
  -append "console=ttyS0 rd.test=$test_id"
  -display none
  -serial "file:$log"
  -monitor none
  -no-reboot
  -fsdev "local,id=fsdev0,path=$SHARE,security_model=none,readonly=on"
  -device virtio-9p-pci,fsdev=fsdev0,mount_tag=hostshare
)

# Two blank scratch disks, for the partition-table and filesystem tools
# (fdisk, sfdisk, partx, blkid, wipefs, losetup, mkfs, mount, ...). They are
# attached read-only at the QEMU level and every boot gets a throwaway overlay,
# so a case can partition or format them without the change leaking into the
# next run or into the host file.
#
# DISKS=0 turns them off, which keeps the non-disk testcases booting exactly as
# before; nothing outside the disk cases reads /dev/vd*, so they do not care.
if [ "${DISKS:-1}" != 0 ]; then
  mkdir -p scratch
  for n in 1 2; do
    img="scratch/disk$n.img"
    # 64 MiB of zeros, created once and then reused: snapshot=on means the
    # guest never writes to it anyway.
    [ -f "$img" ] || truncate -s 64M "$img"
    qemu_cmd+=(
      -drive "file=$img,if=none,id=disk$n,format=raw,snapshot=on"
      -device "virtio-blk-pci,drive=disk$n"
    )
  done
  echo "==> two scratch disks attached (snapshot=on)"
fi

echo "==> qemu ${qemu_cmd[*]}"
start=$SECONDS
timeout --foreground -k 5 "$TIMEOUT_SEC" "${qemu_cmd[@]}"
rc=$?
elapsed=$(( SECONDS - start ))

echo "==> qemu exited rc=$rc after ${elapsed}s"
echo "==> QEMU-INIT/QEMU-TEST lines from $log:"
grep -a -n -E 'QEMU-INIT:|QEMU-TEST:' "$log" | sed 's/^/      /'

if grep -a -q 'QEMU-TEST: PASS' "$log"; then
  echo "RESULT: PASS $test_id (${elapsed}s, log=$log)"
  exit 0
fi
if grep -a -q 'QEMU-TEST: FAIL' "$log"; then
  echo "RESULT: FAIL $test_id (${elapsed}s, log=$log)"
  exit 1
fi
if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
  echo "RESULT: TIMEOUT after ${TIMEOUT_SEC}s (log=$log)"
  exit 3
fi
echo "RESULT: UNKNOWN $test_id (no QEMU-TEST marker, qemu rc=$rc, log=$log)"
exit 1
