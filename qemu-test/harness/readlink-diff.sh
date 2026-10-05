#!/usr/bin/env bash
# Differential test for readlink.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=readlink
SYS=/usr/bin/readlink
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

# The canonical answers contain the temporary directory, which differs between
# the two runs, so rewrite it to a fixed token.
normalise() {
  sed -e "s#$SYS#$TOOL#g" -e "s#^\(.\{0,40\}\)/$TOOL:#$TOOL:#" \
      -e "s#Try '.*/$TOOL --help'#Try '$TOOL --help'#" \
      -e "s#$T/[ab]#DIR#g" "$1"
}

F='mkdir d; printf x > plain; ln -s plain link; ln -s d dlink; ln -s nowhere dangling; ln -s d/gone gone'
run_case "target"              "$F" 'readlink link'
run_case "target relative"     "$F" 'readlink dlink'
run_case "no newline"          "$F" 'readlink -n link; echo'
run_case "zero"                "$F" 'readlink -z link | od -c | head -1'
run_case "zero no newline"     "$F" 'readlink -z -n link | od -c | head -1'
run_case "several links"       "$F" 'readlink link dlink dangling'
run_case "plain file"          "$F" 'readlink plain; echo "exit=$?"'
run_case "directory"           "$F" 'readlink d; echo "exit=$?"'
run_case "missing"             "$F" 'readlink nope; echo "exit=$?"'
run_case "missing verbose"     "$F" 'readlink -v nope; echo "exit=$?"'
run_case "dangling target"     "$F" 'readlink dangling'
run_case "verbose plain file"  "$F" 'readlink -v plain; echo "exit=$?"'
run_case "silent missing"      "$F" 'readlink -s nope; echo "exit=$?"'
run_case "canonicalize"        "$F" 'readlink -f link'
run_case "canonicalize dir"    "$F" 'readlink -f dlink'
run_case "canonicalize missing tail" "$F" 'readlink -f gone; echo "exit=$?"'
run_case "canonicalize missing mid"  "$F" 'readlink -f a/b/c; echo "exit=$?"'
run_case "canonicalize missing all" "$F" 'readlink -m a/b/c; echo "exit=$?"'
run_case "canonicalize existing"     "$F" 'readlink -e link'
run_case "canonicalize existing missing" "$F" 'readlink -e gone; echo "exit=$?"'
run_case "canonicalize long form"    "$F" 'readlink --canonicalize dlink'
run_case "canonicalize missing long"  "$F" 'readlink --canonicalize-missing a/b'
run_case "dot slash"           "$F" 'readlink -f ./link'
run_case "dotdot"              "$F; mkdir -p d/sub" 'cd d/sub && '"$TOOL"' -f ../../link'
run_case "absolute input"      "$F" "readlink -f $T/a/link | sed 's#[ab]#DIR#'"
run_case "no operand"          "$F" 'readlink; echo "exit=$?"'
run_case "bad short option"    "$F" 'readlink -Z link; echo "exit=$?"'
run_case "bad long option"     "$F" 'readlink --nonsense link; echo "exit=$?"'
run_case "double dash"         "$F; ln -s -- -weird dashlink" 'readlink -- dashlink'
run_case "loop"                "$F; ln -s loop1 loop2; ln -s loop2 loop1" 'readlink loop1; echo "exit=$?"'

summary