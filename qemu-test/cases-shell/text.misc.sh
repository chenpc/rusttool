#!/usr/bin/env bash
# The text tools in the guest: cut, tr, sort, uniq, tee and readlink.
#
# Every expected value below is what GNU coreutils prints for the same input and
# command; they were generated with the system tools rather than written by
# hand, because a hand-written expectation is how a case starts lying.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }
check() {
  if [ "$2" != "$3" ]; then
    printf 'FAIL: %s\n  expected: [%s]\n  actual:   [%s]\n' "$1" "$2" "$3" >&2
    exit 1
  fi
}

W=/tmp/text-case
rm -rf "$W"
mkdir -p "$W"

printf 'a\tb\tc\nd\te\nlonely\nf\tg\th\ti\n' > "$W/fields"
printf 'apple\nbanana\ncherry\napple\n' > "$W/words"
printf '10\n9\n2\n100\n' > "$W/numbers"
printf 'a\na\nb\nb\nb\nc\n' > "$W/repeats"

# --- cut ------------------------------------------------------------------
check "cut one field" 'a
d
lonely
f' "$(cut -f1 "$W/fields")"

# The manual says a line with no delimiter is printed as it is, which is why
# "lonely" shows up between the selected fields.
check "cut two fields" 'b	c
e
lonely
g	h' "$(cut -f2,3 "$W/fields")"

check "cut a range" 'b	c
e
lonely
g	h' "$(cut -f2-3 "$W/fields")"

check "cut only delimited" 'a
d
f' "$(cut -s -f1 "$W/fields")"

check "cut complement" 'b	c
e
lonely
g	h	i' "$(cut --complement -f1 "$W/fields")"

check "cut characters" 'ab
de
ln
fg' "$(cut -c1,3 "$W/fields")"

check "cut custom delimiter" 'b' "$(printf 'a:b:c\n' | cut -d: -f2)"
check "cut output delimiter" 'a,c
d
lonely
f,h' "$(cut -f1,3 --output-delimiter=, "$W/fields")"

if cut "$W/fields" >/dev/null 2>&1; then
  fail "cut without -b/-c/-f succeeded"
fi
if cut -f0 "$W/fields" >/dev/null 2>&1; then
  fail "cut -f0 succeeded"
fi

# --- tr -------------------------------------------------------------------
check "tr translate" 'bca' "$(printf abc | tr abc bca)"
check "tr upper case" 'HELLO' "$(printf hello | tr a-z A-Z)"
check "tr class" '12' "$(printf a1b2 | tr -cd '[:digit:]')"
check "tr delete" 'abc' "$(printf a1b2c3 | tr -d '[:digit:]')"
check "tr squeeze" 'a b' "$(printf 'a   b' | tr -s ' ')"
check "tr octal" 'a-b' "$(printf 'a	b' | tr '\t' '-')"
check "tr range" '111' "$(printf abc | tr a-c 1)"

# --- sort -----------------------------------------------------------------
check "sort plain" 'apple
apple
banana
cherry' "$(sort "$W/words")"
check "sort reverse" 'cherry
banana
apple
apple' "$(sort -r "$W/words")"
check "sort numeric" '2
9
10
100' "$(sort -n "$W/numbers")"
check "sort unique" 'apple
banana
cherry' "$(sort -u "$W/words")"
check "sort key" 'a 3
b 1' "$(printf 'b 1\na 3\n' | sort -k1,1)"
check "sort version" 'a-2
a-10' "$(printf 'a-10\na-2\n' | sort -V)"
check "sort check" 'sorted' "$(printf 'a\nb\nc\n' | sort -c && echo sorted)"

# --- uniq -----------------------------------------------------------------
check "uniq default" 'a
b
c' "$(uniq "$W/repeats")"
check "uniq count" '      2 a
      3 b
      1 c' "$(uniq -c "$W/repeats")"
check "uniq repeated" 'a
b' "$(uniq -d "$W/repeats")"
check "uniq unique" 'c' "$(uniq -u "$W/repeats")"
check "uniq ignore case" 'A
B' "$(printf 'A\na\nB\n' | uniq -i)"
# -f1 compares what follows the first field, so "a 1" and "b 1" are one group.
check "uniq skip fields" 'a 1' "$(printf 'a 1\nb 1\n' | uniq -f1)"

# --- tee ------------------------------------------------------------------
printf 'tee\n' | tee "$W/tee-out" > /dev/null || fail "tee failed"
check "tee wrote the file" 'tee' "$(cat "$W/tee-out")"
check "tee echoed to stdout" 'through' "$(printf 'through\n' | tee "$W/tee-out")"
# The check above truncated the file, so what -a adds follows "through".
printf 'more\n' | tee -a "$W/tee-out" > /dev/null
check "tee append" 'through
more' "$(cat "$W/tee-out")"
printf 'two\n' | tee "$W/tee-a" "$W/tee-b" > /dev/null
[ -f "$W/tee-a" ] && [ -f "$W/tee-b" ] || fail "tee did not write both files"

# --- readlink -------------------------------------------------------------
printf 'target\n' > "$W/plain"
ln -s plain "$W/link" || fail "symlink creation failed"
check "readlink target" 'plain' "$(readlink "$W/link")"
check "readlink no newline" 'plain' "$(readlink -n "$W/link"; echo)"
if readlink "$W/plain" >/dev/null 2>&1; then
  fail "readlink of a plain file succeeded"
fi
mkdir -p "$W/dir"
ln -s dir "$W/dirlink"
got=$(readlink -f "$W/dirlink")
case "$got" in
  */dir) ;;
  *) fail "readlink -f: [$got]" ;;
esac

rm -rf "$W"
echo "text tools ok"
exit 0