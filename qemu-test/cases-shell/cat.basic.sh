#!/usr/bin/env bash
# cat(1) guest case. Expected bytes verified against GNU coreutils by
# qemu-test/tmp/coreutils-diff.sh.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }
check() {
  if [ "$2" != "$3" ]; then
    printf 'FAIL: %s\n  expected: [%s]\n  actual:   [%s]\n' "$1" "$2" "$3" >&2
    exit 1
  fi
}

W=/tmp/cat-case
mkdir -p "$W"
printf 'a\nb\nc\n' > "$W/three"

check "plain" 'a
b
c' "$(cat "$W/three")"

check "numbering" '     1	a
     2	b
     3	c' "$(cat -n "$W/three")"

check "number non-blank" '     1	a

     2	b' "$(printf 'a\n\nb\n' | cat -nb)"

check "squeeze" 'a

b' "$(printf 'a\n\n\n\nb\n' | cat -s)"

check "show ends" 'a$
b$' "$(printf 'a\nb\n' | cat -E)"

check "show all" 'a^Ib$
c$' "$(printf 'a\tb\nc\n' | cat -A)"

check "meta notation" 'M-^@M-a' "$(printf '\200\341' | cat -v)"

check "two files" 'a
b
c
a
b
c' "$(cat "$W/three" "$W/three")"

check "stdin" 'x' "$(printf 'x' | cat)"

# a missing file is reported and the exit status is non-zero
if cat "$W/definitely-not-here" 2>/dev/null; then
  fail "missing file succeeded"
fi
# ... but the readable ones are still printed
check "missing then readable" 'a
b
c' "$(cat "$W/definitely-not-here" "$W/three" 2>/dev/null)"

rm -r "$W"
echo "cat ok"
exit 0
