# 剩餘待實作清單

> 由 `HANDOVER.md` 第 2.3 節搬出來當成一份可以逐項打勾的工作清單。
> 每個工具都要過三層驗證才算完成（unit / differential vs `/usr/bin/<tool>` / QEMU guest）。
> 最後更新：2026-10-04

## 現況

- **已完成 39 個**：`basename` `bits` `cat` `col` `colcrt` `colrm` `column` `comm` `cp` `cut`
  `dirname` `dmesg` `env` `expand` `fallocate` `head` `hexdump` `line` `ln` `lscpu` `mkdir` `mktemp`
  `mv` `nl` `paste` `readlink` `rev` `rm` `rmdir` `seq` `sort` `tail` `tee` `touch` `tr` `ul`
  `unexpand` `uniq` `wc`
- **workspace 單元測試**：594 passed, 0 failed
- **differential**：1562 identical, 3 different（3 個是已知的 `hexdump -X`）
- **QEMU suite**：61 PASS, 0 FAIL

> `fallocate` / `dmesg` / `lscpu` 有實作但沒進 differential 清單，
> 見 `qemu-test/tmp/final-tally.sh`。

## 待實作（24 個）

`blkid` `cksum` `df` `du` `stat` `findmnt` `fstrim` `getopt` `join` `ls` `lsblk`
`lslocks` `mcookie` `nsenter` `partx` `setarch` `setsid` `split` `taskset` `unshare`
`whereis` `md5sum`

所有 27 個的 oracle（`/usr/bin/<tool>` 或 `/sbin/<tool>`）在這台機器上都有，
所以每個都能做 differential，不需要靠猜。

## 優先順序

### 第 1 梯隊：純邏輯，無 syscall（先做這批，最省力）

| # | 工具 | 難度 | 為什麼先做 | 備註 |
|---|---|---|---|---|
| 1 | `getopt` | **中** | 純 argv 解析 + shell quoting 輔助函式 | 需要 render man page。**別低估**：要重做 getopt_long 的 permutation、長選項縮寫比對、optional/required argument、`POSIXLY_CORRECT`/`GETOPT_COMPATIBLE`，再加 sh/bash/csh/tcsh 四種 quoting |
| 2 | ~~`expand`~~ | 小 | 純文字轉換，51 行 man page | **已完成**：25 unit / 77 diff / fuzz 8500 輪 |
| 3 | ~~`unexpand`~~ | 小 | 同上，54 行 | **已完成**：8 unit / 52 diff / fuzz 5500 輪（骨架已建） |
| 4 | `whereis` | 小 | 路徑清單 + 檔名搜尋 | 需要 render man page |
| 5 | `cksum` | 小 | CRC32 + 位元組數 | |
| 6 | `setsid` | 小 | fork + setsid | 需要 render man page |
| 7 | `mcookie` | 小 | 只要亂數 | 需要 render man page |

### 第 2 梯隊：文字處理 / 表格運算

| # | 工具 | 難度 | 備註 |
|---|---|---|---|
| 8 | `md5sum` | 中 | **需要 MD5 實作**；還要建骨架；有 `-c` 檢查模式 |
| 9 | `split` | 中 | 行/位元組切檔、數字範圍、suffix |
| 10 | `stat` | 中 | **補 guest-tools 缺口**；`-i`/`-u`/`-S`/`-C`/`--` |
| 11 | `join` | 中 | `comm` 的近親，做完可抽共用模組 |

### 第 3 梯隊：檔案系統查詢（要 syscall / 遞迴）

| # | 工具 | 難度 | 需要的東西 | 備註 |
|---|---|---|---|---|
| 12 | `df` | 中 | `statvfs` | **補 guest-tools 缺口** |
| 13 | `du` | 中高 | `statvfs` + `readdir` 遞迴 | **補 guest-tools 缺口** |
| 14 | `stat` | 高 | `stat`/`lstat` + 一堆輸出格式 | **補 guest-tools 缺口**，170 行 man page |

### 第 4 梯隊：syscall 包裝（util-linux）

| # | 工具 | 難度 | 需要的東西 |
|---|---|---|---|
| 15 | `taskset` | 中 | `sched_getaffinity`/`sched_setaffinity` |
| 16 | `setarch` | 中 | 架構相關（x86_64 的 `setarch` 幾乎是 no-op） |
| 17 | `nsenter` | 中高 | namespaces + `/proc/<pid>/ns/*` |
| 18 | `unshare` | 中高 | namespaces |
| 19 | `partx` | 中高 | `/sys/class/block` + `ioctl(BLKRRPART)` |

### 第 5 梯隊：sysfs / procfs 解析（util-linux）

| # | 工具 | 難度 | 資料來源 |
|---|---|---|---|
| 20 | `findmnt` | 中 | `/proc/self/mountinfo` |
| 21 | `lslocks` | 中 | `/proc/locks` |
| 22 | `lsblk` | 中高 | `/sys/class/block` |
| 23 | `blkid` | 高 | 需要真的磁碟；oracle 在 `/sbin/blkid` |
| 24 | `fstrim` | 中 | `ioctl(FIDEDRANGE)`；oracle 在 `/sbin/fstrim` |

### 最後：最難的兩個

| # | 工具 | 難度 | 備註 |
|---|---|---|---|
| 25 | `ls` | 很高 | 選項多、排序多、輸出格式複雜；**補 guest-tools 缺口**，226 行 man page |
| 26 | `stat` | 高 | 見上，格式最多 |

## 每個工具的收尾清單

照 `HANDOVER.md` 第 4 節的流程，**外加 fuzzer**（見 2.7，`comm` 的兩個真 bug 都是 fuzzer 抓的）：

- [ ] `man --nh --nj <tool> > qemu-test/tmp/man/<tool>.txt`（還沒 render 的先補）
- [ ] `tools/<tool>/src/lib.rs`：純邏輯 + 單元測試
- [ ] `tools/<tool>/src/main.rs`：CLI 與 I/O
- [ ] 把 crate 加進 `Cargo.toml` 的 `members`
- [ ] `cargo test -p <tool>`
- [ ] `qemu-test/tmp/<tool>-diff.sh`（先驗證有真的跑到我們的 binary）
- [ ] `qemu-test/tmp/<tool>-fuzz.py`（隨機輸入 × 隨機選項 × 隨機 operand 形狀）
- [ ] 加進 `qemu-test/guest-tools.txt`
- [ ] `qemu-test/cases-shell/<tool>.misc.sh`，**期望值用 `/usr/bin/<tool>` 生成**
- [ ] 驗 case 兩邊都跑：`env PATH=/usr/bin:/bin` 與 `env PATH=target/debug:/usr/bin:/bin`
- [ ] 加進 `final-tally.sh` 的 differential 清單
- [ ] `bash final-tally.sh` 全綠

## 已修的 bug

- `sort` / `uniq`：**完全空的輸入會被當成一個空行**（`-z` 下則是多吐一個 NUL）。
  `buffer.split(...)` 對空 buffer 會回傳一個空 slice，`had_terminator` 為 false，
  於是 `count = 1`。GNU 的兩個工具在空輸入上都不輸出任何東西。
  修法是 `if buffer.is_empty() { continue }`，並註明「只有一個結尾符的輸入
  是另一回事：那是一筆記錄，而且這筆記錄是空的」。
  這個 bug 讓 `difflib.sh` 的 `snapshot()` 在空 fixture 目錄上多跑一次 `cat ""`，
  於是每個 `*-diff.sh` 都噴 `cat: : No such file or directory`。
  （2026-10-04，做 `env` 時發現。）
- `colrm`：operand 錯誤多印了一行 `Try 'colrm --help'`。
  upstream 對「不是數字的 operand」用裸的 `error()`，對「不存在的選項」用
  `usage()`，所以只有後面那一類有 Try 行。已拆成 `Rejected::Operand` /
  `Rejected::Usage` 兩類，並加上單元測試釘住這個差別。
- `seq`：選項掃描原本會排列（把 operand 之後的選項撈回前面），
  而且 6 類錯誤訊息都不對。詳見 `HANDOVER.md` 第 11 節。

## 已知差異（有紀錄、無法在 stable Rust 消除）

- `seq -f '%a'`：GNU 用 `long double` 印 `%La`，得到 `0x8p-3`；
  Rust 沒有 `long double`，只能用 `double`，得到 `0x1p+0`。
  所以 `%La`／`%A` 不同（`%f` `%e` `%g` 都一樣）。
- `quoteaf`：與系統工具在「同時有撇號和一串兩個以上控制字元」時差一個單引號
  （237 個探測案例裡 236 個相同）。細節與驗證方式見 `HANDOVER.md` 第 10 節。

## 已經知道的 libc 差異（做特權／訊號工具前先讀）

`env` 收尾時踩到的，寫在 `HANDOVER.md` 第 10.8 節。之後做
`agetty` `login` `su` `sulogin` `uuidd` 或任何碰訊號的工具都會再遇到：

- **musl 的 `nl_langinfo(CODESET)` 永遠是 UTF-8**，沒有 locale 檔。
  任何「輸出形狀取決於 locale」的工具，在 guest 裡的行為跟本機 glibc 不同。
- **musl 沒有訊號 34 的位元，`SIGRTMIN()` 回 35**，`sigaddset(34)` 回 EINVAL。
  「封鎖所有訊號」之類的 case 在 guest 裡會少一行。
- **musl 的 `putenv`/`setenv` 拒絕空名稱**，glibc 允許。
- **`environ` 陣列由 null element 結尾**，不是 null pointer。
