#!/usr/bin/env bash
# Differential test for mv against GNU mv.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
SYS=/usr/bin/mv
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
pass=0
fail=0

fixture() {
  local dir="$1"
  printf 'alpha\nbeta\n' > "$dir/plain"
  printf 'other\n' > "$dir/other"
  mkdir -p "$dir/tree/sub"
  printf 'one\n' > "$dir/tree/one"
  printf 'two\n' > "$dir/tree/sub/two"
  ln -s plain "$dir/link"
  mkdir -p "$dir/emptydir"
  mkdir -p "$dir/full"
  printf 'x\n' > "$dir/full/child"
}

snapshot() {
  local dir="$1"
  (cd "$dir" && find . -printf '%y %m %p\n' | LC_ALL=C sort)
  (cd "$dir" && find . -type f -print0 | LC_ALL=C sort -z \
     | while IFS= read -r -d '' file; do
         printf 'content %s: ' "$file"; cat "$file"; printf '\n'
       done)
  (cd "$dir" && find . -mindepth 1 -type l -printf 'link %p -> %l\n' | LC_ALL=C sort)
}

normalise() {
  sed -e "s#$SYS#mv#g" -e "s#^\(.\{0,40\}\)/mv:#mv:#" -e "s#Try '.*/mv --help'#Try 'mv --help'#" "$1"
}

run_case() {
  local name="$1"; shift
  local setup="$1"; shift
  local a="$T/a" b="$T/b"
  rm -rf "$a" "$b"; mkdir -p "$a" "$b"
  (cd "$a" && eval "$setup") >/dev/null 2>&1
  (cd "$b" && eval "$setup") >/dev/null 2>&1
  (cd "$a" && eval "$*") > "$T/out.a" 2> "$T/err.a"; local ra=$?
  (cd "$b" && eval "${*//mv /$SYS }") > "$T/out.b" 2> "$T/err.b"; local rb=$?
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
      diff "$T/out.a.n" "$T/out.b.n" | sed -n '1,6p' | sed 's/^/    out| /'
      diff "$T/err.a.n" "$T/err.b.n" | sed -n '1,6p' | sed 's/^/    err| /'
      diff "$T/tree.a" "$T/tree.b" | sed -n '1,12p' | sed 's/^/  tree| /'
    fi
  fi
}

F='fixture .'
run_case "rename file"             "$F" 'mv plain renamed'
run_case "rename onto existing"    "$F" 'mv plain other; cat other'
run_case "into directory"          "$F; mkdir dest" 'mv plain dest; ls dest'
run_case "two into directory"      "$F; mkdir dest" 'mv plain other dest; ls dest'
run_case "two without directory"   "$F" 'mv plain other out'
run_case "missing source"          "$F" 'mv nope out'
run_case "verbose file"            "$F" 'mv -v plain renamed'
run_case "verbose overwrite"       "$F" 'mv -v plain other'
run_case "verbose directory"       "$F" 'mv -v tree renamed'
run_case "directory into new"      "$F" 'mv tree renamed; find renamed | LC_ALL=C sort'
run_case "directory into existing" "$F; mkdir dest" 'mv tree dest; find dest | LC_ALL=C sort'
run_case "directory same name"     "$F; mkdir dest" 'mv tree dest/tree; find dest | LC_ALL=C sort'
run_case "no clobber"              "$F" 'mv -n plain other; cat other'
run_case "force overwrite"         "$F" 'mv -f plain other; cat other'
run_case "update older"            "$F; touch -d 2030-01-01 other" 'mv -u plain other; cat other'
run_case "update older reverse"    "$F; touch -d 2000-01-01 plain" 'mv -u plain other; cat other'
run_case "symlink move"            "$F" 'mv link moved; find . -maxdepth 1 -printf "%y %p\n" | LC_ALL=C sort'
run_case "symlink into dir"        "$F; mkdir dest" 'mv link dest; find dest | LC_ALL=C sort'
run_case "empty dir"               "$F" 'mv emptydir renamed; find renamed | LC_ALL=C sort'
run_case "self move"               "$F" 'mv plain plain'
run_case "same file other name"    "$F" 'mv plain .'
run_case "backup"                  "$F" 'mv -b plain other; ls other*'
run_case "backup suffix"           "$F" 'mv -b -S .old plain other; ls other*'
run_case "target directory"        "$F; mkdir dest" 'mv -t dest plain other; ls dest'
run_case "target dir missing"      "$F" 'mv -t nodir plain'
run_case "no target directory"     "$F; mkdir dest" 'mv -T plain dest; ls dest'
run_case "no target dir 3 srcs"    "$F" 'mv -T plain other dest'
run_case "strip trailing slash"    "$F" 'mv --strip-trailing-slashes tree/ out; find out | LC_ALL=C sort'
run_case "file over directory"     "$F; mkdir dest" 'mv plain dest'
run_case "directory over file"     "$F" 'mv tree plain'
run_case "no operand"              "$F" 'mv'
run_case "one operand"             "$F" 'mv plain'
run_case "bad option"              "$F" 'mv --nonsense plain out'
run_case "interactive no"          "$F" 'printf n | mv -i plain other; cat other'
run_case "interactive yes"         "$F" 'printf y | mv -i plain other; cat other'
run_case "no copy same device"     "$F" 'mv --no-copy plain renamed'
run_case "unwritable destination"  "$F; mkdir dest; chmod 500 dest" 'mv plain dest/renamed'
run_case "many files"              "$F; mkdir dest; printf 1 > f1; printf 2 > f2; printf 3 > f3" 'mv f1 f2 f3 dest; ls dest'

echo "==> $pass identical, $fail different (of $((pass + fail)))"
[ "$fail" = 0 ]