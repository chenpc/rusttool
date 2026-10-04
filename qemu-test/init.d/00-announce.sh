#!/usr/bin/env bash
# Init stage: announce the guest and what it can run.
#
# Stage scripts live on the 9p share (guest-root/init.d/*.sh) and run in name
# order before any testcase, executed by brush with PATH=/mnt/bin. They need
# no initramfs repack, so they are the place for boot-time checks and setup.
set -u

echo "[stage] guest up: uname-style id via /proc"
echo "[stage] tools available:"

# Only the tools with a testcase are published to the share; list what is here.
for tool in /mnt/bin/*; do
  [ -x "$tool" ] || continue
  name=${tool##*/}
  [ "$name" = brush ] && continue
  echo "[stage]   $name"
done

exit 0
