#!/usr/bin/env bash
# Smoke test for the guest-side shell harness itself: proves that a
# shell-script testcase can run brush builtins, reach the tools under test
# through PATH, pipe, substitute output, and use the tmpfs scratch space.
set -u

fail() { echo "FAIL: $*" >&2; exit 1; }

# shell plumbing
[ "abc" = "abc" ] || fail "test builtin"
line=hello
[ "$line" = hello ] || fail "variable expansion"

# a tool under test, by name, via PATH=/mnt/bin
out=$(echo hello | rev)
[ "$out" = olleh ] || fail "rev via PATH: got '$out'"

out=$(printf 'ab\ncd\n' | rev)
[ "$out" = "ba
dc" ] || fail "rev multiline: got '$out'"

# pipes and exit codes
if echo x | rev | rev | rev > /tmp/smoke.out 2>/tmp/smoke.err; then
  :
else
  fail "pipe of rev exited non-zero"
fi
read -r piped < /tmp/smoke.out
[ "$piped" = x ] || fail "triple rev: got '$piped'"

# writable scratch on tmpfs, and redirection
echo scratch > /tmp/scratch.txt || fail "write /tmp"
[ -s /tmp/scratch.txt ] || fail "empty /tmp/scratch.txt"
read -r back < /tmp/scratch.txt
[ "$back" = scratch ] || fail "read back: got '$back'"

# a failing command must report non-zero
if echo hello | rev | grep-definitely-not-a-real-binary 2>/dev/null; then
  fail "missing command unexpectedly succeeded"
fi

echo "smoke-ok"
exit 0
