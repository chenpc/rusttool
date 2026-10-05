#!/usr/bin/env bash
# Differential test for tee.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
B=/home/chenpc/git/rusttool/target/debug
TOOL=tee
SYS=/usr/bin/tee
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

F=':'
run_case "one file"             "$F" 'printf hello | tee out; cat out'
run_case "two files"            "$F" 'printf hello | tee a b; cat a; cat b'
run_case "append"               "$F; printf old > out" 'printf new | tee -a out; cat out'
run_case "append long"          "$F" 'tee a; printf new | tee -a a; cat a'
run_case "truncates by default" "$F; printf oldlonger > out" 'printf new | tee out; cat out'
run_case "no files"             "$F" 'printf hello | tee'
run_case "existing directory"   "$F; mkdir d" 'printf hello | tee d'
run_case "missing directory"    "$F" 'printf hello | tee nodir/out'
run_case "dash is stdout"       "$F" 'printf hello | tee - | cat'
run_case "unwritable file"      "$F; : > out; chmod 400 out" 'printf hello | tee out; echo "exit=$?"'
run_case "one bad one good"     "$F; : > out; chmod 400 out" 'printf hello | tee out ok; echo "exit=$?"; cat ok'
run_case "output-error warn"    "$F; mkdir d" 'printf hello | tee --output-error=warn d; echo "exit=$?"'
run_case "output-error bare"    "$F; mkdir d" 'printf hello | tee --output-error d; echo "exit=$?"'
run_case "output-error invalid" "$F" 'printf hello | tee --output-error=maybe out'
run_case "pipe mode"            "$F; mkdir d" 'printf hello | tee -p d; echo "exit=$?"'
run_case "ignore interrupts"    "$F" 'printf hello | tee -i out; cat out'
run_case "empty input"          "$F" 'printf "" | tee out; stat -c %s out'
run_case "binary data"          "$F" 'printf "\001\002\377" | tee out; od -An -tx1 out'
run_case "bad option"           "$F" 'printf hello | tee --nonsense out'
run_case "bad short option"     "$F" 'printf hello | tee -Z out'
run_case "double dash"          "$F" 'printf hello | tee -- -weird; cat ./-weird'
run_case "append to new file"   "$F" 'printf hello | tee -a fresh; cat fresh'

summary