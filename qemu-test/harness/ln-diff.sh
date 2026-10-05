#!/usr/bin/env bash
# Differential test for ln against GNU ln.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=ln
SYS=/usr/bin/ln
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

# The snapshot has to follow a link's target, or a wrong one goes unnoticed.
snapshot() {
  local dir="$1"
  (cd "$dir" && find . -mindepth 1 | LC_ALL=C sort)
  (cd "$dir" && find . -mindepth 1 -type l -printf 'link %p -> %l\n' | LC_ALL=C sort)
  # The inode number cannot match between two runs, so the file names are what
  # is compared; the link cases check sharing an inode with `test -ef`.
  (cd "$dir" && find . -mindepth 1 -type f -printf 'file %p\n' | LC_ALL=C sort)
}

F='printf x > a; printf y > b; mkdir dir; ln -s a existing'
run_case "symbolic"            "$F" 'ln -s a link; ls -l link | sed "s/.* link/link/"'
run_case "hard link"           "$F" 'ln a hard; test hard -ef a && echo same-inode'
run_case "hard link mode"      "$F" 'ln a hard; stat -c %a hard'
run_case "one operand"         "$F" 'ln -s a; ls -l a | sed "s/.*a/a/"'
run_case "one operand hard"    "$F" 'ln a; test a -ef a && echo same'
run_case "into a directory"    "$F" 'ln -s a dir; ls -l dir | sed "s/.*dir\///"'
run_case "two into directory"  "$F" 'ln -s a b dir; ls dir'
run_case "target directory"    "$F" 'mkdir d; ln -s a -t d; ls d'
run_case "target dir missing"  "$F" 'ln -s a -t nodir'
run_case "exists"              "$F" 'ln -s b existing'
run_case "force"               "$F" 'ln -sf b existing; ls -l existing | sed "s/.* existing/existing/"'
run_case "backup"              "$F" 'ln -sbf b existing; ls existing*'
run_case "backup suffix"       "$F" 'ln -sbf -S .old b existing; ls existing*'
run_case "no dereference"      "$F; ln -s a dirlink" 'ln -sn dirlink; ls -ld dirlink | sed "s/.* dirlink/dirlink/"'
run_case "relative"            "$F; mkdir -p sub/deep" 'ln -sr a sub/deep/link; ls -l sub/deep/link | sed "s/.* link/link/"'
run_case "relative up"         "$F; mkdir -p sub/deep" 'ln -sr a sub/deep/up; ls -l sub/deep/up | sed "s/.* up/up/"'
run_case "verbose"             "$F" 'ln -sv a link'
run_case "hard to itself"      "$F" 'ln a a'
run_case "symbolic to itself"  "$F" 'ln -s a a'
run_case "missing target"      "$F" 'ln -s nowhere link'
run_case "missing hard target" "$F" 'ln nowhere link'
run_case "no operands"         "$F" 'ln'
run_case "one operand and dir" "$F" 'ln -s a dir b'
run_case "bad option"          "$F" 'ln --nonsense a link'
run_case "bad short option"    "$F" 'ln -Z a link'
run_case "double dash"         "$F" 'ln -s -- -weird link; ls -l link | sed "s/.* link/link/"'
run_case "link to a directory" "$F" 'ln -s dir dlink; ls -ld dlink | sed "s/.* dlink/dlink/"'

summary