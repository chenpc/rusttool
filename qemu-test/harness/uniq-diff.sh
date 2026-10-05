#!/usr/bin/env bash
# Differential test for uniq.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=uniq
SYS=/usr/bin/uniq
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

F='printf "a\na\nb\nb\nb\nc\n" > input'
run_case "default"              "$F" 'uniq input'
run_case "count"                "$F" 'uniq -c input'
run_case "count long"           "$F" 'uniq --count input'
run_case "repeated"             "$F" 'uniq -d input'
run_case "unique"               "$F" 'uniq -u input'
run_case "all repeated"         "$F" 'uniq -D input'
run_case "all repeated none"    "$F" 'uniq --all-repeated=none input'
run_case "all repeated prepend" "$F" 'uniq --all-repeated=prepend input'
run_case "all repeated separate" "$F" 'uniq --all-repeated=separate input'
run_case "group"                "$F" 'uniq --group input'
run_case "group prepend"        "$F" 'uniq --group=prepend input'
run_case "group append"         "$F" 'uniq --group=append input'
run_case "group both"           "$F" 'uniq --group=both input'
run_case "group separate"       "$F" 'uniq --group=separate input'
run_case "count and repeated"   "$F" 'uniq -cd input'
run_case "count and unique"     "$F" 'uniq -cu input'
run_case "count and all"        "$F" 'uniq -cD input'
run_case "ignore case"          ':' 'printf "A\na\nB\n" > input; uniq -i input'
run_case "no ignore case"       ':' 'printf "A\na\nB\n" > input; uniq input'
run_case "skip fields"          ':' 'printf "a 1\na 2\nb 1\n" > input; uniq -f1 input'
run_case "skip fields two"      ':' 'printf "a x 1\na x 2\nb y 1\n" > input; uniq -f2 input'
run_case "skip chars"           ':' 'printf "xxa\nxxb\nyyc\n" > input; uniq -s2 input'
run_case "check chars"          ':' 'printf "a1\na2\nb1\n" > input; uniq -w1 input'
run_case "skip and check"       ':' 'printf "k a1\nk a2\nk b1\n" > input; uniq -s2 -w1 input'
run_case "attached numbers"     ':' 'printf "a 1\na 2\n" > input; uniq -f1 input'
run_case "long options"         ':' 'printf "a\na\n" > input; uniq --skip-fields=1 --count input'
run_case "zero terminated"      ':' 'printf "a\0a\0b\0" | uniq -z | od -c | head -2'
run_case "stdin"                ':' 'printf "a\na\n" | uniq'
run_case "input and output"     "$F" 'uniq input out; cat out'
run_case "missing input"        ':' 'uniq nope'
run_case "extra operand"        "$F" 'uniq input out extra'
run_case "no operands"          ':' 'printf "a\na\n" | uniq'
run_case "empty input"          ':' 'printf "" | uniq'
run_case "single line"          ':' 'printf "only\n" | uniq'
run_case "all same"             ':' 'printf "a\na\na\n" | uniq -c'
run_case "all different"        ':' 'printf "a\nb\nc\n" | uniq -c'
run_case "no trailing newline"  ':' 'printf "a\na" | uniq -c'
run_case "repeated and unique"  ':' 'printf "a\nb\n" | uniq -du'
run_case "bad option"           ':' 'uniq --nonsense'
run_case "bad short option"     ':' 'uniq -Z'
run_case "negative skip"        ':' 'printf "a\n" | uniq -f-1'
run_case "junk skip"            ':' 'printf "a\n" | uniq -fx'
run_case "bad method"           ':' 'printf "a\n" | uniq --group=nope'
run_case "blank lines"          ':' 'printf "a\n\na\n" | uniq -c'
run_case "tabs as blanks"       ':' 'printf "a\t1\nb\t2\n" > input; uniq -f1 input'
run_case "long runs"            ':' 'seq 1 50 | uniq -c | tail -3'

summary