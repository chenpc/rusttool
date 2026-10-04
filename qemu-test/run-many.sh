#!/usr/bin/env bash
# Boot the guest once per test id, several boots in parallel, and summarise.
#
#   usage: run-many.sh [filter]
#
# <filter> is an extended regexp matched against the ids listed in
# guest-root/tests/manifest.txt (default: every id). The per-boot verdict comes
# from run-one.sh, which greps the QEMU-TEST marker out of the serial log, so
# this script only has to schedule, collect and report.
#
# Exit status: 0 when every selected id PASSed, 1 when any of them failed,
# timed out or could not be started, 2 on usage/manifest errors.
set -uo pipefail
cd "$(dirname "$(readlink -f "$0")")"

MANIFEST=guest-root/tests/manifest.txt
RESULTS=logs/run-many.results
# Each guest takes -m 256M of KVM RAM; 8 boots fit comfortably in WSL's default.
JOBS="${JOBS:-8}"
TIMEOUT_SEC="${TIMEOUT_SEC:-120}"

usage() {
  echo "usage: $(basename "$0") [filter]" >&2
  exit 2
}

[ $# -le 1 ] || usage
filter="${1:-.}"
[ -f "$MANIFEST" ] || { echo "missing $MANIFEST -- run ./pack-initramfs.sh first" >&2; exit 2; }
[ -f tiny.cpio ] || { echo "missing tiny.cpio -- run ./pack-initramfs.sh first" >&2; exit 2; }
[ -x ./run-one.sh ] || { echo "missing ./run-one.sh" >&2; exit 2; }

mapfile -t ids < <(grep -E -- "$filter" "$MANIFEST" | LC_ALL=C sort -u)
[ ${#ids[@]} -gt 0 ] || { echo "no test id matches '$filter' (see $MANIFEST)" >&2; exit 2; }

mkdir -p logs
: > "$RESULTS"

# Runs in the xargs subshell, hence the exported environment. run-one.sh echoes
# its own summary; keep that next to the serial log instead of interleaving it
# with the other boots' output.
worker() {
  local id=$1 output rc verdict
  output=$(TIMEOUT_SEC=$TIMEOUT_SEC ./run-one.sh "$id" 2>&1)
  rc=$?
  printf '%s\n' "$output" > "logs/${id}.runner.log"
  case "$rc" in
    0) verdict=PASS ;;
    1) verdict=FAIL ;;
    2) verdict=ERROR ;;
    3) verdict=TIMEOUT ;;
    *) verdict="UNKNOWN(rc=$rc)" ;;
  esac
  # O_APPEND of a single short line, so the parallel writes do not interleave.
  printf '%-28s %s\n' "$id" "$verdict" >> "$RESULTS"
}
export -f worker
export TIMEOUT_SEC RESULTS

echo "==> ${#ids[@]} test id(s) matching '$filter', $JOBS in parallel"
printf '%s\n' "${ids[@]}" \
  | xargs -P "$JOBS" -n 1 bash -c 'worker "$1"' _

sort "$RESULTS"

count() { grep -c -- "$1" "$RESULTS" 2>/dev/null || true; }
passed=$(count ' PASS$')
failed=$(count ' FAIL$')
timedout=$(count ' TIMEOUT$')
errored=$(count ' ERROR$')
unknown=$(count ' UNKNOWN')

# A failing boot says nothing without its serial lines, so quote the verdict
# markers (and /init's stage trace) straight out of the log.
while read -r id verdict; do
  case "$verdict" in PASS) continue ;; esac
  echo "==> $id: $verdict"
  if [ -f "logs/${id}.log" ]; then
    grep -a -E 'QEMU-INIT:|QEMU-TEST:' "logs/${id}.log" | tail -6 | sed 's/^/      /'
  fi
done < "$RESULTS"

echo "==> SUMMARY: $passed PASS, $failed FAIL, $timedout TIMEOUT, $errored ERROR, $unknown UNKNOWN (of ${#ids[@]})"
echo "==> per-boot logs: logs/<id>.log (serial), logs/<id>.runner.log (harness)"
[ "$passed" -eq "${#ids[@]}" ] || exit 1
exit 0
