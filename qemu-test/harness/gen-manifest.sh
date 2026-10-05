#!/usr/bin/env bash
# Generate MANIFEST.md — the bill of every tool this repository owes.
#
# The list is generated, never hand-written: it comes from the file lists of the
# installed packages that provide the oracles, cross-checked against what actually
# exists under tools/. A hand-written list goes stale the moment a crate lands,
# and a stale bill of work is worse than none.
#
#   qemu-test/harness/gen-manifest.sh            # rewrite MANIFEST.md
#   qemu-test/harness/gen-manifest.sh --stdout   # print it instead of writing
#
# Status of a name:
#   done      tools/<crate>/src/lib.rs or src/main.rs has content
#   empty     tools/<crate>/ exists but both sources are 0 bytes (a skeleton)
#   todo      no crate at all
#
# Aliases are GNU binaries that are the same program reached through a different
# argv[0] (dir/vdir are ls, arch is uname, hd is hexdump, [ is test). They are
# billed to the canonical crate, because that is how they get implemented: one
# crate that dispatches on argv[0], not a crate per name.
set -u
cd /home/chenpc/git/rusttool

PKGS="coreutils util-linux util-linux-extra mount fdisk bsdextrautils bsdmainutils"

# name -> canonical crate, for the argv[0] families
alias_crate() {
  case "$1" in
    dir|vdir)            echo ls ;;
    arch)                echo uname ;;
    hd)                  echo hexdump ;;
    getty)               echo agetty ;;
    i386|x86_64|linux32|linux64) echo setarch ;;
    md5sum.textutils)    echo md5sum ;;
    \[)                  echo test ;;
    *)                   echo "$1" ;;
  esac
}

# Names that are not a tool we owe, whatever dpkg says.
excluded() {
  case "$1" in
    md5sum.textutils) return 0 ;;   # dpkg ships the same binary twice
  esac
  return 1
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# 1. every oracle binary the packages provide
for p in $PKGS; do
  dpkg -L "$p" 2>/dev/null | grep -E '^/(usr/)?s?bin/[^/]+$'
done | sort -u > "$tmp/paths"

# 2. classify each name
: > "$tmp/rows"
while read -r path; do
  name=${path##*/}
  excluded "$name" && continue
  crate=$(alias_crate "$name")
  if [ -s "tools/$crate/src/lib.rs" ] || [ -s "tools/$crate/src/main.rs" ]; then
    status=done
  elif [ -d "tools/$crate" ]; then
    status=empty
  else
    status=todo
  fi
  # an alias is only "done" when the canonical crate is done AND we actually
  # dispatch on the name; until then it stays on the bill
  printf '%s\t%s\t%s\t%s\t%s\n' "$status" "$name" "$crate" "$path" \
    "$([ "$name" != "$crate" ] && echo alias || echo -)" >> "$tmp/rows"
done < "$tmp/paths"

sort -k2 "$tmp/rows" > "$tmp/rows.sorted"

done_n=$(awk -F'\t' '$1=="done"'  "$tmp/rows.sorted" | wc -l)
empty_n=$(awk -F'\t' '$1=="empty"' "$tmp/rows.sorted" | wc -l)
todo_n=$(awk -F'\t' '$1=="todo"'  "$tmp/rows.sorted" | wc -l)
total_n=$(wc -l < "$tmp/rows.sorted")

# 3. crates that exist here but have no oracle on this machine
comm -13 <(cut -f3 "$tmp/rows.sorted" | sort -u) <(ls tools | sort -u) \
  > "$tmp/no-oracle"

{
  echo "# MANIFEST — 每個工具欠多少"
  echo
  echo "> 由 \`qemu-test/harness/gen-manifest.sh\` 產生，**不要手改**。"
  echo "> 來源是 oracle 所在 package 的檔案清單（\`$PKGS\`）交叉比對 \`tools/\`。"
  echo "> 生成時間：$(date -u '+%Y-%m-%d %H:%M UTC')"
  echo
  echo "## 現況"
  echo
  echo "| 狀態 | 數量 |"
  echo "|---|---|"
  echo "| done（已實作，三層驗證另計） | $done_n |"
  echo "| empty（有 crate 骨架但 0 bytes） | $empty_n |"
  echo "| todo（完全沒骨架） | $todo_n |"
  echo "| **合計** | **$total_n** |"
  echo "| 還要做（empty + todo） | $((empty_n + todo_n)) |"
  echo
  echo "\`done\` 只代表有程式碼。**真正結算以 \`qemu-test/harness/final-tally.sh\` 為準**："
  echo "unit + differential 逐位元組 + QEMU guest + fuzzer 四項全綠才算完成。"
  echo
  echo "## 名字家族（同一個 crate 依 argv[0] 分派）"
  echo
  echo "| 名字 | 歸到 |"
  echo "|---|---|"
  awk -F'\t' -v bt='`' '$5=="alias" {print "| " bt $2 bt " | " bt $3 bt " |"}' "$tmp/rows.sorted"
  echo
  echo "## 帳單"
  echo
  echo "| 狀態 | 工具 | crate | oracle | man page |"
  echo "|---|---|---|---|---|"
  while IFS=$'\t' read -r status name crate path isalias; do
    man=$([ -s "qemu-test/harness/man/$crate.txt" ] && echo yes || echo no)
    oracle=$([ -x "$path" ] && echo "$path" || echo "MISSING")
    printf '| %s | `%s` | `%s` | `%s` | %s |\n' "$status" "$name" "$crate" "$oracle" "$man"
  done < "$tmp/rows.sorted"
  echo
  echo "## 有 crate 但這台機器沒有 oracle"
  echo
  echo "這些無法做 host differential，只能靠 unit 與 guest 驗證"
  echo "（\`quoting\`/\`hashes\` 是共用模組，不是工具）："
  echo
  while read -r c; do printf -- '- `%s`\n' "$c"; done < "$tmp/no-oracle"
} > "${2:-MANIFEST.md}.new"

if [ "${1:-}" = "--stdout" ]; then
  cat "${2:-MANIFEST.md}.new"
else
  mv "${2:-MANIFEST.md}.new" MANIFEST.md
fi
