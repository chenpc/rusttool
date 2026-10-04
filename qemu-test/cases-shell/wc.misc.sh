#!/usr/bin/env bash
# wc(1), seq(1), basename(1) and dirname(1) guest cases.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }
check() {
  if [ "$2" != "$3" ]; then
    printf 'FAIL: %s\n  expected: [%s]\n  actual:   [%s]\n' "$1" "$2" "$3" >&2
    exit 1
  fi
}

W=/tmp/misc-case
mkdir -p "$W"
printf 'a\nb\nc\n' > "$W/three"
printf 'one two\nthree\n' > "$W/words"
printf 'a b\n' > "$W/w1"

# --- wc -----------------------------------------------------------------
check "wc default columns" '3 3 6' "$(wc < "$W/three")"
check "wc -l" '3' "$(wc -l < "$W/three")"
check "wc -w" '3' "$(wc -w < "$W/words")"
check "wc -c" '6' "$(wc -c < "$W/three")"
check "wc -lw" '3 3' "$(wc -lw < "$W/three")"
# Seven-wide fields when the input is not a regular file (stdin), and a
# one-column layout for a single named file with a single count.
check "wc single file names it" '3 /tmp/misc-case/three' "$(wc -l "$W/three")"
check "wc two files align and total" ' 1  2  4 /tmp/misc-case/w1
 3  3  6 /tmp/misc-case/three
 4  5 10 total' "$(wc "$W/w1" "$W/three")"

# --- seq ----------------------------------------------------------------
check "seq single argument" '1
2
3
4
5' "$(seq 5)"
check "seq first last" '2
3
4' "$(seq 2 4)"
check "seq with step" '1
3
5' "$(seq 1 2 5)"
check "seq fractional" '0.0
0.5
1.0' "$(seq 0 0.5 1)"
check "seq attached separator" '1,2,3' "$(seq -s, 1 3)"
check "seq separate separator" '1,2,3' "$(seq -s , 1 3)"
check "seq equal width zero pads" '08
09
10' "$(seq -w 8 10)"
check "seq counts down" '3
2
1' "$(seq 3 -1 1)"
check "seq empty range" '' "$(seq 5 1)"

# --- basename / dirname -------------------------------------------------
check "basename absolute" 'sort' "$(basename /usr/bin/sort)"
check "basename relative" 'sort' "$(basename sort)"
check "basename suffix" 'sort' "$(basename /usr/bin/sort.exe .exe)"
check "basename trailing slash" 'bin' "$(basename /usr/bin/)"
check "basename root" '/' "$(basename /)"
check "dirname absolute" '/usr/bin' "$(dirname /usr/bin/sort)"
check "dirname bare" '.' "$(dirname sort)"
check "dirname top level" '/' "$(dirname /a)"
check "dirname trailing slash" '/usr' "$(dirname /usr/bin/)"
check "dirname empty" '.' "$(dirname '')"

rm -r "$W"
echo "wc/seq/basename/dirname ok"
exit 0
