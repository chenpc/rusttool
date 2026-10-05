#!/usr/bin/env bash
# Differential test: our coreutils batch vs the system binaries.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
REPO=/home/chenpc/git/rusttool
BIN=$REPO/target/debug
cargo build --quiet --manifest-path "$REPO/Cargo.toml" 2>/dev/null

pass=0
fail=0
failed=()

T=$(mktemp -d); trap 'rm -rf "$T"' EXIT; cd "$T"
printf 'a\nb\nc\n'      > f3
printf 'one two\nthree\n' > fw
printf 'x'               > f1
seq 1 5                 > f5

cmp_run() { # cmp_run <label> <our-binary> <system-binary> <args...>
  local label=$1 ours=$2 sys=$3; shift 3
  "$BIN/$ours" "$@" > o.out 2> o.err; local orc=$?
  "$sys"        "$@" > s.out 2> s.err; local src=$?
  if cmp -s o.out s.out && [ "$orc" = "$src" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); failed+=("$label")
    printf 'DIFF %-28s rc %s vs %s\n' "$label" "$orc" "$src"
    diff <(cat -A s.out) <(cat -A o.out) | head -6
    diff <(cat -A s.err) <(cat -A o.err) | head -4
  fi
}

cmp_stdin() { # cmp_stdin <label> <input-file> <our-binary> <system-binary> <args...>
  local label=$1 input=$2 ours=$3 sys=$4; shift 4
  "$BIN/$ours" "$@" < "$input" > o.out 2> o.err; local orc=$?
  "$sys"        "$@" < "$input" > s.out 2> s.err; local src=$?
  if cmp -s o.out s.out && [ "$orc" = "$src" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); failed+=("$label")
    printf 'DIFF %-28s rc %s vs %s\n' "$label" "$orc" "$src"
    diff <(cat -A s.out) <(cat -A o.out) | head -6
  fi
}

# cat
for f in f3 fw f1 f5; do
  cmp_run "cat $f" cat /usr/bin/cat "$f"
  cmp_run "cat -n $f" cat /usr/bin/cat -n "$f"
  cmp_run "cat -b $f" cat /usr/bin/cat -b "$f"
  cmp_run "cat -s $f" cat /usr/bin/cat -s "$f"
  cmp_run "cat -A $f" cat /usr/bin/cat -A "$f"
  cmp_run "cat -E $f" cat /usr/bin/cat -E "$f"
  cmp_run "cat -t $f" cat /usr/bin/cat -t "$f"
  cmp_run "cat -v $f" cat /usr/bin/cat -v "$f"
  cmp_run "cat -nE $f" cat /usr/bin/cat -nE "$f"
done
cmp_run "cat two files" cat /usr/bin/cat f3 fw
cmp_run "cat missing" cat /usr/bin/cat nosuch

# head / tail
for f in f3 f5 fw; do
  cmp_run "head $f" head /usr/bin/head "$f"
  cmp_run "head -n2 $f" head /usr/bin/head -n 2 "$f"
  cmp_run "head -n1 $f" head /usr/bin/head -n1 "$f"
  cmp_run "head -c3 $f" head /usr/bin/head -c 3 "$f"
  cmp_run "head -q $f" head /usr/bin/head -q "$f"
  cmp_run "tail $f" tail /usr/bin/tail "$f"
  cmp_run "tail -n2 $f" tail /usr/bin/tail -n 2 "$f"
  cmp_run "tail -n1 $f" tail /usr/bin/tail -n1 "$f"
  cmp_run "tail -c3 $f" tail /usr/bin/tail -c 3 "$f"
done
cmp_run "head two files" head /usr/bin/head f3 fw
cmp_run "tail two files" tail /usr/bin/tail f3 fw

# wc
for f in f3 fw f1; do
  cmp_run "wc $f" wc /usr/bin/wc "$f"
  cmp_run "wc -l $f" wc /usr/bin/wc -l "$f"
  cmp_run "wc -w $f" wc /usr/bin/wc -w "$f"
  cmp_run "wc -c $f" wc /usr/bin/wc -c "$f"
  cmp_run "wc -lw $f" wc /usr/bin/wc -lw "$f"
done
cmp_run "wc two files" wc /usr/bin/wc f3 fw
cmp_run "wc three files" wc /usr/bin/wc f1 f3 fw
cmp_stdin "wc stdin" f3 wc /usr/bin/wc
cmp_stdin "wc -l stdin" f3 wc /usr/bin/wc -l

# seq
cmp_run "seq 5" seq /usr/bin/seq 5
cmp_run "seq 2 4" seq /usr/bin/seq 2 4
cmp_run "seq 1 2 9" seq /usr/bin/seq 1 2 9
cmp_run "seq 0 .25 1" seq /usr/bin/seq 0 0.25 1
cmp_run "seq -s, 1 3" seq /usr/bin/seq -s, 1 3
cmp_run "seq -w 8 10" seq /usr/bin/seq -w 8 10
cmp_run "seq 5 1" seq /usr/bin/seq 5 1
# Option parsing stops at the first operand, so these are operands and not
# options: nothing is permuted to the front.
cmp_run "seq counts down" seq /usr/bin/seq 3 -1 1
cmp_run "seq negative last" seq /usr/bin/seq -3
cmp_run "seq negative first" seq /usr/bin/seq -3 5
cmp_run "seq leading point" seq /usr/bin/seq -.5 0
cmp_run "seq hex float" seq /usr/bin/seq -0x3 0x1
cmp_run "seq dash is an operand" seq /usr/bin/seq - 
cmp_run "seq numeric prefix" seq /usr/bin/seq -3w
cmp_run "seq inf is options" seq /usr/bin/seq -inf
cmp_run "seq option after operand" seq /usr/bin/seq 1 -w 5
cmp_run "seq option after operand 2" seq /usr/bin/seq 5 -w 1
cmp_run "seq shebang line" seq /usr/bin/seq -f '%g' 1 3
cmp_run "seq too many operands" seq /usr/bin/seq 1 2 3 4
cmp_run "seq missing operand" seq /usr/bin/seq
cmp_run "seq bad number" seq /usr/bin/seq x
cmp_run "seq bad second" seq /usr/bin/seq 1 x
cmp_run "seq bad third" seq /usr/bin/seq 1 2 x
cmp_run "seq nan" seq /usr/bin/seq nan
cmp_run "seq zero increment" seq /usr/bin/seq 1 0 5
cmp_run "seq unknown short" seq /usr/bin/seq -i 1
cmp_run "seq unknown long" seq /usr/bin/seq --nonsense
cmp_run "seq missing -f value" seq /usr/bin/seq -f
cmp_run "seq missing -s value" seq /usr/bin/seq -s
cmp_run "seq missing separator" seq /usr/bin/seq --separator
cmp_run "seq -f and -w" seq /usr/bin/seq -w -f %g 1 2
# The default format depends on which operands were written down.
cmp_run "seq precision ignores last" seq /usr/bin/seq 3.25
cmp_run "seq precision from first" seq /usr/bin/seq 1.5 3
cmp_run "seq precision from step" seq /usr/bin/seq 0 0.25 1
cmp_run "seq exponent precision" seq /usr/bin/seq 1e-3 0.1
cmp_run "seq -w widths" seq /usr/bin/seq -w 08 10
cmp_run "seq -w zero" seq /usr/bin/seq -w 0 100
cmp_run "seq -w negative" seq /usr/bin/seq -w -8 10
cmp_run "seq -w fractional" seq /usr/bin/seq -w 8.5 10
# -f is handed to printf, so every conversion and flag has to agree.
cmp_run "seq -f %g" seq /usr/bin/seq -f %g 1 2
cmp_run "seq -f %f" seq /usr/bin/seq -f %f 1 2
cmp_run "seq -f %e" seq /usr/bin/seq -f %e 1 2
cmp_run "seq -f %.2f" seq /usr/bin/seq -f %.2f 1 2
cmp_run "seq -f %10.3f" seq /usr/bin/seq -f %10.3f 1 2
cmp_run "seq -f %-8g|" seq /usr/bin/seq -f '%-8g|' 1 2
cmp_run "seq -f %08.2f" seq /usr/bin/seq -f %08.2f 1 2
cmp_run "seq -f %Lg" seq /usr/bin/seq -f %Lg 1 2
cmp_run "seq -f %.2Lf" seq /usr/bin/seq -f %.2Lf 1 2
cmp_run "seq -f x%gy" seq /usr/bin/seq -f 'x%gy' 1 2
cmp_run "seq -f %+g" seq /usr/bin/seq -f %+g 1 2
cmp_run "seq -f %#g" seq /usr/bin/seq -f %#g 1 2
cmp_run "seq -f %%g" seq /usr/bin/seq -f '% g' 1 2
cmp_run "seq -f two directives" seq /usr/bin/seq -f '%g %g' 1 2
cmp_run "seq -f no directive" seq /usr/bin/seq -f %% 1 2
cmp_run "seq -f literal only" seq /usr/bin/seq -f x 1 2
cmp_run "seq -f ends in %" seq /usr/bin/seq -f % 1 2
cmp_run "seq -f width only" seq /usr/bin/seq -f %5 1 2
cmp_run "seq -f unknown %d" seq /usr/bin/seq -f %d 1 2
cmp_run "seq -f unknown %l" seq /usr/bin/seq -f %lf 1 2
cmp_run "seq -f second point" seq /usr/bin/seq -f %5.3.2g 1 2
cmp_run "seq -f escaped percent" seq /usr/bin/seq -f '%%%g' 1 2
cmp_run "seq -f suffix percent" seq /usr/bin/seq -f '%g%%' 1 2
# The last number past LAST gets a second look, which is what keeps a
# three-digit step from losing its last digit.
cmp_run "seq rounding rule" seq /usr/bin/seq 0 0.000001 0.000003

# basename / dirname
for p in /usr/bin/sort sort / /usr/bin/ a/b/c ""; do
  cmp_run "basename $p" basename /usr/bin/basename "$p"
  cmp_run "dirname $p" dirname /usr/bin/dirname "$p"
done
cmp_run "basename suffix" basename /usr/bin/basename /usr/bin/sort.exe .exe
cmp_run "dirname trailing" dirname /usr/bin/dirname /usr/bin/

# mkdir / rmdir / rm (filesystem effects compared separately below)
echo "==> $pass identical, $fail different (of $((pass + fail)))"
if [ "$fail" -gt 0 ]; then
  printf '    %s\n' "${failed[@]}"
  exit 1
fi
