#!/usr/bin/env bash
# Init stage: prove the share and the scratch space work before any testcase
# relies on them. Fails the boot (and therefore the test) if it does not.
set -u

fail() { echo "[stage] FAIL: $*" >&2; exit 1; }

# The share is mounted read-only; scratch is the tmpfs on /tmp. There is no
# `rm` in the guest (brush has no coreutils), and none is needed: /tmp is a
# fresh tmpfs on every boot.
probe=/tmp/init-selftest
echo scratch > "$probe" || fail "cannot write /tmp"
read -r back < "$probe" || fail "cannot read back"
[ "$back" = scratch ] || fail "unexpected content: $back"

# A published tool must answer --version.
for tool in rev column col colrm bits colcrt line ul; do
  [ -x "/mnt/bin/$tool" ] || continue
  /mnt/bin/"$tool" --version >/dev/null 2>&1 || fail "$tool --version failed"
done

echo "[stage] selftest ok"
exit 0
