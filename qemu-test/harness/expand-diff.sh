#!/usr/bin/env bash
# Differential test for expand against GNU expand.
set -u
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TOOL=expand
SYS=/usr/bin/expand
. /home/chenpc/git/rusttool/qemu-test/harness/difflib.sh

# Fixtures: tabs at every column, leading tabs, runs of blanks and backspaces.
TABS="printf 'a\tb\n\tc\nab\tc\td\n' > t1"
MIX="printf 'x\ty\n  \tz\n\t\n\t\t\n\n' > t2"
BS="printf 'abc\b\b\td\ne\tf' > t3"
BLANK="printf '\t \t\ta\n \t \t b\n' > t4"
WIDE="printf '12345678901234567890\tx\n' > t5"
EMPTY=": > e1"
TWO="$TABS; printf 'q\tr\n' > t2"

run_case "default"           "$TABS" 'expand t1'
run_case "default mix"       "$MIX" 'expand t2'
run_case "empty file"        "$EMPTY" 'expand e1'
run_case "no newline"        "$BS" 'expand t3'
run_case "blank runs"        "$BLANK" 'expand t4'
run_case "past column 8"     "$WIDE" 'expand t5'
run_case "two files"         "$TWO" 'expand t1 t2'
# The operands are one stream, so a first file with no trailing newline leaves
# the column and the tab-stop cursor where they were.
run_case "no newline carries" "$BS; printf -- '-\tz\n' > t4" 'expand t3 t4'
run_case "newline resets"     "$BS; printf '\n' > t4" 'expand t3 t4'
run_case "three no newline"   'printf a > u1; printf b > u2; printf -- "c\td\n" > u3' 'expand u1 u2 u3'
run_case "stdin"             "$TABS" 'expand < t1'
run_case "dash is stdin"     "$TABS" 'expand - < t1'
run_case "double dash"       "$TABS" 'expand -- t1'

run_case "initial"           "$TABS" 'expand -i t1'
run_case "initial mix"       "$MIX" 'expand -i t2'
run_case "initial blanks"    "$BLANK" 'expand -i t4'
run_case "initial long"      "$TABS" 'expand --initial t1'
run_case "initial backspace" "$BS" 'expand -i t3'

run_case "t2"                "$TABS" 'expand -t 2 t1'
run_case "t4"                "$TABS" 'expand -t 4 t1'
run_case "t4 attached"       "$TABS" 'expand -t4 t1'
run_case "t4 space arg"      "$TABS" 'expand -t 4 t1'
run_case "t1"                "$TABS" 'expand -t 1 t1'
run_case "t16"               "$TABS" 'expand -t 16 t1'
run_case "digit option"      "$TABS" 'expand -4 t1'
run_case "digit bundle"      "$TABS" 'expand -i4 t1'
run_case "digit arg"         "$TABS" 'expand -4x t1'
run_case "long tabs"         "$TABS" 'expand --tabs=4 t1'
run_case "long tabs arg"     "$TABS" 'expand --tabs 4 t1'
run_case "list 2,6"          "$TABS" 'expand -t 2,6 t1'
run_case "list 0,4"          "$TABS" 'expand -t 0,4 t1'
run_case "list 4,2"          "$TABS" 'expand -t 4,2 t1'
run_case "list spaces"       "$TABS" 'expand -t "2 6" t1'
run_case "list trailing"     "$TABS" 'expand -t 4, t1'
run_case "list empty"        "$TABS" 'expand -t "" t1'
run_case "two lists"         "$TABS" 'expand -t 2 -t 4 t1'
run_case "two lists down"    "$TABS" 'expand -t 4 -t 2 t1'
run_case "two lists zero"    "$TABS" 'expand -t 4 -t 0 t1'
run_case "zero"              "$TABS" 'expand -t 0 t1'
run_case "list on blanks"    "$BLANK" 'expand -t 2,6 t4'
run_case "list on wide"      "$WIDE" 'expand -t 4 t5'
# The stop cursor only moves past a stop the column has reached, so a backspace
# can take it back to one that is still ahead.
run_case "list and backspace" "printf 'a\tb\b\tc\n' > l1" 'expand -t 4,8 l1'
run_case "list back to first" "printf '\ta\tb\n' > l2" 'expand -t 2,6 l2'
run_case "list repeated stop" "printf '\t\ta\tb\n' > l3" 'expand -t 2,6 l3'

run_case "extend 4/2"        "$TABS" 'expand -t 4/2 t1'
run_case "extend 2,4/3"      "$TABS" 'expand -t 2,4/3 t1'
run_case "extend 2,4,/3"     "$TABS" 'expand -t 2,4,/3 t1'
run_case "extend alone"      "$TABS" 'expand -t /3 t1'
run_case "increment alone"   "$TABS" 'expand -t +2 t1'
run_case "increment 4,+2"    "$TABS" 'expand -t 4,+2 t1'
run_case "increment 2,+3"    "$TABS" 'expand -t 2,+3 t1'
run_case "increment 2,4,+3"  "$TABS" 'expand -t 2,4,+3 t1'
run_case "increment twice"   "$TABS" 'expand -t 2,+3,4 t1'
run_case "slash then plus"   "$TABS" 'expand -t /3 -t +1 t1'
run_case "plus then slash"   "$TABS" 'expand -t +1 -t /3 t1'
run_case "slash twice"       "$TABS" 'expand -t /3 -t /4 t1'
run_case "plus twice"        "$TABS" 'expand -t +1 -t +2 t1'
run_case "spec after number" "$TABS" 'expand -t 2/3 t1'
run_case "spec after number2" "$TABS" 'expand -t 4+x t1'
run_case "spec mid list"     "$TABS" 'expand -t /2,6 t1'
run_case "plus mid list"     "$TABS" 'expand -t +2,6 t1'

run_case "bad char"          "$TABS" 'expand -t x t1'
run_case "bad char in list"  "$TABS" 'expand -t 4x,2 t1'
run_case "too large"         "$TABS" 'expand -t 99999999999999999999 t1'
run_case "too large in list" "$TABS" 'expand -t 1,99999999999999999999 t1'
run_case "negative"          "$TABS" 'expand -t -1 t1'

run_case "initial and t"     "$TABS" 'expand -i -t 2,6 t1'
run_case "bundle it"         "$TABS" 'expand -it4 t1'
run_case "missing file"      "$TABS" 'expand nope'
run_case "missing second"    "$TABS" 'expand t1 nope'
run_case "directory"         "$TABS" 'expand /tmp'
run_case "bad option"        "$TABS" 'expand -Q t1'
run_case "bad long option"   "$TABS" 'expand --nonsense t1'
run_case "missing t arg"     "$TABS" 'expand t1 -t'
run_case "missing long arg"  "$TABS" 'expand t1 --tabs'
run_case "help"              "$TABS" 'expand --help | head -1'
run_case "version"           "$TABS" 'expand --version | head -1'

summary