#!/usr/bin/env bash
# Differential test for touch against GNU touch. Timestamps are compared after
# setting them explicitly, and "now" cases only compare the shape of the result.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
SYS=/usr/bin/touch
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
pass=0
fail=0

fixture() {
  local dir="$1"
  printf 'x\n' > "$dir/plain"
  mkdir -p "$dir/tree"
  ln -s plain "$dir/link"
  # Pin the fixture's own timestamps, otherwise two runs differ before touch
  # has done anything at all.
  /usr/bin/touch -d "2001-01-01 00:00:00" "$dir/plain" "$dir/tree" "$dir/link"
}

snapshot() {
  local dir="$1"
  # Seconds, not nanoseconds: two runs that set the same date must agree.
  # "." is the case directory, whose timestamp is the time of the run and
  # would differ between the two sides; everything else is pinned.
  (cd "$dir" && find . -mindepth 1 -printf '%y %m %p %TY-%Tm-%Td %TH:%TM:%TS\n' | LC_ALL=C sort \
     | sed 's/\.[0-9]*$//')
}

normalise() {
  sed -e "s#$SYS#touch#g" -e "s#^\(.\{0,40\}\)/touch:#touch:#" \
      -e "s#Try '.*/touch --help'#Try 'touch --help'#" "$1"
}

run_case() {
  local name="$1"; shift
  local setup="$1"; shift
  local probe="$1"; shift
  local command="$*"
  local a="$T/a" b="$T/b"
  rm -rf "$a" "$b"; mkdir -p "$a" "$b"
  (cd "$a" && eval "$setup") >/dev/null 2>&1
  (cd "$b" && eval "$setup") >/dev/null 2>&1
  (cd "$a" && eval "$command") > "$T/out.a" 2> "$T/err.a"; local ra=$?
  (cd "$b" && eval "${command//touch /$SYS }") > "$T/out.b" 2> "$T/err.b"; local rb=$?
  normalise "$T/err.a" > "$T/err.a.n"; normalise "$T/err.b" > "$T/err.b.n"
  normalise "$T/out.a" > "$T/out.a.n"; normalise "$T/out.b" > "$T/out.b.n"
  { snapshot "$a"; (cd "$a" && eval "$probe" 2>&1); } > "$T/tree.a"
  { snapshot "$b"; (cd "$b" && eval "$probe" 2>&1); } > "$T/tree.b"
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
      diff "$T/err.a.n" "$T/err.b.n" | sed -n '1,6p' | sed 's/^/    err| /'
      diff "$T/tree.a" "$T/tree.b" | sed -n '1,10p' | sed 's/^/  tree| /'
    fi
  fi
}

F='fixture .'
P='stat -c "%n %Y %X %a" plain created 2>&1'
run_case "create missing"          "$F" "$P" 'touch created; stat -c "%n %a" created'
run_case "create several"          "$F" "$P" 'touch a b c; ls a b c'
run_case "no create"               "$F" "$P" 'touch -c created; ls created 2>&1'
run_case "no create long"          "$F" "$P" 'touch --no-create created; ls created 2>&1'
run_case "existing file"           "$F" "$P" 'touch plain; stat -c %a plain'
run_case "access only"             'fixture .; /usr/bin/touch -d "2001-01-01 00:00:00" plain' 'stat -c "%Y %X" plain' 'touch -a -d "2011-02-03 04:05:06" plain'
run_case "modification only"       'fixture .; /usr/bin/touch -d "2001-01-01 00:00:00" plain' 'stat -c "%Y %X" plain' 'touch -m -d "2011-02-03 04:05:06" plain'
run_case "date iso"                "$F" "$P" 'touch -d "2011-02-03 04:05:06" plain; stat -c "%Y" plain'
run_case "date iso T"              "$F" "$P" 'touch -d "2011-02-03T04:05:06" plain; stat -c "%Y" plain'
run_case "date day month year"     "$F" "$P" 'touch -d "3 Feb 2011 04:05:06" plain; stat -c "%Y" plain'
run_case "date with weekday"       "$F" "$P" 'touch -d "Thu, 3 Feb 2011 04:05:06" plain; stat -c "%Y" plain'
run_case "date time only"          "$F" "$P" 'touch -d "04:05:06" plain; stat -c "%Y" plain'
run_case "date epoch"              "$F" "$P" 'touch -d "@1000000000" plain; stat -c "%Y" plain'
run_case "date invalid"            "$F" "$P" 'touch -d "not a date" plain'
run_case "date invalid month"      "$F" "$P" 'touch -d "2011-13-45" plain'
run_case "stamp eight digits"      "$F" "$P" 'touch -t 201102030405 plain; stat -c "%Y" plain'
run_case "stamp with seconds"      "$F" "$P" 'touch -t 201102030405.06 plain; stat -c "%Y" plain'
run_case "stamp two digit year"    "$F" "$P" 'touch -t 1102030405 plain; stat -c "%Y" plain'
run_case "stamp invalid"           "$F" "$P" 'touch -t 2011-02-03 plain'
run_case "reference"               'fixture .; /usr/bin/touch -d "2005-06-07 08:09:10" plain' 'ls plain ref; stat -c "%Y" ref' 'touch -r plain ref; stat -c "%Y" ref'
run_case "reference missing"       "$F" "$P" 'touch -r nope created'
run_case "dereference symlink"     'fixture .; /usr/bin/touch -d "2001-01-01 00:00:00" plain' 'stat -c "%Y" plain' 'touch -d "2011-02-03 04:05:06" link; stat -c "%Y" plain'
run_case "no dereference symlink"  'fixture .; /usr/bin/touch -d "2001-01-01 00:00:00" plain' 'stat -c "%Y %N" plain link' 'touch -h -d "2011-02-03 04:05:06" link; stat -c "%Y %N" plain link'
run_case "directory"               "$F" "$P" 'touch -d "2011-02-03 04:05:06" tree; stat -c "%Y" tree'
run_case "time word access"        "$F" "$P" 'touch --time=access -d "2011-02-03 04:05:06" plain; stat -c "%Y" plain'
run_case "time word use"           "$F" "$P" 'touch --time=use: -d "2011-02-03 04:05:06" plain; stat -c "%Y" plain'
run_case "time word invalid"       "$F" "$P" 'touch --time=birth plain'
run_case "force ignored"           "$F" "$P" 'touch -f plain; stat -c %a plain'
run_case "double dash"             "$F" "$P" 'touch -- -weird; ls -weird'
run_case "no operand"              "$F" "$P" 'touch'
run_case "unwritable dir"          'fixture .; mkdir d; chmod 500 d' 'ls d' 'touch d/new'
run_case "several with dates"      "$F" "$P" 'touch -d "2011-02-03 04:05:06" plain created; stat -c "%Y" plain created'
run_case "now is roughly now"      "$F" 'stat -c %Y plain' 'touch plain; test $(( $(stat -c %Y plain) - $(date +%s) )) -lt 5 && echo recent'
run_case "empty date is midnight"  "$F" "$P" 'touch -d "" plain; stat -c "%Y" plain'

echo "==> $pass identical, $fail different (of $((pass + fail)))"
[ "$fail" = 0 ]