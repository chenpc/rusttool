#!/usr/bin/env bash
# Generate every expected value for the expand guest case from /usr/bin/expand.
set -eu

W=/tmp/opencode/expand-case
rm -rf "$W"
mkdir -p "$W"
printf 'a\tb\n\tc\nab\tc\td\n' > "$W/t1"
printf 'x\ty\n  \tz\n\t\n\t\t\n\n' > "$W/t2"
printf 'abc\b\b\td\ne\tf' > "$W/t3"
printf '\t \t\ta\n \t \t b\n' > "$W/t4"
printf '12345678901234567890\tx\n' > "$W/t5"
printf -- '-\tz\n' > "$W/t6"

emit() {
  local label=$1
  shift
  printf '=== %s :: expand %s\n' "$label" "$*"
  local out rc
  out=$("$@" 2>"$W/.err") && rc=0 || rc=$?
  printf 'rc=%s\n' "$rc"
  printf 'out=%q\n' "$out"
  [ -s "$W/.err" ] && printf 'err=%q\n' "$(cat "$W/.err")"
  printf '\n'
}

emit default        /usr/bin/expand "$W/t1"
emit initial        /usr/bin/expand -i "$W/t1"
emit initial_mix    /usr/bin/expand -i "$W/t2"
emit initial_blank  /usr/bin/expand -i "$W/t4"
emit t4             /usr/bin/expand -t 4 "$W/t1"
emit t2             /usr/bin/expand -t 2 "$W/t1"
emit list           /usr/bin/expand -t 2,6 "$W/t1"
emit list_blank     /usr/bin/expand -t '2 6' "$W/t1"
emit extend         /usr/bin/expand -t 2,4,/3 "$W/t1"
emit increment      /usr/bin/expand -t 2,+3 "$W/t1"
emit extend_alone   /usr/bin/expand -t /3 "$W/t1"
emit increment_alone /usr/bin/expand -t +2 "$W/t1"
emit digit          /usr/bin/expand -4 "$W/t1"
emit total_two      /usr/bin/expand "$W/t1" "$W/t2"
emit blank_runs     /usr/bin/expand "$W/t4"
emit wide           /usr/bin/expand "$W/t5"
emit backspace      /usr/bin/expand "$W/t3"
emit backspace_i    /usr/bin/expand -i "$W/t3"
emit carry          /usr/bin/expand "$W/t3" "$W/t6"
emit stdin          sh -c "/usr/bin/expand < $W/t1"
emit err_zero       /usr/bin/expand -t 0 "$W/t1"
emit err_desc       /usr/bin/expand -t 4,2 "$W/t1"
emit err_char       /usr/bin/expand -t 4x,2 "$W/t1"
emit err_spec       /usr/bin/expand -t 4+x "$W/t1"
emit err_big        /usr/bin/expand -t 99999999999999999999 "$W/t1"
emit err_repeat     /usr/bin/expand -t /3 -t /4 "$W/t1"
emit err_both       /usr/bin/expand -t /3 -t +1 "$W/t1"
emit err_missing    /usr/bin/expand "$W/nope"
emit err_no_arg     /usr/bin/expand "$W/t1" -t
emit err_bad_opt    /usr/bin/expand -Q "$W/t1"