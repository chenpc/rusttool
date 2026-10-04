#!/usr/bin/env bash
# Boot the guest and land in brush on the serial console, with every tool
# that has a testcase on PATH.
#
#   usage: run-shell.sh                 interactive session (type `exit` to power off)
#          run-shell.sh -c 'commands'   run a command/script non-interactively
#          run-shell.sh -m 512          more guest RAM (default 256M)
#
# Init stage scripts (init.d/*.sh) run first, exactly like in run-one.sh.
# The guest share is mounted read-only, so scratch work belongs in /tmp
# (a tmpfs). Tools live in /mnt/bin: rev column col colrm bits colcrt line ul
# ... plus any tool that has a case directory under cases/.
#
# Two things to know:
#
#   * Wait for the guest (the `QEMU-INIT: stage=` lines, ~2 s) before typing.
#     Bytes that reach the emulated UART before the guest finishes booting are
#     dropped, so piping input in from the very first moment loses the head of
#     the first line. Use `-c` for scripted guest-shell work: the command
#     travels on the share, not through stdin, and is immune to this.
#   * brush runs with `--input-backend basic` on a pty the guest creates, so you
#     get a prompt and line editing without the cursor-position queries that a
#     serial line cannot answer. `exit` powers the machine off; Ctrl-A x is
#     QEMU's escape hatch.
set -uo pipefail
cd "$(dirname "$(readlink -f "$0")")"

KERNEL=assets/vmlinuz-virt-6.18.52-0
INITRD=tiny.cpio
SHARE=guest-root
MEM=256M
CMD=""
SCRIPTED=0

usage() {
  echo "usage: $(basename "$0") [-c 'commands'] [-m MEM]" >&2
  exit 2
}

while [ $# -gt 0 ]; do
  case "$1" in
    -c) [ $# -ge 2 ] || usage; CMD=$2; SCRIPTED=1; shift 2 ;;
    -m) [ $# -ge 2 ] || usage; MEM=$2; shift 2 ;;
    -h | --help) usage ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
done

[ -f "$KERNEL" ] || { echo "missing $KERNEL" >&2; exit 2; }
[ -f "$INITRD" ] || { echo "missing $INITRD -- run ./pack-initramfs.sh first" >&2; exit 2; }
[ -x "$SHARE/bin/brush" ] || { echo "missing $SHARE/bin/brush" >&2; exit 2; }

mkdir -p logs

# A scripted session is handed to the guest as a file on the share: the
# kernel command line cannot carry quotes or spaces.
SHELL_CMD_FILE=""
if [ "$SCRIPTED" = 1 ]; then
  SHELL_CMD_FILE="$PWD/$SHARE/tests/.shellcmd"
  printf '%s\n' "$CMD" > "$SHELL_CMD_FILE"
  APPEND="console=ttyS0 rd.shellscript"
else
  APPEND="console=ttyS0 rd.shell"
fi

cleanup() {
  [ -n "$SHELL_CMD_FILE" ] && rm -f "$SHELL_CMD_FILE"
  return 0
}
trap cleanup EXIT

if [ "$SCRIPTED" = 1 ]; then
  echo "==> booting guest (scripted): $CMD"
else
  cat <<'BANNER'
==> booting guest into brush on the serial console.
    type `exit` (or Ctrl-A x) to power off; tools are on PATH, /tmp is writable.
BANNER
fi

qemu-system-x86_64 \
  -machine pc \
  -accel kvm \
  -cpu host \
  -m "$MEM" \
  -smp 1 \
  -kernel "$KERNEL" \
  -initrd "$INITRD" \
  -append "$APPEND" \
  -display none \
  -serial stdio \
  -monitor none \
  -no-reboot \
  -fsdev "local,id=fsdev0,path=$SHARE,security_model=none,readonly=on" \
  -device virtio-9p-pci,fsdev=fsdev0,mount_tag=hostshare
rc=$?

echo
echo "==> guest exited rc=$rc"
exit "$rc"
