#!/usr/bin/env bash
# Every differential harness, with our binaries on PATH so the "ours" side is
# really ours.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
cd /home/chenpc/git/rusttool
cargo build --quiet --workspace
cd qemu-test/harness
total_pass=0
total_fail=0
SKIP='small-diff.sh'
for script in *-diff.sh; do
  [ -f "$script" ] || continue
  case " $SKIP " in *" $script "*) continue ;; esac
  bash "$script" > "/tmp/all-$script.log" 2>&1
  line=$(grep -E 'identical' "/tmp/all-$script.log" | tail -1)
  printf '%-20s %s\n' "$script" "$line"
  case "$line" in
    *"$script:"*) ;;
    *) line=$(grep -oE '[0-9]+ identical, [0-9]+ different' "/tmp/all-$script.log" | tail -1) ;;
  esac
  pass=$(printf '%s' "$line" | sed -n 's/^\([0-9]*\) identical.*/\1/p')
  fail=$(printf '%s' "$line" | sed -n 's/.* \([0-9]*\) different.*/\1/p')
  [ -n "$pass" ] && total_pass=$((total_pass + pass))
  [ -n "$fail" ] && total_fail=$((total_fail + fail))
  grep '^FAIL' "/tmp/all-$script.log" | sed 's/^/    /'
done
echo "==> total: $total_pass identical, $total_fail different"