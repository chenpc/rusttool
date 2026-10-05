#!/usr/bin/env bash
# Re-run every per-tool differential now that they really use our binaries.
set -u
cd /home/chenpc/git/rusttool/qemu-test/harness
for script in cp-diff.sh mv-diff.sh touch-diff.sh tee-diff.sh mktemp-diff.sh \
              readlink-diff.sh cut-diff.sh tr-diff.sh sort-diff.sh uniq-diff.sh; do
  bash "$script" > "/tmp/$script.log" 2>&1
  summary=$(grep 'identical' "/tmp/$script.log" | tail -1)
  printf '%-18s %s\n' "$script" "$summary"
  grep '^FAIL' "/tmp/$script.log" | sed 's/^/    /'
done