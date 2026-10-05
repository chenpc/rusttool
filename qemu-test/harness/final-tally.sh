#!/usr/bin/env bash
# The final tally: unit tests for every crate, every differential harness, and the
# QEMU suite.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
cd /home/chenpc/git/rusttool
echo "=== unit tests (whole workspace) ==="
cargo test --workspace 2>&1 | grep -E 'test result:' | awk '
  { pass += $4; fail += $6 }
  END { printf "unit tests: %d passed, %d failed\n", pass, fail }'
echo
echo "=== differentials ==="
cd qemu-test/harness
total_pass=0
total_fail=0
# Every harness in the directory, discovered rather than listed: a hard-coded
# list is how small-diff.sh and the util-linux harnesses quietly stopped being
# counted. Superseded harnesses are named here so the exclusion is visible.
SKIP='small-diff.sh'
for script in *-diff.sh; do
  case " $SKIP " in *" $script "*) continue ;; esac
  bash "$script" > "/tmp/final-$script.log" 2>&1
  line=$(grep -oE '[0-9]+ identical, [0-9]+ different' "/tmp/final-$script.log" | tail -1)
  printf '%-20s %s\n' "$script" "$line"
  pass=$(printf '%s' "$line" | sed -n 's/^\([0-9]*\) identical.*/\1/p')
  fail=$(printf '%s' "$line" | sed -n 's/.* \([0-9]*\) different.*/\1/p')
  total_pass=$((total_pass + ${pass:-0}))
  total_fail=$((total_fail + ${fail:-0}))
done
echo "==> differentials: $total_pass identical, $total_fail different"
echo
echo "=== QEMU guest suite ==="
# Booted fresh here rather than read from /tmp/qemu-final.log, so the tally can
# never quote a suite that no longer matches the share on disk.
cd /home/chenpc/git/rusttool/qemu-test
./pack-initramfs.sh > logs/pack.log 2>&1 || { echo "pack failed, see logs/pack.log" >&2; exit 1; }
./run-many.sh 2>&1 | grep -aE '==> SUMMARY'
echo
echo "=== guest share ==="
ls /home/chenpc/git/rusttool/qemu-test/guest-root/bin/ | tr '\n' ' '
echo
du -sh /home/chenpc/git/rusttool/qemu-test/guest-root/