#!/usr/bin/env bash
# Run a command in the QEMU guest, with our tools on PATH, and write the guest's
# own output to a host file (or stdout).
#
#   usage: guest-run.sh [-o outfile] 'commands'
#
# Our binaries are not executed on the host (see HANDOVER section 12): a tool that
# buffers its whole input grows until the OOM killer fires, and the host has no
# boundary around it. The guest runs the same binaries under -m 256M, so a runaway
# allocation costs the guest and not the machine.
#
# The guest share is read-only, so scratch work belongs in /tmp (a tmpfs) and the
# output comes back through the serial console. Only the text between the two
# markers is kept, so boot noise never enters the comparison, and NUL bytes survive.
set -uo pipefail
cd "$(dirname "$(readlink -f "$0")")"
cd ..

OUT=/dev/stdout
if [ "${1:-}" = "-o" ]; then
  [ $# -ge 3 ] || { echo "usage: $(basename "$0") [-o outfile] 'commands'" >&2; exit 2; }
  OUT=$2
  shift 2
fi
[ $# -eq 1 ] || { echo "usage: $(basename "$0") [-o outfile] 'commands'" >&2; exit 2; }

raw=$(mktemp)
trap 'rm -f "$raw"' EXIT

# The markers are printed by the guest itself, so everything between them is the
# command's own output. `exit` powers the guest off, which is how the boot ends.
#
# Capture starts only after `stage=shell-start`: run-shell.sh echoes the command it
# was given, and the command contains the markers, so without that gate the echoed
# command itself would be read as the payload.
bash run-shell.sh -c "echo @@GUEST-BEGIN@@
$1
echo @@GUEST-END@@
exit" > "$raw" 2>&1
rc=$?

awk '/stage=shell-start/{booted=1} booted && /@@GUEST-BEGIN@@/{flag=1;next} booted && /@@GUEST-END@@/{flag=0} flag' "$raw" > "$OUT"

echo "==> guest rc=$rc"
exit "$rc"
