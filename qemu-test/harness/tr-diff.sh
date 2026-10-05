#!/usr/bin/env bash
# Differential test for tr.
# # Our debug builds must come first: the cases call the tools by bare name,
# # so without this the "ours" side would be the system tool and every case
# # would compare the system with itself.
export PATH="/home/chenpc/git/rusttool/target/debug:$PATH"

set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=tr
SYS=/usr/bin/tr
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

run_case "rotate"                 ':' 'printf abc | tr abc bca'
run_case "case conversion"        ':' 'printf Hello | tr a-z A-Z'
run_case "lower to upper class"   ':' 'printf hello | tr "[:lower:]" "[:upper:]"'
run_case "upper to lower class"   ':' 'printf HELLO | tr "[:upper:]" "[:lower:]"'
run_case "delete one"             ':' 'printf abcabc | tr a d'
run_case "delete set"             ':' 'printf hello | tr a-z d'
run_case "delete class"           ':' 'printf a1b2c3 | tr -d "[:digit:]"'
run_case "squeeze"                ':' 'printf "a   b" | tr -s " "'
run_case "squeeze class"          ':' 'printf "aaabbbcccd" | tr -s "[:alpha:]"'
run_case "squeeze and translate"  ':' 'printf aaa | tr -s a b'
run_case "translate then squeeze" ':' 'printf "xaaaa" | tr -s "[:alpha:]" x'
run_case "complement delete"      ':' 'printf "a1b2" | tr -cd "[:digit:]"'
run_case "complement translate"   ':' 'printf "a1b2" | tr -c "[:digit:]" .'
run_case "octal escapes"          ':' 'printf "a\tb" | tr "\t" "-"'
run_case "octal numeric"          ':' 'printf "\101\102" | tr "\101\102" xy'
run_case "backslash escape"       ':' 'printf "a\\\\b" | tr "\\\\" "/"'
run_case "newline escape"         ':' 'printf "a\nb" | tr "\n" "|"; echo'
run_case "range in set1"          ':' 'printf "abcdef" | tr a-f 1'
run_case "range both ways"        ':' 'printf "0123456789" | tr 0-4 a-e'
run_case "digit class translate"  ':' 'printf "a1b2" | tr "[:digit:]" ab'
run_case "space class"            ':' 'printf "a\tb\nc d" | tr "[:space:]" "-"'
run_case "punct class"            ':' 'printf "a!b?c" | tr "[:punct:]" "-"'
run_case "upper case class"       ':' 'printf "aB1" | tr "[:upper:]" "[:lower:]"'
run_case "alnum class"            ':' 'printf "a1-b2" | tr -d "[:alnum:]"'
run_case "xdigit class"           ':' 'printf "deadBEEF01" | tr "[:xdigit:]" x'
run_case "blank class"            ':' 'printf "a\t b" | tr "[:blank:]" "-"'
run_case "truncate set1"          ':' 'printf abcdef | tr -t abc xy'
run_case "truncate longer"        ':' 'printf abc | tr -t abcdef xy'
run_case "pad set2"               ':' 'printf abcd | tr abcd x'
run_case "delete with two strings" ':' 'printf abc | tr -d ab xy'
run_case "squeeze delete both"    ':' 'printf "aabb  cc" | tr -sd "[:alpha:]"'
run_case "equivalence class"      ':' 'printf abc | tr "[=b=]" B'
run_case "repeat in set2"         ':' 'printf ab | tr ab "[x*2]"'
run_case "repeat with count"      ':' 'printf ab | tr ab "[x*3]"'
run_case "missing operand"        ':' 'printf abc | tr'
run_case "three operands"         ':' 'printf abc | tr a b c'
run_case "no operand input"       ':' 'tr a b < /dev/null'
run_case "bad escape"             ':' 'printf abc | tr "\q" x'
run_case "bad class"              ':' 'printf abc | tr "[:nope:]" x'
run_case "bad range"              ':' 'printf abc | tr c-a x'
run_case "bad option"             ':' 'printf abc | tr --nonsense a b'
run_case "bad short option"       ':' 'printf abc | tr -Z a b'
run_case "empty input"            ':' 'printf "" | tr a b'
run_case "high bytes"             ':' 'printf "\200\377" | tr "\200\377" ab'
run_case "tab as set"             ':' 'printf "a\tb" | tr "\t" "\t"'
run_case "long options"           ':' 'printf abc | tr --delete a; printf abc | tr --squeeze-repeats a'
run_case "complement long option" ':' 'printf "a1b2" | tr --complement --delete "[:digit:]"'
run_case "double dash"            ':' 'printf -- -a | tr -- -a b'

summary