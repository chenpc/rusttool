#!/usr/bin/env bash
# Differential test for sort.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=sort
SYS=/usr/bin/sort
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

W='printf "banana\napple\ncherry\napple\n" > words'
N='printf "10\n9\n2\n100\n" > numbers'
run_case "plain sort"            "$W" 'sort words'
run_case "reverse"               "$W" 'sort -r words'
run_case "numeric"               "$N" 'sort -n numbers'
run_case "numeric reverse"       "$N" 'sort -nr numbers'
run_case "numeric with text"     ':' 'printf "a10\nb9\n" | sort -n'
run_case "general numeric"       ':' 'printf "a2x\nb10y\n" | sort -g'
run_case "human numeric"         ':' 'printf "1G\n2K\n999\n" | sort -h'
run_case "version sort"          ':' 'printf "a-10\n a-2\na-1\n" | sort -V'
run_case "version sort mixed"    ':' 'printf "1.0\n1.0.1\n1.0.10\n" | sort -V'
run_case "month sort"            ':' 'printf "DEC\nJAN\nzzz\nFEB\n" | sort -M'
run_case "dictionary"            ':' 'printf "a-b\nab\na_b\n" | sort -d'
run_case "fold case"             ':' 'printf "banana\nApple\ncherry\n" | sort -f'
run_case "ignore blanks"         ':' 'printf "  b\na\n   c\n" | sort -b'
run_case "ignore nonprinting"    ':' 'printf "a\x01b\nab\n" | sort -i'
run_case "unique"                "$W" 'sort -u words'
run_case "unique numeric"        ':' 'printf "1\n01\n2\n" | sort -nu'
run_case "unique reverse"        ':' 'printf "1\n01\n2\n" | sort -nru'
run_case "stable"                ':' 'printf "b 1\na 2\nb 0\na 1\n" | sort -s -k1,1'
run_case "key field"             ':' 'printf "b 2\na 3\nb 1\n" | sort -k1,1'
run_case "key field reverse"     ':' 'printf "b 2\na 3\nb 1\n" | sort -k1,1r'
run_case "key from second field" ':' 'printf "x 3\ny 1\nz 2\n" | sort -k2'
run_case "key attached"          ':' 'printf "b 2\na 3\n" | sort -k1,1'
run_case "key long option"       ':' 'printf "b 2\na 3\n" | sort --key=1,1'
run_case "key with options"      ':' 'printf "B 2\na 3\n" | sort -k1,1f'
run_case "key character offset"  ':' 'printf "abc 1\nabd 2\n" | sort -k1.2,1.2'
run_case "key with separator"    ':' 'printf "b:2\na:3\n" | sort -t: -k1,1'
run_case "separator and key"     ':' 'printf "b:2:z\na:3:q\n" | sort -t: -k2,2n'
run_case "check sorted"          ':' 'printf "a\nb\nc\n" | sort -c; echo "exit=$?"'
run_case "check unsorted"        ':' 'printf "b\na\n" | sort -c; echo "exit=$?"'
run_case "check quiet"           ':' 'printf "b\na\n" | sort -C; echo "exit=$?"'
run_case "check unique"          ':' 'printf "a\na\nb\n" | sort -cu; echo "exit=$?"'
run_case "merge"                 ':' 'printf "a\nc\n" > one; printf "b\nd\n" > two; sort -m one two'
run_case "zero terminated"       ':' 'printf "b\0a\0" | sort -z | od -c | head -2'
run_case "stdin"                 ':' 'printf "b\na\n" | sort'
run_case "two files"             ':' 'printf "b\n" > one; printf "a\n" > two; sort one two'
run_case "missing file"          ':' 'sort nope'
run_case "empty input"           ':' 'printf "" | sort'
run_case "single line"           ':' 'printf "only\n" | sort'
run_case "no trailing newline"   ':' 'printf "b\na" | sort'
run_case "empty lines"           ':' 'printf "b\n\na\n" | sort'
run_case "bad option"            ':' 'sort --nonsense'
run_case "bad short option"      ':' 'sort -Z'
run_case "bad key"               ':' 'sort -k0'
run_case "bad separator"         ':' 'sort -tab -k1'
run_case "bad sort word"         ':' 'sort --sort=nonsense'
run_case "sort word numeric"     "$N" 'sort --sort=numeric numbers'
run_case "sort word version"     ':' 'printf "a-10\na-2\n" | sort --sort=version'
run_case "many lines"            ':' 'seq 1 100 | sort -n | head -3'
run_case "duplicates keep first" ':' 'printf "b 1\na 2\nb 3\n" | sort -k1,1'
run_case "tabs in fields"        ':' 'printf "b\t2\na\t3\n" | sort -k1,1'
run_case "leading blanks fields" ':' 'printf "  b 2\na 3\n" | sort -k1,1'

summary