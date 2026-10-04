#!/usr/bin/env bash
# Build every guest binary and pack an uncompressed (cpio newc) initramfs,
# qemu-test/tiny.cpio. The initramfs itself only holds:
#
#   /init            static musl Rust init (qemu-test/guest-init)
#   /modules/*.ko    the 9p modules extracted from Alpine's modloop
#   /proc /sys /dev /tmp /mnt   empty mountpoints
#
# Everything that is *tested* lives on the 9p share (guest-root/), so it can be
# refreshed without repacking:
#
#   guest-root/bin/<tool>            the tool under test
#   guest-root/bin/brush             bash/POSIX-compatible shell (musl static)
#   guest-root/tests/<tool>.<case>   one payload per testcase (static binary)
#   guest-root/tests/<id>.sh         one payload per testcase (shell script)
#   guest-root/tests/fixtures/<tool>/  fixtures shipped by the case crate
#   guest-root/tests/manifest.txt    every runnable id, one per line
#
# The guest has no initramfs shell: /init is a static Rust binary and the
# payload is exec'd directly. Shell-script testcases run through brush from
# the share, so nothing has to be repacked to add one.
#
# The set of tools is derived from the case crates: every directory under
# cases/ is a tool that must also exist as tools/<tool>/Cargo.toml. Adding a
# tool therefore means adding the crate and the cases directory; nothing here
# needs to change. Testcases that are simpler as shell scripts need no crate
# at all: drop `cases-shell/<id>.sh` and it becomes a runnable id.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"

# cargo lives in ~/.cargo/bin, which non-interactive shells do not always have.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

TARGET=x86_64-unknown-linux-musl
# Build into the workspace target dir so the musl artifacts are shared
# with `cargo build --workspace` instead of duplicated under qemu-test/.
TARGET_DIR="$PWD/../target"
STAGE="$PWD/tmp/initramfs"
OUT=tiny.cpio
SHARE=guest-root
MANIFEST="$SHARE/tests/manifest.txt"

# musl already links statically, but be explicit (see ENV-SETUP.md section 1).
export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
export CARGO_TARGET_DIR="$TARGET_DIR"

# Case crate directory name -> tool name, e.g. cases/rev -> rev.
case_dirs=()
shopt -s nullglob
for dir in cases/*/; do
  case_dirs+=("${dir%/}")
done
shopt -u nullglob
[ ${#case_dirs[@]} -gt 0 ] || { echo "no case crates under cases/" >&2; exit 1; }

ids=()

echo "==> cargo build guest-init"
cargo build --release --target "$TARGET" --manifest-path guest-init/Cargo.toml

echo "==> cargo build probe-ok"
cargo build --release --target "$TARGET" --manifest-path probe-ok/Cargo.toml

init_bin="$TARGET_DIR/$TARGET/release/init"
probe_bin="$TARGET_DIR/$TARGET/release/probe-ok"
for bin in "$init_bin" "$probe_bin"; do
  [ -f "$bin" ] || { echo "missing $bin" >&2; exit 1; }
done

echo "==> stage initramfs in $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE"/{proc,sys,dev,tmp,mnt,modules}
install -m 0755 "$init_bin" "$STAGE/init"

mkdir -p "$SHARE/bin" "$SHARE/tests" "$SHARE/tests/fixtures"

# brush: the guest-side bash/POSIX-compatible shell used by *.sh testcases.
# Built once with cargo install into shell/install (musl static, so it runs
# from the 9p share with no loader present in the guest).
BRUSH_SRC="$PWD/shell/install/bin/brush"
if [ ! -x "$BRUSH_SRC" ]; then
  echo "==> cargo install brush (musl static)"
  cargo install --locked brush --root "$PWD/shell/install" --target "$TARGET"
fi
[ -x "$BRUSH_SRC" ] || { echo "missing $BRUSH_SRC" >&2; exit 1; }
install -m 0755 "$BRUSH_SRC" "$SHARE/bin/brush"

# The payloads live on the 9p share (guest-root/), not in the initramfs, but
# publish the freshly built probe-ok so run-one.sh probe-ok just works.
install -m 0755 "$probe_bin" "$SHARE/tests/probe-ok"
ids+=("probe-ok")

# cargo prints one JSON object per line with --message-format=json; the
# compiler-artifact records carry the "executable" field, which is all we need
# (and reading it keeps this script independent of the crate's layout).
built_executables() {
  local manifest=$1 log
  if ! log=$(cargo build --release --target "$TARGET" --manifest-path "$manifest" \
      --message-format=json 2>&1); then
    printf '%s\n' "$log" >&2
    return 1
  fi
  printf '%s\n' "$log" | sed -n 's/.*"executable":"\([^"]*\)".*/\1/p'
}

for dir in "${case_dirs[@]}"; do
  tool=$(basename "$dir")
  tool_manifest="../tools/$tool/Cargo.toml"
  [ -f "$tool_manifest" ] || {
    echo "case crate $dir has no $tool_manifest" >&2
    exit 1
  }

  echo "==> cargo build cases $dir"
  mapfile -t exes < <(built_executables "$dir/Cargo.toml")
  [ ${#exes[@]} -gt 0 ] || { echo "no bin target built from $dir" >&2; exit 1; }

  for exe in "${exes[@]}"; do
    # Cargo crate names cannot contain '.', so the bin target <tool>_<case> of
    # testcase <tool>.<case> is renamed on install. Enforced here so a typo in a
    # case crate fails the build instead of silently landing under a wrong id.
    base=$(basename "$exe")
    case "$base" in
      "${tool}_"*) id="${tool}.${base#"${tool}_"}" ;;
      *)
        echo "bin target '$base' in $dir must be named ${tool}_<case>" >&2
        exit 1
        ;;
    esac
    install -m 0755 "$exe" "$SHARE/tests/$id"
    ids+=("$id")
    echo "      $base -> $SHARE/tests/$id"
  done

  if [ -d "$dir/fixtures" ]; then
    mkdir -p "$SHARE/tests/fixtures/$tool"
    for fixture in "$dir"/fixtures/*; do
      [ -f "$fixture" ] || continue
      install -m 0644 "$fixture" "$SHARE/tests/fixtures/$tool/$(basename "$fixture")"
    done
  fi
done

# Shell-script testcases: cases-shell/<id>.sh needs no Rust crate. /init
# resolves /mnt/tests/<id> first and falls back to /mnt/tests/<id>.sh, which
# it runs through brush with PATH=/mnt/bin.
#
shopt -s nullglob
shell_cases=()
for script in cases-shell/*.sh; do
  shell_cases+=("$script")
done
shopt -u nullglob
for script in "${shell_cases[@]}"; do
  id=$(basename "$script" .sh)
  case "$id" in
    ""|*/*|*..*) echo "unsafe testcase id: $id" >&2; exit 1 ;;
  esac
  install -m 0755 "$script" "$SHARE/tests/$id.sh"
  ids+=("$id")
  echo "      $script -> $SHARE/tests/$id.sh"
done

# Init stage scripts: boot policy lives on the share, not in the initramfs.
mkdir -p "$SHARE/init.d"
shopt -s nullglob
stage_scripts=()
for script in init.d/*.sh; do
  stage_scripts+=("$script")
done
shopt -u nullglob
for script in "${stage_scripts[@]}"; do
  install -m 0755 "$script" "$SHARE/init.d/$(basename "$script")"
done

# Guest rc for brush (prompt etc), used by the interactive shell mode.
if [ -f etc/brushrc ]; then
  mkdir -p "$SHARE/etc"
  install -m 0644 etc/brushrc "$SHARE/etc/brushrc"
fi

# terminfo for the guest: brush's line editor needs it once stdin is a pty, and
# the image ships none. Copy a handful of profiles off the host so the same
# image works on any machine with ncurses-base installed.
TERMINFO_HOST="${TERMINFO:-/usr/share/terminfo}"
if [ -d "$TERMINFO_HOST" ]; then
  mkdir -p "$SHARE/etc/terminfo"
  for term in vt100 vt102 xterm xterm-256color linux ansi screen dumb; do
    initial=${term:0:1}
    if [ -f "$TERMINFO_HOST/$initial/$term" ]; then
      mkdir -p "$SHARE/etc/terminfo/$initial"
      install -m 0644 "$TERMINFO_HOST/$initial/$term" "$SHARE/etc/terminfo/$initial/$term"
    fi
  done
  echo "==> copied terminfo profiles from $TERMINFO_HOST"
fi

# Tools listed in guest-tools.txt are always published, whatever their cases
# look like: that is what puts them on PATH inside the guest for every testcase
# and for the interactive shell. Tools found through cases are merged in.
declare -A want_tools=()
if [ -f guest-tools.txt ]; then
  while read -r tool _rest; do
    case "$tool" in ''|\#*) continue ;; esac
    want_tools["$tool"]=1
  done < guest-tools.txt
fi
for dir in "${case_dirs[@]}"; do
  want_tools["$(basename "$dir")"]=1
done
for script in ${shell_cases[@]+"${shell_cases[@]}"}; do
  id=$(basename "$script" .sh)
  tool=${id%%.*}
  [ "$tool" = "$id" ] || want_tools["$tool"]=1
done

for tool in "${!want_tools[@]}"; do
  tool_dir="../tools/$tool"
  tool_manifest="$tool_dir/Cargo.toml"
  if [ ! -f "$tool_manifest" ]; then
    echo "    note: tools/$tool/Cargo.toml not there yet, skipping" >&2
    continue
  fi
  # A skeleton crate carries the manifest but no code, so it builds to nothing
  # and cannot join the guest PATH. Listing it above is the intent, not a
  # promise it is ready, so skip it until there is something to run.
  if [ ! -s "$tool_dir/src/main.rs" ] && [ ! -s "$tool_dir/src/lib.rs" ]; then
    echo "    note: tools/$tool is still a skeleton, skipping" >&2
    continue
  fi
  echo "==> cargo build tool $tool"
  cargo build --release --target "$TARGET" --manifest-path "$tool_manifest" -p "$tool"
  tool_bin="$TARGET_DIR/$TARGET/release/$tool"
  [ -f "$tool_bin" ] || { echo "missing $tool_bin" >&2; exit 1; }
  install -m 0755 "$tool_bin" "$SHARE/bin/$tool"
done

printf '%s\n' "${ids[@]}" | LC_ALL=C sort -u > "$MANIFEST"

echo "==> wrote $MANIFEST ($(wc -l < "$MANIFEST") ids)"
sed 's/^/      /' "$MANIFEST"

MODSRC=$(echo assets/modules-*-virt)
[ -d "$MODSRC" ] || { echo "no assets/modules-*-virt directory" >&2; exit 1; }
# Every module in the tree goes into the initramfs, flattened by basename; the
# load order lives in guest-init/src/main.rs (REQUIRED_MODULES / OPTIONAL_MODULES).
# Two modules with the same basename would collide here, so refuse rather than
# let the later one silently win.
echo "==> copy modules from $MODSRC (load order matters, see guest-init/src/main.rs)"
for ko in \
  kernel/fs/netfs/netfs.ko \
  kernel/net/9p/9pnet.ko \
  kernel/fs/9p/9p.ko \
  kernel/net/9p/9pnet_virtio.ko
do
  [ -f "$MODSRC/$ko" ] || { echo "missing $MODSRC/$ko" >&2; exit 1; }
done
declare -A seen_basename=()
copied=0
while IFS= read -r ko; do
  base=$(basename "$ko")
  if [ -n "${seen_basename[$base]:-}" ]; then
    echo "module basename collision: $base is both ${seen_basename[$base]} and $ko" >&2
    exit 1
  fi
  seen_basename[$base]=$ko
  install -m 0644 "$ko" "$STAGE/modules/$base"
  copied=$((copied + 1))
done < <(find "$MODSRC" -name '*.ko' | LC_ALL=C sort)
echo "    $copied module(s) staged"
sed 's/^/      /' <(cd "$STAGE/modules" && find . -name '*.ko' | LC_ALL=C sort)

echo "==> pack $OUT (cpio newc, uncompressed)"
rm -f "$OUT"
( cd "$STAGE" && find . -print0 | LC_ALL=C sort -z | cpio --null -o --format=newc --quiet ) > "$OUT"

echo "==> $OUT: $(stat -c %s "$OUT") bytes"
echo "    contents:"
cpio -itv --quiet < "$OUT" | sed 's/^/      /'
