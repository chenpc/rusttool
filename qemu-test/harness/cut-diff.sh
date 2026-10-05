#!/usr/bin/env bash
# Differential test for cut.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=cut
SYS=/usr/bin/cut
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

F='printf "a\tb\tc\nd\te\nlonely\nf\tg\th\ti\n" > input'
run_case "field one"           "$F" 'cut -f1 input'
run_case "field two"           "$F" 'cut -f2 input'
run_case "field three"         "$F" 'cut -f3 input'
run_case "field list"          "$F" 'cut -f1,3 input'
run_case "field range"         "$F" 'cut -f2-3 input'
run_case "open range"          "$F" 'cut -f2- input'
run_case "range from start"    "$F" 'cut -f-2 input'
run_case "past the end"        "$F" 'cut -f9 input'
run_case "reversed range"      "$F" 'cut -f3-1 input'
run_case "overlapping ranges"  "$F" 'cut -f1-2,2-3 input'
run_case "order follows input" "$F" 'cut -f3,1 input'
run_case "attached list"       "$F" 'cut -f1,3 input'
run_case "separate list"       "$F" 'cut -f 1,3 input'
run_case "long option"         "$F" 'cut --fields=1 input'
run_case "long option equals"  "$F" 'cut --fields 1 input'
run_case "custom delimiter"    "$F" 'cut -d" " -f1 input; printf "a b c\n" | cut -d" " -f2'
run_case "delimiter colon"     "$F" 'printf "a:b:c\n" | cut -d: -f2'
run_case "delimiter attached"  "$F" 'printf "a:b:c\n" | cut -d: -f2'
run_case "only delimited"      "$F" 'cut -s -f1 input'
run_case "complement"          "$F" 'cut --complement -f2 input'
run_case "complement delimited" "$F" 'cut --complement -d" " -f1 <<< "a b c"'
run_case "output delimiter"    "$F" 'cut -f1,3 --output-delimiter="," input'
run_case "output delimiter attached" "$F" 'cut -f1,3 -d" " --output-delimiter=- <<< "a b c"'
run_case "characters"          "$F" 'cut -c1,3 <<< "abcdef"'
run_case "characters range"    "$F" 'cut -c2-4 <<< "abcdef"'
run_case "bytes"               "$F" 'cut -b1,2 <<< "abcdef"'
run_case "characters complement" "$F" 'cut -c --complement <<< "abcdef"'
run_case "no unit"             "$F" 'cut input'
run_case "two units"           "$F" 'cut -f1 -c1 input'
run_case "bad list"            "$F" 'cut -f0 input'
run_case "junk list"           "$F" 'cut -fz input'
run_case "empty list"          "$F" 'cut -f "" input'
run_case "missing list"        "$F" 'cut -f'
run_case "missing file"        "$F" 'cut -f1 nope'
run_case "stdin"               "$F" 'cut -f1 < input'
run_case "dash is stdin"       "$F" 'cut -f1 - < input'
run_case "two files"           "$F" 'cut -f1 input input'
run_case "no trailing newline" "$F" 'printf "a\tb" | cut -f1'
run_case "empty input"         "$F" 'printf "" | cut -f1'
run_case "empty line"          "$F" 'printf "a\tb\n\nc\td\n" | cut -f1'
run_case "zero terminated"     "$F" 'printf "a\tb\0c\td\0" | cut -z -f1 | od -c | head -2'
run_case "n is ignored"        "$F" 'cut -n -f1 input'
run_case "bad delimiter"       "$F" 'cut -dab -f1 input'
run_case "missing delimiter"   "$F" 'cut -d -f1 input'
run_case "bad option"          "$F" 'cut --nonsense input'
run_case "bad short option"    "$F" 'cut -Z input'
run_case "tab is the default"  "$F" 'cut -f2 <<< "a b	c"'

summary