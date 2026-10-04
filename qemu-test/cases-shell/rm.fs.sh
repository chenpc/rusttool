#!/usr/bin/env bash
# mkdir(1), rmdir(1) and rm(1) guest case: the filesystem effects matter more
# than the output here, so each step asserts on what is left on disk.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }

W=/tmp/fs-case
rm -rf "$W"
mkdir -p "$W"

# --- mkdir --------------------------------------------------------------
mkdir "$W/one" || fail "mkdir failed"
[ -d "$W/one" ] || fail "mkdir did not create the directory"

if mkdir "$W/one" 2>/dev/null; then
  fail "mkdir over an existing directory succeeded"
fi

mkdir -p "$W/a/b/c" || fail "mkdir -p failed"
[ -d "$W/a/b/c" ] || fail "mkdir -p did not create the whole chain"
mkdir -p "$W/a/b/c" || fail "mkdir -p over an existing chain failed"

# -p through an existing *file* is still an error
: > "$W/file"
if mkdir -p "$W/file/child" 2>/dev/null; then
  fail "mkdir -p through a file succeeded"
fi

# --- rm -----------------------------------------------------------------
: > "$W/one/file"
rm "$W/one/file" || fail "rm of a file failed"
[ -e "$W/one/file" ] && fail "rm left the file behind"

if rm "$W/one" 2>/dev/null; then
  fail "rm removed a directory without -r"
fi
rm -r "$W/a" || fail "rm -r failed"
[ -e "$W/a" ] && fail "rm -r left the tree behind"

rm -f "$W/definitely-not-here" || fail "rm -f of a missing file failed"
if rm -f "$W" 2>/dev/null; then
  fail "rm -f removed a directory"
fi

# --- rmdir --------------------------------------------------------------
mkdir -p "$W/x/y/z"
rmdir "$W/x/y/z" || fail "rmdir failed"
[ -e "$W/x/y/z" ] && fail "rmdir left the directory behind"

: > "$W/x/y/child"
if rmdir "$W/x/y" 2>/dev/null; then
  fail "rmdir removed a non-empty directory"
fi
rm "$W/x/y/child"
rmdir -p "$W/one/two" 2>/dev/null || true
mkdir -p "$W/p/q/r"
# -p walks up until something refuses; $W is not empty, so that is where it
# stops, with a diagnostic and a non-zero status, exactly like GNU.
if rmdir -p "$W/p/q/r" 2>/dev/null; then
  fail "rmdir -p claimed success past a non-empty parent"
fi
[ -e "$W/p/q/r" ] && fail "rmdir -p left the leaf behind"
[ -e "$W/p/q" ] && fail "rmdir -p left an inner parent behind"
[ -e "$W/p" ] && fail "rmdir -p left the outer parent behind"

# --- a round trip through the tools we just added -----------------------
mkdir -p "$W/round/sub"
printf 'one\ntwo\nthree\n' > "$W/round/sub/data"
got=$(head -n 2 "$W/round/sub/data")
[ "$got" = "one
two" ] || fail "head of a freshly written file: [$got]"
got=$(tail -n 1 "$W/round/sub/data")
[ "$got" = "three" ] || fail "tail of a freshly written file: [$got]"

rm -r "$W/round"
[ -e "$W/round" ] && fail "cleanup left the tree behind"

rm -rf "$W"
[ -e "$W" ] && fail "final cleanup failed"
echo "mkdir/rmdir/rm ok"
exit 0
