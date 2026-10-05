#!/usr/bin/env bash
# Differential test for unexpand against GNU unexpand.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=unexpand
SYS=/usr/bin/unexpand
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

LEAD="printf '        a\n  b\n  \tc\n' > u1"
MID="printf 'a        b\na \na       b\na   b\na    b\n' > u2"
BLANKS="printf '\t \t\ta\n \t \t b\n' > u3"
BS="printf 'a       \b\td\nb       \b\bd\n' > u4"
TABS="printf '\ta\n\ta\n' > u5"
NONL="printf '    a' > u6"
TWO="$LEAD; printf 'q        r\n' > u2"

run_case "default"          "$LEAD" 'unexpand u1'
run_case "middle runs"      "$MID" 'unexpand u2'
run_case "blank runs"       "$BLANKS" 'unexpand u3'
run_case "tabs"             "$TABS" 'unexpand u5'
run_case "backspace"        "$BS" 'unexpand u4'
run_case "no newline"       "$NONL" 'unexpand u6'
run_case "two files"        "$TWO" 'unexpand u1 u2'
run_case "no nl then file"  "printf '  a' > u7; printf 'b\n' > u8" 'unexpand u7 u8'
run_case "stdin"            "$LEAD" 'unexpand < u1'
run_case "dash is stdin"    "$LEAD" 'unexpand - < u1'
run_case "double dash"      "$LEAD" 'unexpand -- u1'

run_case "all"              "$MID" 'unexpand -a u2'
run_case "all long"         "$MID" 'unexpand --all u2'
run_case "all blanks"       "$BLANKS" 'unexpand -a u3'
run_case "all tabs"         "$TABS" 'unexpand -a u5'
run_case "all backspace"    "$BS" 'unexpand -a u4'
run_case "all no newline"   "$NONL" 'unexpand -a u6'
run_case "first-only beats all" "$MID" 'unexpand -a --first-only u2'
run_case "first-only alone" "$MID" 'unexpand --first-only u2'
run_case "first-only long"  "$MID" 'unexpand -a --first-only u1'

run_case "t2"               "$LEAD" 'unexpand -t 2 u1'
run_case "t2 attached"      "$LEAD" 'unexpand -t2 u1'
run_case "t2 all"           "$MID" 'unexpand -a -t 2 u2'
run_case "t4"               "$LEAD" 'unexpand -t 4 u1'
run_case "t4 long"          "$LEAD" 'unexpand --tabs=4 u1'
run_case "t4 long arg"      "$LEAD" 'unexpand --tabs 4 u1'
run_case "list 2,6"         "$LEAD" 'unexpand -t 2,6 u1'
run_case "list 2,6 all"     "$MID" 'unexpand -a -t 2,6 u2'
run_case "digit option"     "$LEAD" 'unexpand -2 u1'
run_case "digit bundle"     "$LEAD" 'unexpand -2,6 u1'
run_case "comma option"     "$LEAD" 'unexpand -,2 u1'
run_case "t implies all"    "$MID" 'unexpand -t 8 u2'
run_case "extend"           "$LEAD" 'unexpand -a -t 2,4,/3 u2'
run_case "increment"        "$LEAD" 'unexpand -a -t 2,+3 u2'
run_case "extend alone"     "$LEAD" 'unexpand -t /3 u1'
run_case "increment alone"  "$LEAD" 'unexpand -t +2 u1'

run_case "zero"             "$LEAD" 'unexpand -t 0 u1'
run_case "descending"       "$LEAD" 'unexpand -t 4,2 u1'
run_case "bad character"    "$LEAD" 'unexpand -t 4x,2 u1'
run_case "misplaced spec"   "$LEAD" 'unexpand -t 4+x u1'
run_case "too large"        "$LEAD" 'unexpand -t 99999999999999999999 u1'
run_case "repeated slash"   "$LEAD" 'unexpand -t /3 -t /4 u1'
run_case "slash and plus"   "$LEAD" 'unexpand -t /3 -t +1 u1'

run_case "missing file"     "$LEAD" 'unexpand nope'
run_case "missing second"   "$LEAD" 'unexpand u1 nope'
run_case "directory"        "$LEAD" 'unexpand /tmp'
run_case "bad option"       "$LEAD" 'unexpand -Q u1'
run_case "bad long option"  "$LEAD" 'unexpand --nonsense u1'
run_case "missing t arg"    "$LEAD" 'unexpand u1 -t'
run_case "bundle"           "$LEAD" 'unexpand -at4 u1'
run_case "help"             "$LEAD" 'unexpand --help | head -1'
run_case "version"          "$LEAD" 'unexpand --version | head -1'

summary