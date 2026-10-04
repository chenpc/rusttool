#!/usr/bin/env bash
# touch(1), mktemp(1) and cp/mv attributes in the guest. Timestamps cannot be
# read back without stat, so these cases check what a shell can see.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }

W=/tmp/fs2-case
rm -rf "$W"
mkdir -p "$W"

# --- touch ----------------------------------------------------------------
touch "$W/created" || fail "touch did not create the file"
[ -f "$W/created" ] || fail "touch left no file"
[ -s "$W/created" ] && fail "touch created a file with content"

touch "$W/existing" || fail "touch of an existing file failed"

touch -a "$W/existing" || fail "touch -a failed"
touch -m "$W/existing" || fail "touch -m failed"
touch -c "$W/never" || fail "touch -c returned an error"
[ -e "$W/never" ] && fail "touch -c created the file"

# A directory is a file whose timestamps can be set, so this has to work.
touch "$W" || fail "touch on a directory failed"
if touch "$W/nodir/file" 2>/dev/null; then
  fail "touch in a missing directory succeeded"
fi
if touch 2>/dev/null; then
  fail "touch without an operand succeeded"
fi

# a date the manual spells out has to be accepted
touch -d "2024-02-29 16:21:42" "$W/dated" || fail "touch -d failed"
touch -t 202402291621 "$W/stamped" || fail "touch -t failed"
touch -d "2024-02-29" "$W/datestamp" || fail "touch -d with a date only failed"
if touch -d "not a date at all" "$W/bad" 2>/dev/null; then
  fail "touch -d with junk succeeded"
fi
[ -e "$W/bad" ] && fail "touch -d with junk created the file"

# a reference file has to exist
printf 'x\n' > "$W/reference"
touch -r "$W/reference" "$W/borrowed" || fail "touch -r failed"
if touch -r "$W/nope" "$W/x" 2>/dev/null; then
  fail "touch -r with a missing reference succeeded"
fi

# --- mktemp ---------------------------------------------------------------
name=$(mktemp) || fail "mktemp failed"
[ -f "$name" ] || fail "mktemp created nothing: [$name]"
[ -s "$name" ] && fail "mktemp created a non-empty file"
# the default template is tmp.XXXXXXXXXX
case "$name" in
  /tmp/tmp.*) ;;
  *) fail "mktemp default name: [$name]" ;;
esac
rm -f "$name"

name=$(mktemp ./tp.XXXXXX) || fail "mktemp with a template failed"
case "$name" in
  ./tp.??????) ;;
  *) fail "mktemp template name: [$name]" ;;
esac
[ -f "$name" ] || fail "mktemp with a template created nothing"
rm -f "$name"

name=$(mktemp -u ./dry.XXXXXX) || fail "mktemp -u failed"
[ -e "$name" ] && fail "mktemp -u created the file"
rm -f "$name"

dir=$(mktemp -d ./d.XXXXXX) || fail "mktemp -d failed"
[ -d "$dir" ] || fail "mktemp -d created no directory"
rmdir "$dir"

if mktemp ./tp.XX 2>/dev/null; then
  fail "mktemp with too few Xs succeeded"
fi
if mktemp ./plain 2>/dev/null; then
  fail "mktemp without Xs succeeded"
fi
rm -f ./plain 2>/dev/null || true

# two names in a row must differ
a=$(mktemp ./p.XXXXXX) || fail "first mktemp failed"
b=$(mktemp ./p.XXXXXX) || fail "second mktemp failed"
[ "$a" != "$b" ] || fail "mktemp returned the same name twice"
rm -f "$a" "$b"

rm -rf "$W"
echo "touch/mktemp ok"
exit 0