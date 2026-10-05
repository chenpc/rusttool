#!/usr/bin/env bash
# Shared differential-test helper: TOOL=the binary under test, SYS=the GNU one.
# Each case builds the same fixture in two directories, runs the same command
# against both, then compares the exit status, both output streams and the
# resulting tree.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

TOOL=${TOOL:?set TOOL}
SYS=${SYS:?set SYS}

# A tool that buffers its whole input grows without bound on an endless input such
# as /dev/zero, and on the host that reaches the OOM killer. Cap the address space
# so a case like that fails as a case instead of killing the machine; the guest runs
# the same binaries under -m 256M, so this mirrors that boundary.
ulimit -v 524288 2>/dev/null || true

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
pass=0
fail=0

snapshot() {
  local dir="$1"
  # "." itself is left out: its timestamp is the time of the run.
  (cd "$dir" && find . -mindepth 1 | LC_ALL=C sort)
  (cd "$dir" && find . -mindepth 1 -type f -print0 2>/dev/null | LC_ALL=C sort -z \
     | while IFS= read -r -d '' file; do
         printf 'content %s: ' "$file"; cat "$file"; printf '\n'
       done)
  (cd "$dir" && find . -mindepth 1 -type l -printf 'link %p -> %l\n' | LC_ALL=C sort)
}

# The binary under test is invoked through PATH, so the shell hands it an
# argv[0] that is the full path it found, and every diagnostic starts with it.
# Both sides then get the bare tool name: the system one by substitution, ours by
# dropping whatever directory the path had.
# NUL-separated output is turned into lines first, so that a tool ending its
# records with NUL can have them compared, and complained about, the same way as
# newline-separated output. Both sides go through it, so nothing is masked.
#
# `_` is set by the shell to the path it just executed and `PWD` to the directory
# it ran in, so the two sides always disagree about those two; only the rest of
# the environment is what a case is measuring.
normalise() {
  tr '\0' '\n' < "$1" \
    | sed -e "s#$SYS#$TOOL#g" -e "s#^.*/$TOOL:#$TOOL:#" \
          -e "s#^_=.*#_=<path>#" -e "s#^PWD=.*#PWD=<dir>#" \
          -e "s#Try '.*/$TOOL --help'#Try '$TOOL --help'#"
}

# run_case NAME SETUP COMMAND
run_case() {
  local name="$1"; shift
  local setup="$1"; shift
  local command="$*"
  local a="$T/a" b="$T/b"
  rm -rf "$a" "$b"; mkdir -p "$a" "$b"
  (cd "$a" && eval "$setup") >/dev/null 2>&1
  (cd "$b" && eval "$setup") >/dev/null 2>&1
  (cd "$a" && eval "$command") > "$T/out.a" 2> "$T/err.a"; local ra=$?
  (cd "$b" && eval "${command//$TOOL /$SYS }") > "$T/out.b" 2> "$T/err.b"; local rb=$?
  normalise "$T/err.a" > "$T/err.a.n"; normalise "$T/err.b" > "$T/err.b.n"
  normalise "$T/out.a" > "$T/out.a.n"; normalise "$T/out.b" > "$T/out.b.n"
  snapshot "$a" > "$T/tree.a"; snapshot "$b" > "$T/tree.b"
  local problems=""
  [ "$ra" = "$rb" ] || problems="$problems exit($ra/$rb)"
  cmp -s "$T/out.a.n" "$T/out.b.n" || problems="$problems stdout"
  cmp -s "$T/err.a.n" "$T/err.b.n" || problems="$problems stderr"
  cmp -s "$T/tree.a" "$T/tree.b" || problems="$problems tree"
  if [ -z "$problems" ]; then
    pass=$((pass + 1)); printf 'ok   %s\n' "$name"
  else
    fail=$((fail + 1)); printf 'FAIL %s ->%s\n' "$name" "$problems"
    if [ -n "${VERBOSE:-}" ]; then
      diff "$T/out.a.n" "$T/out.b.n" | sed -n '1,8p' | sed 's/^/    out| /'
      diff "$T/err.a.n" "$T/err.b.n" | sed -n '1,8p' | sed 's/^/    err| /'
      diff "$T/tree.a" "$T/tree.b" | sed -n '1,10p' | sed 's/^/  tree| /'
    fi
  fi
}

summary() {
  echo "==> $TOOL: $pass identical, $fail different (of $((pass + fail)))"
  [ "$fail" = 0 ]
}