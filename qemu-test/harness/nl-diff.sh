#!/usr/bin/env bash
# Differential test for nl against GNU nl.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=nl
SYS=/usr/bin/nl
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

# The section delimiters are '\:' (footer), '\:\:' (body) and '\:\:\:' (header),
# so the fixtures use them verbatim.
PAGES=$(printf '%s\n' 'H1' '\:' '' 'B1' 'B2' '\:\:' '' 'B3' '\:\:\:' 'F1' '' 'F2')
TABS=$(printf '%s\n' 'a' '' 'b' '' '' 'c')
TWO=$(printf '%s\n' 'x y' 'p q' 'r s')

run_case "default"          "$PAGES" 'nl'
run_case "no number"        "$PAGES" 'nl -bn'
run_case "all"              "$PAGES" 'nl -ba'
run_case "all header"       "$PAGES" 'nl -hha'
run_case "all footer"       "$PAGES" 'nl -ffa'
run_case "header t"         "$PAGES" 'nl -ht'
run_case "footer t"         "$PAGES" 'nl -ft'
run_case "no renumber"      "$PAGES" 'nl -ba -p'
run_case "start 0"          "$PAGES" 'nl -ba -v0'
run_case "start 7"          "$PAGES" 'nl -ba -v7'
run_case "start negative"   "$PAGES" 'nl -ba -v-3'
run_case "increment 2"      "$PAGES" 'nl -ba -i2'
run_case "increment 0"      "$PAGES" 'nl -ba -i0'
run_case "increment -1"     "$PAGES" 'nl -ba -i-1'
run_case "width 3"          "$PAGES" 'nl -ba -w3'
run_case "width 1"          "$PAGES" 'nl -ba -w1'
run_case "width 8"          "$PAGES" 'nl -ba -w8'
run_case "format left"      "$PAGES" 'nl -ba -nln'
run_case "format zero"      "$PAGES" 'nl -ba -nrz'
run_case "format zero w3"   "$PAGES" 'nl -ba -nrz -w3'
run_case "format zero neg"  "$PAGES" 'nl -ba -nrz -v-2 -w5'
run_case "separator dots"   "$PAGES" 'nl -ba -s.'
run_case "separator empty"  "$PAGES" 'nl -ba -s'
run_case "separator many"   "$PAGES" 'nl -ba -s" | " -w4'
run_case "join 2"           "$TABS" 'nl -ba -l2'
run_case "join 3"           "$TABS" 'nl -ba -l3'
run_case "join 2 style t"   "$TABS" 'nl -bt -l2'
run_case "join 1"           "$TABS" 'nl -ba -l1'
run_case "delimiter colon"  "$(printf '%s\n' 'a' '::' 'b')" 'nl -ba -d:'
run_case "delimiter ab"     "$(printf '%s\n' 'a' 'abab' 'ab')" 'nl -ba -dab'
run_case "delimiter long"   "$(printf '%s\n' 'a' 'xyz' 'b')" 'nl -ba -dxyz'
run_case "delimiter empty"  "$(printf '%s\n' 'a' '\:' 'b')" 'nl -ba -d""'
# A missing second -d character implies ':', so '-da' builds on 'a:' and '-d:'
# on '::'. Three or more characters is the GNU extension and is kept as given;
# only the first two characters gate a line, so "abc" is never a delimiter even
# when the prefix matches.
run_case "delimiter one char" "$(printf '%s\n' 'x' 'a:' 'a:a:' 'a:a:a:' 'aa' 'aa:' 'aa:a:')" 'nl -ba -da'
run_case "delimiter two char" "$(printf '%s\n' 'x' 'a:' 'a:a:' 'a:a:a:' 'ab' 'abab' 'ababab')" 'nl -ba -da:'
run_case "delimiter three char" "$(printf '%s\n' 'x' 'ab' 'abab' 'ababab' 'abc')" 'nl -ba -dab:'
run_case "delimiter two char ab" "$(printf '%s\n' 'x' 'ab' 'abab' 'ababab' 'abc')" 'nl -ba -dab'
run_case "delimiter leading colon" "$(printf '%s\n' 'x' 'ab' 'abab')" 'nl -ba -d:ab'
run_case "delimiter backslash" "$(printf '%s\n' 'x' '\:' '\:\:' '\:\:\:')" 'nl -ba -d\'
run_case "delimiter partial" "$(printf '%s\n' 'a' '\:x' '\:\:' 'b')" 'nl -ba'
run_case "header one char"  "$(printf '%s\n' '\:' 'a')" 'nl -ba'
run_case "pattern digit"    "$TWO" 'nl -bp"[0-9]"'
run_case "pattern start"    "$TWO" 'nl -bp"^p"'
run_case "pattern none"     "$TWO" 'nl -bp"zzz"'
run_case "pattern header"   "$PAGES" 'nl -bpn -h p"^H"'
run_case "pattern footer"   "$PAGES" 'nl -bpn -f p"^F"'
run_case "pattern empty"    "$TWO" 'nl -bp""'
run_case "pattern star"     "$TWO" 'nl -bp"q*"'
run_case "pattern bracket"  "$TWO" 'nl -bp"[xy]"'
run_case "pattern group"    "$TWO" 'nl -bp"\(x\|y\) "'
run_case "pattern dot"      "$TWO" 'nl -bp".."'
run_case "no trailing nl"   'a\nb' 'nl -ba'
run_case "single empty"     '' 'nl -ba'
run_case "one newline"      '\n' 'nl -ba'
run_case "bad style"        "$TWO" 'nl -bz'
run_case "bad header style" "$TWO" 'nl -hz'
run_case "bad footer style" "$TWO" 'nl -fz'
run_case "bad format"       "$TWO" 'nl -nxx'
run_case "bad width"        "$TWO" 'nl -wx'
run_case "bad width zero"   "$TWO" 'nl -w0'
run_case "bad join zero"    "$TWO" 'nl -l0'
run_case "bad start"        "$TWO" 'nl -vx'
run_case "bad increment"    "$TWO" 'nl -ix'
run_case "missing arg"      "$TWO" 'nl -w'
run_case "missing long arg" "$TWO" 'nl --number-width'
run_case "missing long sep" "$TWO" 'nl --number-separator'
run_case "missing long del" "$TWO" 'nl --section-delimiter'
run_case "bad option"       "$TWO" 'nl -Z'
run_case "bad long option"  "$TWO" 'nl --nonsense'
run_case "bad regex"        "$TWO" 'nl -bp"["'
run_case "bad regex header" "$TWO" 'nl -hp"["'
run_case "bad regex footer" "$TWO" 'nl -fp"["'
run_case "bad regex long"   "$TWO" 'nl --body-numbering="p["'
run_case "missing file"     "$TWO" 'nl nope'
run_case "two files"        "$TWO" 'nl -ba <(echo one) <(echo two)'
run_case "dash is stdin"    "$TWO" 'nl -ba -'
run_case "double dash"      "$TWO" 'nl -ba -- <(echo one)'

summary