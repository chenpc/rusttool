#!/usr/bin/env bash
# Generate every expected value for the paste guest case from /usr/bin/paste.
set -eu

W=/tmp/opencode/paste-case
rm -rf "$W"
mkdir -p "$W"
printf '%s\n' 'a' 'b' 'c' > "$W/f"
printf '%s\n' 'x' 'y' 'z' > "$W/g"
printf '%s\n' '1' '2' > "$W/h"
printf '%s\n' 'p q' 'r s' > "$W/spaces"
: > "$W/empty"

emit() {
  local label=$1
  shift
  printf '=== %s :: %s\n' "$label" "$*"
  local out rc
  out=$("$@" 2>"$W/.err") && rc=0 || rc=$?
  printf 'rc=%s\n' "$rc"
  printf 'out=%q\n' "$out"
  [ -s "$W/.err" ] && printf 'err=%q\n' "$(cat "$W/.err")"
  printf '\n'
}

emit par-two          /usr/bin/paste "$W/f" "$W/g"
emit par-three        /usr/bin/paste "$W/f" "$W/g" "$W/h"
emit par-short-last   /usr/bin/paste "$W/f" "$W/h"
emit par-short-mid    /usr/bin/paste "$W/f" "$W/h" "$W/g"
emit par-empty        /usr/bin/paste "$W/empty" "$W/g"
emit par-empty-first  /usr/bin/paste "$W/empty" "$W/f"
emit par-spaces       /usr/bin/paste "$W/spaces" "$W/g"
emit par-d-comma      /usr/bin/paste -d, "$W/f" "$W/g"
emit par-d-xy         /usr/bin/paste -dxy "$W/f" "$W/g"
emit par-d-xyz        /usr/bin/paste -dxyz "$W/f" "$W/g" "$W/h"
emit par-d-cycle      /usr/bin/paste -dxy "$W/f" "$W/g" "$W/h"
emit par-d-empty      /usr/bin/paste -d '' "$W/f" "$W/g"
emit par-d-long       /usr/bin/paste --delimiters=xy "$W/f" "$W/g"
emit ser-one          /usr/bin/paste -s "$W/f"
emit ser-two          /usr/bin/paste -s "$W/f" "$W/g"
emit ser-three        /usr/bin/paste -s "$W/f" "$W/g" "$W/h"
emit ser-empty-mid    /usr/bin/paste -s "$W/f" "$W/empty" "$W/g"
emit ser-d-xy         /usr/bin/paste -s -dxy "$W/f"
emit ser-d-empty      /usr/bin/paste -s -d '' "$W/f"
emit stdin-one        sh -c "/usr/bin/paste < $W/f"
emit stdin-two        sh -c "/usr/bin/paste - $W/g < $W/f"
emit stdin-dash-twice sh -c "/usr/bin/paste - - < $W/f"
emit err-missing      /usr/bin/paste "$W/nope" "$W/g"
emit err-missing-ser  /usr/bin/paste -s "$W/nope" "$W/g"
emit err-bad-opt      /usr/bin/paste -Q "$W/f"
emit err-bad-d        /usr/bin/paste "$W/f" -d
emit err-bad-long     /usr/bin/paste "$W/f" --delimiters
emit err-bs           /usr/bin/paste -d 'xy\' "$W/f"