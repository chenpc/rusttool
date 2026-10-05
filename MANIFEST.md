# MANIFEST — 每個工具欠多少

> 由 `qemu-test/harness/gen-manifest.sh` 產生，**不要手改**。
> 來源是 oracle 所在 package 的檔案清單（`coreutils util-linux util-linux-extra mount fdisk bsdextrautils bsdmainutils`）交叉比對 `tools/`。
> 生成時間：2026-10-05 05:30 UTC

## 現況

| 狀態 | 數量 |
|---|---|
| done（已實作，三層驗證另計） | 38 |
| empty（有 crate 骨架但 0 bytes） | 27 |
| todo（完全沒骨架） | 137 |
| **合計** | **202** |
| 還要做（empty + todo） | 164 |

`done` 只代表有程式碼。**真正結算以 `qemu-test/harness/final-tally.sh` 為準**：
unit + differential 逐位元組 + QEMU guest + fuzzer 四項全綠才算完成。

## 名字家族（同一個 crate 依 argv[0] 分派）

| 名字 | 歸到 |
|---|---|
| `[` | `test` |
| `arch` | `uname` |
| `dir` | `ls` |
| `getty` | `agetty` |
| `hd` | `hexdump` |
| `i386` | `setarch` |
| `linux32` | `setarch` |
| `linux64` | `setarch` |
| `vdir` | `ls` |
| `x86_64` | `setarch` |

## 帳單

| 狀態 | 工具 | crate | oracle | man page |
|---|---|---|---|---|
| todo | `[` | `test` | `/usr/bin/[` | no |
| todo | `addpart` | `addpart` | `/usr/bin/addpart` | no |
| todo | `agetty` | `agetty` | `/usr/sbin/agetty` | no |
| todo | `arch` | `uname` | `/usr/bin/arch` | no |
| todo | `b2sum` | `b2sum` | `/usr/bin/b2sum` | no |
| todo | `base32` | `base32` | `/usr/bin/base32` | no |
| todo | `base64` | `base64` | `/usr/bin/base64` | no |
| done | `basename` | `basename` | `/usr/bin/basename` | no |
| todo | `basenc` | `basenc` | `/usr/bin/basenc` | no |
| todo | `blkdiscard` | `blkdiscard` | `/usr/sbin/blkdiscard` | no |
| empty | `blkid` | `blkid` | `/usr/sbin/blkid` | no |
| todo | `blkzone` | `blkzone` | `/usr/sbin/blkzone` | no |
| todo | `blockdev` | `blockdev` | `/usr/sbin/blockdev` | no |
| done | `cat` | `cat` | `/usr/bin/cat` | no |
| todo | `cfdisk` | `cfdisk` | `/usr/sbin/cfdisk` | no |
| todo | `chcon` | `chcon` | `/usr/bin/chcon` | no |
| todo | `chcpu` | `chcpu` | `/usr/sbin/chcpu` | no |
| todo | `chgrp` | `chgrp` | `/usr/bin/chgrp` | no |
| todo | `chmem` | `chmem` | `/usr/sbin/chmem` | no |
| todo | `chmod` | `chmod` | `/usr/bin/chmod` | no |
| todo | `choom` | `choom` | `/usr/bin/choom` | no |
| todo | `chown` | `chown` | `/usr/bin/chown` | no |
| todo | `chroot` | `chroot` | `/usr/sbin/chroot` | no |
| todo | `chrt` | `chrt` | `/usr/bin/chrt` | no |
| empty | `cksum` | `cksum` | `/usr/bin/cksum` | yes |
| done | `col` | `col` | `/usr/bin/col` | no |
| done | `colcrt` | `colcrt` | `/usr/bin/colcrt` | no |
| done | `colrm` | `colrm` | `/usr/bin/colrm` | no |
| done | `column` | `column` | `/usr/bin/column` | no |
| done | `comm` | `comm` | `/usr/bin/comm` | yes |
| done | `cp` | `cp` | `/usr/bin/cp` | yes |
| todo | `csplit` | `csplit` | `/usr/bin/csplit` | no |
| todo | `ctrlaltdel` | `ctrlaltdel` | `/usr/sbin/ctrlaltdel` | no |
| done | `cut` | `cut` | `/usr/bin/cut` | yes |
| todo | `date` | `date` | `/usr/bin/date` | no |
| todo | `dd` | `dd` | `/usr/bin/dd` | no |
| todo | `delpart` | `delpart` | `/usr/bin/delpart` | no |
| empty | `df` | `df` | `/usr/bin/df` | yes |
| empty | `dir` | `ls` | `/usr/bin/dir` | yes |
| todo | `dircolors` | `dircolors` | `/usr/bin/dircolors` | no |
| done | `dirname` | `dirname` | `/usr/bin/dirname` | no |
| done | `dmesg` | `dmesg` | `/usr/bin/dmesg` | no |
| empty | `du` | `du` | `/usr/bin/du` | yes |
| todo | `echo` | `echo` | `/usr/bin/echo` | no |
| done | `env` | `env` | `/usr/bin/env` | yes |
| done | `expand` | `expand` | `/usr/bin/expand` | yes |
| todo | `expr` | `expr` | `/usr/bin/expr` | no |
| todo | `factor` | `factor` | `/usr/bin/factor` | no |
| done | `fallocate` | `fallocate` | `/usr/bin/fallocate` | no |
| todo | `false` | `false` | `/usr/bin/false` | no |
| todo | `fdisk` | `fdisk` | `/usr/sbin/fdisk` | no |
| todo | `findfs` | `findfs` | `/usr/sbin/findfs` | no |
| empty | `findmnt` | `findmnt` | `/usr/bin/findmnt` | no |
| todo | `flock` | `flock` | `/usr/bin/flock` | no |
| todo | `fmt` | `fmt` | `/usr/bin/fmt` | no |
| todo | `fold` | `fold` | `/usr/bin/fold` | no |
| todo | `fsck` | `fsck` | `/usr/sbin/fsck` | no |
| todo | `fsck.cramfs` | `fsck.cramfs` | `/usr/sbin/fsck.cramfs` | no |
| todo | `fsck.minix` | `fsck.minix` | `/usr/sbin/fsck.minix` | no |
| todo | `fsfreeze` | `fsfreeze` | `/usr/sbin/fsfreeze` | no |
| empty | `fstrim` | `fstrim` | `/usr/sbin/fstrim` | no |
| empty | `getopt` | `getopt` | `/usr/bin/getopt` | yes |
| todo | `getty` | `agetty` | `/usr/sbin/getty` | no |
| todo | `groups` | `groups` | `/usr/bin/groups` | no |
| todo | `hardlink` | `hardlink` | `/usr/bin/hardlink` | no |
| done | `hd` | `hexdump` | `/usr/bin/hd` | no |
| done | `head` | `head` | `/usr/bin/head` | no |
| done | `hexdump` | `hexdump` | `/usr/bin/hexdump` | no |
| todo | `hostid` | `hostid` | `/usr/bin/hostid` | no |
| empty | `i386` | `setarch` | `/usr/bin/i386` | no |
| todo | `id` | `id` | `/usr/bin/id` | no |
| todo | `install` | `install` | `/usr/bin/install` | no |
| todo | `ionice` | `ionice` | `/usr/bin/ionice` | no |
| todo | `ipcmk` | `ipcmk` | `/usr/bin/ipcmk` | no |
| todo | `ipcrm` | `ipcrm` | `/usr/bin/ipcrm` | no |
| todo | `ipcs` | `ipcs` | `/usr/bin/ipcs` | no |
| todo | `isosize` | `isosize` | `/usr/sbin/isosize` | no |
| empty | `join` | `join` | `/usr/bin/join` | yes |
| todo | `last` | `last` | `/usr/bin/last` | no |
| todo | `lastb` | `lastb` | `/usr/bin/lastb` | no |
| todo | `ldattach` | `ldattach` | `/usr/sbin/ldattach` | no |
| todo | `link` | `link` | `/usr/bin/link` | no |
| empty | `linux32` | `setarch` | `/usr/bin/linux32` | no |
| empty | `linux64` | `setarch` | `/usr/bin/linux64` | no |
| done | `ln` | `ln` | `/usr/bin/ln` | no |
| todo | `logname` | `logname` | `/usr/bin/logname` | no |
| todo | `look` | `look` | `/usr/bin/look` | no |
| todo | `losetup` | `losetup` | `/usr/sbin/losetup` | no |
| empty | `ls` | `ls` | `/usr/bin/ls` | yes |
| empty | `lsblk` | `lsblk` | `/usr/bin/lsblk` | no |
| done | `lscpu` | `lscpu` | `/usr/bin/lscpu` | no |
| todo | `lsipc` | `lsipc` | `/usr/bin/lsipc` | no |
| empty | `lslocks` | `lslocks` | `/usr/bin/lslocks` | no |
| todo | `lslogins` | `lslogins` | `/usr/bin/lslogins` | no |
| todo | `lsmem` | `lsmem` | `/usr/bin/lsmem` | no |
| todo | `lsns` | `lsns` | `/usr/bin/lsns` | no |
| empty | `mcookie` | `mcookie` | `/usr/bin/mcookie` | no |
| todo | `md5sum` | `md5sum` | `/usr/bin/md5sum` | yes |
| todo | `mesg` | `mesg` | `/usr/bin/mesg` | no |
| done | `mkdir` | `mkdir` | `/usr/bin/mkdir` | no |
| todo | `mkfifo` | `mkfifo` | `/usr/bin/mkfifo` | no |
| todo | `mkfs` | `mkfs` | `/usr/sbin/mkfs` | no |
| todo | `mkfs.bfs` | `mkfs.bfs` | `/usr/sbin/mkfs.bfs` | no |
| todo | `mkfs.cramfs` | `mkfs.cramfs` | `/usr/sbin/mkfs.cramfs` | no |
| todo | `mkfs.minix` | `mkfs.minix` | `/usr/sbin/mkfs.minix` | no |
| todo | `mknod` | `mknod` | `/usr/bin/mknod` | no |
| todo | `mkswap` | `mkswap` | `/usr/sbin/mkswap` | no |
| done | `mktemp` | `mktemp` | `/usr/bin/mktemp` | yes |
| todo | `more` | `more` | `/usr/bin/more` | no |
| todo | `mount` | `mount` | `/usr/bin/mount` | no |
| todo | `mountpoint` | `mountpoint` | `/usr/bin/mountpoint` | no |
| done | `mv` | `mv` | `/usr/bin/mv` | yes |
| todo | `namei` | `namei` | `/usr/bin/namei` | no |
| todo | `nice` | `nice` | `/usr/bin/nice` | no |
| done | `nl` | `nl` | `/usr/bin/nl` | yes |
| todo | `nohup` | `nohup` | `/usr/bin/nohup` | no |
| todo | `nproc` | `nproc` | `/usr/bin/nproc` | no |
| empty | `nsenter` | `nsenter` | `/usr/bin/nsenter` | no |
| todo | `numfmt` | `numfmt` | `/usr/bin/numfmt` | no |
| todo | `od` | `od` | `/usr/bin/od` | no |
| empty | `partx` | `partx` | `/usr/bin/partx` | no |
| done | `paste` | `paste` | `/usr/bin/paste` | yes |
| todo | `pathchk` | `pathchk` | `/usr/bin/pathchk` | no |
| todo | `pinky` | `pinky` | `/usr/bin/pinky` | no |
| todo | `pivot_root` | `pivot_root` | `/usr/sbin/pivot_root` | no |
| todo | `pr` | `pr` | `/usr/bin/pr` | no |
| todo | `printenv` | `printenv` | `/usr/bin/printenv` | no |
| todo | `printf` | `printf` | `/usr/bin/printf` | no |
| todo | `prlimit` | `prlimit` | `/usr/bin/prlimit` | no |
| todo | `ptx` | `ptx` | `/usr/bin/ptx` | no |
| todo | `pwd` | `pwd` | `/usr/bin/pwd` | no |
| done | `readlink` | `readlink` | `/usr/bin/readlink` | yes |
| todo | `readprofile` | `readprofile` | `/usr/sbin/readprofile` | no |
| todo | `realpath` | `realpath` | `/usr/bin/realpath` | no |
| todo | `rename.ul` | `rename.ul` | `/usr/bin/rename.ul` | no |
| todo | `resizepart` | `resizepart` | `/usr/bin/resizepart` | no |
| done | `rev` | `rev` | `/usr/bin/rev` | no |
| done | `rm` | `rm` | `/usr/bin/rm` | no |
| done | `rmdir` | `rmdir` | `/usr/bin/rmdir` | no |
| todo | `rtcwake` | `rtcwake` | `/usr/sbin/rtcwake` | no |
| todo | `runcon` | `runcon` | `/usr/bin/runcon` | no |
| todo | `runuser` | `runuser` | `/usr/sbin/runuser` | no |
| done | `seq` | `seq` | `/usr/bin/seq` | no |
| empty | `setarch` | `setarch` | `/usr/bin/setarch` | no |
| todo | `setpriv` | `setpriv` | `/usr/bin/setpriv` | no |
| empty | `setsid` | `setsid` | `/usr/bin/setsid` | no |
| todo | `setterm` | `setterm` | `/usr/bin/setterm` | no |
| todo | `sfdisk` | `sfdisk` | `/usr/sbin/sfdisk` | no |
| todo | `sha1sum` | `sha1sum` | `/usr/bin/sha1sum` | no |
| todo | `sha224sum` | `sha224sum` | `/usr/bin/sha224sum` | no |
| todo | `sha256sum` | `sha256sum` | `/usr/bin/sha256sum` | no |
| todo | `sha384sum` | `sha384sum` | `/usr/bin/sha384sum` | no |
| todo | `sha512sum` | `sha512sum` | `/usr/bin/sha512sum` | no |
| todo | `shred` | `shred` | `/usr/bin/shred` | no |
| todo | `shuf` | `shuf` | `/usr/bin/shuf` | no |
| todo | `sleep` | `sleep` | `/usr/bin/sleep` | no |
| done | `sort` | `sort` | `/usr/bin/sort` | yes |
| empty | `split` | `split` | `/usr/bin/split` | yes |
| empty | `stat` | `stat` | `/usr/bin/stat` | yes |
| todo | `stdbuf` | `stdbuf` | `/usr/bin/stdbuf` | no |
| todo | `stty` | `stty` | `/usr/bin/stty` | no |
| todo | `su` | `su` | `/usr/bin/su` | no |
| todo | `sulogin` | `sulogin` | `/usr/sbin/sulogin` | no |
| todo | `sum` | `sum` | `/usr/bin/sum` | no |
| todo | `swaplabel` | `swaplabel` | `/usr/sbin/swaplabel` | no |
| todo | `swapoff` | `swapoff` | `/usr/sbin/swapoff` | no |
| todo | `swapon` | `swapon` | `/usr/sbin/swapon` | no |
| todo | `switch_root` | `switch_root` | `/usr/sbin/switch_root` | no |
| todo | `sync` | `sync` | `/usr/bin/sync` | no |
| todo | `tac` | `tac` | `/usr/bin/tac` | no |
| done | `tail` | `tail` | `/usr/bin/tail` | no |
| empty | `taskset` | `taskset` | `/usr/bin/taskset` | no |
| done | `tee` | `tee` | `/usr/bin/tee` | yes |
| todo | `test` | `test` | `/usr/bin/test` | no |
| todo | `timeout` | `timeout` | `/usr/bin/timeout` | no |
| done | `touch` | `touch` | `/usr/bin/touch` | yes |
| done | `tr` | `tr` | `/usr/bin/tr` | yes |
| todo | `true` | `true` | `/usr/bin/true` | no |
| todo | `truncate` | `truncate` | `/usr/bin/truncate` | no |
| todo | `tsort` | `tsort` | `/usr/bin/tsort` | no |
| todo | `tty` | `tty` | `/usr/bin/tty` | no |
| todo | `uclampset` | `uclampset` | `/usr/bin/uclampset` | no |
| done | `ul` | `ul` | `/usr/bin/ul` | no |
| todo | `umount` | `umount` | `/usr/bin/umount` | no |
| todo | `uname` | `uname` | `/usr/bin/uname` | no |
| done | `unexpand` | `unexpand` | `/usr/bin/unexpand` | yes |
| done | `uniq` | `uniq` | `/usr/bin/uniq` | yes |
| todo | `unlink` | `unlink` | `/usr/bin/unlink` | no |
| empty | `unshare` | `unshare` | `/usr/bin/unshare` | no |
| todo | `users` | `users` | `/usr/bin/users` | no |
| todo | `utmpdump` | `utmpdump` | `/usr/bin/utmpdump` | no |
| empty | `vdir` | `ls` | `/usr/bin/vdir` | yes |
| done | `wc` | `wc` | `/usr/bin/wc` | no |
| todo | `wdctl` | `wdctl` | `/usr/bin/wdctl` | no |
| empty | `whereis` | `whereis` | `/usr/bin/whereis` | no |
| todo | `who` | `who` | `/usr/bin/who` | no |
| todo | `whoami` | `whoami` | `/usr/bin/whoami` | no |
| todo | `wipefs` | `wipefs` | `/usr/sbin/wipefs` | no |
| todo | `write` | `write` | `/usr/bin/write` | no |
| empty | `x86_64` | `setarch` | `/usr/bin/x86_64` | no |
| todo | `yes` | `yes` | `/usr/bin/yes` | no |
| todo | `zramctl` | `zramctl` | `/usr/sbin/zramctl` | no |

## 有 crate 但這台機器沒有 oracle

這些無法做 host differential，只能靠 unit 與 guest 驗證
（`quoting`/`hashes` 是共用模組，不是工具）：

- `bits`
- `hashes`
- `line`
- `quoting`
