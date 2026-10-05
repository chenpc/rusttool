#!/usr/bin/env bash
# Crate inventory: which tools are implemented, which are still 0-byte skeletons,
# and which names on the bill have no crate at all yet.
#
# The old version of this script hard-coded a list of tool names, which meant it
# silently stopped covering anything added later. Everything here is discovered.
set -u
cd /home/chenpc/git/rusttool

done_list=()
empty_list=()
for dir in tools/*/; do
  name=${dir#tools/}; name=${name%/}
  [ -f "$name/Cargo.toml" ] || continue
  lib=$(wc -c < "tools/$name/src/lib.rs" 2>/dev/null || echo 0)
  main=$(wc -c < "tools/$name/src/main.rs" 2>/dev/null || echo 0)
  if [ "$lib" -eq 0 ] && [ "$main" -eq 0 ]; then
    empty_list+=("$name")
  else
    done_list+=("$name")
  fi
done

printf 'implemented (%d):\n' "${#done_list[@]}"
printf '  %s\n' "${done_list[@]}"
printf '\nempty skeletons (%d):\n' "${#empty_list[@]}"
printf '  %s\n' "${empty_list[@]}"

echo
echo "names on the bill with no crate at all:"
if [ -f MANIFEST.md ]; then
  awk -F'|' '/^\| todo \|/ {gsub(/ /,"",$3); print "  " $3}' MANIFEST.md | sort -u
else
  echo "  (run qemu-test/harness/gen-manifest.sh first)"
fi

echo
echo "man pages rendered: $(ls qemu-test/harness/man/*.txt 2>/dev/null | wc -l)"
echo "differential harnesses: $(ls qemu-test/harness/*-diff.sh 2>/dev/null | wc -l)"
echo "fuzzers: $(ls qemu-test/harness/*-fuzz.py 2>/dev/null | wc -l)"
echo "guest shell cases: $(ls qemu-test/cases-shell/*.sh 2>/dev/null | wc -l)"
