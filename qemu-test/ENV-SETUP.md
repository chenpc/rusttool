# QEMU IT 測試環境建置手冊

> 目的：重現本專案 QEMU-based 整合測試（IT）所需的完整環境。
> 架構：host（WSL）用 QEMU+KVM 開 guest，每個 testcase fresh boot 一次；
> guest 內容以 9pfs 唯讀掛載 host 的 `guest-root/`，免重打包；結果走 serial console 回傳。

## 0. 前提

- Windows 10/11 + WSL2，發行版為 Ubuntu-24.04（可用 `wsl -l -v` 確認）。
- 專案在 WSL 內路徑：`/home/chenpc/git/rusttool`
  （Windows 側 UNC：`\\wsl.localhost\Ubuntu-24.04\home\chenpc\git\rusttool`）。
- 以下所有 Linux 指令都在 **WSL 內**執行；從 Windows 側呼叫時一律用：
  `wsl.exe -- bash -lc '<script>'`（PowerShell 外層雙引號、內層單引號）。

## 1. Rust 工具鏈（WSL 內）

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

`cargo` 必須對**非互動式 login shell**可見（`bash -lc` 只讀 `~/.profile`/`~/.bash_profile`，
不讀 `~/.bashrc`），所以：

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.profile
```

開新 shell 驗證，並加 musl target（guest binary 全部靜態連結）：

```bash
cargo --version && rustc --version
rustup target add x86_64-unknown-linux-musl
```

> **狀態更新（2026-10-04）**：rustup 已安裝於 `~/.cargo/bin`，且 `~/.profile:28` 有
> `. "$HOME/.cargo/env"`（`.bashrc` 也有），因此 `bash -lc` 非互動式 login shell
> 已可直接找到 `cargo`／`rustc`，**不需**額外 append PATH。
> `x86_64-unknown-linux-musl` target 亦已安裝。
> guest 靜態連結建議明確加上 `-C target-feature=+crt-static`。

## 2. 系統套件（需 sudo）

```bash
sudo -n apt-get update
sudo -n apt-get install -y qemu-system-x86 cpio musl-tools
```

> **狀態更新（2026-10-04）**：本機已設定 passwordless sudo（`sudo -n true` 回傳成功），
> 自動化腳本可直接用 `sudo -n`，不再需要互動密碼。

## 3. KVM 存取權限

```bash
ls -la /dev/kvm   # 預期屬主 root:kvm
sudo usermod -aG kvm "$USER"
```

生效方式（二選一）：`wsl --shutdown` 後重進 WSL，或當次用 `sg kvm` 起 shell。

```bash
groups                # 應包含 kvm
qemu-system-x86_64 --version
```

> **狀態更新（2026-10-04）**：`chenpc` 已加入 `kvm`（`getent group kvm` → `kvm:x:993:chenpc`）。
> 新起的 `bash -lc` 已可直接讀寫 `/dev/kvm`，不需再 `sg kvm`；
> 若沿用舊的長駐 shell，用 `sg kvm -c '<cmd>'` 立即取得群組。

## 4. Guest kernel（Alpine virt，x86_64）

下載近期 stable 的 `vmlinuz-virt` 到 `qemu-test/assets/`。

Alpine 的 **netboot 目錄同時提供 kernel 與對應 config**，是最省事的來源
（不必下整包 `linux-lts-<ver>.apk`）：

```bash
BASE=https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64
curl -sSLf -o qemu-test/assets/vmlinuz-virt-6.18.52-0  "$BASE/netboot/vmlinuz-virt"
curl -sSLf -o qemu-test/assets/config-6.18.52-0-virt "$BASE/netboot/config-6.18.52-0-virt"
```

已下載（Alpine 3.24.2 stable，kernel `6.18.52-0-virt`）：

| 檔案 | 大小 | 用途 |
| --- | --- | --- |
| `assets/vmlinuz-virt-6.18.52-0` | 12,608,512 | bzImage，QEMU `-kernel` |
| `assets/config-6.18.52-0-virt` | 151,985 | 判定 feature 內建/模組 |
| `assets/modules-6.18.52-0-virt/` | ~570 KB | 9p 模組（見下） |

### 4.1 9P 支援狀態：**模組形式（module-only）**

`config-6.18.52-0-virt` 實測結果：

```text
CONFIG_NETFS_SUPPORT=m        <-- netfs 也是模組，必須先載入（見下表）
CONFIG_9P_FS=m            <-- 模組，非內建
CONFIG_NET_9P=m
CONFIG_NET_9P_VIRTIO=m
CONFIG_9P_FS_POSIX_ACL=y
# CONFIG_9P_FS_SECURITY is not set
```

所以 guest **必須**先 `insmod` 才能掛 9p。已從 netboot 的 `modloop-virt`
（squashfs，22,945,792 bytes）抽出對應版本的 9p 模組到
`assets/modules-6.18.52-0-virt/`（vermagic 皆為 `6.18.52-0-virt`，與 kernel 一致）：

```text
kernel/fs/netfs/netfs.ko    depends: (無)          <-- 9pnet 需要它，必須第一個載
kernel/fs/9p/9p.ko          depends: netfs,9pnet
kernel/net/9p/9pnet.ko       depends: netfs
kernel/net/9p/9pnet_virtio.ko depends: 9pnet    <-- virtio-9p transport（要用這個）
kernel/net/9p/9pnet_fd.ko    depends: 9pnet     （debug 用，fd transport）
```

> **實測更正（2026-10-04）**：`netfs` **不是**內建，而是模組。少了
> `netfs.ko`，`9pnet.ko` 會以 `Unknown symbol netfs_write_subrequest_terminated (err -2)`
> 失敗，開機 log 看得到。抽取指令見 `tmp/fetch-netfs.sh`。

> 抽取注意：`modloop-virt` 的 squashfs 內部有一層實體 `squashfs-root/` 目錄，
> 用 `unsquashfs -l` 看到的是 `squashfs-root/modules/<ver>/...`，
> 但 `unsquashfs -d <dir>` 解出來後會**去掉**該前綴，實際落在
> `<dir>/modules/<ver>/...`。直接對 `squashfs-root/...` 做檔案過濾會靜默抓不到檔案。

打包 tiny-initramfs 時把模組平鋪到 guest 的 `/modules/`（`pack-initramfs.sh` 負責），
載入順序：

```text
/modules/netfs.ko
/modules/9pnet.ko
/modules/9p.ko
/modules/9pnet_virtio.ko
```

然後掛載：

```bash
mount -t 9p -o trans=virtio,version=9p2000.L,ro hostshare /mnt
```

### 4.2 `init_module` 系統呼叫的 ABI（6.13+）：第三個參數不能是 NULL

Linux 6.13 起 `init_module`/`finit_module` 的第三個參數改成指向
`struct module_init_info`（flags / len_of_params / params），傳 NULL 會直接
回 `-EFAULT`（`Bad address`），而且 kernel **完全沒有 log**，極難 debug。
`guest-init` 的作法是先傳一個 32 bytes 的全零 buffer（等價於「無 flag、無參數」），
失敗再退回舊式空字串參數與 `finit_module`，兩條路都印出 errno。

> libc（musl/gnu）在 x86_64-musl 上根本沒有 `init_module`/`finit_module` 宣告，
> 只能用 `libc::syscall(SYS_init_module, ...)` 發動。

### 4.3 Guest shell：brush（2026-10-04 加入）

guest 的 initramfs 裡**沒有** shell（只有 `/init` 這支靜態 Rust binary），但需要一個
bash 相容 shell 來跑 `.sh` 型 testcase，因此用 Rust 寫的 brush：

```bash
cargo install --locked brush --root qemu-test/shell/install \
  --target x86_64-unknown-linux-musl
```

- 產物 `qemu-test/shell/install/bin/brush`（約 14 MB，`static-pie linked`），由
  `pack-initramfs.sh` 裝到 9p 分享目錄的 `guest-root/bin/brush`（gitignore，`shell/install/`）。
- 選 `brush` 的原因：官方支援 musl 目標、單一靜態檔、bash 相容（`set -euo pipefail`、
  管線、命令替換、`read` 等內建），而且跟本專案同一語言。
- `/init` 的 payload 解析順序：`/mnt/tests/<id>`（靜態 binary）優先；不存在時改跑
  `/mnt/tests/<id>.sh`，交給 `/mnt/bin/brush` 執行。
- payload 一律帶環境變數 `PATH=/mnt/bin`、`HOME=/tmp`、`TMPDIR=/tmp`，
  所以 `.sh` 測項可以直接用工具名稱呼叫被測工具（`echo hi | rev`）。
- 9p 掛載為唯讀，測項要寫檔一律寫 `/tmp`（`/init` 開機時掛的 tmpfs）。

### 4.4 開機 shell 與 init stage script（`run-shell.sh`）

`/init` 保持「只做平台開機」：掛 `/dev`（devtmpfs）→ `/proc`、`/sys` → 載入 9p 模組 →
掛 `/tmp` tmpfs → 掛 9p。開機政策放在 share 上：

```text
qemu-test/init.d/*.sh   →  guest-root/init.d/*.sh   （依檔名順序，用 brush 執行）
```

stage script 在任何 testcase 之前跑，第一支失敗就是 `FAIL(stage <name>: exit=N)`。
要加開機檢查／前置作業，直接丟一支 `.sh` 進 `init.d/` 即可，不用重打包 initramfs。

開機模式由 kernel cmdline 決定（`/proc/cmdline`）：

| cmdline | 行為 |
| --- | --- |
| `rd.test=<id>` | 跑 `guest-root/tests/<id>`（binary）或 `<id>.sh`（brush） |
| `rd.shell`（無 token 時的預設） | 互動 brush：guest 內開 pty，proxy 串接 console |
| `rd.shellpipe` | brush 的 stdin 用管線餵（無 prompt、無行編輯，但不會卡） |
| `rd.shelltty` | brush 直接掛在 console tty（需要 host 端 pty 會回應 DSR） |
| `rd.shellscript` | 跑 `guest-root/tests/.shellcmd`（`run-shell.sh -c` 用） |

```bash
./run-shell.sh                    # 互動：等 stage= 訊息出來再打字，exit 離開
./run-shell.sh -c 'echo hi | rev' # 腳本：指令走 share，不經 stdin
./run-shell.sh -m 512             # 加大 guest 記憶體
```

實測得到的五個重要限制（每個都真的踩過）：

1. **不要在開機瞬間用管線餵 stdin**：模擬 UART 在 guest 還沒開完機時收到的位元組會被丟掉
   （實測：立即送 `echo AAA` 只收到 `AAA`，延遲 4 秒送則完整）。真人互動等 banner 再打字
   沒問題；腳本用途請用 `-c`。
2. **brush 預設的行編輯器不能用**：reedline 開機會送 `ESC[6n` 查游標位置，等不到回應就以
   `The cursor position could not be read within a normal duration` 離開。host 上用 `script`
   給它真的 pty 也一樣失敗（`script` 不回答 DSR）。解法是 `--input-backend basic`。
3. **pty 一定要先設 raw**：ICANON 開著時 proxy 送進去的 `ESC[1;1R` 沒有換行就不會被讀走，
   行編輯器會一直等；ICRNL 會把 Enter 的 CR 轉成 LF 而不被接受，所以 console 也要 raw；
   關掉 OPOST 才不會讓 shell 已輸出的 CRLF 變成 CRCRLF。
4. **guest 沒有 coreutils**：只有 brush 內建指令 + 有 testcase 的工具（`/mnt/bin`）。
   `rm`/`cat`/`grep` 都不存在，stage script 與 testcase 不能用。
5. **需要 terminfo**：行編輯器要看終端資料庫。`pack-initramfs.sh` 從 host 的
   `/usr/share/terminfo` 複製幾個 profile 到 `guest-root/etc/terminfo/`，
   `/init` 以 `TERMINFO=/mnt/etc/terminfo` + `TERM=xterm` 啟動 brush。

prompt 由 `qemu-test/etc/brushrc`（`PS1='\w\$ '`）決定，經 `--rcfile` 載入。guest 是 root
且 cwd 就是 `$HOME`，所以實測 prompt 顯示為 `~# `。


## 5. 驗證清單

於 2026-10-04 全數通過（實測輸出）：

- [x] WSL 新 shell 可執行 `cargo --version` → `cargo 1.99.0 (5f94df478 2026-08-27)`
- [x] `rustc --version` → `rustc 1.99.0 (b940084d7 2026-09-28)`
- [x] `rustup target list --installed` 含 `x86_64-unknown-linux-musl`
      （靜態連結實測：`static-pie linked`，並可直接執行）
- [x] `qemu-system-x86_64 --version` → `QEMU emulator version 8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.18)`
- [x] `cpio` 2.15、`musl-gcc` 13.3.0 已安裝
- [x] `qemu-test/assets/vmlinuz-virt-6.18.52-0` 存在（sha256 `40f620bc…559c`）
- [x] `groups` 含 `kvm`，`/dev/kvm`（`root:kvm`, `crw-rw----`）可直接讀寫
- [x] `-accel kvm` 可啟動（QEMU 未報錯，僅因 `-machine none` 無退出條件被 timeout 收掉）

## 6. 目錄規劃

```text
qemu-test/
  ENV-SETUP.md   # 本文件
  assets/        # kernel、kernel modules（二進位，不進 git，見 .gitignore）
  pack-initramfs.sh   # cargo build 兩個 crate → tiny.cpio（cpio newc）
  guest-init/    # guest /init（musl 靜態、std + libc，detached workspace）
  probe-ok/      # 單一測項 crate，產物複製到 guest-root/tests/probe-ok
  cases/<tool>/  # guest case crate：一 case 一支 src/bin/<id>.rs（見 §8.1）
  cases-shell/<id>.sh  # shell 型 testcase：一個 .sh 就是一個 test id（見 §4.3）
  shell/install/ # cargo install brush 的根（gitignore，pack 時自動重建）
  guest-root/    # 9p 分享目录：bin/ tests/（payload 放這裡，guest 唯讀掛在 /mnt）
  run-one.sh     # 單次開機跑一個 test-id，結果看 logs/<id>.log
  run-many.sh    # 依 manifest 排多顆並行開機，彙整 logs/run-many.results
  logs/          # serial log（gitignore）
  tiny.cpio      # 產生的 initramfs（gitignore）
  target/        # cargo 產出（gitignore）
  tmp/           # 一次性 scratch 腳本（僅 verify-rev.sh 等可重複使用者留存）
  runner/        # Rust 版 host runner（worker pool、serial 解析）— 尚未開始，
                 # 目前並行排程由 run-many.sh 負責
```

## 7. 已知阻塞

### 已解除（2026-10-04）

- ~~WSL 內 `sudo` 需要互動密碼~~ → 已設定 passwordless sudo，`sudo -n` 可直接用。
- ~~`qemu-system-x86`、`musl-tools`、`Alpine kernel` 尚未安裝/下載~~ → 已完成安裝與下載，
  見第 4、5 節。

### 仍需注意

- **9P 為模組形式**，guest 開機後必須 `insmod`
  `netfs.ko` → `9pnet.ko` → `9p.ko` → `9pnet_virtio.ko` 才能掛載；
  tiny-initramfs 打包時要把 `assets/modules-6.18.52-0-virt/` 底下的 `.ko` 一併放進 `/modules/`。
- **kernel 與模組必須同版本**：Alpine `main` 倉庫的 `linux-lts` 已前進到 `6.18.54-r0`，
  而本 repo 內的 kernel 是 netboot 同期的 `6.18.52-0-virt`。若之後要換 kernel，
  要連 `config-*`、`modloop-virt`（模組來源）一起換，否則 vermagic 不符、模組載入失敗。
- **CDN 速度不穩**：`dl-cdn.alpinelinux.org` 曾跑到約 34 KB/s（12 MB kernel 下載逾時）。
  伺服器支援 `Accept-Ranges: bytes`，可用 `curl -C -` 續傳。
- `kvm` 群組對**已經開著**的長駐 WSL shell 不生效；需重開 shell 或 `sg kvm -c '<cmd>'`。
- WSL2 關機/重啟後 `/dev/kvm` 需 WSL 自動建立（目前正常）。

## 8. 下一步（環境就緒後）

1. ~~寫 guest `/init`（極簡 Rust init：掛載 9p → 讀 cmdline 跑測項 → serial 報結果 →
   poweroff）與 tiny-initramfs 打包腳本。~~ **已完成（2026-10-04）**
   - `guest-init/`（`/init`，musl static、std+libc）、`pack-initramfs.sh`、
     `run-one.sh`、`run-many.sh`、`.gitignore`、`probe-ok/` + `guest-root/tests/probe-ok`。
   - 單次開機已打通（~2 秒）：`./pack-initramfs.sh && ./run-one.sh probe-ok` →
     `RESULT: PASS`，serial log 出現 `QEMU-TEST: PASS` 與 `reboot: Power down`，
     qemu rc=0 正常結束（非 timeout）。
   - 並行開機也已打通：`./run-many.sh rev` 以 `JOBS=8`（預設）同時起多顆 guest，
     逐顆沿用 `run-one.sh` 的 qemu 參數與 `QEMU-TEST:` marker 契約解析 serial，
     彙整成 `logs/run-many.results`；全數 PASS 才 exit 0。
   - 過程中補掉兩個環境坑：`netfs` 也是模組（4.1）、`init_module` 第三參數
     不能是 NULL（4.2）。
2. ~~先拿 `rev` 打通 single-boot，再開並行~~ **已完成（2026-10-04）**，並把
   testcase 模板定形（見 8.1）。
3. 以 `rev` 模板為基準，把 115 個工具的 IT 逐批搬進 guest case 格式。
   - 2026-10-04：guest shell（brush）已上線（§4.3），`.sh` 型 testcase 不需要 Rust
     crate，只要丟一個 `cases-shell/<id>.sh` 就是一個可跑的 id；`harness.smoke`
     已驗證內建指令、管線、`PATH` 呼叫工具、tmpfs 寫入、非零 exit 都正常。
     已完成的工具：`rev` `column` `col` `colrm` `bits` `colcrt` `line` `ul`（共 46 個 id）。

### 8.1 `rev` testcase 模板（其他工具照抄）

三段式分工：`tools/rev` 是被測工具本體，`qemu-test/cases/rev` 是 guest case。

| 位置 | 角色 | 說明 |
| --- | --- | --- |
| `tools/rev/` | 被測 binary | workspace member；`cargo test -p rev` 跑 host 端單元測試 |
| `qemu-test/cases/rev/` | guest case crate | 一個 case 一支 `src/bin/<id>.rs`，目前 5 支：`rev_basic`、`rev_files`、`rev_missing`、`rev_nonewline`、`rev_version`；musl static 後由 initramfs 帶進 guest |
| `qemu-test/guest-root/tests/manifest.txt` | case 註冊表 | 一行一個 test id；`run-one.sh <id>` 與 `run-many.sh <filter>` 都以它為選擇依據 |

可重複使用的全循環驗證腳本：`qemu-test/tmp/verify-rev.sh`
（`cargo test -p rev` → 清除所有 build 輸出 → `pack-initramfs.sh` → `run-many.sh rev`）。
2026-10-04 實測 6 個 id 全數 `PASS`（`logs/run-many.results`）：

```text
rev.basic  PASS      rev.missing    PASS
rev.files  PASS      rev.nonewline  PASS
rev.version PASS     probe-ok       PASS
```

> 尚未做：Rust 版的 host `runner`（§6 `runner/` 仍空缺）。目前並行排程由
> `run-many.sh`（bash + `xargs -P`）負責，足以應付 115 個工具的批次；
> 待 case 數量或重試需求變大再寫 Rust 版。

