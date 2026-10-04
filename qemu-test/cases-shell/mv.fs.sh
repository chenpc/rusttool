#!/usr/bin/env bash
# mv(1) in the guest.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }

W=/tmp/mv-case
rm -rf "$W"
mkdir -p "$W"

printf 'alpha\n' > "$W/plain"
printf 'other\n' > "$W/other"

# --- a rename -------------------------------------------------------------
mv "$W/plain" "$W/renamed" || fail "rename failed"
[ -f "$W/renamed" ] || fail "rename did not create the new name"
[ -e "$W/plain" ] && fail "rename left the old name"

# --- overwriting ----------------------------------------------------------
mv "$W/renamed" "$W/other" || fail "overwrite move failed"
got=$(cat "$W/other")
[ "$got" = "alpha" ] || fail "overwrite content: [$got]"
[ -e "$W/renamed" ] && fail "the overwritten source is still there"

# --- into a directory -----------------------------------------------------
mkdir -p "$W/dest"
printf 'x\n' > "$W/a"
printf 'y\n' > "$W/b"
mv "$W/a" "$W/b" "$W/dest" || fail "move into a directory failed"
[ -f "$W/dest/a" ] && [ -f "$W/dest/b" ] || fail "the files did not land"

# --- -n does not replace --------------------------------------------------
printf 'keep\n' > "$W/out"
mv -n "$W/plain" "$W/out" 2>/dev/null || true
got=$(cat "$W/out")
[ "$got" = "keep" ] || fail "-n replaced the destination: [$got]"

# --- directories ----------------------------------------------------------
mkdir -p "$W/tree/sub"
printf 'one\n' > "$W/tree/one"
printf 'two\n' > "$W/tree/sub/two"
mv "$W/tree" "$W/tree-moved" || fail "moving a directory failed"
[ -f "$W/tree-moved/sub/two" ] || fail "moving a directory lost a file"
[ -e "$W/tree" ] && fail "the moved directory is still there"

# a directory into an existing directory keeps its own subdirectory
mkdir -p "$W/parent"
mv "$W/tree-moved" "$W/parent" || fail "move into an existing directory failed"
[ -f "$W/parent/tree-moved/sub/two" ] || fail "the nested name was not kept"

# --- missing source -------------------------------------------------------
if mv "$W/nope" "$W/out" 2>/dev/null; then
  fail "moving a missing file succeeded"
fi

# --- onto itself ----------------------------------------------------------
printf 'x\n' > "$W/same"
if mv "$W/same" "$W/same" 2>/dev/null; then
  fail "moving a file onto itself succeeded"
fi
[ -f "$W/same" ] || fail "the self move removed the file"

rm -rf "$W"
echo "mv ok"
exit 0