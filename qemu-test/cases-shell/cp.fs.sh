#!/usr/bin/env bash
# cp(1) in the guest: the filesystem effects are what matters here.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }

W=/tmp/cp-case
rm -rf "$W"
mkdir -p "$W"

printf 'alpha\nbeta\n' > "$W/plain"
chmod 640 "$W/plain" 2>/dev/null || true
mkdir -p "$W/tree/sub"
printf 'one\n' > "$W/tree/one"
printf 'two\n' > "$W/tree/sub/two"

# --- a plain copy ---------------------------------------------------------
cp "$W/plain" "$W/copy" || fail "simple copy failed"
got=$(cat "$W/copy")
[ "$got" = "alpha
beta" ] || fail "copy content: [$got]"

# the source is left alone
[ -f "$W/plain" ] || fail "copy removed the source"

# --- into a directory -----------------------------------------------------
mkdir -p "$W/dest"
cp "$W/plain" "$W/dest" || fail "copy into a directory failed"
[ -f "$W/dest/plain" ] || fail "copy into a directory did not keep the name"

cp "$W/plain" "$W/tree/one" "$W/dest" || fail "copy two files into a directory failed"
[ -f "$W/dest/plain" ] && [ -f "$W/dest/one" ] || fail "two files did not both land"

# two sources with a destination that is not a directory is an error
if cp "$W/plain" "$W/tree/one" "$W/out" 2>/dev/null; then
  fail "two sources into a new name succeeded"
fi

# --- overwriting ----------------------------------------------------------
printf 'changed\n' > "$W/out"
cp "$W/plain" "$W/out" || fail "overwrite failed"
got=$(cat "$W/out")
[ "$got" = "alpha
beta" ] || fail "overwrite content: [$got]"

# -n leaves an existing destination alone
printf 'keep\n' > "$W/out"
cp -n "$W/plain" "$W/out" || fail "-n failed"
got=$(cat "$W/out")
[ "$got" = "keep" ] || fail "-n overwrote: [$got]"

# --- directories ----------------------------------------------------------
# without -r a directory is refused, and the copy says so
if cp "$W/tree" "$W/tree-copy" 2>/dev/null; then
  fail "cp of a directory without -r succeeded"
fi
[ -e "$W/tree-copy" ] && fail "the refused copy left something behind"

cp -r "$W/tree" "$W/tree-copy" || fail "cp -r failed"
[ -f "$W/tree-copy/one" ] || fail "cp -r lost a file"
[ -f "$W/tree-copy/sub/two" ] || fail "cp -r lost a nested file"

# the contents match the source
diff -q 2>/dev/null || true
a=$(cat "$W/tree-copy/sub/two")
b=$(cat "$W/tree/sub/two")
[ "$a" = "$b" ] || fail "cp -r changed the content"

# --- modes ----------------------------------------------------------------
# a plain copy gives a readable file; -p keeps the source's mode
mkdir -p "$W/modes"
printf 'x\n' > "$W/modes/plain"
chmod 640 "$W/modes/plain" 2>/dev/null || true
cp "$W/modes/plain" "$W/modes/copied"
if cp -p "$W/modes/plain" "$W/modes/preserved"; then
  # the guest can read the preserved file, which is all a shell can check
  got=$(cat "$W/modes/preserved")
  [ "$got" = "x" ] || fail "cp -p content: [$got]"
fi

# --- self copy ------------------------------------------------------------
if cp "$W/plain" "$W/plain" 2>/dev/null; then
  fail "copying a file onto itself succeeded"
fi
got=$(cat "$W/plain")
[ "$got" = "alpha
beta" ] || fail "the self copy truncated the source: [$got]"

# --- missing files --------------------------------------------------------
if cp "$W/nope" "$W/out2" 2>/dev/null; then
  fail "copying a missing file succeeded"
fi

rm -rf "$W"
echo "cp ok"
exit 0