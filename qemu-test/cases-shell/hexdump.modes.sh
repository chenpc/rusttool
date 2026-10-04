#!/usr/bin/env bash
# hexdump guest cases. Expected bytes were captured from util-linux and are
# also checked byte-for-byte against the host binary by qemu-test/tmp/hx-diff.sh.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }

# Trailing blanks are ignored: a short final block is blank-padded to the full
# line width, and that padding is verified byte-for-byte on the host side by
# qemu-test/tmp/hx-diff.sh. Here we care about the values.
strip_blanks() { # strip_blanks <text> -> text with trailing blanks removed per line
  while IFS= read -r line; do
    printf '%s\n' "${line%"${line##*[![:space:]]}"}"
  done <<< "$1"
}

check() { # check <label> <expected> <actual>
  if [ "$(strip_blanks "$2")" != "$(strip_blanks "$3")" ]; then
    printf 'FAIL: %s\n  expected: [%s]\n  actual:   [%s]\n' "$1" "$2" "$3" >&2
    exit 1
  fi
}

# 1. canonical over a 16-byte file
printf '0123456789abcdef' > /tmp/hd16
out=$(hexdump -C /tmp/hd16)
check "canonical 16" \
  '00000000  30 31 32 33 34 35 36 37  38 39 61 62 63 64 65 66  |0123456789abcdef|
00000010' "$out"

# 2. canonical over a short file: the hex column pads, the ascii column does not
printf 'abc' > /tmp/hd3
out=$(hexdump -C /tmp/hd3)
check "canonical short" \
  '00000000  61 62 63                                          |abc|
00000003' "$out"

# 3. squeeze: identical blocks collapse to a single *
printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' > /tmp/hd32
out=$(hexdump -C /tmp/hd32)
check "squeeze" \
  '00000000  61 61 61 61 61 61 61 61  61 61 61 61 61 61 61 61  |aaaaaaaaaaaaaaaa|
*
00000020' "$out"

# 4. -v defeats the squeeze: two dump lines plus the end address
#    (no coreutils in the guest, so count lines with the read builtin)
out=$(hexdump -Cv /tmp/hd32)
lines=0
while IFS= read -r _line; do
  lines=$((lines + 1))
done <<< "$out"
[ "$lines" = 3 ] || fail "-Cv line count: $lines"
last=$(while IFS= read -r _line; do last=$_line; done <<< "$out"; printf '%s' "$last")
check "-Cv last line" '00000020' "$last"

# 5. one-byte octal, stdin
out=$(printf '0123456789abcdef' | hexdump -b)
check "one-byte octal stdin" \
  '0000000 060 061 062 063 064 065 066 067 070 071 141 142 143 144 145 146
0000010' "$out"

# 6. skip counts into the address
out=$(hexdump -C -s 8 /tmp/hd16)
check "skip 8" \
  '00000008  38 39 61 62 63 64 65 66                           |89abcdef|
00000010' "$out"

# 7. length truncates
out=$(hexdump -C -n 5 /tmp/hd16)
check "length 5" \
  '00000000  30 31 32 33 34                                    |01234|
00000005' "$out"

# 8. one-byte char renders control bytes as escapes
printf 'a\tb\n' > /tmp/hdtab
out=$(hexdump -c /tmp/hdtab)
check "one-byte char escapes" \
  '0000000   a  \t   b  \n
0000004' "$out"

# 9. two-byte decimal
out=$(printf 'abc' | hexdump -d)
check "two-byte decimal" \
  '0000000   25185   00099
0000003' "$out"

# 10. explicit format string
out=$(printf '0123456789abcdef' | hexdump -e '16/1 "%02x " "\n"')
check "explicit format" \
  '30 31 32 33 34 35 36 37 38 39 61 62 63 64 65 66' "$out"

# 11. a data byte that is a space survives: the nospace rule is about the
#     format string, never about the data, so the format's own trailing blank
#     goes but the space inside the data stays
printf 'x y' > /tmp/hdsp
out=$(hexdump -e '3/1 "%_c|" "\n"' /tmp/hdsp)
check "printable space in data" 'x| |y|' "$out"

# 12. error paths
if printf '' | hexdump -e 'bogus' 2>/dev/null; then
  fail "bad format accepted"
fi
msg=$(printf '' | hexdump -e 'bogus' 2>&1 >/dev/null)
case "$msg" in
  *"bad format {bogus}"*) ;;
  *) fail "bad format message: $msg" ;;
esac

if hexdump -C /tmp/definitely-not-here 2>/dev/null; then
  fail "missing file accepted"
fi

# 13. exit codes
printf 'abc' | hexdump -C > /dev/null || fail "exit code on success"
printf '' | hexdump -e 'bogus' > /dev/null 2>&1 && fail "exit code on bad format"

echo "hexdump cases ok"
exit 0
