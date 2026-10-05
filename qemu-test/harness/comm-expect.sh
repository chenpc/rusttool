#!/usr/bin/env bash
# Generate every expected value for the comm guest case from /usr/bin/comm.
set -eu

W=/tmp/opencode/comm-case
rm -rf "$W"
mkdir -p "$W"
printf '%s\n' a b c d e > "$W/f1"
printf '%s\n' b c d f > "$W/f2"
printf '%s\n' a b c > "$W/same"
printf '%s\n' b a c > "$W/unsorted"
printf 'a b\nb\n' > "$W/prefix"

emit() {
  local label=$1
  shift
  printf '=== %s :: comm %s\n' "$label" "$*"
  local out rc
  out=$("$@" 2>"$W/.err") && rc=0 || rc=$?
  printf 'rc=%s\n' "$rc"
  printf 'out=%q\n' "$out"
  [ -s "$W/.err" ] && printf 'err=%q\n' "$(cat "$W/.err")"
  printf '\n'
}

emit plain            /usr/bin/comm "$W/f1" "$W/f2"
emit only1            /usr/bin/comm -1 "$W/f1" "$W/f2"
emit only2            /usr/bin/comm -2 "$W/f1" "$W/f2"
emit only3            /usr/bin/comm -3 "$W/f1" "$W/f2"
emit only12           /usr/bin/comm -12 "$W/f1" "$W/f2"
emit only123          /usr/bin/comm -123 "$W/f1" "$W/f2"
emit total            /usr/bin/comm --total "$W/f1" "$W/f2"
emit total1           /usr/bin/comm --total -1 "$W/f1" "$W/f2"
emit same             /usr/bin/comm "$W/same" "$W/same"
emit total_same       /usr/bin/comm --total "$W/same" "$W/same"
emit delim            /usr/bin/comm --output-delimiter=, "$W/f1" "$W/f2"
emit delim_multi      /usr/bin/comm --output-delimiter=' | ' "$W/f1" "$W/f2"
emit prefix           /usr/bin/comm "$W/prefix" "$W/f2"
emit stdin            sh -c "/usr/bin/comm - $W/f2 < $W/f1"
emit unsorted         /usr/bin/comm "$W/unsorted" "$W/f2"
emit unsorted_total   /usr/bin/comm --total "$W/unsorted" "$W/f2"
emit unsorted_nocheck /usr/bin/comm --nocheck-order "$W/unsorted" "$W/f2"
emit unsorted_check   /usr/bin/comm --check-order "$W/unsorted" "$W/f2"
emit second_only_bad  sh -c "printf 'a\n' > $W/one; printf 'b\na\n' > $W/two; /usr/bin/comm $W/one $W/two"
emit err_missing      /usr/bin/comm "$W/nope" "$W/f2"
emit err_no_operand   /usr/bin/comm "$W/f1"
emit err_extra        /usr/bin/comm "$W/f1" "$W/f2" "$W/f1"
emit err_bad_opt      /usr/bin/comm -Q "$W/f1" "$W/f2"
emit err_no_delim_arg /usr/bin/comm "$W/f1" "$W/f2" --output-delimiter