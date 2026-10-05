# rusttool 交接任務文件

> 本文件給下一位接手的人（或下一個 session 的自己）。
> 環境建置請看 [`qemu-test/ENV-SETUP.md`](qemu-test/ENV-SETUP.md)，本文只講**專案狀態與怎麼做事**。
>
> 最後更新：2026-10-05（本輪立下第 12 節的執行規定，並修掉 `head` 的無界緩衝）

---

## 1. 專案目標

> **最新狀態：`nl`、`paste`、`comm`、`expand`、`unexpand` 都已完成三層驗證。**
> 現況 **568 unit / 1336 differential（3 個已知例外）/ 60 QEMU PASS**，全部 0 failed。
> 完整待辦清單在 [`TODO.md`](TODO.md)，收尾紀錄見 2.4 ~ 2.8。

用 Rust 重新實作 GNU coreutils 與 util-linux 的工具，**每個工具都要通過三層驗證**才算完成：

| 層級 | 手段 | 通過標準 |
|---|---|---|
| 單元測試 | `cargo test -p <tool>` | 純邏輯與邊界條件有覆蓋 |
| **differential** | 同一組輸入餵給 ours 與 `/usr/bin/<tool>` | 輸出、stderr、exit code **逐位元組相同** |
| **QEMU 整合測試** | 全新開機的 Alpine guest，工具掛在 9pfs 上 | case 全數 PASS |

**differential 的 oracle 是 `/usr/bin/<tool>`（Ubuntu 24.04, coreutils 9.4）。**
這是本專案的核心紀律：不要憑 man page 猜行為，先問真的 binary。

---

## 2. 目前狀態

### 2.1 已完成並全數驗證（第二批，11 個工具）

`cp` `mv` `touch` `tee` `mktemp` `readlink` `cut` `tr` `sort` `uniq` `ln`

| 驗證 | 結果（那一輪的數字，見 2.4 / 2.5 的現況） |
|---|---|
| 單元測試（整個 workspace） | 484 passed, 0 failed |
| differential vs GNU | 1562 identical, 3 different |
| QEMU guest 全新開機 | 61 PASS, 0 FAIL |

那 3 個差異是**系統 util-linux 2.40 沒有 `hexdump -X`**，屬已知項，非缺陷。

更早的 util-linux 工具（`rev` `column` `col` `colrm` `bits` `colcrt` `line` `ul` `hexdump`）
與第一批 coreutils（`cat` `rm` `mkdir` `rmdir` `head` `tail` `wc` `seq` `basename` `dirname`）
也已完成，但沒有納入上面那三個數字的統計範圍（見 `qemu-test/harness/final-tally.sh` 的清單）。

**還欠多少個工具不是手數的**：`qemu-test/harness/gen-manifest.sh` 會從 oracle 所在
package 的檔案清單交叉比對 `tools/`，產生 `MANIFEST.md`。目前 202 個名字：
38 done / 27 空殼 / 137 完全沒骨架，**還要做 164 個**。

### 2.2 ~~進行中：`nl`~~ → 已完成，見 2.4

### 2.3 已建好但**還是空的** crate 骨架

**這些有 `Cargo.toml`，但 `src/lib.rs` 與 `src/main.rs` 都是 0 bytes，完全沒實作。**

本專案為了第三批而新建的 11 個（man page 都已備妥）：

`paste` `expand` `split` `comm` `join` `cksum` `env` `stat` `ls` `du` `df`

更早就存在、屬 util-linux 但一樣是空殼的 17 個（**沒有為它們 render man page**）：

`blkid` `dmesg` `fallocate` `findmnt` `fstrim` `getopt` `lsblk` `lscpu` `lslocks`
`mcookie` `nsenter` `partx` `setarch` `setsid` `taskset` `unshare` `whereis`

> 快速確認哪些還是空的：
> ```bash
> cd /home/chenpc/git/rusttool/tools && for t in */; do [ ! -s "$t/src/lib.rs" ] && echo "EMPTY: ${t%/}"; done
> ```
>
> 這 28 個空的都在 `Cargo.toml` 的 workspace members 裡（做完就會加進去），
> 所以 `cargo build --workspace` 會去碰它們，**535 這個單元測試數字不包含空殼**。

man page 已備妥（`qemu-test/harness/man/*.txt`）：

- 有骨架的 11 個：全都有（`paste`、`comm` 已做完，剩 9 個空殼）
- **只有 man page、沒建骨架**：`unexpand` `md5sum`

> `qemu-test/guest-tools.txt` 裡列了 `env` `stat` `du` `df` `ls`，
> 但它們還沒實作。`pack-initramfs.sh` 現在會偵測 `src/main.rs` 與 `src/lib.rs`
> 是否都為空，是就跳過並留一行 `note: tools/<name> is still a skeleton, skipping`。
> 這 5 個是 guest share 完整性的缺口。
> （原本 `pack-initramfs.sh` 只檢查 `Cargo.toml` 存不存在，於是這些空殼 crate
> 會讓 `cargo build` 報 "current package believes it's in a workspace" 而整個中止。）

### 2.4 `nl` 收尾結果（已完成）

| 驗證 | 結果 |
|---|---|
| 單元測試 | **25 passed, 0 failed**（`nl` crate） |
| differential vs GNU | **76 identical, 0 different** |
| QEMU guest 全新開機 | `nl.misc` **PASS** |

這一輪修掉四件事，其中兩個是**真的行為缺陷**（原本的 case 抓不到）：

1. **`-d` 給單一字元時錯了**。GNU 的規則是「缺少第二個字元就補 `:`」，不是重複成兩份。
   `-da` 的 footer 是 `a:`，我們原本算成 `aa`。抓到的方法是讀 `nl.c`：

   ```c
   case 'd':
     len = strlen (optarg);
     if (len == 1 || len == 2)  /* POSIX.  */
       { char *p = section_del; while (*optarg) *p++ = *optarg++; }
     else
       section_del = optarg;  /* GNU extension.  */
   ```

   長度 1 或 2 是**就地覆寫**預設的 `"\\:"` 陣列，所以留下來的第二個字元是 `:`。
   三個字元以上則整段當作 `optarg`。修正後 `-da` / `-d:` / `-d\` 都對了。

2. **兩種「缺參數」訊息要分開**。getopt 對短選項印
   `nl: option requires an argument -- 'w'`，對長選項才印
   `nl: option '--number-width' requires an argument`。
   現在是 `requires_argument_message(letter)` 與 `long_requires_argument_message(name)` 兩個函式。

3. **壞 regex 不印 Try line**。coreutils 是 regex 編譯器自己呼叫 `error()` 死掉，
   永遠走不到 usage hint。順手把原本用 `message.starts_with("nl: Invalid")` 字串比對來
   區分兩種失敗的方式，改成 `enum StyleError { Style(String), Regex(String) }`。

4. **`final-tally.sh` 漏了 `nl-diff.sh`**，而且 QEMU 段是讀 `/tmp/qemu-final.log` 的舊結果。
   已加入 `nl-diff.sh`，QEMU 段改成實際 `pack-initramfs.sh` + `run-many.sh` 重跑。
   現在 `final-tally.sh` 與 `all-diffs.sh` 都改成**自動發現 `*-diff.sh`**，
   不再維護硬寫清單（舊清單就是漏數字的來源）；被取代的 harness 寫在腳本裡的
   `SKIP=` 而不是默默消失。

5. **整個驗證設施不在版本控制裡**。`qemu-test/.gitignore` 忽略 `tmp/`，
   而 20 個 `*-diff.sh`、4 個 `*-fuzz.py`、`difflib.sh`、`final-tally.sh`、
   render 好的 man page 全住在 `tmp/`。也就是 fresh clone 會拿到所有工具程式碼、
   卻拿不到任何一個驗證它們的東西，「每工具一 commit、可 bisect」根本做不到。
   已把這些搬到 `qemu-test/harness/`（入版本控制），`tmp/` 留給一次性
   probe / fix / tidy 腳本。搬完 differential 基準不變：1562 identical / 3 different。

順手修的兩個 harness／文件問題：

- `pack-initramfs.sh` 只用 `Cargo.toml` 存不存在來判斷工具能不能 build，
  空殼 crate 會讓整個 pack 中止（見 2.3 的說明）。
- HANDOVER 第 4 步寫 `cd qemu-test/tmp && bash pack-initramfs.sh`，
  但 `pack-initramfs.sh` 在 `qemu-test/` 不在 `tmp/`。已改正。

### 2.5 `paste`（已完成）

| 驗證 | 結果 |
|---|---|
| 單元測試 | **23 passed, 0 failed** |
| differential vs GNU | **96 identical, 0 different** |
| QEMU guest 全新開機 | `paste.misc` **PASS** |

讀 `paste.c` 才知道的四件事：

1. **`-d` 清單每行重新從頭算**，而且是「每個檔案（最後一個除外）用一個」。
   所以 `-d ab` 三個檔案每一行都是 `a b`，不是 `a b` / `b a` 輪流。
   serial 模式則是**每個檔案**重新從頭算。

2. **`-d` 吃反斜線跳脫**，`\0` 表示「這個位置不加分隔符」。
   而 `-d ''` 在跳脫折疊**之前**就被換成 `"\0"`，所以空值是「沒有分隔符」而不是錯誤。

3. **serial 模式遇到開不了的檔只是警告並繼續**，而且**不補那一行**；
   但**開得起來卻是空的檔會補一行** newline。parallel 模式遇到開不了的檔直接 `exit(EXIT_FAILURE)`，
   已經讀到的東西也不輸出。

4. **每個 `-` 都是同一個 `FILE *`**。所以 `paste - -` 是一個 stream 被兩欄輪流著讀，
   不是第一欄吃掉全部。實作上用 `Rc<RefCell<Cursor>>` 讓多個 `-` 共用游標。

`paste` 有**兩個不同的 quoting 函式**，很容易搞混（`paste.c` 裡兩者都用得到）：

| 函式 | 用在哪 | 什麼時候加引號 | 樣子 |
|---|---|---|---|
| `quotef` | 檔名（cannot open） | 有任何 byte 不是「plain」時 | `'no pe'`、`"no'pe"`、`'a'$'\t''b'` |
| `quotearg_n_style_colon` | `-d` 跳脫失敗的訊息 | 有 `"`、`:` 或不可列印 byte 時 | `"a:b\\"` |

兩個都不會因為**空格**就加引號 —— 這點我第一版寫錯，用
`qemu-test/tmp/paste-quote-probe.sh` 與 `paste-quotef-map.py` 逐一問過 binary 才修正。

`quotef` 的 plain 集合（字母數字之外）是 `# % + , - . / @ ] _ { } ~`；
`'` 不在裡面（所以會觸發雙引號），`;` `\` `$` `<` `>` `=` `|` `!` 也不在。
不可列印的 byte 會變成 `$'\001'` 這種 chunk，而且**位在開頭時前面要多一組 `''`**。

> 順帶查清楚了 coreutils 的 quoting 家族，`nl` 與 `cut` 用的是不同的：
> `quote(v)`（選項**值**）永遠用 typographic `‘v’`，
> `quotef(f)`（**檔名**）是條件式 ASCII quoting。別以為是同一個函式。

### 2.6 `comm`（已完成）

| 驗證 | 結果 |
|---|---|
| 單元測試 | **27 passed, 0 failed** |
| differential vs GNU | **79 identical, 0 different** |
| **隨機 fuzz** vs GNU | **6300 輪，0 mismatches** |
| QEMU guest 全新開機 | `comm.misc` **PASS** |

`comm` 的程式碼幾乎都在「順序檢查」上，merge 本身很平凡。
讀 `comm.c` + fuzz 才搞清楚的五件事：

1. **預設只在「有行配不上對」之後才檢查順序**。
   所以一個所有行都剛好配上對的檔案，就算順序亂掉也不會被抱怨。
   `--check-order` 從第一對就查，`--nocheck-order` 完全不查。

2. **檔案讀到結尾時會把倒數第二對再檢查一次**。這不是冗餘：
   讀取時那次檢查可能因為「還沒有配不上對的行」被跳過，
   而結尾那次是在 `seen_unpairable` 已經是 true 之後才跑。
   抓得到的例子：`comm` 比較 `c b` 與 `c` 會報 file 1 亂序，
   因為 `b` 是在 `c` 配對成功之後才被讀到的。

3. **兩個檔案都亂序時，訊息順序是「發現順序」不是檔案編號順序**。
   常常是 file 2 先。這個是 fuzzer 抓到的，手寫 case 完全沒想到。

4. **`comm - -` 也是共用同一個 stream**（跟 `paste` 一樣），而且
   comm.c 是 **i = 0 先、i = 1 後**推進，所以共用 stream 時先推進的是第一欄。
   順序寫反的話 fuzz 立刻抓到。

5. **`comm - -` 最後會因為「關閉標準輸入兩次」而失敗**：
   `comm: -: Bad file descriptor`，exit 1。而且這個 `error()` 發生在
   `--total` 的摘要**之前**，所以 `comm --total - -` 不會有 total 那行。
   若同時又是 `--check-order` fatal，那更是提早離開，連這句都不會印。

其他細節：

- 比較用 **`memcmp2`** 不是 `strcmp`：先比共同長度，相同再比長度。
  所以 `"b"` 排在 `"a b"` **前面**，空行排在所有東西前面。
  這一點讓 `a\n\nb\n` 被判定成亂序（`a` 後面跟著更小的空行）。
- 空的 `--output-delimiter=` 會寫出**一個 NUL byte**，不是什麼都不寫
  （`col_sep_len` 被強制成 1，寫的是空字串自己的 terminator）。
- 第三欄會寫**兩個**分隔符（前面兩欄都還在印的話）。
  手寫期望值時我連錯四次，差點以為實作壞了。
- getopt_long 會 permutation，所以選項可以出現在 operand 之後。

### 2.7 Fuzzing：比手寫 case 強得多

`comm` 的手寫 differential 有 79 個 case 全綠，但 fuzz 立刻抓到兩個真 bug
（訊息順序、共用 stream 的推進順序）。
`qemu-test/harness/comm-fuzz.py` 是範本：

- 隨機產生兩個檔案（行數 0~5、含空行、含前綴、含無結尾的行、20% 用 NUL 結尾）
- 隨機疊加選項（`-1`/`-2`/`-3`/`--total`/`--check-order`/`--nocheck-order`/
  `--output-delimiter=`/`-z`…）
- 隨機選 operand 形狀（兩個具名／其中一個 `-`／兩個都是 `-`）
- 三個通道逐位元組比：stdout、stderr、exit status

用法（第二個參數是種子，改種子等於換一批輸入）：

```bash
cargo build -p comm
python3 qemu-test/harness/comm-fuzz.py 1200 7
```

**建議之後每個工具都配一個 fuzzer。** 手寫 case 只能覆蓋你想到的輸入，
而 GNU 的行為裡最容易被漏掉的恰恰是那些「順序／時序」類的細節。

### 2.8 `expand` 與 `unexpand`（已完成）

| 工具 | unit | differential | fuzz | QEMU |
|---|---|---|---|---|
| `expand` | 25 | 77/77 | 8500 輪 0 mismatch | `expand.misc` PASS |
| `unexpand` | 8 | 52/52 | 5500 輪 0 mismatch | `unexpand.misc` PASS |

兩個工具共用 `expand-common.c` 的 tab-stop 文法（`-t N` / `-t LIST` / `/N` / `+N`），
所以 `lib.rs` 裡各有一份**相同的** `TabStops`。
專案慣例是每個 crate 自足（只依賴 libc），所以先複製一份，
但這兩份遲早要合併成共用模組 —— 已在兩邊的註解裡標出。

### 2.8.1 三個真 bug（fuzzer 抓的，兩個跨工具）

1. **operand 是一個 stream，不是一個一個**。`expand.c` 與 `unexpand.c` 的
   `next_file()` 是在**行迴圈裡**呼叫的，所以沒有結尾換行的檔案會把
   **column / tab-stop 游標 / convert 旗標**交給下一個檔案。
   最小的重現：

   ```sh
   printf '0\010zZ Z\0100'  > f1   # 8 bytes, ends with a backspace, no newline
   printf -- '-zZ0ba\t\010\n' > f2
   expand f1 f2   # GNU: ...-zZ0ba␣␣␣␣␣␣␃
   ```

   f1 結束時 column=4，f2 的第一個 tab 從 10 跑到 16（6 個空格）。
   修法很簡單：`main.rs` 把所有 operand **串接**起來再展開一次，
   因為串接後的換行重設跟「一行一行處理」等價。

2. **`tab_index` 在回傳停駐點時不該前進**。C 的
   `get_next_tab_column()` 只在 `column >= tab_list[i]` 時才 `tab_index++`，
   回傳一個「還在前面」的停駐點時游標是**不動**的。
   我一開始寫成「先 `++` 再比較」，`expand` 和 `unexpand` 都錯。
   症狀是 backspace 之後游標回不去本來該在的位置。
   > 這個 bug 是寫 `unexpand` 的單元測試時**順手發現 `expand` 也有同樣的錯**的，
   > 兩邊一起修，然後 `expand` 的 fuzz 又跑 5 個種子確認沒退化。

3. **`pending_blank[0] = '\t'` 在 C 是預先配置的緩衝區**，槽位永遠存在；
   Rust 的空 `Vec` 不能寫 `[0]`，會 panic。
   fuzzer 用 `-t 1`（停駐點間隔 1）觸發出來。修法是加一個 `poke()` helper。

### 2.8.2 其他讀原始碼才知道的

- `/` 和 `+` 要寫在數字**前面**：`-t 2/3` 是錯的，`-t 2,/3` 和 `-t /3` 才對。
- **specifier 放錯位置時 GNU 會繼續掃描**，所以 `-t 4+x` 會印**兩行**
  （ misplaced `+` 然後 invalid `x`）。`parse()` 回傳 `Vec<StopError>` 而不是 `Result`。
- 空的 `--output-delimiter=`／`-d ""` 會寫出**一個 NUL byte**，不是什麼都不寫。
- `unexpand` 的短選項字串是 `",0123456789at:"` —— **逗號和數字都是選項**，
  所以 `-2` 是兩欄間隔、`-2,6` 是清單。`expand` 則是 `"it:0::1::...9::"`，
  數字是帶 optional argument 的選項。
- `unexpand` 的 `one_blank_before_tab_stop`：單獨落在停駐點上的一個空格**不會**變成 tab，
  而 `printf 'a        b' | unexpand -a` 得到 `a<TAB>␣b`（不是 `a<TAB>`）就是這個旗標的效果。
- 換行不是 blank，所以它會走「非 blank」那條路徑，**順便把 pending blanks 沖出來**。
  我第一版在迴圈開頭就特判換行，結果 `printf 'a \n' | unexpand -a` 少了一個空格。
- 輸入結束會被當成「最後一個非 blank 字元」，所以檔案結尾的一串空白**還是會輸出**。

---

## 3. 目錄結構

```
/home/chenpc/git/rusttool/
├── Cargo.toml              # workspace，resolver=2，release profile（strip/panic=abort）
├── tools/<name>/           # 每個工具一個 crate
│   ├── Cargo.toml          # libc.workspace = true
│   └── src/
│       ├── lib.rs          # 純邏輯 + #[cfg(test)] 單元測試（不碰 I/O）
│       └── main.rs         # CLI 解析、檔案 I/O、stderr 訊息
└── qemu-test/
    ├── ENV-SETUP.md        # 環境建置手冊（先讀這份）
    ├── guest-tools.txt     # 明列要發佈到 guest 的工具
    ├── pack-initramfs.sh   # 合併工具清單 → cargo build → 裝進 guest-root/
    ├── run-one.sh / run-many.sh / run-shell.sh
    ├── cases-shell/*.sh    # guest 內的整合測試 case
    ├── logs/               # <case-id>.log（serial）、<case-id>.runner.log
    ├── harness/            # 入版本控制的驗證設施（原在 tmp/，2026-10-05 搬過來）
    │   ├── difflib.sh      # 共用 differential harness
    │   ├── *-diff.sh       # 每個工具一份
    │   ├── *-fuzz.py       # 每個工具一份（見 2.7）
    │   ├── final-tally.sh  # 一次跑完 unit + 全部 differential + QEMU
    │   ├── all-diffs.sh    # 只跑 differential（會把我們的 binary 放进 PATH）
    │   ├── gen-manifest.sh # 產生 MANIFEST.md（帳單，不手寫）
    │   └── man/*.txt       # man --nh --nj 的輸出（實作時的規格來源）
    └── tmp/                # 一次性 probe / fix / tidy 腳本（gitignore）
```

---

## 4. 開發一個工具的標準流程

### 步驟 1：讀 man page

```bash
man --nh --nj paste > qemu-test/harness/man/paste.txt
```

**看不懂、行為可疑時，直接問真的 binary，不要猜：**

```bash
/usr/bin/paste -- <fixture> ; echo "exit=$?"
/usr/bin/paste --help
```

### 步驟 2：`lib.rs` 寫純邏輯

- 所有 `Problem` 變體、數字/清單解析、選項預設值、輸出組裝都放這裡
- 每個函式配 `#[cfg(test)]` 單元測試，測試名用英文敘述句
- **不要**註解廢話；文件註解只寫「為什麼這樣」和 man page 沒說的事

### 步驟 3：`main.rs` 寫 CLI 與 I/O

沿用既有慣例（可直接抄 `tools/cut/src/main.rs` 或 `tools/ln/src/main.rs`）：

- 長選項 `--name=value` 與短選項 `-nVALUE` 兩種形狀都要支援
- 選項缺值用 `@@REQUIRES@@<letter>` 之類的 sentinel 帶回診斷層統一處理
- 診斷訊息全部放 `lib.rs` 的 `*_message()` 函式，`main.rs` 只負責印
- coreutils 的慣例：**unrecognized/invalid option 印完再印 Try line**；單純的數值錯誤**不印** Try line

### 步驟 4：寫 differential harness

複製 `qemu-test/harness/nl-diff.sh` 改名字，**先確認 `difflib.sh` 已經把 debug binaries 放進 PATH**（見第 5 節陷阱）。

fixture 用 `$(printf '%s\n' ...)`，不要用 `printf` 的 `\n\` 手動拼（踩過坑，見第 5 節）。

### 步驟 5：跑測試

```bash
cd /home/chenpc/git/rusttool
cargo test -p <tool>                          # 單元測試
cd qemu-test/harness && bash <tool>-diff.sh       # differential
```

`VERBOSE=1` 會印出差異細節。**harness 報 0/0（全部失敗）或全綠，先懷疑 harness。**

### 步驟 6：加進 guest 整合測試

1. 把工具加進 `qemu-test/guest-tools.txt`
2. 在 `qemu-test/cases-shell/` 加 case，**期望值用 `/usr/bin/<tool>` 的實際輸出生成**，不要手寫
3. 跑完整 suite：

```bash
cd /home/chenpc/git/rusttool/qemu-test/tmp
bash pack-initramfs.sh     # build → guest-root/bin/
bash run-cu-cases.sh       # 逐 case fresh boot
```

失敗要看 log：

```bash
bash fail-detail.sh <case-id>
```

### 步驟 7：收尾

```bash
cd /home/chenpc/git/rusttool/qemu-test/harness && bash final-tally.sh
```

---

## 5. 踩過的坑（重要）

### 坑 1：PowerShell 會毀掉 WSL 的引號

從 Windows 側呼叫 WSL 時，巢狀引號、heredoc、`|`、多層 `$'...'` 幾乎一定被吃掉。

**解法：任何複雜指令都寫成腳本檔再執行。**

```bash
# 差：heredoc 直接被 PowerShell 解析後報 SyntaxError
wsl.exe -- bash -lc 'cd repo && python3 - <<PY
...
PY'

# 好：寫檔後執行
wsl.exe -- python3 /home/chenpc/git/rusttool/qemu-test/tmp/fix.py
```

`qemu-test/tmp/` 底下的 `*-probe.sh`、`*-msg.sh`、`*-tidy*.py` 都是這樣來的。

### 坑 2：differential 假通過（最嚴重）

第二批十個工具的 harness 最初**沒有把 `target/debug` 放進 PATH**，
於是「ours」其實執行的是系統工具 —— 等於自己跟自己比，全部假通過。
修好 PATH 後立刻浮出 **84 個真實差異**。

**每份新 harness 都要先驗證它真的跑到了我們的 binary。**

### 坑 3：手寫的期望值不可信

guest case 的期望值我手寫錯了 5 次（`cut` 漏行、`touch` 對目錄本來就該成功、
`tee` 前一步已 truncate、`uniq -f1` 的兩行其實相等…）。
最後改成**用 `/usr/bin/<tool>` 的實際輸出生成**，才一次通過。

`nl` 這一輪又寫錯 4 次，其中一個特別值得記：
**沒有編號的行不是「編號欄留白」，而是「寬度 + 分隔符長度」個空格**。
所以期望值寫成 `     5\taa` 是錯的，應該是 `       aa`（7 個空格）。

驗證 case 的正確做法是**兩邊都跑一次**：

```bash
env PATH=/usr/bin:/bin                bash cases-shell/nl.misc.sh   # 期望值必須等於 GNU
env PATH=target/debug:/usr/bin:/bin   bash cases-shell/nl.misc.sh   # 我們的也要過
```

第一道用系統 `nl` 驗證「期望值本身對不對」，第二道驗證「實作對不對」。
只跑第二道的話，期望值寫錯會表現成實作有 bug，方向就搞反了。

另外含 tab 的期望值一律用 `$(printf '...\t...')` 展開，不要在引號字串裡手打 tab ——
看不見的空白是最容易抄錯的東西。

### 坑 4：先讀原始碼，不要靠實測反推

`nl` 的實測結果一度讓我以為 `-h`/`-f` 兩個選項根本沒作用。
實際是**我的 fixture 沒送出正確的分隔符**（`-d` 是前綴，`\:` 是 footer、`\:\:` 是 body、
`\:\:\:` 是 header，我送成 `:\:`）。後來直接抓 GNU 原始碼確認：

```bash
# 從 GitHub 拿某一版的原始碼，比反推快得多
curl -sL https://raw.githubusercontent.com/coreutils/coreutils/v9.4/src/<tool>.c
```

原始碼揭示了 man page 沒寫的行為：`nl` 的分隔符行會被**取代成空行**（不 echo 原文）、
`-l` 只對 STYLE `a` 生效且在**第 N 個空行**才計數。

> 更正：這裡原本寫「`-d` 給一個字元會被重複成兩個」，**那是錯的**。
> 讀完 `nl.c` 的 `case 'd'` 才發現是「缺少第二個字元就補 `:`」，
> 所以 `-da` 的 footer 是 `a:` 而不是 `aa`。當時的差異 case 抓不到，
> 是補了會真正命中的 fixture 才浮出來。見 2.4 第 1 點。

**補一句教訓：讀原始碼要讀到「這個寫法為什麼這樣」，才不會讀完還是猜錯。**
`-d` 那段我第一眼讀成「把字元複製進預設陣列」，沒注意到 `*p++ = *optarg++`
只寫 `strlen(optarg)` 個字元，所以單一字元時 `section_del[1]` 仍是預設的 `:`。

### 坑 5：Rust 字串裡的反斜線

寫 BRE 相關測試時用 raw string `r"a\{2\}"`，不要用 `"a\\{2\\}"`，後者容易漏轉義。

### 坑 6：`printf '%0*'` 與負數

`printf("%04d", -7)` 得 `-007`（符號在零前面），不是 `00-7`。

---

## 6. 其他慣例

- **`argv[0]` 要跟著走**：GNU 的 Try line 會印實際呼叫路徑
- **typographic quotes**：coreutils 的診斷訊息用 `‘x’`（U+2018/U+2019），不是 `'x'`
- **訊息後不加句點**：GNU 的訊息通常沒有句點，但 Try line 有
- **release binary 大小**：static musl + Rust runtime，空 `fn main()` 約 530 KB，不是 debug symbol；
  已用 `[profile.release] strip = true, panic = "abort"` 降到約 445 KB
- **`target/` 只有一份**：`pack-initramfs.sh` 透過 `CARGO_TARGET_DIR` 共用 workspace 的 `target/`，
  不要在 `qemu-test/` 下再開一份
- **不要動 git**：本專案尚未 `git init`，只在 WSL 檔案系統上工作

---

## 7. 下一步建議順序

1. ~~**收尾 `nl`**~~ → **已完成**（見 2.4）
2. ~~**`paste`**~~ → **已完成**（見 2.5）
3. ~~**`comm`**~~ → **已完成**（見 2.6）
4. **`join`**：`comm` 的下一步，同屬比較類。做完可以順手把 `comm` 的
   `Reader`/`Stream` 共用程式搬到一個共用模組
5. **`env`**：補齊 guest-tools 的缺口，`-i`/`-u`/`-S`/`-C`/`--` 的行為要照 man page
6. **`expand` / `unexpand` / `split`**：同屬文字處理，`unexpand` 目前只有 man page
7. **`ls` / `stat`**：工作量最大（選項多、輸出格式複雜），建議最後處理
8. `md5sum`：只有 man page，且需要 hash 實作

> 做下一個工具時，**順便照 `qemu-test/harness/comm-fuzz.py` 寫一份 fuzzer**（見 2.7）。
> `comm` 的兩個真 bug 都是 fuzzer 抓的，手寫 case 抓不到。

---

## 8. 相關檔案速查

| 想做的事 | 看哪個檔案 |
|---|---|
| 環境怎麼建 | `qemu-test/ENV-SETUP.md` |
| 加一個工具要寫什麼 | `tools/cut/src/`、`tools/ln/src/`（最完整的三個範例） |
| differential 怎麼運作 | `qemu-test/harness/difflib.sh` |
| 一次跑完整驗證 | `qemu-test/harness/final-tally.sh` |
| guest 怎麼組裝 | `qemu-test/pack-initramfs.sh` |
| 有哪些 man page | `qemu-test/harness/man/` |
| 上一輪的完整驗證紀錄 | `/tmp/qemu-final.log`、`qemu-test/logs/` |

> `qemu-test/tmp/` 是**暫存區**，裡面有大量一次性 probe/fix 腳本。
> 交接時若要清理，保留 `difflib.sh`、`*-diff.sh`、`run-cu-cases.sh`、`final-tally.sh`、`man/`
> 這幾類就夠了，其餘可安全刪除。
---

## 9. QEMU harness 升級：磁碟與特權工具（2026-10-04）

`mount` / `fdisk` / `mkfs` / `blkid` / `losetup` 這批要 root、要有真磁碟。
使用者要求**全部同標準過 QEMU**，所以 harness 先升級一次，之後每個工具都走同一套門檻。

### 9.1 加了什麼

| 項目 | 內容 |
| --- | --- |
| 虛擬磁碟 | `run-one.sh` 掛 **2 顆 64 MiB 空白 virtio disk**，`snapshot=on`，所以 case 可以放心 fdisk/format，每次開機都是乾淨的 |
| 磁碟模組 | 從 Alpine modloop 抽出 11 個（`virtio_blk` `loop` `ext4` `vfat` `fat` `msdos` `f2fs` `ntfs3` `crc16` `mbcache` `jbd2`），見 `qemu-test/tmp/fetch-blkdev.sh` |
| 模組載入 | `guest-init` 拆成 `REQUIRED_MODULES`（9p，失敗即中止開機）與 `OPTIONAL_MODULES`（磁碟/fs，只報不誤） |
| 打包 | `pack-initramfs.sh` 改成走訪整棵 `assets/modules-*-virt/`，並拒絕 basename 衝突 |
| 開關 | `DISKS=0 ./run-one.sh <id>` 可以完全不掛磁碟 |

實測（`6.18.52-0-virt`，開機 3 秒）：`/dev/vda`、`/dev/vdb`、`/dev/loop0..7`、
`/sys/block/{vda,vdb,loop*}`、`/proc/filesystems` 裡的 `ext3 ext4 vfat` 全部就位。
升級後既有 **60 個 case 仍然 60 PASS**。

> initramfs 從 1.7 MB 變成 8 MB（`ext4.ko` 2.4 MB、`f2fs.ko` 2.2 MB、`ntfs3.ko` 0.7 MB 是主因）。
> 壓縮的話可以降回 2 MB 左右，但目前 uncompressed cpio 讀起來更快，先不動。

### 9.2 三個踩到的坑

1. **`modinfo -F vermagic` 的完整字串不是版本號**，而是
   `"6.18.52-0-virt SMP preempt mod_unload modversions "`。
   用 `== "$VER-virt"` 比會把**每一個**模組都判成 vermagic 不符而全部跳過，
   結果是「installed 0 module(s)」還不報錯。要用前綴比對。

2. **6.x 裡很多驅動已經內建或合併**，照舊的檔名清單去抓會全部 NOT FOUND：
   `CONFIG_VIRTIO=y`、`CONFIG_VIRTIO_PCI=y`、`CONFIG_VIRTIO_PCI_LEGACY=y`，
   所以根本沒有 `virtio.ko` / `virtio_pci.ko` 要抽；
   `msdos` 分區表現在在 `kernel/fs/fat/msdos.ko`（不是 `block/partitions/`）。
   **先讀 `assets/config-<ver>` 決定內建還是模組，再決定要抽什麼。**

3. **guest 裡沒有 `ls`**（`ls` 還是空殼，`pack-initramfs.sh` 會跳過），
   所以 probe 腳本用 `ls` 會什麼都不印而看起來像「沒掛磁碟」。
   guest 內的腳本請用 glob（`echo /sys/block/*`）與 `[ -e ]`。

### 9.3 特權工具的 case 規格

跟一般工具一樣走 `cases-shell/<tool>.misc.sh` + `manifest.txt`，
期望值一樣用本機 oracle 生成。需要 root 前置的寫在 case 開頭，
**缺前置就 FAIL，不跳過** —— 跳過會讓「60 PASS」變成沒有意義的數字。

- loop 類（`losetup` / `mkfs` / `mount`）：用 `scratch/*.img` + `losetup`，自給自足
- 分割表類（`fdisk` / `sfdisk` / `partx`）：只動 `/dev/vda`、`/dev/vdb`，不碰開機碟
- 終端類（`agetty` / `login` / `su` / `sulogin`）：guest 已有 devtmpfs + devpts，
  用 `script` 包一層跑，assert exit code 與輸出

> ⚠️ 分割表類 case 的檔案名要寫死成 `/dev/vda` 與 `/dev/vdb`。
> 寫錯會寫到 guest 的開機碟。

---

## 10. `env(1)`（2026-10-04）

第 39 個工具。17 個單元測試 / 170 個 differential / 5000 輪 fuzz / QEMU `env.misc`。

### 10.1 `-S` 的狀態機：三個容易搞錯的規則

`env.c` 的 `build_argv` 是一個手寫狀態機，除了引號與跳脫，還有三件事不會
從程式碼表面看得出來：

1. **起始 `sep = true`。** 所以第一個「準備被附加的字元」會先結束一個**空的**
   第一個 argument。`build_argv` 回傳的 `argc` 從 1 起算，所以第一次
   `check_start_new_arg` 關掉的是 **argv[0] 這個程式名稱槽位**，不是真的 argument。
   程式碼裡表現為「`args` 的第一個元素永遠要丟掉」。
   這就是為什麼 `-S ''` 產生 **0 個** argument，而 `-S "''"` 產生 **1 個**空 argument。

2. **`check_start_new_arg` 在每個「附加的字元」之前都跑**，不只是在空白處。
   引號字元自己也呼叫它，所以 `-S '""'` 會產生一個空 argument，
   即使引號本身不貢獻任何位元組。**跳脫出來的字元走的是同一條路**，
   所以 `a\#b` 是一個 argument（`#` 不是在 argument 開頭）。

3. **`${VAR}` 只有在變數「有被設定」時才呼叫它。** 沒設定的變數不貢獻任何東西，
   **而且也不會切開 argument**，所以 `-S '${NOSUCH}x'` 是**一個** argument `x`。

跳脫表也很短、也很反直覺：**沒有 `\a`、沒有 `\b`、沒有接行反斜線**，
而且**跳脫空白是非法的**（要用引號）。真正存在的是：

| 輸入 | 括號外 | 雙引號內 |
| --- | --- | --- |
| `\' \" \# \$ \\` | 原字元 | 原字元 |
| `\_` | **argument 分隔符**（等同空白） | 空格 |
| `\c` | **直接結束整個字串** | 錯誤 `must not appear in double-quoted -S string` |
| `\f \n \r \t \v` | 對應控制字元 | 同左 |
| 其他 | `invalid sequence '\X' in -S` | 同左 |

`#` 在「argument 會開始的位置」直接跳到 `eos`，而 `eos:` 標籤在
「引號沒閉合」檢查**之後**，所以 `-S "'#'x"` 不報錯、還會得到一個 argument `#x`——
引號根本沒閉合也無所謂。`\c` 的跳轉同理。

### 10.2 `-S` 會改寫 argv 並重掃

`parse_split_string` 把切出來的 argument 塞回 argv 並把 `optind` 設成 0，
所以 **`-S` 切出來的字串可以自己帶選項**：`env -S '-i -C/tmp' cmd` 是合法的。
這也是為什麼 parser 必須是一個有狀態的 struct，而不是對 slice 的迴圈。

### 10.3 選項掃描不排列

`shortopts[] = "+C:iS:u:v0" C_ISSPACE_CHARS` 開頭的 `+` 表示 **POSIX 行為**：
第一個 operand 就結束選項掃描。所以 `env A=1 -i cmd` 真的會去執行 `-i`。
連帶的結果：`env -i A=1 B=2 -u A` 中 `-u` 不是選項而是命令名。
（寫 guest case 時踩過。）

### 10.4 兩種引號，兩種 locale

`env` 有**兩個**不同的 quoting 函式，而輸出的形狀取決於 locale：

| | `quote()`（診斷訊息、`-v`） | `quoteaf()`（`-C` 的錯誤） |
| --- | --- | --- |
| locale 是 UTF-8 | `‘a’` 花括號引號，`'` **不跳脫** | `'a'` |
| 其他 locale | `'a'b'` 直引號，`'` **跳脫成 `\'`** | `'a'` |
| 位元組 >= 0x80 | 原樣輸出 | 原樣輸出 |
| 位元組 >= 0x80（非 UTF-8） | `\303\251` 八進位 | 原樣輸出 |
| 有控制字元 | 八進位跳脫 | `'a'$'\n''b'`，逐段加引號 |

所以在 C locale 下 `env nosuchcmd` 印 `'nosuchcmd'`，在 UTF-8 locale 下印 `‘nosuchcmd’`。
實作上 `main` 一開始要 `setlocale(LC_ALL, "")` 然後看
`nl_langinfo(CODESET) == "UTF-8"`，否則 Rust 預設的 C locale 會讓本機測試跟
UTF-8 shell 的結果不一致（fuzzer 抓到的第一個 bug 就是這個）。

### 10.5 訊號表不是 `<signal.h>`

gnulib 的表跟 `signal.h` 的宏**不一樣**：

- 32、33 兩個保留給 C 庫，**沒有名字**，`--block-signal=32` 會被拒絕。
- 別名只有 `IOT`→ABRT、`CLD`→CHLD、`IO`→POLT。**沒有 `UNUSED`**。
- `RTMIN`=34、`RTMAX`=64，中段命名是 `RTMIN+n`（34..49）與 `RTMAX-n`（50..64）。
- `RTMIN+n` / `RTMAX-n` **先算再驗**，不是先檢查範圍，所以
  `RTMIN+30` 剛好等於 64 是合法的，`RTMIN+31` 才不合法。
- 空格可以放在 base word 與符號之間（`RTMIN +2` 可以、`RTMIN+ 2` 不行），
  其他任何地方都不行。
- `EXIT` 解析成 0，`env` 自己拒絕它（訊息一樣是 `invalid signal`）。

### 10.6 `--default-signal` 也會動 mask

`main` 的 option 處理：

```c
case DEFAULT_SIGNAL_OPTION:   /* --default-signal */
  parse_signal_action_params (optarg, true);
  parse_block_signal_params (optarg, false);   /* <-- 順便 unblock！ */
case IGNORE_SIGNAL_OPTION:    /* --ignore-signal：動 handler，不動 mask */
case BLOCK_SIGNAL_OPTION:     /* --block-signal */
```

所以 `--default-signal=INT` 除了把 handler 重設回預設，還會**取消封鎖 INT**。
`--ignore-signal` 則不動 mask。`-v` 下兩者都有自己的 log 格式
（`Reset signal INT (2) to DEFAULT`、`signal INT (2) mask set to UNBLOCK`），
而 mask 的 log **會跳過 32、33**（有進 mask 但沒有名字可報）。

### 10.7 執行順序

1. `setlocale`、`SIGPIPE` 改回 `SIG_DFL`（Rust runtime 預設忽略 SIGPIPE，
   而系統工具不會；`--list-signal-handling` 會看到差別）
2. 選項掃描（含 `-S` 的 argv 改寫）
3. `-i` 清空環境／`-u` 取消變數（**在 assignment 之前**）
4. `NAME=VALUE` 賦值
5. 沒有命令就印環境
6. `sigaction` 重設 → `sigprocmask` → `--list-signal-handling` → `chdir` → `execvp`

> `-v` 的輸出順序會暴露第 3、4 步的先後：`env -v -i A=1` 先印
> `cleaning environ` 再印 `setenv:   A=1`。

### 10.8 libc 差異（guest 是 musl）

寫 guest case 時撞到三個 musl 與 glibc 的不同，**都不是實作的錯**：

1. **`nl_langinfo(CODESET)` 永遠回 UTF-8。** musl 沒有 locale 檔，
   所以 guest 裡不管 `LC_ALL` 是什麼，診斷都是花括號引號。
   → guest case 統一把 `LC_ALL` 釘成 `C.UTF-8`，讓 oracle（glibc）產生同樣的形狀。
2. **訊號 34 沒有位元可放，`SIGRTMIN()` 回 35。** musl 的 `sigaddset(34)` 直接
   回 EINVAL，所以「封鎖所有訊號」在 guest 裡列出來會少一行。
   → guest case 不做「封鎖全部」，改用點名的訊號清單；
   這個形狀留給 differential（本機 glibc）覆蓋。
3. **`putenv`/`setenv` 拒絕空名稱，`env =x` 因此不能走 libc。**
   → 自己複製一份環境陣列並多加一格；重複的空名稱要**就地取代**
   （`env =x =y` 只有一個 `=y`，位置在第一個 `=x` 原本的地方）。
   注意**不能 `realloc` 原本的陣列**（那不是 malloc 出來的），
   要配置新陣列再把 `environ` 指過去。

> 順帶：`environ` 陣列是由**null element** 結尾，不是 null pointer。
> 只檢查 cursor 是不是 null 會走過頭拿到垃圾然後 segfault。

### 10.9 `difflib.sh` 這次改了兩處

1. `normalise` 的路徑去前綴規則從 `^\(.\{0,40\}\)/$TOOL:` 放寬成 `^.*/$TOOL:`。
   本機的 debug build 在 `target/debug/` 下，絕對路徑超過 40 字元時舊規則擋不掉，
   每個含診斷訊息的 case 都會假失敗。
2. 多了一層 `tr '\0' '\n'`，**在 sed 之前**。
   這是為了 `-0` 這類以 NUL 結尾記錄的輸出：`^PWD=` / `^_=` 兩條取代只有逐行時才成立。
   兩邊都過同一層，所以不會蓋掉任何東西。

`_`（shell 記錄的「剛剛執行的指令」）與 `PWD`（shell 記錄的目錄）永遠兩邊不同，
被 `normalise` 換成 `<path>` / `<dir>`。

---

## 10. 共用模組：`tools/quoting`（2026-10-05）

原本「每個 crate 自足」的慣例，在這裡第一次被打破，因為**第二個** crate 就需要
同一組東西，而剩下 150 多個工具幾乎每個都會印至少一個診斷訊息。

`env` 要 `quote()` 和 `quoteaf()`；`seq` 要 `quote()`。所以抽出
`tools/quoting`，由 `env` 與 `seq` 共用。

### 10.1 `quote()` 的形狀取決於 locale

這是 fuzzer 抓到 env 的第一個 bug：**同一台機器上，同一個引數可能被報成
`‘a’` 或 `'a'`**。

| | UTF-8 locale | 其他 locale |
| --- | --- | --- |
| 引號 | `‘a’` U+2018/U+2019 | `'a'` |
| 撇號 | **不跳脫** → `‘a'b’` | **跳脫成 `\'`** → `'a\'b'` |
| 位元組 >= 0x80 | 原樣 | `‘\303\251’` 八進位 |

所以診斷訊息拼法取決於 locale，**而每個工具要不要呼叫 `setlocale` 也會改變結果**：

| 工具 | 有呼叫 `setlocale`？ | 在 C locale 下的形狀 |
| --- | --- | --- |
| `env` `seq` | 有 | 隨 locale 變 |
| `colrm` | 沒有 | 永遠是直引號 |

`quoting::open_locale()` 負責 `setlocale(LC_ALL, "")` 並記下
`nl_langinfo(CODESET) == "UTF-8"`；沒呼叫它的工具應該直接用
`quote_for(text, false)`，把這件事寫在程式碼裡而不是留給行程的 locale。

### 10.2 `quoteaf()` 跟 `quote()` 是兩個不同的函式

`quoteaf()` **不看 locale**，而且形狀是由「shell 會怎麼解讀」決定的：

- 整個字串用 `'...'` 包起來；
- 撇號變成 `'\''`（單引號裡唯一放得進撇號的寫法）；
- C locale 印不出來的字元開一個 `$'...'` 群組，裡面寫 `\n` 或三位八進位；
- 群組後面接一般的文字時，用 `''` 收尾。

**唯一會走雙引號的情況**：字串有撇號，而且沒有 `$`、反引號、`\`、`"`、`!`
—— 這幾個在雙引號裡仍然有意義。實測 95 個可列印 ASCII 每個都問過：
允許雙引號的是 ` %'+,-./0-9:@A-Z]_a-z`，其餘 21 個強制單引號。

`quoteaf()` 是照 `system.h` 裡的
`quotearg_style (shell_escape_always_quoting_style, arg)` 寫的，
**不是** `quotef()` 用的 `shell_escape_quoting_style`。差一個字，行為差很多。

### 10.3 驗證方式與已知差異

`quoteaf()` 的行為是**量出來的**，不是猜的：用 `env -C <字串> /bin/true`
（它會把 `quoteaf` 的結果放進錯誤訊息）當 oracle，跑 237 個案例
（每個可列印 ASCII 在兩個位置、空白形狀、控制字元、UTF-8 多位元組字元），
逐一比對字串。**236 個相同。**

唯一不同：`a'b\n\n`（撇號 + 兩個以上控制字元）時系統工具多一個單引號。
這個差異在 `quotearg` 的「唯讀掃描後重掃」路徑裡，
而 9.4 版和 gnulib master 的 `quotearg.c` 這一段是一樣的，所以看不出來源。
需要「含撇號又含一串控制字元的目錄名」才會碰到，所以記下來而不是猜。
單元測試裡有一條測試**釘住這個差異**（`quoteaf_diverges_from_the_system_tool_on_one_shape`），
免得它哪天悄悄變了或悄悄被「修好」而沒人知道。

### 10.4 順手找到 `quote()` 的一個坑

`quote()` 的撇號處理在兩種 locale 下**不一樣**（見 10.1 表格）。
寫成「一律跳脫」或「一律不跳脫」都會有一邊錯，而 differential 測試
只會測到其中一邊——除非測試**自己**把 locale 釘住。

> 這就是為什麼 `cases-shell/env.misc.sh` 每一條 case 都自己寫
> `LC_ALL=C.UTF-8 env ...`：guest 的 locale 不是測該不該依賴的東西。

---

## 11. `seq(1)` 的重寫（2026-10-05）

本來 7 個 case 就通過了。空輸入掃描（本來是為了找 `sort -z` 的 bug）
順手比較所有工具的輸出，發現 `seq` 的選項掃描與 6 類錯誤訊息都不對。
順帶說明：coreutils-diff.sh 裡 `seq` 原本只有 7 個 case，所以這些都沒被蓋到。

### 11.1 選項掃描不排列

`seq 3 -w` 的 `-w` 是 **operand**，不是選項：
`seq: invalid floating point argument: ‘-w’`。
所以 short option 字串開頭的 `+` 生效，**第一個 operand 就結束選項掃描**。

但 `-3` 又必須是 operand 而不是選項束，所以規則是：

> 以 `-` 開頭、而**後面接著數字前綴**的字串是 operand。

而且前綴不需要是完整的數字：`seq -3w` 把整個 `-3w` 當 operand
（然後才抱怨它不是數字）。`nan` 和 `inf` 沒有數字可提供，所以
`seq -inf` 是選項束，抱怨 `-i`。

### 11.2 預設輸出格式不是單一規則

`prec = MAX(first.precision, step.precision)` —— **LAST 不算**。
所以 `seq 3.25` 印 `1 2 3` 而不是 `1.00 2.00 3.00`。
指數會**調整** precision 而不是取消它：`seq 1e-3 0.1` 印 `0.001`。
只有十六進位 operand 會保留 sentinel（→ `%g`）。

`-w` 的寬度算法（`get_default_format`）連帶算點數佔位：

```c
first_width = first.width + (prec - first.precision);
last_width  = last.width  + (prec - last.precision);
if (last.precision  && !prec) last_width--;
if (!last.precision &&  prec) last_width++;
if (!first.precision && prec) first_width++;
width = MAX (first_width, last_width);
```

注意寬度是**最小**寬度：`%04.2f` 印 1.00 就是 `1.00`（剛好 4 個字元，不補）。
`seq -w 8.5 10` 才會補成 `08.5`。

### 11.3 最後一個數字會再看一次

序列是 `first + i * step`（不是反覆相加，所以長序列不累積誤差），
而且越界之後那個數字會**再看一次**：如果它印出來等於 LAST，
但和前一個印出來的不一樣，就照印。這就是
`seq 0 0.000001 0.000003` 為什麼不會吃掉最後一位。
實作要用到「把印出來的字串去掉 suffix 再讀回數字」。

### 11.4 `-f` 是真的交給 printf

`seq` 不是把 `%g` 換成數字，是**把格式交給 printf，傳一個 double 進去**。
所以實作直接呼叫 `snprintf`，不自己實作格式。

但 `seq` 會把格式改寫成 `long double` 版本（在轉換前插入 `L`），
所以要先做 `long_double_format` 的檢查：

- 找**第一個不是 `%%` 的 `%`**（`%%` 是字面百分號，不是 directive，
  所以 `seq -f '%%'` 抱怨「沒有 % directive」）；
- 掃 flag（`- + # 0 空格 '`，順序與次數不限）、寬度、精度；
- 然後是一個選用的 `L`；
- 轉換必須是 `efgaEFGA` 其一，否則 `has unknown %X directive`；
- **只能有一個 directive**，否則 `has too many %% directives`。

`prefix_len` 與 `suffix_len` 量的是**輸出後**的字元數，
所以 `a%%b%gc` 的 prefix 是 3 不是 4（`%%` 印成一個 `%`）。

**已知差異**：`seq.c` 傳的是 `long double`，Rust 沒有名稱給那個型別。
所以 `%La` 我們給 `0x1p+0`、系統工具給 `0x8p-3`。
`%f` `%e` `%g` 都一樣（那些轉換對兩種型別的結果相同）。
實作上把 `L` 拿掉再送 `double` 進 `snprintf`——
**留在原地會讓 printf 從 x87 堆疊讀到垃圾而印出 `nan`**（真的踩過）。

### 11.5 錯誤訊息一覽

| 情況 | 訊息 | 有 Try 行？ | exit |
| --- | --- | --- | --- |
| 不認得的短選項 | `invalid option -- 'X'` | 有 | 1 |
| 不認得的長選項 | `unrecognized option '--x'` | 有 | 1 |
| 選項少參數 | `option requires an argument -- 'f'` | 有 | 1 |
| 第 4 個 operand | `extra operand ‘x’` | 有 | 1 |
| 沒有 operand | `missing operand` | 有 | 1 |
| 數字解析失敗 | `invalid floating point argument: ‘x’` | 有 | 1 |
| NaN | `invalid not-a-number argument: ‘nan’` | 有 | 1 |
| increment 是 0 | `invalid Zero increment value: ‘0’` | 有 | 1 |
| `-f` 與 `-w` 一起 | `format string may not be specified with equal-width` | 有 | 1 |
| `-f` 格式問題 | `format ‘x’ has …` | **沒有** | 1 |

> `seq` 的 exit 是 **1**，不是其他 coreutils 工具慣用的 125。

---

## 12. 執行規定：我們的 binary 不在 host 跑，在 QEMU 裡跑（2026-10-05）

**規定：凡是執行「我們寫的工具」，一律在 QEMU guest 裡跑。**
不要在本機直接 `./target/debug/<tool>`，也不要用本機 shell 去 probe 我們的 binary。

### 12.1 為什麼

這一輪 `sum-diff.sh` 跑到 OOM，本機被 OOM killer 殺了一個 process：

```
Out of memory: Killed process 2444 (head) total-vm:16780524kB, anon-rss:9618944kB
```

原因是 `head` 的 `main.rs` 用 `read_to_end` / `fs::read` **把整個輸入讀進記憶體**再選前 N 個。
`sums-cases.sh` 的「big file blocks」case 有 `head -c 2000 /dev/zero`，而 `difflib.sh` 把
`target/debug` 放在 PATH 前面，所以那個 `head` 是我們的 —— 它對一個永遠不會結束的輸入
無限配置記憶體。本機沒有任何邊界擋住它；guest 有 `-m 256M`。

> 這不是 `head` 一個工具的問題。會「先讀完再選」的 crate 有 18 個：
> `cat` `colcrt` `comm` `cp` `cut` `dmesg` `expand` `hexdump` `line` `nl` `paste` `sort`
> `tail` `tr` `ul` `unexpand` `uniq` `wc`。
> 對一般檔案沒差別，但對 `/dev/zero`、`/dev/urandom`、或一個 fifo 就是無界成長。
> 之後每個工具都應該用**邊讀邊輸出**的寫法（`head` 已經改成這樣，見 12.3）。

### 12.2 怎麼執行

| 要做的事 | 用什麼 |
|---|---|
| probe 我們的 binary、問行為 | `qemu-test/harness/guest-run.sh 'command'`（開一次機，輸出抓回本機檔案） |
| 整合測試 | `qemu-test/run-one.sh <case-id>` / `run-many.sh` |
| 單元測試 | `cargo test -p <tool>`（純邏輯，不碰 I/O，留在本機） |
| differential | 本機跑，但 `difflib.sh` 已經加了 `ulimit -v 524288` |

differential 是這條規定的**唯一例外**，因為 oracle（`/usr/bin/<tool>`）在本機，兩邊必須在
同一個環境裡比，否則 locale、quoting、訊號編號（見 10.8）會造成假差異。所以 differential
繼續在本機跑，但**有記憶體上限**：case 撞到上限就 FAIL，而那個 FAIL 是**真缺陷**
（無界緩衝），不是 harness 的毛病。

`guest-run.sh` 的用法：

```bash
bash qemu-test/harness/guest-run.sh -o /tmp/ours.txt 'head -c 10 /dev/zero | wc -c'
```

輸出會經過 serial console 抓回來，NUL byte 不會被吃掉；boot 的雜訊被 marker 擋掉。

### 12.3 `head` 的修法

`lib.rs` 加了一個 `Taker`：`take()` 需要整個輸入，`Taker` 是它的串流版，餵一個 chunk
回傳「屬於前 N 個的位元組」，`done()` 之後 `main.rs` 就停止讀。

```rust
let piece = taker.push(&buffer[..read]);
if !piece.is_empty() { out.write_all(piece); }
if taker.done() { break; }
```

`head -n 5 /dev/zero` 現在是「讀到 5 個換行就離開」，不會再長到 9.6 GB。

> 注意：`/dev/zero` 沒有換行，所以 `head -n 3 /dev/zero` 本來就會一直讀 —— 系統工具也一樣。
> 差別在**記憶體**：GNU 邊讀邊印，我們之前是把全部留下來。
> `cases-shell/head.basic.sh` 加了一條 case 釘住這件事：`ulimit -v 16384` 之下讀 20 MB 的
> `/dev/zero`，舊寫法會直接 `out of memory`，串流寫法照常輸出。

