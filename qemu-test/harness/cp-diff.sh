#!/usr/bin/env bash
# Differential test for cp against GNU cp.
#
# Each case builds the same fixture in two directories, runs the same cp
# invocation against our binary in one and /usr/bin/cp in the other, then
# compares the exit status, the two output streams and the resulting tree.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
OURS=/home/chenpc/git/rusttool/target/debug/cp
SYS=/usr/bin/cp
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
pass=0
fail=0

# Build the fixture the cases share.
fixture() {
  local dir="$1"
  printf 'alpha\nbeta\ngamma\n' > "$dir/plain"
  chmod 640 "$dir/plain"
  mkdir -p "$dir/tree/sub"
  printf 'one\n' > "$dir/tree/one"
  printf 'two\n' > "$dir/tree/sub/two"
  printf 'exec\n' > "$dir/script"
  chmod 755 "$dir/script"
  ln -s plain "$dir/link"
  printf 'x' > "$dir/nonewline"
}

# Everything about a tree that a caller can observe, minus the values that
# cannot match between two directories (inode numbers, device numbers).
snapshot() {
  local dir="$1"
  (cd "$dir" && find . -printf '%y %m %p\n' | LC_ALL=C sort)
  (cd "$dir" && find . -type f -print0 | LC_ALL=C sort -z \
     | while IFS= read -r -d '' file; do
         printf 'content %s: ' "$file"
         cat "$file"
         printf '\n'
       done)
  (cd "$dir" && find . -mindepth 1 -type l -printf 'link %p -> %l\n' | LC_ALL=C sort)
}

# Normalise what can never match: the program name coreutils prints from
# argv[0], and bare inode numbers.
normalise() {
  sed -e "s#$SYS#cp#g" -e "s#^\\(.\\{0,40\\}\\)/cp:#cp:#" -e "s#Try '.*/cp --help'#Try 'cp --help'#" \
      -e 's#^\([0-9][0-9]*\)$#INODE#' "$1"
}

# run_case NAME SETUP COMMAND...
run_case() {
  local name="$1"; shift
  local setup="$1"; shift
  local a="$T/a" b="$T/b"
  rm -rf "$a" "$b"
  mkdir -p "$a" "$b"
  (cd "$a" && eval "$setup") >/dev/null 2>&1
  (cd "$b" && eval "$setup") >/dev/null 2>&1

  (cd "$a" && eval "$*") > "$T/out.a" 2> "$T/err.a"; local ra=$?
  (cd "$b" && eval "${*//cp /$SYS }") > "$T/out.b" 2> "$T/err.b"; local rb=$?

  normalise "$T/err.a" > "$T/err.a.n"
  normalise "$T/err.b" > "$T/err.b.n"
  normalise "$T/out.a" > "$T/out.a.n"
  normalise "$T/out.b" > "$T/out.b.n"
  snapshot "$a" > "$T/tree.a"
  snapshot "$b" > "$T/tree.b"

  local problems=""
  [ "$ra" = "$rb" ] || problems="$problems exit($ra/$rb)"
  cmp -s "$T/out.a.n" "$T/out.b.n" || problems="$problems stdout"
  cmp -s "$T/err.a.n" "$T/err.b.n" || problems="$problems stderr"
  cmp -s "$T/tree.a" "$T/tree.b" || problems="$problems tree"

  if [ -z "$problems" ]; then
    pass=$((pass + 1))
    printf 'ok   %s\n' "$name"
  else
    fail=$((fail + 1))
    printf 'FAIL %s ->%s\n' "$name" "$problems"
    if [ -n "${VERBOSE:-}" ]; then
      diff "$T/out.a.n" "$T/out.b.n" | sed -n '1,8p' | sed 's/^/    out| /'
      diff "$T/err.a.n" "$T/err.b.n" | sed -n '1,8p' | sed 's/^/    err| /'
      diff "$T/tree.a" "$T/tree.b" | sed -n '1,12p' | sed 's/^/  tree| /'
    fi
  fi
}

F='fixture .'
run_case "single file"            "$F" 'cp plain copy'
run_case "overwrite"              "$F; printf changed > copy" 'cp plain copy'
run_case "into directory"         "$F; mkdir dest" 'cp plain dest; ls dest'
run_case "two into directory"     "$F; mkdir dest" 'cp plain script dest; ls dest'
run_case "two without directory"  "$F" 'cp plain script out'
run_case "two onto a file"        "$F; printf x > afile" 'cp plain script afile'
run_case "missing source"         "$F" 'cp nope out'
run_case "directory no -r"        "$F" 'cp tree out'
run_case "recursive"              "$F" 'cp -r tree out'
run_case "recursive capital R"    "$F" 'cp -R tree out'
run_case "archive"                "$F" 'cp -a tree out'
run_case "verbose file"           "$F" 'cp -v plain out'
run_case "verbose recursive"      "$F" 'cp -rv tree out'
run_case "verbose two files"      "$F; mkdir dest" 'cp -v plain script dest'
run_case "no clobber"             "$F; printf keep > out" 'cp -n plain out; cat out'
run_case "update none"            "$F; printf keep > out" 'cp --update=none plain out; cat out'
run_case "update older"           "$F; printf newer > out; touch -d 2030-01-01 out" 'cp -u plain out; cat out'
run_case "update older other way" "$F; printf newer > out; touch -d 2030-01-01 out" 'cp --update=older plain out; cat out'
run_case "preserve mode"          "$F" 'cp -p script out; stat -c %a out'
run_case "plain mode"             "$F" 'cp plain out; stat -c %a out'
run_case "preserve timestamps"    "$F; touch -d 2001-02-03 04:05:06 plain" 'cp -p plain out; stat -c %Y out'
run_case "target directory"       "$F; mkdir dest" 'cp -t dest plain script; ls dest'
run_case "target dir is a file"   "$F; printf x > afile" 'cp -t afile plain'
run_case "no target directory"    "$F; mkdir dest" 'cp -T plain dest; ls dest'
run_case "no target dir two srcs" "$F" 'cp -T plain script dest'
run_case "parents"                "$F; mkdir dest" 'cp --parents tree/sub/two dest; find dest | LC_ALL=C sort'
run_case "parents missing dir"    "$F" 'cp --parents tree/sub/two dest'
run_case "symlink default"        "$F" 'cp link out'
run_case "symlink -P"             "$F" 'cp -P link out'
run_case "symlink in tree"        "$F; ln -s one tree/link" 'cp -r tree out; find out | LC_ALL=C sort'
run_case "symlink in tree -L"     "$F; ln -s one tree/link" 'cp -rL tree out; find out | LC_ALL=C sort'
run_case "dangling symlink"       "$F; ln -s nowhere dangling" 'cp dangling out'
run_case "dangling symlink -P"    "$F; ln -s nowhere dangling" 'cp -P dangling out'
run_case "hard link"              "$F" 'cp -l plain out; test out -ef plain && echo linked'
run_case "symbolic link option"   "$F" 'cp -s plain out; find out -printf "%p %l\n"'
run_case "self copy"              "$F" 'cp plain plain'
run_case "same file different"    "$F" 'cp plain .'
run_case "strip trailing slash"   "$F" 'cp --strip-trailing-slashes tree/ out'
run_case "trailing slash kept"    "$F" 'cp tree/ out'
run_case "empty file"             "$F; : > empty" 'cp empty out; stat -c %s out'
run_case "nonewline file"         "$F" 'cp nonewline out'
run_case "attributes only"        "$F" 'cp --attributes-only plain out; stat -c %s out'
run_case "attributes only mode"   "$F" 'cp --attributes-only script out; stat -c %a out'
run_case "backup"                 "$F; printf old > out" 'cp -b plain out; ls out*'
run_case "backup suffix"          "$F; printf old > out" 'cp -b -S .bak plain out; ls out*'
run_case "force unwritable"       "$F; printf old > out; chmod 000 out" 'cp -f plain out; stat -c %a out'
run_case "stdin to file"          "$F" 'printf piped | cp /dev/stdin out; cat out'
run_case "preserve all"           "$F" 'cp --preserve=all script out; stat -c %a out'
run_case "no preserve timestamps" "$F" 'cp --no-preserve=timestamps script out'
run_case "recursive into new"     "$F" 'cp -r tree deep; find deep | LC_ALL=C sort'
run_case "update flag with -r"    "$F" 'cp -ru tree out'
run_case "bad option"             "$F" 'cp --nonsense plain out'
run_case "no operand"             "$F" 'cp'
run_case "one operand"            "$F" 'cp plain'

echo "==> $pass identical, $fail different (of $((pass + fail)))"
[ "$fail" = 0 ]