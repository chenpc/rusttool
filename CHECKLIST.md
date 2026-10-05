# CHECKLIST — 還沒完成的工具

四層全綠才算完成：**unit + differential（逐位元組）+ QEMU guest + fuzzer**。
`-` 代表那一層**沒有覆蓋**，是缺口，不是通過。

數字來源：`qemu-test/harness/check-skeletons.sh`、`final-tally.sh`，2026-10-05。
帳單本身（202 個名字）由 `gen-manifest.sh` 產生，在 `MANIFEST.md`，不要手改；這裡只列**還沒完成**的部分。

## 現況

| 層 | 結果 |
|---|---|
| unit | 639 passed, 0 failed |
| differential | 1975 identical, **29 different** |
| QEMU guest | 61 PASS, 0 FAIL |
| fuzzer | 4 個 |

還要做：**168 個工具**（已實作但未驗證通過 19 ＋ 空骨架 20 ＋ 完全沒有 129）。

---

## 1. 有程式碼、differential 未全綠（9）

全部在 `*sum` 家族加 `hexdump`。

| 工具 | unit | differential | 缺口 |
|---|---|---|---|
| `cksum` | 0 | 57 / **16** | guest, fuzz |
| `md5sum` | 0 | 46 / **3** | guest, fuzz |
| `b2sum` | 0 | 53 / **2** | guest, fuzz |
| `sha1sum` | 0 | 48 / **1** | guest, fuzz |
| `sha224sum` | 0 | 48 / **1** | guest, fuzz |
| `sha256sum` | 0 | 48 / **1** | guest, fuzz |
| `sha384sum` | 0 | 48 / **1** | guest, fuzz |
| `sha512sum` | 0 | 48 / **1** | guest, fuzz |
| `hexdump` | 12 | 421 / **3**（`hx-diff.sh`，`-X` 已知例外） | fuzz |

已知的三個 `*sum` 缺陷：`check.rs` 的 `no file was verified` 只在該檔案有格式正確的行時才印（我們無條件印）；WARNING 統計是**逐 check file**、我們是全域一次；`check.rs` 裡有一組重複的 `Ok(bytes)` 分支，第二個永遠不會執行，所以 status/quiet 模式下 `verified` 不會增加。另外 GNU 會跳過 `#` 開頭的註解行，我們沒有。

## 2. 有程式碼、但 differential 完全沒有覆蓋（10）

只能靠 guest 驗證，等於沒有 oracle 比對。

`bits` `col` `colcrt` `colrm` `column` `dmesg` `fallocate` `lscpu` `rev` `ul`

其中 `dmesg`、`fallocate`、`lscpu` 連 unit test 都是 0。
注意：`basename` `cat` `dirname` `head` `line` `mkdir` `rm` `rmdir` `seq` `sort` `tail` `wc` 由共用的 `coreutils-diff.sh` 覆蓋，`tee` `mktemp` `readlink` 由 `small-diff.sh` 覆蓋，所以不算缺口。

## 3. 有 crate 但 0 bytes（6）

`df` `du` `join` `ls` `split` `stat`

## 4. 只有目錄、連 Cargo.toml 都沒有（14）

`blkid` `findmnt` `fstrim` `getopt` `lsblk` `lslocks` `mcookie` `nsenter` `partx` `setarch` `setsid` `taskset` `unshare` `whereis`

## 5. 完全沒有（129）

`[` `addpart` `agetty` `arch` `base32` `base64` `basenc` `blkdiscard` `blkzone` `blockdev` `cfdisk` `chcon` `chcpu` `chgrp` `chmem` `chmod` `choom` `chown` `chroot` `chrt` `csplit` `ctrlaltdel` `date` `dd` `delpart` `dircolors` `echo` `expr` `factor` `false` `fdisk` `findfs` `flock` `fmt` `fold` `fsck` `fsck.cramfs` `fsck.minix` `fsfreeze` `getty` `groups` `hardlink` `hostid` `id` `install` `ionice` `ipcmk` `ipcrm` `ipcs` `isosize` `last` `lastb` `ldattach` `link` `logname` `look` `losetup` `lsipc` `lslogins` `lsmem` `lsns` `mesg` `mkfifo` `mkfs` `mkfs.bfs` `mkfs.cramfs` `mkfs.minix` `mknod` `mkswap` `more` `mount` `mountpoint` `namei` `nice` `nohup` `nproc` `numfmt` `od` `pathchk` `pinky` `pivot_root` `pr` `printenv` `printf` `prlimit` `ptx` `pwd` `readprofile` `realpath` `rename.ul` `resizepart` `rtcwake` `runcon` `runuser` `setpriv` `setterm` `sfdisk` `shred` `shuf` `sleep` `stdbuf` `stty` `su` `sulogin` `swaplabel` `swapoff` `swapon` `switch_root` `sync` `tac` `test` `timeout` `true` `truncate` `tsort` `tty` `uclampset` `umount` `uname` `unlink` `users` `utmpdump` `wdctl` `who` `whoami` `wipefs` `write` `yes` `zramctl`

---

## 逐工具一覽（已實作的 47 個）

```
tool           unit     differential      guest   fuzz
b2sum             0 53 identical, 2 different (of 55)          -      -
basename          5                -          -      -
bits             17                -        6/6      -
cat               9                -        1/1      -
cksum             0 57 identical, 16 different (of 73)         -      -
col              32                -        6/6      -
colcrt           14                -        6/6      -
colrm            32                -        6/6      -
column           43                -        7/7      -
comm             27 79 identical, 0 different (of 79)        1/1    yes
cp               39 54 identical, 0 different (of 54)        1/1      -
cut              21 47 identical, 0 different (of 47)          -      -
dirname           4                -          -      -
dmesg             0                -          -      -
env              14 170 identical, 0 different (of 170)       1/1    yes
expand           25 77 identical, 0 different (of 77)        1/1    yes
fallocate         0                -          -      -
head              9                -        1/1      -
hexdump          12 421 identical, 3 different (of 421)      1/1      -
line              5                -        4/4      -
ln               12 27 identical, 0 different (of 27)          -      -
lscpu             0                -          -      -
md5sum            0 46 identical, 3 different (of 49)         -      -
mkdir             3                -        1/1      -
mktemp           20 31 identical, 0 different (of 31)         -      -
mv               18 38 identical, 0 different (of 38)        1/1      -
nl               25 76 identical, 0 different (of 76)        1/1      -
paste            23 96 identical, 0 different (of 96)        1/1      -
readlink          7 30 identical, 0 different (of 30)         -      -
rev              21                -        5/5      -
rm                4                -        1/1      -
rmdir             3                -          -      -
seq               9                -          -      -
sha1sum           0 48 identical, 1 different (of 49)         -      -
sha224sum         0 48 identical, 1 different (of 49)         -      -
sha256sum         0 48 identical, 1 different (of 49)         -      -
sha384sum         0 48 identical, 1 different (of 49)         -      -
sha512sum         0 48 identical, 1 different (of 49)         -      -
sort             31 51 identical, 0 different (of 51)          -      -
sum               0 17 identical, 0 different (of 17)          -      -
tail              5                -          -      -
tee              10 22 identical, 0 different (of 22)          -      -
touch            20 34 identical, 0 different (of 34)         -      -
tr               26 48 identical, 0 different (of 48)          -      -
ul               10                -        4/4      -
unexpand          8 52 identical, 0 different (of 52)        1/1    yes
uniq             19 46 identical, 0 different (of 46)         -      -
wc                8                -        1/1      -
```

四層全綠的只有 **`comm`、`env`、`expand`、`unexpand`** 四個。
