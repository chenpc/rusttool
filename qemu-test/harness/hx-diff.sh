#!/usr/bin/env bash
# Differential test: our hexdump vs the system one, byte for byte.
#
#   usage: qemu-test/harness/hx-diff.sh
#
# The host binary is util-linux 2.39.3, ours tracks 2.42. -X (one-byte hex)
# only exists in newer releases, so it is excluded and checked separately.
set -u
# Non-interactive shells do not always have cargo on PATH.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
cd /home/chenpc/git/rusttool

cargo build -q -p hexdump || { echo "build failed" >&2; exit 1; }
# Absolute paths: the fixture directory is entered below.
MINE=$PWD/target/debug/hexdump
ORACLE=/usr/bin/hexdump

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
cd "$T"

printf '0123456789abcdef'          > p16
printf 'abc'                       > p3
printf ''                          > p0
printf 'a'                         > p1
printf '0123456789abcde'           > p15
printf '0123456789abcdef0123456789abcde'  > p31
printf '0123456789abcdef0123456789abcdef' > p32
# Deterministic fixtures: a byte ramp covers every value (including the high
# ones, where the escape/printable conversions get interesting) and keeps runs
# reproducible. /dev/urandom made failures impossible to reproduce.
ramp() { # ramp <file> <length>
  local file=$1 len=$2 i
  : > "$file"
  for ((i = 0; i < len; i++)); do printf "\\$(printf '%03o' $((i % 256)))"; done >> "$file"
}
ramp prand64 64
ramp pramp256 256
ramp pramp255 255
ramp pramp31 217
ramp pramp245 245
ramp pramp1024 1024
head -c 100 /dev/zero             > pz100
printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' > pa32
printf 'hello\tworld\n\0\001\377\177' > pctl
printf '0123456789abcdef01234567'  > p24
i=0
while [ $i -lt 12 ]; do
  ramp "r$i" $((i * 7))
  i=$((i + 1))
done

pass=0
fail=0
failed_cases=()

# Normalise the two things that cannot match by construction: the release
# string (we track 2.42, the host has 2.39.3) and the program name getopt
# prints (argv[0], which necessarily differs).
norm() { sed -e 's/^.*hexdump: /hexdump: /' -e 's/hexdump from util-linux .*/hexdump from util-linux X/' "$1"; }

check() { # check <label> <args...>
  local label=$1; shift
  "$ORACLE" "$@" > o.out 2> o.err.raw; local orc=$?
  "$MINE"   "$@" > m.out 2> m.err.raw; local mrc=$?
  norm o.err.raw > o.err; norm m.err.raw > m.err
  if cmp -s o.out m.out && cmp -s o.err m.err && [ "$orc" = "$mrc" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    failed_cases+=("$label")
    if [ "${VERBOSE:-0}" = 1 ]; then
      echo "--- MISMATCH: $label"
      echo "    oracle rc=$orc mine rc=$mrc"
      diff <(cat -A o.out) <(cat -A m.out) | head -8
      diff <(cat -A o.err) <(cat -A m.err) | head -4
    fi
  fi
}

for f in p0 p1 p3 p15 p16 p24 p31 p32 pa32 pctl prand64 pz100 pramp31 pramp245 pramp255 pramp256 pramp1024 r0 r1 r2 r3 r7 r11; do
  for mode in "" -b -c -C -d -o -x -v; do
    if [ -z "$mode" ]; then check "default $f" "$f"; else check "$mode $f" "$mode" "$f"; fi
  done
  check "-Cv $f" -Cv "$f"
  check "-xv $f" -xv "$f"
  check "-C -x $f" -C -x "$f"
  check "-b -C $f" -b -C "$f"
done

# skip / length
for f in p16 p24 p32 prand64; do
  for s in 0 1 3 8 16 17; do
    check "-s $s $f" -C -s "$s" "$f"
    check "-b -s $s $f" -b -s "$s" "$f"
  done
  for n in 1 5 8 15 16 17 40; do
    check "-n $n $f" -C -n "$n" "$f"
    check "-x -n $n $f" -x -n "$n" "$f"
  done
  check "-s 4 -n 10 $f" -C -s 4 -n 10 "$f"
done

# offsets with bases and multipliers
check "-s 0x8" -C -s 0x8 p16
check "-s 010" -C -s 010 p16
check "-s 1k" -b -s 1k p16
check "-s 1k big" -C -s 1k prand64

# explicit formats
check "-e hex" -e '16/1 "%02x " "\n"' p16
check "-e hex short" -e '16/1 "%02x " "\n"' p3
check "-e addr" -e '"%08_ax  " 8/1 "%02x " "  " 8/1 "%02x " ' p16
check "-e iter" -e '4/1 "%u" " \n"' p16
check "-e oct4" -e '4/4 "%010o" " \n"' p16
check "-e dec2" -e '8/2 "%05d " "\n"' p16
check "-e str" -e '1/8 "%.8s\n"' pctl
check "-e p" -e '"%_p\n"' p16
check "-e two formats" -e '"%_p\n"' -e '"%08_ax\n"' p16
check "-e A" -e '"%08_a  " 8/1 "%02x " "\n"' -e '"%_Ax\n"' p16
check "-e nospace-star" -e '"%08_ax " 16/1 "%03o " "\n"' p16

# stdin
stdin_case() { # stdin_case <label> <args...>  (reads p16)
  local label=$1; shift
  "$ORACLE" "$@" < p16 > o.out 2> o.err.raw; local orc=$?
  "$MINE"   "$@" < p16 > m.out 2> m.err.raw; local mrc=$?
  norm o.err.raw > o.err; norm m.err.raw > m.err
  if cmp -s o.out m.out && cmp -s o.err m.err && [ "$orc" = "$mrc" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); failed_cases+=("stdin $label")
    [ "${VERBOSE:-0}" = 1 ] && { echo "--- MISMATCH stdin $label"; diff <(cat -A o.out) <(cat -A m.out) | head -6; }
  fi
}
stdin_case "-b" -b
stdin_case "-C" -C
stdin_case "-Cv" -Cv
stdin_case "default"
stdin_case "-s 3 -n 5 -C" -C -s 3 -n 5

# error paths
check "bad format" -e 'bogus' p16
check "bad conv" -e '"%_Ap"' p16
check "bad size" -e '1/3 "%02x "' p16
check "%s no size" -e '"%s\n"' p16
check "unknown opt" -Z p16
check "missing file" -C nosuchfile
check "missing + ok" -C nosuchfile p16
check "bad offset" -C -s zz p16
check "bad length" -C -n zz p16
check "-e missing arg" -e
# -X (one-byte hex) only exists upstream from 2.40 on, and the version string
# differs by design, so help/version are checked by hand instead.
check "-X one-byte-hex" -X p16
check "-X short" -X p3
check "-X -v" -X -v pa32

# long options
check "--canonical" --canonical p16
check "--one-byte-octal" --one-byte-octal p16
check "--two-bytes-hex" --two-bytes-hex p16
check "--format" --format '16/1 "%02x "' p16
check "--length" --canonical --length 5 p16
check "--skip" --canonical --skip 3 p16
check "--no-squeezing" --canonical --no-squeezing p32

echo "==> $pass identical, $fail different (of $((pass + fail)))"
if [ "$fail" -gt 0 ]; then
  printf '    %s\n' "${failed_cases[@]}"
  exit 1
fi
