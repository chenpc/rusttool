#!/usr/bin/env bash
# head(1) and tail(1) guest cases.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }
check() {
  if [ "$2" != "$3" ]; then
    printf 'FAIL: %s\n  expected: [%s]\n  actual:   [%s]\n' "$1" "$2" "$3" >&2
    exit 1
  fi
}

W=/tmp/ht-case
mkdir -p "$W"
seq 1 20 > "$W/nums"

check "head default is ten lines" '1
2
3
4
5
6
7
8
9
10' "$(head "$W/nums")"

check "head -n 3" '1
2
3' "$(head -n 3 "$W/nums")"

check "head -n3 attached" '1
2
3' "$(head -n3 "$W/nums")"

check "head -c 4 takes four bytes" '1
2' "$(head -c 4 "$W/nums")"
check "head -c 5 cuts mid line" '1
2
3' "$(head -c 5 "$W/nums")"

check "head from stdin" '1
2' "$(seq 1 5 | head -n 2)"

check "head more than the file has" '1
2' "$(printf '1\n2\n' | head -n 9)"

check "head two files" '==> a <==
1
2

==> b <==
3
4' "$(printf '1\n2\n' > "$W/a"; printf '3\n4\n' > "$W/b"; head -n 2 "$W/a" "$W/b")"

check "head -q drops the headers" '1
2
3
4' "$(head -q -n 2 "$W/a" "$W/b")"

check "tail default is ten lines" '11
12
13
14
15
16
17
18
19
20' "$(tail "$W/nums")"

check "tail -n 3" '18
19
20' "$(tail -n 3 "$W/nums")"

check "tail -n1 attached" '20' "$(tail -n1 "$W/nums")"

check "tail -c 3 is three bytes" '20' "$(tail -c 3 "$W/nums")"

check "tail more than the file has" '1
2
3' "$(printf '1\n2\n3\n' | tail -n 9)"

check "tail counts an unterminated line" 'b
c' "$(printf 'a\nb\nc' | tail -n 2)"

check "tail two files" '==> a <==
1
2

==> b <==
3
4' "$(tail -n 2 "$W/a" "$W/b")"

# head must not buffer its input. Under a small address-space cap, an input bigger
# than the cap still has to work: a tool that read the whole input before selecting
# would fail here instead of printing the first line. This is what made head grow to
# 9.6 GB on /dev/zero on the host.
ulimit -v 16384
head -c 20000000 /dev/zero | head -n 1 > /tmp/head-buffer-check
ulimit -v unlimited
check "head streams, it does not slurp" '20000000' "$(wc -c < /tmp/head-buffer-check)"

rm -r "$W"
echo "head/tail ok"
exit 0
